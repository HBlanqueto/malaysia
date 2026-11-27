use std::iter;
use std::time::{Duration, Instant};

use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::CropRenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::keyboard::Keycode;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::render_elements;
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::drm::Drm;
use crate::effects::{Badge, Clipped, Effects, Lines, Placement, Shadow, round_edges};
use crate::winit::Winit;
use crate::workspaces::desktops::{Slide, desktop_x, gap, gap_x, stride};
use crate::workspaces::expansion::Expansion;
use crate::workspaces::expansion::strip::{
    add_centre, close_centre, pill, thumbnail, thumbnail_factor,
};

render_elements! {
    pub Element<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=Clipped,
    Thumbnail=CropRenderElement<Clipped>,
    Lines=Lines,
    Shadow=Shadow,
    Solid=SolidColorRenderElement,
    Badge=Badge,
    Text=CropRenderElement<TextureRenderElement<GlesTexture>>,
}

pub struct Row<'a> {
    pub desktops: &'a [Space<Window>],
    pub current: usize,
    pub slide: Option<&'a Slide>,
    pub expansion: &'a Expansion,
    pub pointer: Point<f64, Logical>,
}

pub fn elements(
    renderer: &mut GlesRenderer,
    effects: &Effects,
    row: Row,
    output: &Output,
    frame: &mut Vec<Element>,
) {
    let Row {
        desktops,
        current,
        slide,
        expansion,
        pointer,
    } = row;
    let position = slide.map_or(current as f64, |slide| slide.position);
    let carried = slide.and_then(|slide| slide.carried.as_ref());
    let scale = output.current_scale().fractional_scale();
    let width = output.current_mode().map_or(0, |mode| mode.size.w);
    let height = output.current_mode().map_or(0, |mode| mode.size.h);
    let stride = stride(width, scale);
    let dragged = expansion.dragged.as_ref().map(|(window, _)| window);
    if let Some((window, grip)) = &expansion.dragged
        && let Some(geometry) = desktops[expansion.desktop].element_geometry(window)
    {
        let factor = expansion.drawn(window, geometry).1;
        let main = desktops[current].elements().next_back() == Some(window);
        effects.push_window(
            renderer,
            window,
            Placement {
                geometry: Rectangle::new((pointer - *grip).to_i32_round(), geometry.size),
                desktop_offset: (desktop_x(expansion.desktop, position, stride), 0).into(),
                expansion: factor,
                ringed: false,
                titled: false,
            },
            scale,
            main,
            frame,
        );
    }
    if let (Some(top), Some(geometry)) = (
        expansion.strip_top(),
        desktops[current].output_geometry(output),
    ) {
        push_strip(
            renderer,
            effects,
            &row,
            top,
            geometry.size.to_f64(),
            scale,
            frame,
        );
    }
    let hovered = (expansion.open && slide.is_none() && dragged.is_none())
        .then(|| expansion.window_at(desktops, pointer))
        .flatten()
        .map(|(window, _)| window);
    let others = desktops
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != current);
    for (index, desktop) in iter::once((current, &desktops[current])).chain(others) {
        if desktop.outputs().next().is_none() {
            continue;
        }
        let strip_x = desktop_x(index, position, stride);
        for (order, window) in desktop.elements().rev().enumerate() {
            if Some(window) == dragged {
                continue;
            }
            let x = if Some(window) == carried { 0 } else { strip_x };
            if x.abs() >= width {
                continue;
            }
            let Some(geometry) = desktop.element_geometry(window) else {
                continue;
            };
            let main = index == current && order == 0;
            let (loc, factor) = expansion.drawn(window, geometry);
            effects.push_window(
                renderer,
                window,
                Placement {
                    geometry: Rectangle::new(loc.to_i32_round(), geometry.size),
                    desktop_offset: (x, 0).into(),
                    expansion: factor,
                    ringed: Some(window) == hovered,
                    titled: Some(window) == hovered && expansion.at_rest(),
                },
                scale,
                main,
                frame,
            );
        }
    }
    if let Some(x) = gap_x(position, width, scale) {
        effects.push_gap(
            Rectangle::new((x, 0).into(), (gap(scale), height).into()),
            frame,
        );
    }
}

fn push_strip(
    renderer: &mut GlesRenderer,
    effects: &Effects,
    row: &Row,
    top: f64,
    output: Size<f64, Logical>,
    scale: f64,
    frame: &mut Vec<Element>,
) {
    let strip = &row.expansion.strip;
    let count = row.desktops.len();
    let shift = Point::from((0.0, top));
    let on_screen = |rectangle: Rectangle<f64, Logical>| {
        round_edges(Rectangle::new(rectangle.loc + shift, rectangle.size).to_physical(scale))
    };
    let bar = on_screen(Rectangle::from_size(Size::from((output.w, strip.height()))));
    let ring = (strip.grown && strip.motion.is_none())
        .then(|| on_screen(thumbnail(row.current, count, output)));
    let close = strip
        .hovered
        .map(|index| (close_centre(thumbnail(index, count, output)) + shift).to_physical(scale));
    effects.push_strip_controls(ring, close, scale, frame);
    let cells = (0..count).map(|index| {
        on_screen(if strip.grown {
            thumbnail(index, count, output)
        } else {
            pill(index, count, output)
        })
    });
    effects.push_desktop_names(renderer, cells, strip.grown, bar, scale, frame);
    if strip.grown {
        let factor = thumbnail_factor(output);
        for (index, desktop) in row.desktops.iter().enumerate() {
            let whole = on_screen(thumbnail(index, count, output));
            let Some(clip) = whole.intersection(bar) else {
                continue;
            };
            for window in desktop.elements().rev() {
                if let Some(geometry) = desktop.element_geometry(window) {
                    let placement = Placement {
                        geometry,
                        desktop_offset: whole.loc,
                        expansion: factor,
                        ringed: false,
                        titled: false,
                    };
                    effects.push_thumbnail(renderer, window, placement, clip, scale, frame);
                }
            }
            effects.push_thumbnail_backdrop(index, clip, frame);
        }
    }
    let add = (add_centre(strip.growth, output) + shift).to_physical(scale);
    let pill = (!strip.grown).then(|| on_screen(pill(row.current, count, output)));
    effects.push_strip_bar(bar, add, pill, scale, frame);
}

pub enum Backend {
    Drm(Box<Drm>),
    Winit(Box<Winit>),
}

pub struct State {
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, State>,
    pub loop_signal: LoopSignal,
    pub start_time: Instant,
    pub backend: Backend,
    pub output: Output,
    pub desktops: Vec<Space<Window>>,
    pub current_desktop: usize,
    pub slide: Option<Slide>,
    pub expansion: Expansion,
    pub popups: PopupManager,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<State>,
    pub needs_redraw: bool,
    pub frame_pending: bool,
    pub suppressed_keys: Vec<Keycode>,
    pub cursor_status: CursorImageStatus,
    pub frame_elements: Vec<Element>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {}

impl State {
    pub fn redraw_if_needed(&mut self) {
        if !self.needs_redraw || self.frame_pending {
            return;
        }
        match &self.backend {
            Backend::Drm(drm) if !drm.is_active() => {}
            Backend::Drm(_) => self.render(),
            Backend::Winit(winit) => {
                self.frame_pending = true;
                winit.request_redraw();
            }
        }
    }

    pub fn render(&mut self) {
        self.needs_redraw = false;
        let now = self.start_time.elapsed();
        self.advance_slide(now);
        self.advance_expansion(now);
        self.frame_pending = true;
        let pointer = self.seat.get_pointer().unwrap().current_location();
        let row = Row {
            desktops: &self.desktops,
            current: self.current_desktop,
            slide: self.slide.as_ref(),
            expansion: &self.expansion,
            pointer,
        };
        let rendered = match &mut self.backend {
            Backend::Drm(drm) => {
                drm.push_cursor(
                    &self.output,
                    &self.cursor_status,
                    pointer,
                    &mut self.frame_elements,
                );
                drm.render(row, &self.output, &mut self.frame_elements)
            }
            Backend::Winit(winit) => winit.render(row, &self.output, &mut self.frame_elements),
        }
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "render failed");
            self.needs_redraw = true;
            false
        });
        self.frame_elements.clear();
        if !rendered {
            self.finish_frame_after_one_refresh();
        } else if let Backend::Winit(_) = self.backend {
            self.frame_done();
        }
    }

    fn finish_frame_after_one_refresh(&self) {
        let frame_interval = self
            .output
            .current_mode()
            .map(|mode| Duration::from_secs_f64(1000.0 / mode.refresh as f64))
            .unwrap_or_default();
        if let Err(error) =
            self.loop_handle
                .insert_source(Timer::from_duration(frame_interval), |_, _, state| {
                    state.frame_done();
                    TimeoutAction::Drop
                })
        {
            tracing::error!(%error, "frame timer failed");
        }
    }

    pub fn frame_done(&mut self) {
        self.frame_pending = false;
        let time = self.start_time.elapsed();
        for window in self
            .desktops
            .iter()
            .filter(|desktop| desktop.outputs().next().is_some())
            .flat_map(Space::elements)
        {
            window.send_frame(&self.output, time, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
        if let (Backend::Drm(_), CursorImageStatus::Surface(surface)) =
            (&self.backend, &self.cursor_status)
        {
            send_frames_surface_tree(surface, &self.output, time, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
    }
}
