//! The client-side titlebar honours the compositor's decoration answer.

use std::io::Cursor;

use barrier_kit::platform::{DecorationResponse, WindowStatus};
use barrier_kit::titlebar::Titlebar;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use freya_testing::TestingRunner;
use torin::prelude::Size2D;

struct Case {
    status: WindowStatus,
}

impl App for Case {
    fn render(&self) -> impl IntoElement {
        Titlebar::preview(self.status, "Ventana", false)
    }
}

fn render(status: WindowStatus) -> Vec<u8> {
    let app = AppComponent::new(Case { status });
    let (mut test, _) = TestingRunner::new(app, Size2D::new(300.0, 120.0), |_| {}, 1.0);
    test.sync_and_update();
    let png = test.render();
    let mut decoder = png::Decoder::new(Cursor::new(png.as_bytes()));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).unwrap();
    buffer[..info.buffer_size()].to_vec()
}

fn status(decoration: DecorationResponse) -> WindowStatus {
    WindowStatus {
        activated: true,
        maximized: false,
        decoration,
        fixed: false,
    }
}

#[test]
fn a_server_configure_has_no_titlebar_and_a_client_one_does() {
    let server = render(status(DecorationResponse::Server));
    let client = render(status(DecorationResponse::Client));
    assert_ne!(server, client, "the client configure must draw a bar");
}
