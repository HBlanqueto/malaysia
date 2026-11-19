use std::path::Path;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use calloop::ping::Ping;
use ctdesign::WALLPAPER_FPS;
use ferricast_player::{
    CapturedFrame, Clock, Color, Due, FrameLimiter, PixelFormat, Player, RawFrame,
};

use crate::image::{DecodedImage, Pixels};
use crate::render::{Nv12, Yuv};

const PREROLL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_LATENESS: Duration = Duration::from_micros(2_000_000 / WALLPAPER_FPS as u64);

type Latest = Arc<Mutex<Option<Frame>>>;

pub(crate) struct Video {
    clock: Arc<Clock>,
    latest: Latest,
}

pub(crate) struct Frame {
    raw: RawFrame,
    yuv: Yuv,
}

impl Video {
    pub(crate) fn open(path: &Path, wake: Option<Ping>) -> anyhow::Result<Self> {
        let player = Player::open(path)?;
        let stream = player.video();
        let yuv = yuv(stream.color, stream.height);
        let clock = Arc::new(Clock::new(MAX_LATENESS));
        clock.pause();
        let latest = Latest::default();
        let (prerolled, preroll) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("video-wallpaper".into())
            .spawn({
                let (clock, latest) = (clock.clone(), latest.clone());
                move || decode(player, &clock, &latest, wake, yuv, &prerolled)
            })?;
        let video = Self { clock, latest };
        match preroll.recv_timeout(PREROLL_TIMEOUT) {
            Ok(Err(error)) => Err(error),
            _ => Ok(video),
        }
    }

    pub(crate) fn set_playing(&self, playing: bool) {
        if playing {
            self.clock.resume();
        } else {
            self.clock.pause();
        }
    }

    pub(crate) fn take(&self) -> Option<Frame> {
        self.latest.lock().ok()?.take()
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.clock.stop();
    }
}

impl Frame {
    pub(crate) fn nv12(&self) -> Option<Nv12<'_>> {
        let raw = &self.raw;
        if raw.format != PixelFormat::Nv12 {
            return None;
        }
        let luma_len = raw.stride as usize * raw.height as usize;
        let chroma_len = raw.stride as usize * (raw.height as usize).div_ceil(2);
        let (luma, chroma) = raw
            .data
            .get(..luma_len + chroma_len)
            .map(|planes| planes.split_at(luma_len))?;
        Some(Nv12 {
            size: (raw.width, raw.height),
            luma,
            luma_stride: raw.stride,
            chroma,
            chroma_stride: raw.stride,
            yuv: self.yuv,
        })
    }

    pub(crate) fn rgba(&self) -> Option<DecodedImage> {
        let nv12 = self.nv12()?;
        let (width, height) = (nv12.size.0 as usize, nv12.size.1 as usize);
        let stride = nv12.luma_stride as usize;
        let Yuv { kr, kb, full_range } = nv12.yuv;
        let kg = 1.0 - kr - kb;
        let (luma_offset, luma_range, chroma_range) = if full_range {
            (0.0, 255.0, 255.0)
        } else {
            (16.0, 219.0, 224.0)
        };
        let mut pixels = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            let luma = nv12.luma.get(row * stride..)?.get(..width)?;
            let chroma = nv12
                .chroma
                .get(row / 2 * stride..)?
                .get(..width.div_ceil(2) * 2)?;
            for (column, &luma) in luma.iter().enumerate() {
                let y = (f32::from(luma) - luma_offset) / luma_range;
                let cb = (f32::from(chroma[column / 2 * 2]) - 128.0) / chroma_range;
                let cr = (f32::from(chroma[column / 2 * 2 + 1]) - 128.0) / chroma_range;
                let r = y + 2.0 * (1.0 - kr) * cr;
                let b = y + 2.0 * (1.0 - kb) * cb;
                let g = (y - kr * r - kb * b) / kg;
                pixels.extend([r, g, b].map(channel_byte));
                pixels.push(255);
            }
        }
        Some(DecodedImage {
            size: nv12.size,
            pixels: Pixels::Srgb8(pixels),
        })
    }
}

fn decode(
    mut player: Player,
    clock: &Clock,
    latest: &Latest,
    wake: Option<Ping>,
    yuv: Yuv,
    prerolled: &SyncSender<anyhow::Result<()>>,
) {
    let mut limiter = FrameLimiter::new(WALLPAPER_FPS);
    let mut prerolling = true;
    loop {
        let raw = match next_frame(&mut player) {
            Ok(raw) => raw,
            Err(error) => {
                if prerolling {
                    let _ = prerolled.send(Err(error));
                } else {
                    tracing::warn!(%error, "video wallpaper error");
                }
                return;
            }
        };
        if !prerolling {
            match clock.wait_for(raw.timestamp_us) {
                Due::Stopped => return,
                Due::Late => continue,
                Due::Show => {}
            }
        }
        if limiter.accepts(raw.timestamp_us) {
            store(latest, Frame { raw, yuv });
            if let Some(wake) = &wake {
                wake.ping();
            }
        }
        if prerolling {
            prerolling = false;
            let _ = prerolled.send(Ok(()));
        }
    }
}

fn next_frame(player: &mut Player) -> anyhow::Result<RawFrame> {
    for _ in 0..2 {
        if let Some(frame) = player.next_frame()? {
            return Ok(CapturedFrame::into_cpu(frame)?);
        }
        player.rewind()?;
    }
    anyhow::bail!("the video gave no frame")
}

fn store(latest: &Latest, frame: Frame) {
    if let Ok(mut slot) = latest.lock() {
        *slot = Some(frame);
    }
}

fn yuv(color: Option<Color>, height: u32) -> Yuv {
    let (kr, kb) = color
        .and_then(|color| luma_weights(color.matrix_coefficients))
        .unwrap_or(if height >= 720 {
            (0.2126, 0.0722)
        } else {
            (0.299, 0.114)
        });
    Yuv {
        kr,
        kb,
        full_range: color.is_some_and(|color| color.full_range),
    }
}

fn luma_weights(matrix_coefficients: u8) -> Option<(f32, f32)> {
    match matrix_coefficients {
        1 => Some((0.2126, 0.0722)),
        4 => Some((0.30, 0.11)),
        5 | 6 => Some((0.299, 0.114)),
        7 => Some((0.212, 0.087)),
        9 => Some((0.2627, 0.0593)),
        _ => None,
    }
}

fn channel_byte(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::Framing;
    use crate::render::{RenderContext, offscreen_texture, padded_bytes_per_row, read_back};
    use futures_lite::future::block_on;

    const WIDTH: usize = 320;
    const HEIGHT: usize = 240;
    const BT709: Yuv = Yuv {
        kr: 0.2126,
        kb: 0.0722,
        full_range: false,
    };
    const WHITE: (u8, u8, u8) = (235, 128, 128);
    const BLACK: (u8, u8, u8) = (16, 128, 128);
    const RED: (u8, u8, u8) = (63, 102, 240);
    const GREEN: (u8, u8, u8) = (173, 42, 26);

    fn bars(colours: &[(u8, u8, u8)], yuv: Yuv) -> Frame {
        let bar = |column: usize| colours[column * colours.len() / WIDTH];
        let mut data = Vec::with_capacity(WIDTH * HEIGHT * 3 / 2);
        for _ in 0..HEIGHT {
            data.extend((0..WIDTH).map(|column| bar(column).0));
        }
        for _ in 0..HEIGHT / 2 {
            for column in (0..WIDTH).step_by(2) {
                data.extend([bar(column).1, bar(column).2]);
            }
        }
        Frame {
            raw: RawFrame {
                width: WIDTH as u32,
                height: HEIGHT as u32,
                stride: WIDTH as u32,
                format: PixelFormat::Nv12,
                data: data.into(),
                timestamp_us: 0,
            },
            yuv,
        }
    }

    fn rgba_at(frame: &Frame, column: usize) -> [u8; 3] {
        let Some(DecodedImage {
            pixels: Pixels::Srgb8(pixels),
            ..
        }) = frame.rgba()
        else {
            panic!("no RGBA frame");
        };
        let at = (HEIGHT / 2 * WIDTH + column) * 4;
        [pixels[at], pixels[at + 1], pixels[at + 2]]
    }

    fn close(found: [u8; 3], wanted: [u8; 3]) -> bool {
        found.iter().zip(wanted).all(|(a, b)| a.abs_diff(b) <= 2)
    }

    #[test]
    fn limited_range_bt709_bars_convert_to_their_colours() {
        let frame = bars(&[WHITE, BLACK, RED, GREEN], BT709);
        for (bar, wanted) in [[255, 255, 255], [0, 0, 0], [255, 0, 0], [0, 255, 0]]
            .into_iter()
            .enumerate()
        {
            let found = rgba_at(&frame, bar * WIDTH / 4 + 10);
            assert!(
                close(found, wanted),
                "bar {bar}: {found:?} is not {wanted:?}"
            );
        }
    }

    #[test]
    fn full_range_uses_every_code_value() {
        let full = Yuv {
            full_range: true,
            ..BT709
        };
        let frame = bars(&[(255, 128, 128), (0, 128, 128)], full);
        assert!(close(rgba_at(&frame, 10), [255, 255, 255]));
        assert!(close(rgba_at(&frame, WIDTH - 10), [0, 0, 0]));
    }

    #[test]
    fn a_frame_with_short_planes_is_not_nv12() {
        let mut frame = bars(&[WHITE], BT709);
        frame.raw.data = frame.raw.data.slice(..WIDTH * HEIGHT);
        assert!(frame.nv12().is_none());
    }

    #[test]
    fn matrix_coefficients_select_the_luma_weights() {
        assert_eq!(luma_weights(1), Some((0.2126, 0.0722)));
        assert_eq!(luma_weights(6), Some((0.299, 0.114)));
        assert_eq!(luma_weights(2), None);
    }

    #[test]
    fn unspecified_colour_falls_back_to_the_height() {
        let tall = yuv(None, 1080);
        let short = yuv(None, 480);
        assert_eq!((tall.kr, tall.kb), (0.2126, 0.0722));
        assert_eq!((short.kr, short.kb), (0.299, 0.114));
        assert!(!tall.full_range);
    }

    #[test]
    fn the_gpu_converts_nv12_like_the_cpu() {
        let mut ctx = block_on(RenderContext::new()).expect("RenderContext");
        let frame = bars(&[WHITE, BLACK, RED, GREEN], BT709);
        let Some(DecodedImage {
            pixels: Pixels::Srgb8(expected),
            ..
        }) = frame.rgba()
        else {
            panic!("no RGBA frame");
        };
        let texture = ctx.video_texture(&frame.nv12().expect("NV12 planes"));
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let size = (WIDTH as u32, HEIGHT as u32);
        let output = offscreen_texture(&ctx.device, size, format);
        ctx.set_uniforms(Framing::default(), size, texture.size);
        ctx.draw(
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
            format,
            &texture,
        );
        let drawn = read_back(&ctx.device, &ctx.queue, &output);
        let row = padded_bytes_per_row(size.0) as usize;
        let difference: u64 = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH * 4).filter(|x| x % 4 != 3).map(move |x| (y, x)))
            .map(|(y, x)| u64::from(drawn[y * row + x].abs_diff(expected[y * WIDTH * 4 + x])))
            .sum();
        let mean = difference as f64 / (WIDTH * HEIGHT * 3) as f64;
        assert!(mean < 3.0, "mean difference {mean} of 255");
    }
}
