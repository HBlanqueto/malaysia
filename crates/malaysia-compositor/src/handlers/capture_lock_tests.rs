use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use smithay::reexports::wayland_server::Display;
use smithay::utils::Point;

use sctk::delegate_shm;
use sctk::reexports::client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use sctk::reexports::client::protocol::wl_output::WlOutput;
use sctk::reexports::client::protocol::wl_registry::WlRegistry;
use sctk::reexports::client::protocol::wl_shm;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::ext::{
    image_capture_source::v1::client::{
        ext_image_capture_source_v1::ExtImageCaptureSourceV1,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
        ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
    },
    session_lock::v1::client::{
        ext_session_lock_manager_v1::ExtSessionLockManagerV1, ext_session_lock_v1::ExtSessionLockV1,
    },
};
use sctk::shm::{Shm, ShmHandler, slot::SlotPool};

use crate::state::State;

struct TestClient {
    shm: Shm,
}

impl ShmHandler for TestClient {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for TestClient {
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

macro_rules! ignore_events {
    ($($t:ty),*) => {
        $(
            impl Dispatch<$t, ()> for TestClient {
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

ignore_events![
    WlOutput,
    ExtImageCaptureSourceV1,
    ExtOutputImageCaptureSourceManagerV1,
    ExtImageCopyCaptureFrameV1,
    ExtImageCopyCaptureManagerV1,
    ExtImageCopyCaptureSessionV1,
    ExtSessionLockManagerV1,
    ExtSessionLockV1
];

delegate_shm!(TestClient);

fn dispatch_client_ack(display: &mut Display<State>, state: &mut State, rx: &mpsc::Receiver<()>) {
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

fn run_client<F>(display: &Display<State>, actions: F) -> mpsc::Receiver<()>
where
    F: FnOnce(Connection, GlobalList, QueueHandle<TestClient>, TestClient, mpsc::Sender<()>)
        + Send
        + 'static,
{
    let (client_side, server_side) = UnixStream::pair().expect("socket pair");
    crate::test_support::insert_client(display, server_side);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let conn = Connection::from_socket(client_side).expect("connection");
        let (globals, queue) = registry_queue_init::<TestClient>(&conn).expect("registry");
        let qh = queue.handle();
        let shm = Shm::bind(&globals, &qh).expect("shm");
        let client = TestClient { shm };
        actions(conn, globals, qh, client, tx);
    });
    rx
}

fn request_output_capture(
    conn: &Connection,
    globals: &GlobalList,
    qh: &QueueHandle<TestClient>,
    client: &TestClient,
) -> (ExtImageCopyCaptureSessionV1, SlotPool) {
    let output = globals
        .bind::<WlOutput, TestClient, ()>(qh, 1..=4, ())
        .expect("output");
    let sources = globals
        .bind::<ExtOutputImageCaptureSourceManagerV1, TestClient, ()>(qh, 1..=1, ())
        .expect("sources");
    let capture = globals
        .bind::<ExtImageCopyCaptureManagerV1, TestClient, ()>(qh, 1..=1, ())
        .expect("capture");
    let source = sources.create_source(&output, qh, ());
    let session = capture.create_session(&source, Options::empty(), qh, ());

    let width = 1280;
    let height = 720;
    let stride = width * 4;
    let mut pool = SlotPool::new((stride * height) as usize, &client.shm).expect("pool");
    let (buffer, _) = pool
        .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        .expect("buffer");
    let frame = session.create_frame(qh, ());
    frame.attach_buffer(buffer.wl_buffer());
    frame.capture();
    drop(frame);
    conn.flush().expect("flush");
    (session, pool)
}

#[test]
fn locking_fails_queued_capture_frames_and_rejects_new_ones() {
    let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
    let rx = run_client(&display, |conn, globals, qh, client, tx| {
        let (_session, _pool) = request_output_capture(&conn, &globals, &qh, &client);
        conn.roundtrip().expect("first frame roundtrip");
        tx.send(()).expect("stage 0 ack");

        let lock_manager = globals
            .bind::<ExtSessionLockManagerV1, TestClient, ()>(&qh, 1..=1, ())
            .expect("lock manager");
        let _lock = lock_manager.lock(&qh, ());
        conn.roundtrip().expect("lock roundtrip");
        tx.send(()).expect("stage 1 ack");

        let (_session2, _pool2) = request_output_capture(&conn, &globals, &qh, &client);
        conn.roundtrip().expect("second frame roundtrip");
        tx.send(()).expect("stage 2 ack");
    });

    dispatch_client_ack(&mut display, &mut state, &rx);
    assert_eq!(
        state.pending_captures.len(),
        1,
        "a frame requested before lock should be queued"
    );

    dispatch_client_ack(&mut display, &mut state, &rx);
    assert!(state.locked, "the session should be locked");
    assert!(
        state.pending_captures.is_empty(),
        "the queued frame must fail when the session locks"
    );

    dispatch_client_ack(&mut display, &mut state, &rx);
    assert!(
        state.pending_captures.is_empty(),
        "a frame requested while locked must not be queued"
    );
}

#[test]
fn locking_cancels_a_pending_screenshot_selection() {
    let (_loop, mut display, mut state) = crate::test_support::test_state((1280, 720));
    state.start_region_screenshot();
    assert!(
        state.screenshot_mode.is_some(),
        "the test should start a region selection"
    );

    let rx = run_client(&display, |conn, globals, qh, _client, tx| {
        let lock_manager = globals
            .bind::<ExtSessionLockManagerV1, TestClient, ()>(&qh, 1..=1, ())
            .expect("lock manager");
        let _lock = lock_manager.lock(&qh, ());
        conn.roundtrip().expect("lock roundtrip");
        tx.send(()).expect("lock ack");
    });

    dispatch_client_ack(&mut display, &mut state, &rx);

    assert!(state.locked, "the session should be locked");
    assert!(
        state.screenshot.is_none(),
        "a pending screenshot request must be dropped on lock"
    );
    assert!(
        state.screenshot_mode.is_none(),
        "the screenshot selection mode must be cleared on lock"
    );
}

#[test]
fn finishing_a_screenshot_is_blocked_while_locked() {
    let (_loop, _display, mut state) = crate::test_support::test_state((1280, 720));

    state.start_region_screenshot();
    state.screenshot_pointer_press(Point::from((10.0, 10.0)));
    state.locked = true;
    state.screenshot_pointer_release(Point::from((20.0, 20.0)));

    assert!(
        state.screenshot.is_none(),
        "finishing a screenshot while locked must not queue a request"
    );
}
