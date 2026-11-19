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

pub(crate) fn marked(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states.data_map.get::<NotificationsLayer>().is_some()
    })
}

pub(crate) fn is_open_panel(anchors: Anchor, surface: &WlSurface) -> bool {
    anchors.contains(Anchor::BOTTOM) && marked(surface)
}

pub(crate) fn open_panel(layers: &Layers) -> Option<Rectangle<i32, Logical>> {
    layers
        .mapped()
        .iter()
        .find(|entry| is_open_panel(entry.anchors, entry.surface.wl_surface()))
        .map(|entry| entry.geometry)
}

pub(crate) fn veil(
    cards: &[Rectangle<i32, Logical>],
    panel_width: i32,
    bar_height: i32,
) -> Option<Rectangle<i32, Logical>> {
    let content = cards.iter().copied().reduce(Rectangle::merge)?;
    let sigma = (ctdesign::NOTIFICATION_SHADOW_BLUR / 2.0).round() as i32;
    let center = content.loc.x + content.size.w / 2;
    let right = center + ctdesign::NOTIFICATION_WIDTH / 2 + ctdesign::NOTIFICATION_MARGIN;
    let width = panel_width - 3 * sigma;
    let top = bar_height + ctdesign::NOTIFICATION_GAP.round() as i32;
    let bottom = content.loc.y + content.size.h + sigma;
    Some(Rectangle::new(
        (right - width, top).into(),
        (width, bottom - top).into(),
    ))
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
    call(method, ());
}

pub(crate) fn screenshot_saved(path: std::path::PathBuf) {
    call("ScreenshotSaved", (path.to_string_lossy().into_owned(),));
}

fn call<B>(method: &'static str, body: B)
where
    B: serde::Serialize + zbus::zvariant::DynamicType + Send + 'static,
{
    std::thread::spawn(move || {
        let called = zbus::blocking::Connection::session().and_then(|connection| {
            connection.call_method(Some(SERVICE), PATH, Some(SERVICE), method, &body)
        });
        if let Err(error) = called {
            tracing::warn!(%error, method, "could not reach the notification panel");
        }
    });
}

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
    fn the_veil_ends_at_the_panel_edge_and_follows_the_content_down_and_across() {
        let panel = ctdesign::NOTIFICATION_WIDTH + 2 * ctdesign::NOTIFICATION_MARGIN + 10;
        let column = |x: i32, y: i32, h: i32| {
            Rectangle::new((x, y).into(), (ctdesign::NOTIFICATION_WIDTH, h).into())
        };
        let left = panel - ctdesign::NOTIFICATION_MARGIN - ctdesign::NOTIFICATION_WIDTH;
        let shown = veil(&[column(left, 40, 60), column(left, 108, 60)], panel, 0).unwrap();
        assert_eq!(shown.loc.x + shown.size.w, panel);
        assert_eq!(shown.loc.y + shown.size.h, 168 + 40);
        let slid = veil(&[column(left + 100, 40, 60)], panel, 0).unwrap();
        assert_eq!(slid.loc.x - shown.loc.x, 100);
        assert_eq!(veil(&[], panel, 0), None);
    }

    #[test]
    fn the_veil_top_follows_the_bar_height_and_its_bottom_the_content() {
        let card = Rectangle::new((0, 80).into(), (ctdesign::NOTIFICATION_WIDTH, 60).into());
        let flush = veil(&[card], 400, 0).unwrap();
        let below_bar = veil(&[card], 400, 30).unwrap();
        assert_eq!(below_bar.loc.y - flush.loc.y, 30);
        assert_eq!(
            below_bar.loc.y + below_bar.size.h,
            flush.loc.y + flush.size.h
        );
    }

    #[test]
    fn scrolling_away_from_the_edge_or_the_wrong_way_does_nothing() {
        let mut swipe = EdgeSwipe::default();
        assert_eq!(swipe.feed(-200.0, false, false, false), None);
        assert_eq!(swipe.feed(200.0, false, true, false), None);
        assert_eq!(swipe.feed(-200.0, false, true, true), None);
    }
}
