use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::wlr_layer::Anchor;

use crate::layers::Layers;

const SERVICE: &str = "org.malaysia.Notifications1";
const PATH: &str = "/org/malaysia/Notifications1";

struct NotificationsLayer;

pub(crate) fn mark(surface: &WlSurface) {
    with_states(surface, |states| {
        states
            .data_map
            .insert_if_missing_threadsafe(|| NotificationsLayer);
    });
}

fn marked(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states.data_map.get::<NotificationsLayer>().is_some()
    })
}

// ADR 0068: the open panel is the notifications surface anchored to the bottom edge too.
pub(crate) fn open_panel(layers: &Layers) -> Option<Rectangle<i32, Logical>> {
    layers
        .mapped()
        .iter()
        .find(|entry| entry.anchors.contains(Anchor::BOTTOM) && marked(entry.surface.wl_surface()))
        .map(|entry| entry.geometry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    Open,
    Close,
}

pub(crate) fn send(request: Request) {
    let method = match request {
        Request::Open => "OpenPanel",
        Request::Close => "ClosePanel",
    };
    std::thread::spawn(move || {
        let called = zbus::blocking::Connection::session().and_then(|connection| {
            connection.call_method(Some(SERVICE), PATH, Some(SERVICE), method, &())
        });
        if let Err(error) = called {
            tracing::warn!(%error, method, "could not reach the notification panel");
        }
    });
}

// A two-finger swipe is a finger scroll; libinput gives no touch positions, so the right
// edge of the screen stands in for the right edge of the trackpad.
#[derive(Debug, Default)]
pub(crate) struct EdgeSwipe {
    travel: f64,
    done: bool,
}

impl EdgeSwipe {
    pub(crate) fn feed(
        &mut self,
        horizontal: f64,
        ended: bool,
        at_edge: bool,
        panel_open: bool,
    ) -> Option<Request> {
        if ended {
            *self = Self::default();
            return None;
        }
        if self.done || !at_edge {
            return None;
        }
        self.travel += horizontal;
        let request = if panel_open && self.travel >= ctdesign::NOTIFICATION_SWIPE {
            Request::Close
        } else if !panel_open && self.travel <= -ctdesign::NOTIFICATION_SWIPE {
            Request::Open
        } else {
            return None;
        };
        self.done = true;
        Some(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leftward_swipe_at_the_edge_opens_once_and_a_rightward_one_closes() {
        let mut swipe = EdgeSwipe::default();
        assert_eq!(swipe.feed(-40.0, false, true, false), None);
        assert_eq!(swipe.feed(-40.0, false, true, false), Some(Request::Open));
        assert_eq!(swipe.feed(-40.0, false, true, false), None);
        swipe.feed(0.0, true, true, false);
        assert_eq!(swipe.feed(70.0, false, true, true), Some(Request::Close));
    }

    #[test]
    fn scrolling_away_from_the_edge_or_the_wrong_way_does_nothing() {
        let mut swipe = EdgeSwipe::default();
        assert_eq!(swipe.feed(-200.0, false, false, false), None);
        assert_eq!(swipe.feed(200.0, false, true, false), None);
        assert_eq!(swipe.feed(-200.0, false, true, true), None);
    }
}
