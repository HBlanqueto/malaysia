use std::cell::Cell;

use smithay::desktop::Window;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
    GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
    GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
    RelativeMotionEvent,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::{self, ResizeEdge};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Serial, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{SurfaceCachedState, ToplevelSurface, XdgToplevelSurfaceData};

use crate::state::State;

#[derive(Clone, Copy)]
struct Resize {
    direction: Point<i32, Logical>,
    size: Size<i32, Logical>,
    last: Option<Serial>,
}

pub struct WindowGrab {
    pub start_data: GrabStartData<State>,
    pub window: Window,
    pub drag: Drag,
}

pub enum Drag {
    Move {
        initial_location: Point<i32, Logical>,
    },
    Resize {
        direction: Point<i32, Logical>,
        initial_size: Size<i32, Logical>,
        size: Size<i32, Logical>,
    },
}

impl Drag {
    pub fn resize(window: &Window, edges: ResizeEdge) -> Option<Self> {
        let direction = Point::from(match edges {
            ResizeEdge::Top => (0, -1),
            ResizeEdge::Bottom => (0, 1),
            ResizeEdge::Left => (-1, 0),
            ResizeEdge::Right => (1, 0),
            ResizeEdge::TopLeft => (-1, -1),
            ResizeEdge::TopRight => (1, -1),
            ResizeEdge::BottomLeft => (-1, 1),
            ResizeEdge::BottomRight => (1, 1),
            _ => return None,
        });
        let initial_size = window.geometry().size;
        window
            .user_data()
            .get_or_insert(|| Cell::new(None::<Resize>))
            .set(Some(Resize {
                direction,
                size: initial_size,
                last: None,
            }));
        Some(Self::Resize {
            direction,
            initial_size,
            size: initial_size,
        })
    }
}

impl State {
    pub fn anchor_resized(&mut self, window: &Window) {
        let Some(cell) = window.user_data().get::<Cell<Option<Resize>>>() else {
            return;
        };
        let Some(mut resize) = cell.get() else {
            return;
        };
        let size = window.geometry().size;
        if size != resize.size {
            let shift = Point::from((
                resize.direction.x.min(0) * (size.w - resize.size.w),
                resize.direction.y.min(0) * (size.h - resize.size.h),
            ));
            for desktop in &mut self.desktops {
                if let Some(location) = desktop.element_location(window) {
                    desktop.relocate_element(window, location + shift);
                }
            }
            resize.size = size;
        }
        let toplevel = window.toplevel().unwrap();
        let Some(last) = resize.last else {
            cell.set(Some(resize));
            send_if_all_acked(toplevel);
            return;
        };
        let finished = toplevel.with_cached_state(|state| {
            state
                .last_acked
                .as_ref()
                .is_some_and(|acked| acked.serial >= last)
        });
        cell.set((!finished).then_some(resize));
    }
}

fn send_if_all_acked(toplevel: &ToplevelSurface) {
    let all_acked = with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .unwrap()
            .lock()
            .unwrap()
            .pending_configures()
            .is_empty()
    });
    if all_acked {
        toplevel.send_pending_configure();
    }
}

fn resized_length(initial: i32, direction: i32, delta: i32, min: i32, max: i32) -> i32 {
    if direction == 0 {
        return initial;
    }
    let max = if max == 0 { i32::MAX } else { max };
    (initial + direction * delta).max(min.max(1)).min(max)
}

impl PointerGrab<State> for WindowGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        match &mut self.drag {
            Drag::Move { initial_location } => {
                let location =
                    initial_location.to_f64() + (event.location - self.start_data.location);
                data.desktops[data.current_desktop]
                    .relocate_element(&self.window, location.to_i32_round());
                data.needs_redraw = true;
            }
            Drag::Resize {
                direction,
                initial_size,
                size: requested,
            } => {
                let toplevel = self.window.toplevel().unwrap();
                let (min, max) = with_states(toplevel.wl_surface(), |states| {
                    let mut guard = states.cached_state.get::<SurfaceCachedState>();
                    let cached = guard.current();
                    (cached.min_size, cached.max_size)
                });
                let delta: Point<i32, Logical> =
                    (event.location - self.start_data.location).to_i32_round();
                let size = Size::from((
                    resized_length(initial_size.w, direction.x, delta.x, min.w, max.w),
                    resized_length(initial_size.h, direction.y, delta.y, min.h, max.h),
                ));
                if size == *requested {
                    return;
                }
                *requested = size;
                toplevel.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Resizing);
                    state.size = Some(size);
                });
                send_if_all_acked(toplevel);
            }
        }
    }

    fn relative_motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut State) {
        if let Drag::Resize { .. } = self.drag {
            let toplevel = self.window.toplevel().unwrap();
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
            });
            let last = toplevel.send_pending_configure();
            let Some(cell) = self.window.user_data().get::<Cell<Option<Resize>>>() else {
                return;
            };
            cell.set(last.and_then(|last| {
                cell.get().map(|resize| Resize {
                    last: Some(last),
                    ..resize
                })
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_the_right_edge_right_grows_the_width() {
        assert_eq!(resized_length(400, 1, 50, 0, 0), 450);
    }

    #[test]
    fn dragging_the_left_edge_right_stops_at_the_minimum_or_at_one_without_minimum() {
        assert_eq!(resized_length(400, -1, 500, 120, 0), 120);
        assert_eq!(resized_length(400, -1, 500, 0, 0), 1);
    }

    #[test]
    fn a_maximum_of_zero_means_no_limit_and_a_set_maximum_stops_the_growth() {
        assert_eq!(resized_length(300, 1, 5000, 0, 0), 5300);
        assert_eq!(resized_length(300, 1, 5000, 0, 600), 600);
    }

    #[test]
    fn an_axis_the_edge_does_not_drag_keeps_its_length() {
        assert_eq!(resized_length(300, 0, 80, 0, 0), 300);
    }

    #[test]
    fn a_maximum_below_the_minimum_wins() {
        assert_eq!(resized_length(400, 1, 10, 500, 200), 200);
    }
}
