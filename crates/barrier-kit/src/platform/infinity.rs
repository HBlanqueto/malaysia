//! Client-side Infinity protocol bindings (ADR 0039).
//!
//! The shell uses this private protocol to ask the compositor for window
//! geometry: halves, quarters, pairs, fill, center, restore and fullscreen.

pub mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    use sctk::reexports::protocols_wlr::foreign_toplevel::v1::client::*;
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use sctk::reexports::protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../../protocols/infinity-window-manager-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("../../protocols/infinity-window-manager-v1.xml");
}

pub use protocol::infinity_window_manager_v1::{InfinityWindowManagerV1, Layout, PairLayout};
