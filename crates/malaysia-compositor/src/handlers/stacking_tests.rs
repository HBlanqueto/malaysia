use std::collections::HashSet;
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use smithay::backend::input::InputTime;
use smithay::reexports::wayland_server::Display;
use smithay::utils::Point;
use smithay::wayland::compositor::CompositorHandler;

use malaysia_scene::scene::WindowId;

use sctk::delegate_shm;
use sctk::reexports::client::globals::{GlobalListContents, registry_queue_init};
use sctk::reexports::client::protocol::wl_buffer::WlBuffer;
use sctk::reexports::client::protocol::wl_compositor::WlCompositor;
use sctk::reexports::client::protocol::wl_registry::WlRegistry;
use sctk::reexports::client::protocol::wl_shm;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use sctk::reexports::protocols::xdg::shell::client::xdg_toplevel::{self, XdgToplevel};
use sctk::reexports::protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};
use sctk::shm::{Shm, ShmHandler, slot::SlotPool};

use crate::state::State;

struct Client {
    shm: Shm,
    suspended: Option<mpsc::Sender<bool>>,
}

impl ShmHandler for Client {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_shm!(Client);

impl Dispatch<WlRegistry, GlobalListContents> for Client {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        base: &XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for Client {
    fn event(
        _: &mut Self,
        surface: &XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
        }
    }
}

macro_rules! ignore_events {
    ($($t:ty),*) => {
        $(
            impl Dispatch<$t, ()> for Client {
                fn event(
                    _: &mut Self,
                    _: &$t,
                    _: <$t as Proxy>::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }
        )*
    };
}

ignore_events![WlCompositor, WlSurface, WlBuffer];

impl Dispatch<XdgToplevel, ()> for Client {
    fn event(
        client: &mut Self,
        _: &XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let (xdg_toplevel::Event::Configure { states, .. }, Some(flags)) =
            (event, &client.suspended)
        {
            let suspended = states.as_chunks::<4>().0.iter().any(|state| {
                u32::from_ne_bytes(*state) == u32::from(xdg_toplevel::State::Suspended)
            });
            let _ = flags.send(suspended);
        }
    }
}

fn open_windows(
    display: &Display<State>,
    sizes: Vec<(i32, i32)>,
    margin: i32,
    suspended: Option<mpsc::Sender<bool>>,
) -> mpsc::Receiver<()> {
    let (client_side, server_side) = UnixStream::pair().expect("socket pair");
    crate::test_support::insert_client(display, server_side);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let conn = Connection::from_socket(client_side).expect("connection");
        let (globals, mut queue) = registry_queue_init::<Client>(&conn).expect("registry");
        let qh = queue.handle();
        let mut client = Client {
            shm: Shm::bind(&globals, &qh).expect("shm"),
            suspended,
        };
        let compositor = globals
            .bind::<WlCompositor, Client, ()>(&qh, 1..=6, ())
            .expect("compositor");
        let base = globals
            .bind::<XdgWmBase, Client, ()>(&qh, 1..=6, ())
            .expect("xdg_wm_base");
        let mut pool = SlotPool::new(800 * 600 * 4 * 2, &client.shm).expect("pool");
        let mut windows = Vec::new();
        for (index, (width, height)) in sizes.into_iter().enumerate() {
            let surface = compositor.create_surface(&qh, ());
            let xdg = base.get_xdg_surface(&surface, &qh, ());
            let toplevel = xdg.get_toplevel(&qh, ());
            toplevel.set_title(format!("window {index}"));
            if margin > 0 {
                xdg.set_window_geometry(margin, margin, width - 2 * margin, height - 2 * margin);
            }
            surface.commit();
            queue.roundtrip(&mut client).expect("configure");
            let (buffer, _) = pool
                .create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)
                .expect("buffer");
            surface.attach(Some(buffer.wl_buffer()), 0, 0);
            surface.damage_buffer(0, 0, width, height);
            surface.commit();
            queue.roundtrip(&mut client).expect("map");
            windows.push((surface, xdg, toplevel, buffer));
            tx.send(()).expect("mapped ack");
        }
        loop {
            if queue.roundtrip(&mut client).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    rx
}

fn dispatch_until_ack(display: &mut Display<State>, state: &mut State, rx: &mpsc::Receiver<()>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        display.dispatch_clients(state).expect("dispatch");
        display.flush_clients().expect("flush");
        if rx.try_recv().is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "client ack timed out");
    }
}

fn check_new_window_and_click(sizes_in_two_clients: bool, margin: i32) {
    let (_loop, mut display, mut state) = crate::test_support::test_state((1600, 1000));
    let (first, second) = if sizes_in_two_clients {
        let rx = open_windows(&display, vec![(800, 600)], margin, None);
        dispatch_until_ack(&mut display, &mut state, &rx);
        let first = state.windows.topmost().expect("the first window is mapped");
        let rx = open_windows(&display, vec![(300, 200)], margin, None);
        dispatch_until_ack(&mut display, &mut state, &rx);
        (
            first,
            state
                .windows
                .topmost()
                .expect("the second window is mapped"),
        )
    } else {
        let rx = open_windows(&display, vec![(800, 600), (300, 200)], margin, None);
        dispatch_until_ack(&mut display, &mut state, &rx);
        let first = state.windows.topmost().expect("the first window is mapped");
        dispatch_until_ack(&mut display, &mut state, &rx);
        (
            first,
            state
                .windows
                .topmost()
                .expect("the second window is mapped"),
        )
    };
    for _ in 0..20 {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_ne!(first, second, "the new window must be the topmost one");
    assert_eq!(state.windows.topmost(), Some(second), "it stays on top");
    assert_eq!(state.focused_window(), Some(second));

    let first_outer = state
        .windows
        .outer(first)
        .expect("the first window has a rect");
    let second_outer = state
        .windows
        .outer(second)
        .expect("the second window has a rect");
    let corner = Point::from((
        first_outer.x() + 60.0,
        first_outer.y() + first_outer.height() - 60.0,
    ));
    assert!(!second_outer.contains(malaysia_scene::geometry::Point::new(corner.x, corner.y)));
    state.pointer_press(crate::input::BUTTON_LEFT, corner, InputTime::from_millis(0));
    for _ in 0..20 {
        display.dispatch_clients(&mut state).expect("dispatch");
        display.flush_clients().expect("flush");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        state.windows.topmost(),
        Some(first),
        "a click on the window behind must raise it"
    );
    assert_eq!(state.focused_window(), Some(first));
}

#[test]
fn a_new_window_opens_in_front_and_a_click_raises_the_one_behind_for_one_or_two_clients_with_either_decoration()
 {
    check_new_window_and_click(false, 0);
    check_new_window_and_click(true, 0);
    check_new_window_and_click(false, 24);
    check_new_window_and_click(true, 24);
}

fn settle(display: &mut Display<State>, state: &mut State) {
    for _ in 0..20 {
        display.dispatch_clients(state).expect("dispatch");
        display.flush_clients().expect("flush");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn with_a_window(
    test: impl FnOnce(&mut Display<State>, &mut State, WindowId, &mpsc::Receiver<bool>),
) {
    let (_loop, mut display, mut state) = crate::test_support::test_state((1600, 1000));
    let (flags, suspended) = mpsc::channel();
    let rx = open_windows(&display, vec![(800, 600)], 0, Some(flags));
    dispatch_until_ack(&mut display, &mut state, &rx);
    let window = state.windows.topmost().expect("the window is mapped");
    settle(&mut display, &mut state);
    while suspended.try_recv().is_ok() {}
    test(&mut display, &mut state, window, &suspended);
}

#[test]
fn a_window_out_of_sight_is_suspended_after_three_seconds_and_woken_at_once() {
    with_a_window(|display, state, window, suspended| {
        let (minimized, start) = (HashSet::new(), Instant::now());
        state.windows.set_hidden(window, true);
        assert!(state.windows.is_out_of_sight(window, &minimized));
        for waited in [Duration::ZERO, Duration::from_millis(2900)] {
            state
                .windows
                .sync_suspended(start + waited, &minimized, false, |_| false);
        }
        settle(display, state);
        assert!(
            suspended.try_recv().is_err(),
            "it is told nothing before the third second"
        );
        let after_three = start + Duration::from_secs(3);
        state
            .windows
            .sync_suspended(after_three, &minimized, false, |_| false);
        settle(display, state);
        assert_eq!(suspended.try_recv(), Ok(true));
        state.windows.set_hidden(window, false);
        state
            .windows
            .sync_suspended(after_three, &minimized, false, |_| false);
        settle(display, state);
        assert_eq!(
            suspended.try_recv(),
            Ok(false),
            "it is woken the moment it returns"
        );
    });
}

#[test]
fn a_minimized_window_is_out_of_sight() {
    with_a_window(|_, state, window, _| {
        let mut minimized = HashSet::new();
        assert!(!state.windows.is_out_of_sight(window, &minimized));
        minimized.insert(window);
        assert!(state.windows.is_out_of_sight(window, &minimized));
    });
}

#[test]
fn a_window_a_capture_reads_is_never_suspended() {
    with_a_window(|display, state, window, suspended| {
        let (minimized, start) = (HashSet::new(), Instant::now());
        state.windows.set_hidden(window, true);
        for waited in [Duration::ZERO, Duration::from_secs(60)] {
            state
                .windows
                .sync_suspended(start + waited, &minimized, false, |id| id == window);
        }
        settle(display, state);
        assert!(suspended.try_recv().is_err());
    });
}

#[test]
fn every_window_is_suspended_while_nobody_looks() {
    with_a_window(|display, state, _, suspended| {
        let (minimized, start) = (HashSet::new(), Instant::now());
        for waited in [Duration::ZERO, Duration::from_secs(3)] {
            state
                .windows
                .sync_suspended(start + waited, &minimized, true, |_| false);
        }
        settle(display, state);
        assert_eq!(suspended.try_recv(), Ok(true));
    });
}

#[test]
fn a_commit_from_a_window_out_of_sight_asks_for_no_redraw() {
    with_a_window(|_, state, window, _| {
        let surface = state.windows.surface_of(window).cloned().unwrap();
        state.take_redraw_request();
        CompositorHandler::commit(state, &surface);
        assert!(state.take_redraw_request(), "a window in sight is redrawn");
        state.windows.set_hidden(window, true);
        state.take_redraw_request();
        CompositorHandler::commit(state, &surface);
        assert!(!state.take_redraw_request(), "a window out of sight is not");
    });
}

#[test]
fn an_inhibitor_on_a_window_out_of_sight_is_not_honored() {
    with_a_window(|_, state, window, _| {
        let surface = state.windows.surface_of(window).cloned().unwrap();
        let keeps_display_awake = |state: &mut State| {
            state.idle.note_activity();
            state.idle.idle_timer(crate::idle::DISPLAY_SLEEP_ID);
            !state.idle.display_asleep()
        };
        crate::idle::inhibit(state, &surface);
        assert!(keeps_display_awake(state), "a window in sight does");
        state.windows.set_hidden(window, true);
        state.sync_inhibition();
        assert!(
            !keeps_display_awake(state),
            "a window out of sight does not"
        );
        state.windows.set_hidden(window, false);
        state.sync_inhibition();
        assert!(keeps_display_awake(state), "it does again when it returns");
        state.locked = true;
        state.sync_inhibition();
        assert!(!keeps_display_awake(state), "no window does while locked");
    });
}
