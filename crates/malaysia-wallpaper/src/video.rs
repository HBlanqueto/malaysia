use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use calloop::ping::Ping;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::image::{DecodedImage, Pixels};

const PREROLL_TIMEOUT: Duration = Duration::from_secs(5);

type Latest = Arc<Mutex<Option<DecodedImage>>>;

pub struct Video {
    pipeline: gst::Pipeline,
    latest: Latest,
}

impl Video {
    pub fn open(path: &Path, wake: Option<Ping>) -> anyhow::Result<Self> {
        gst::init()?;
        let element = |factory: &str| {
            gst::ElementFactory::make(factory)
                .build()
                .map_err(|_| anyhow::anyhow!("GStreamer has no {factory}"))
        };
        let source = element("filesrc")?;
        source.set_property("location", path.to_string_lossy().as_ref());
        let decoder = element("decodebin3")?;
        let convert = element("videoconvert")?;
        let sink = gst_app::AppSink::builder()
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "RGBA")
                    .build(),
            )
            .max_buffers(2)
            .drop(true)
            .build();
        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&source, &decoder, &convert, sink.upcast_ref()])?;
        source.link(&decoder)?;
        convert.link(&sink)?;
        let video_input = convert
            .static_pad("sink")
            .ok_or_else(|| anyhow::anyhow!("videoconvert has no sink pad"))?;
        let weak = pipeline.downgrade();
        decoder.connect_pad_added(move |_, pad| {
            if pad.name().starts_with("video_") && !video_input.is_linked() {
                if pad.link(&video_input).is_err() {
                    tracing::warn!("the video wallpaper's stream could not be linked");
                }
            } else if let Some(pipeline) = weak.upgrade() {
                discard(&pipeline, pad);
            }
        });
        let latest = Latest::default();
        let (sample_slot, preroll_slot) = (latest.clone(), latest.clone());
        let preroll_wake = wake.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    store(
                        &sample_slot,
                        &sink.pull_sample().map_err(|_| gst::FlowError::Eos)?,
                    );
                    if let Some(wake) = wake.as_ref() {
                        wake.ping();
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .new_preroll(move |sink| {
                    store(
                        &preroll_slot,
                        &sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?,
                    );
                    if let Some(wake) = preroll_wake.as_ref() {
                        wake.ping();
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        let weak = pipeline.downgrade();
        pipeline
            .bus()
            .ok_or_else(|| anyhow::anyhow!("the video pipeline has no bus"))?
            .set_sync_handler(move |_, message| {
                match message.view() {
                    gst::MessageView::Eos(_) => {
                        if let Some(pipeline) = weak.upgrade() {
                            // EOS arrives on a streaming thread; seeking from it would deadlock.
                            pipeline.call_async(|pipeline| {
                                let _ = pipeline.seek_simple(
                                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                                    gst::ClockTime::ZERO,
                                );
                            });
                        }
                    }
                    gst::MessageView::Error(error) => {
                        tracing::warn!(error = %error.error(), "video wallpaper error");
                    }
                    _ => {}
                }
                gst::BusSyncReply::Drop
            });
        pipeline.set_state(gst::State::Paused)?;
        let (result, _, _) = pipeline.state(gst::ClockTime::from_nseconds(
            PREROLL_TIMEOUT.as_nanos() as u64,
        ));
        let video = Self { pipeline, latest };
        result?;
        if video
            .latest
            .lock()
            .ok()
            .and_then(|frame| frame.clone())
            .is_none()
        {
            anyhow::bail!("the video gave no frame");
        }
        Ok(video)
    }

    pub fn set_playing(&self, playing: bool) {
        let state = if playing {
            gst::State::Playing
        } else {
            gst::State::Paused
        };
        let _ = self.pipeline.set_state(state);
    }

    pub fn take(&self) -> Option<DecodedImage> {
        self.latest.lock().ok()?.take()
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

// An unlinked decodebin3 pad stops the pipeline, so audio and extra streams go to a fakesink.
fn discard(pipeline: &gst::Pipeline, pad: &gst::Pad) {
    let Ok(sink) = gst::ElementFactory::make("fakesink").build() else {
        return;
    };
    let linked = pipeline.add(&sink).is_ok()
        && sink.sync_state_with_parent().is_ok()
        && sink
            .static_pad("sink")
            .is_some_and(|sink| pad.link(&sink).is_ok());
    if !linked {
        tracing::warn!("a stream of the video wallpaper could not be discarded");
    }
}

fn store(slot: &Latest, sample: &gst::Sample) {
    if let (Some(frame), Ok(mut slot)) = (rgba(sample), slot.lock()) {
        *slot = Some(frame);
    }
}

fn rgba(sample: &gst::Sample) -> Option<DecodedImage> {
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let buffer = sample.buffer()?.map_readable().ok()?;
    let (width, height) = (info.width(), info.height());
    let stride = *info.stride().first()? as usize;
    let row = width as usize * 4;
    let pixels = buffer
        .chunks(stride)
        .take(height as usize)
        .flat_map(|line| &line[..row])
        .copied()
        .collect();
    Some(DecodedImage {
        size: (width, height),
        pixels: Pixels::Srgb8(pixels),
    })
}
