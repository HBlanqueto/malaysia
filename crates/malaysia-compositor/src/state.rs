use std::time::{Duration, Instant};

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::{Color32F, gles::GlesRenderer};
use smithay::desktop::space::SpaceRenderElements;
use smithay::desktop::{Space, Window};
use smithay::input::keyboard::Keycode;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::drm::Drm;
use crate::winit::Winit;

pub const CLEAR_COLOR: Color32F = Color32F::new(0.1, 0.1, 0.12, 1.0);

pub type Element = SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>;

pub fn elements(
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    output: &Output,
) -> Vec<Element> {
    space
        .render_elements_for_output(renderer, output, 1.0)
        .unwrap_or_default()
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
    pub space: Space<Window>,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<State>,
    pub needs_redraw: bool,
    pub frame_pending: bool,
    pub suppressed_keys: Vec<Keycode>,
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
        self.frame_pending = true;
        let rendered = match &mut self.backend {
            Backend::Drm(drm) => drm.render(&self.space, &self.output),
            Backend::Winit(winit) => winit.render(&self.space, &self.output),
        }
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "render failed");
            false
        });
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
        for window in self.space.elements() {
            window.send_frame(&self.output, time, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
    }
}
