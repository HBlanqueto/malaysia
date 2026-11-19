use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::Mode as KdeMode;

use super::{
    BlockerKind, child_grab_serial_ok, decoration_for_kde_mode, decoration_for_xdg_mode,
    pending_blocker, write_before_deadline,
};
use crate::config::DecorationPolicy;

#[test]
fn a_clipboard_reader_that_never_reads_times_out() {
    let (_reader, writer) = std::io::pipe().expect("pipe");
    let bytes = vec![0u8; 4 << 20];
    let result = write_before_deadline(writer.into(), &bytes, std::time::Duration::from_millis(50));
    assert_eq!(
        result.map_err(|error| error.kind()),
        Err(std::io::ErrorKind::TimedOut)
    );
}

#[test]
fn a_clipboard_reader_gets_every_byte() {
    use std::io::Read;
    let (mut reader, writer) = std::io::pipe().expect("pipe");
    let bytes: Vec<u8> = (0..300_000u32).map(|i| i as u8).collect();
    let expected = bytes.clone();
    let read = std::thread::spawn(move || {
        let mut got = Vec::new();
        reader.read_to_end(&mut got).expect("read");
        got
    });
    write_before_deadline(writer.into(), &bytes, std::time::Duration::from_secs(5)).expect("write");
    assert_eq!(read.join().expect("reader"), expected);
}
use malaysia_scene::scene::Decoration;
use smithay::utils::Serial;

#[test]
fn an_xdg_client_that_accepts_server_side_gets_server_chrome() {
    assert_eq!(
        decoration_for_xdg_mode(Mode::ServerSide),
        Decoration::Server
    );
}

#[test]
fn an_xdg_client_that_requests_client_side_keeps_its_own_chrome() {
    assert_eq!(
        decoration_for_xdg_mode(Mode::ClientSide),
        Decoration::Client
    );
}

#[test]
fn a_kde_client_that_requests_server_side_gets_server_chrome() {
    assert_eq!(decoration_for_kde_mode(KdeMode::Server), Decoration::Server);
}

#[test]
fn a_kde_client_that_requests_client_side_keeps_its_own_chrome() {
    assert_eq!(decoration_for_kde_mode(KdeMode::Client), Decoration::Client);
}

#[test]
fn a_kde_client_that_asks_for_no_decoration_gets_none_from_malaysia_compositor() {
    assert_eq!(decoration_for_kde_mode(KdeMode::None), Decoration::Client);
}

#[test]
fn effective_decoration_covers_every_policy_and_native_case() {
    use crate::decoration::effective_decoration;
    for policy in [
        DecorationPolicy::Default,
        DecorationPolicy::ForceServer,
        DecorationPolicy::ForceClient,
    ] {
        for requested in [None, Some(Decoration::Client), Some(Decoration::Server)] {
            let native_expected = requested.unwrap_or(Decoration::Client);
            assert_eq!(
                effective_decoration(policy, requested, true),
                native_expected,
                "{policy:?} must honour a native app's requested mode"
            );
            let foreign_expected = match policy {
                DecorationPolicy::ForceServer => Decoration::Server,
                DecorationPolicy::ForceClient => Decoration::Client,
                DecorationPolicy::Default => requested.unwrap_or(Decoration::Server),
            };
            assert_eq!(
                effective_decoration(policy, requested, false),
                foreign_expected,
                "{policy:?} must keep its policy for a foreign app"
            );
        }
    }
}

#[test]
fn a_commit_without_a_dmabuf_never_blocks() {
    assert_eq!(pending_blocker(false, false), None, "no dmabuf, no blocker");
    assert_eq!(
        pending_blocker(false, true),
        None,
        "an acquire point without a buffer is a protocol error, not a blocker"
    );
}

#[test]
fn explicit_sync_wins_over_the_implicit_fence() {
    assert_eq!(pending_blocker(true, true), Some(BlockerKind::Explicit));
    assert_eq!(pending_blocker(true, false), Some(BlockerKind::Implicit));
}

#[test]
fn a_child_popup_uses_its_parent_grab_serial_or_a_later_one_it_received() {
    let chain = Serial::from(10u32);
    assert!(child_grab_serial_ok(chain, chain, None));
    assert!(child_grab_serial_ok(
        chain,
        Serial::from(12u32),
        Some(Serial::from(12u32))
    ));
}

#[test]
fn a_child_serial_the_client_never_received_or_older_than_the_chain_is_rejected() {
    let chain = Serial::from(10u32);
    assert!(!child_grab_serial_ok(
        chain,
        Serial::from(20u32),
        Some(Serial::from(12u32))
    ));
    assert!(!child_grab_serial_ok(
        chain,
        Serial::from(9u32),
        Some(Serial::from(12u32))
    ));
}
