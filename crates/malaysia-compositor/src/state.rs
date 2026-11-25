use std::time::{Duration, Instant};

use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::{Color32F, gles::GlesRenderer};
use smithay::desktop::space::{OutputError, SpaceRenderElements};
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
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::drm::Drm;
use crate::winit::Winit;

pub const CLEAR_COLOR: Color32F = Color32F::new(0.1, 0.1, 0.12, 1.0);

render_elements! {
    pub Element<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Solid=SolidColorRenderElement,
}

pub fn elements(
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    output: &Output,
    frame: &mut Vec<Element>,
) -> Result<(), OutputError> {
    let space_elements = space.render_elements_for_output(renderer, output, 1.0)?;
    frame.extend(space_elements.into_iter().map(Element::from));
    Ok(())
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
        self.frame_pending = true;
        let pointer = self.seat.get_pointer().unwrap().current_location();
        let rendered = match &mut self.backend {
            Backend::Drm(drm) => drm.render(
                &self.space,
                &self.output,
                &self.cursor_status,
                pointer,
                &mut self.frame_elements,
            ),
            Backend::Winit(winit) => {
                winit.render(&self.space, &self.output, &mut self.frame_elements)
            }
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
        for window in self.space.elements() {
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
