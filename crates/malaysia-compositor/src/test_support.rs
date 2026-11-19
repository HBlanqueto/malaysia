use std::os::unix::net::UnixStream;
use std::sync::Arc;

use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::calloop::ping::make_ping;
use smithay::reexports::wayland_server::{Client, Display};
use smithay::utils::Size;
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

use crate::session::Session;
use crate::state::{ClientDisconnectQueue, ClientState, State};

pub(crate) fn test_state(size: (i32, i32)) -> (EventLoop<'static, Session>, Display<State>, State) {
    let event_loop = EventLoop::<Session>::try_new().expect("the test needs an event loop");
    let display = Display::<State>::new().expect("the test needs a display");
    let feedback = DmabufFeedbackBuilder::new(
        0,
        [Format {
            code: Fourcc::Xrgb8888,
            modifier: Modifier::Invalid,
        }],
    )
    .build()
    .expect("the feedback should build");
    let state = State::new(
        event_loop.get_signal(),
        &display.handle(),
        Size::from(size),
        crate::state::BufferImport {
            feedback,
            syncobj: None,
        },
        Box::new(event_loop.handle()),
        Box::new(()),
    )
    .expect("the state should build");
    (event_loop, display, state)
}

pub(crate) fn insert_client(display: &Display<State>, server_side: UnixStream) -> Client {
    let (disconnect_ping, _) = make_ping().expect("the test needs a ping");
    display
        .handle()
        .insert_client(
            server_side,
            Arc::new(ClientState::new(
                ClientDisconnectQueue::default(),
                disconnect_ping,
                false,
            )),
        )
        .expect("the server should accept the client")
}

#[allow(unsafe_code)]
pub(crate) fn try_test_renderer() -> Option<smithay::backend::renderer::gles::GlesRenderer> {
    use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
    use smithay::backend::renderer::gles::GlesRenderer;
    let device = EGLDevice::enumerate().ok()?.next()?;
    let display = unsafe { EGLDisplay::new(device) }.ok()?;
    let context = EGLContext::new(&display).ok()?;
    unsafe { GlesRenderer::new(context) }.ok()
}

pub(crate) fn assert_hole_paints_nothing(
    renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
    program: &smithay::backend::renderer::gles::GlesPixelProgram,
    (area, uniforms, holes): crate::effects::ShaderQuad,
    scale: f64,
) {
    use smithay::backend::renderer::element::{Element, Kind};
    use smithay::backend::renderer::gles::element::PixelShaderElement;
    use smithay::utils::Scale;

    let element = PixelShaderElement::new(
        program.clone(),
        area,
        None,
        1.0,
        uniforms,
        Kind::Unspecified,
    );
    let size = element.geometry(Scale::from(scale)).size;
    assert!(holes.iter().any(Option::is_some), "the frame has an inside");
    let whole = paint(
        renderer,
        crate::effects::Hollow::new(element.clone(), [None, None]),
        size,
    );
    let hollow = paint(renderer, crate::effects::Hollow::new(element, holes), size);
    assert!(
        whole.iter().any(|byte| *byte != 0),
        "the quad paints something"
    );
    let first_difference = whole
        .iter()
        .zip(&hollow)
        .position(|(a, b)| a.abs_diff(*b) > 2);
    if let Some(byte) = first_difference {
        let (x, y) = ((byte / 4) as i32 % size.w, (byte / 4) as i32 / size.w);
        panic!(
            "the holes changed the pixel at {x},{y} (scale {scale}, holes {holes:?}): {:?} became {:?}",
            &whole[byte / 4 * 4..][..4],
            &hollow[byte / 4 * 4..][..4]
        );
    }
}

fn paint(
    renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
    element: crate::effects::Hollow,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
) -> Vec<u8> {
    use smithay::backend::renderer::element::{Element, RenderElement};
    use smithay::backend::renderer::gles::GlesTexture;
    use smithay::backend::renderer::{Bind, ExportMem, Frame, Offscreen, Renderer};
    use smithay::utils::{Buffer, Rectangle, Transform};

    let area = Rectangle::from_size(size);
    let mut target = Offscreen::<GlesTexture>::create_buffer(
        renderer,
        Fourcc::Abgr8888,
        Size::from((size.w, size.h)),
    )
    .expect("the target is created");
    {
        let mut framebuffer = renderer.bind(&mut target).unwrap();
        let mut frame = renderer
            .render(&mut framebuffer, size, Transform::Normal)
            .unwrap();
        frame.clear([0.0; 4].into(), &[area]).unwrap();
        element
            .draw(&mut frame, element.src(), area, &[area], &[], None)
            .unwrap();
        let _synced = frame.finish().unwrap();
    }
    let framebuffer = renderer.bind(&mut target).unwrap();
    let mapping = renderer
        .copy_framebuffer(
            &framebuffer,
            Rectangle::from_size(Size::<i32, Buffer>::from((size.w, size.h))),
            Fourcc::Abgr8888,
        )
        .unwrap();
    renderer.map_texture(&mapping).unwrap().to_vec()
}
