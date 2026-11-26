use std::iter;
use std::time::{Duration, Instant};

use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
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
use smithay::utils::Rectangle;
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::drm::Drm;
use crate::effects::{Clipped, Effects, Lines, Placement, Shadow};
use crate::winit::Winit;
use crate::workspaces::desktops::{Slide, desktop_x, gap, gap_x, stride};
use crate::workspaces::expansion::Expansion;

render_elements! {
    pub Element<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=Clipped,
    Lines=Lines,
    Shadow=Shadow,
    Solid=SolidColorRenderElement,
}

pub struct Row<'a> {
    pub desktops: &'a [Space<Window>],
    pub current: usize,
    pub slide: Option<&'a Slide>,
    pub expansion: &'a Expansion,
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
    } = row;
    let position = slide.map_or(current as f64, |slide| slide.position);
    let carried = slide.and_then(|slide| slide.carried.as_ref());
    let scale = output.current_scale().fractional_scale();
    let width = output.current_mode().map_or(0, |mode| mode.size.w);
    let height = output.current_mode().map_or(0, |mode| mode.size.h);
    let stride = stride(width, scale);
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
            let x = if Some(window) == carried { 0 } else { strip_x };
            if x.abs() >= width {
                continue;
            }
            let Some(geometry) = desktop.element_geometry(window) else {
                continue;
            };
            let main = index == current && order == 0;
            let (loc, factor) = if index == current {
                expansion.drawn(window, geometry)
            } else {
                (geometry.loc.to_f64(), 1.0)
            };
            effects.push_window(
                renderer,
                window,
                Placement {
                    geometry: Rectangle::new(loc.to_i32_round(), geometry.size),
                    desktop_offset: (x, 0).into(),
                    expansion: factor,
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
