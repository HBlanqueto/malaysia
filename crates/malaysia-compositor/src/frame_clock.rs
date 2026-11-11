use std::cell::Cell;
use std::time::{Duration, Instant};

use smithay::backend::renderer::element::{
    RenderElementStates, default_primary_scanout_output_compare,
};
use smithay::backend::renderer::utils::{Buffer, RendererSurfaceStateUserData};
use smithay::desktop::utils::{
    OutputPresentationFeedback, send_frames_surface_tree,
    surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
    take_presentation_feedback_surface_tree, update_surface_primary_scanout_output,
    with_surfaces_surface_tree,
};
use smithay::input::pointer::CursorImageStatus;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

use smithay::wayland::compositor::{
    SurfaceAttributes, SurfaceData, TraversalAction, with_surface_tree_downward,
};

use smithay::wayland::shell::wlr_layer::Layer;

use crate::session::{PendingFrameRelease, Session};
use crate::state::State;

pub(crate) fn queue_estimated_vblank(session: &mut Session) {
    if session.estimated_vblank.is_some() {
        return;
    }
    let delay = next_vblank_delay(
        session.last_frame_duration,
        Duration::from(session.state.clock.now()),
        refresh_interval(&session.state),
    );
    match session.loop_handle.insert_source(
        Timer::from_duration(delay),
        |_, _, session: &mut Session| {
            session.estimated_vblank = None;
            session.state.frame_callback_cycle = session.state.frame_callback_cycle.wrapping_add(1);
            if session.state.animating(Instant::now()) {
                session.backend.window().request_redraw();
            } else {
                let now = Duration::from(session.state.clock.now());
                send_frame_callbacks_for_state(&session.state, now);
            }
            TimeoutAction::Drop
        },
    ) {
        Ok(token) => session.estimated_vblank = Some(token),
        Err(error) => tracing::warn!(%error, "scheduling the estimated vblank"),
    }
}

pub(crate) fn collect_drawn_buffers(state: &State) -> Vec<Buffer> {
    let mut buffers = Vec::new();
    if state.locked {
        for lock_surface in &state.lock_surfaces {
            if lock_surface.alive() {
                collect_tree_buffers(&mut buffers, lock_surface.wl_surface());
            }
        }
        if state.lock_transition.shown() {
            for entry in state.layers.on_layer(Layer::Background) {
                collect_tree_buffers(&mut buffers, entry.surface.wl_surface());
            }
            return buffers;
        }
    }
    for window in state.windows.iter() {
        if window.is_mapped() {
            collect_tree_buffers(&mut buffers, window.surface());
        }
    }
    for (id, _parent, _rect) in state.windows.iter_popups() {
        if let Some(surface) = state.popups.surface_of(id) {
            collect_tree_buffers(&mut buffers, surface);
        }
    }
    for entry in state.layers.all() {
        collect_tree_buffers(&mut buffers, entry.surface.wl_surface());
    }
    buffers
}

pub(crate) fn collect_tree_buffers(buffers: &mut Vec<Buffer>, surface: &WlSurface) {
    with_surface_tree_downward(
        surface,
        (),
        |_, states, _| {
            if let Some(state) = states.data_map.get::<RendererSurfaceStateUserData>()
                && let Some(buffer) = state.lock().unwrap().buffer().cloned()
            {
                buffers.push(buffer);
            }
            TraversalAction::DoChildren(())
        },
        |_, _, _| {},
        |_, _, _| true,
    );
}

pub(crate) fn schedule_frame_release(
    loop_handle: &LoopHandle<'static, Session>,
    release: PendingFrameRelease,
) {
    let Some(fd) = release.sync.export() else {
        if !release.sync.is_reached() {
            tracing::error!("frame fence is neither exportable nor signalled");
        }
        return;
    };
    let Ok(fd) = fd.try_clone() else {
        tracing::error!("could not duplicate the frame fence fd");
        return;
    };
    let source = Generic::new(fd, Interest::READ, Mode::Level);
    let mut release = Some(release);
    if let Err(error) = loop_handle.insert_source(source, move |_, _, _: &mut Session| {
        if let Some(release) = release.take() {
            tracing::trace!(count = release.buffers.len(), "released a frame's buffers");
            drop(release);
        }
        Ok(PostAction::Remove)
    }) {
        tracing::error!(%error, "could not arm the frame fence source");
    }
}

#[derive(Default)]
pub(crate) struct LastFrameCallbackCycle(Cell<Option<u32>>);

// niri a8562a1 src/niri.rs:205: hidden surfaces still get a callback about once a second.
pub(crate) const FRAME_CALLBACK_THROTTLE: Duration = Duration::from_millis(995);

pub(crate) fn for_each_frame_surface(state: &State, mut visit: impl FnMut(&WlSurface, bool)) {
    if let CursorImageStatus::Surface(surface) = &state.cursor_status {
        visit(surface, true);
    }
    if state.locked {
        for surface in state.lock_surfaces.iter().filter(|surface| surface.alive()) {
            visit(surface.wl_surface(), true);
        }
        // ADR 0074: the wallpaper stays under the lock screen; the rest only while it fades.
        if state.lock_transition.shown() {
            for entry in state.layers.on_layer(Layer::Background) {
                visit(entry.surface.wl_surface(), false);
            }
            return;
        }
    }
    for surface in state.windows.surfaces() {
        visit(surface, !state.windows.is_placed(surface));
    }
    for entry in state.layers.all() {
        visit(entry.surface.wl_surface(), false);
    }
    for (id, _parent, _rect) in state.windows.iter_popups() {
        if let Some(surface) = state.popups.surface_of(id) {
            visit(surface, false);
        }
    }
    for popup in &state.ime_popups {
        visit(popup.wl_surface(), false);
    }
}

pub(crate) fn update_primary_scanout_outputs(state: &State, render_states: &RenderElementStates) {
    for_each_frame_surface(state, |surface, _| {
        with_surfaces_surface_tree(surface, |surface, states| {
            update_surface_primary_scanout_output(
                surface,
                &state.output,
                states,
                None,
                render_states,
                default_primary_scanout_output_compare,
            );
        });
    });
}

// niri a8562a1 src/niri.rs:5342: one callback per surface and cycle, or a frame-only client busy-loops.
pub(crate) fn send_frame_callbacks_for_state(state: &State, time: Duration) {
    let cycle = state.frame_callback_cycle;
    let output = &state.output;
    for_each_frame_surface(state, |surface, always| {
        if always {
            send_frame_callbacks(surface, time, cycle);
            return;
        }
        send_frames_surface_tree(
            surface,
            output,
            time,
            Some(FRAME_CALLBACK_THROTTLE),
            |surface, states| {
                let shown =
                    surface_primary_scanout_output(surface, states).as_ref() == Some(output);
                (shown && first_in_cycle(states, cycle)).then(|| output.clone())
            },
        );
    });
}

pub(crate) fn first_in_cycle(states: &SurfaceData, cycle: u32) -> bool {
    let last = states
        .data_map
        .get_or_insert(LastFrameCallbackCycle::default);
    last.0.replace(Some(cycle)) != Some(cycle)
}

// send_frames_surface_tree skips bufferless surfaces; a toplevel awaiting its first configure needs one.
pub(crate) fn send_frame_callbacks(surface: &WlSurface, time: Duration, cycle: u32) {
    let time_ms = time.as_millis() as u32;
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            if !first_in_cycle(states, cycle) {
                return;
            }
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            for callback in cached.current().frame_callbacks.drain(..) {
                callback.done(time_ms);
            }
        },
        |_, _, &()| true,
    );
}

pub(crate) fn send_overdue_frame_callbacks(state: &State) {
    let time = Duration::from(state.clock.now());
    for_each_frame_surface(state, |surface, _| {
        send_frames_surface_tree(
            surface,
            &state.output,
            time,
            Some(FRAME_CALLBACK_THROTTLE),
            |_, _| None,
        );
    });
}

pub(crate) fn watch_overdue_frame_callbacks<D: crate::idle::IdleHost + 'static>(
    handle: &LoopHandle<'static, D>,
) {
    let timer = Timer::from_duration(FRAME_CALLBACK_THROTTLE);
    if let Err(error) = handle.insert_source(timer, |_, _, data: &mut D| {
        send_overdue_frame_callbacks(data.idle_state());
        TimeoutAction::ToDuration(FRAME_CALLBACK_THROTTLE)
    }) {
        tracing::warn!(%error, "arming the frame callback fallback timer");
    }
}

pub(crate) fn take_presentation_feedback(
    state: &State,
    render_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut feedback = OutputPresentationFeedback::new(&state.output);
    for_each_frame_surface(state, |surface, _| {
        take_presentation_feedback_surface_tree(
            surface,
            &mut feedback,
            surface_primary_scanout_output,
            |surface, _| {
                surface_presentation_feedback_flags_from_states(surface, None, render_states)
            },
        );
    });
    feedback
}

pub(crate) fn refresh_interval(state: &State) -> Duration {
    let millihertz = state
        .output
        .current_mode()
        .map_or(60_000, |mode| mode.refresh)
        .max(1);
    Duration::from_micros(1_000_000_000 / millihertz as u64)
}

pub(crate) fn next_vblank_delay(
    last_vblank: Option<Duration>,
    now: Duration,
    refresh: Duration,
) -> Duration {
    let Some(last) = last_vblank.filter(|last| *last <= now) else {
        return refresh;
    };
    let since = (now - last).as_nanos() % refresh.as_nanos();
    refresh - Duration::from_nanos(since as u64)
}
