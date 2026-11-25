use std::fs::OpenOptions;
use std::os::fd::OwnedFd;
use std::sync::Arc;

use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::{GbmFramebufferExporter, NodeFilter};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::{Color32F, gles::GlesRenderer};
use smithay::output::OutputModeSource;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use smithay::reexports::drm::control::{Device, ModeTypeFlags, connector};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{DeviceFd, Transform};
use smithay::wayland::socket::ListeningSocketSource;
use tracing_subscriber::{filter::Targets, fmt, prelude::*};

const CLEAR_COLOR: Color32F = Color32F::new(0.1, 0.1, 0.12, 1.0);

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

struct State {
    display_handle: DisplayHandle,
    renderer: GlesRenderer,
    compositor: Compositor,
}

impl State {
    fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let frame = self.compositor.render_frame::<_, SolidColorRenderElement>(
            &mut self.renderer,
            &[],
            CLEAR_COLOR,
            FrameFlags::DEFAULT,
        )?;
        if !frame.is_empty {
            self.compositor.queue_frame(())?;
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|directives| directives.parse().ok())
        .unwrap_or_else(|| Targets::new().with_default(tracing::Level::INFO));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: malaysia-compositor /dev/dri/cardN")?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let fd = DrmDeviceFd::new(DeviceFd::from(OwnedFd::from(file)));
    let (mut drm, notifier) = DrmDevice::new(fd.clone(), true)?;
    let gbm = GbmDevice::new(fd)?;
    let egl = unsafe { EGLDisplay::new(gbm.clone())? };
    let renderer = unsafe { GlesRenderer::new(EGLContext::new(&egl)?)? };

    let resources = drm.resource_handles()?;
    let connector = resources
        .connectors()
        .iter()
        .map(|handle| drm.get_connector(*handle, false))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|info| info.state() == connector::State::Connected)
        .ok_or("no connected connector")?;
    let mode = connector
        .modes()
        .iter()
        .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .or(connector.modes().first())
        .copied()
        .ok_or("connector has no mode")?;
    tracing::info!(
        connector = ?connector.interface(),
        id = connector.interface_id(),
        width = mode.size().0,
        height = mode.size().1,
        refresh = mode.vrefresh(),
        "mode chosen"
    );
    let crtc = connector
        .encoders()
        .iter()
        .filter_map(|handle| drm.get_encoder(*handle).ok())
        .find_map(|encoder| {
            resources
                .filter_crtcs(encoder.possible_crtcs())
                .first()
                .copied()
        })
        .ok_or("no crtc for the connector")?;
    let surface = drm.create_surface(crtc, mode, &[connector.handle()])?;
    let (width, height) = mode.size();
    let compositor = Compositor::new(
        OutputModeSource::Static {
            size: (width as i32, height as i32).into(),
            scale: 1.0.into(),
            transform: Transform::Normal,
        },
        surface,
        None,
        GbmAllocator::new(
            gbm.clone(),
            GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
        ),
        GbmFramebufferExporter::new(gbm.clone(), NodeFilter::None),
        [Fourcc::Argb8888, Fourcc::Xrgb8888],
        renderer.egl_context().dmabuf_render_formats().clone(),
        drm.cursor_size(),
        Some(gbm),
    )?;

    let mut event_loop = EventLoop::<State>::try_new()?;
    let display = Display::<State>::new()?;
    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();

    let handle = event_loop.handle();
    handle.insert_source(socket, |stream, _, state| {
        let _ = state.display_handle.insert_client(stream, Arc::new(()));
    })?;
    let display_handle = display.handle();
    handle.insert_source(
        Generic::new(display, Interest::READ, Mode::Level),
        |_, display, state| {
            unsafe { display.get_mut().dispatch_clients(state)? };
            Ok(PostAction::Continue)
        },
    )?;
    handle.insert_source(notifier, |event, _, state| match event {
        DrmEvent::VBlank(_) => {
            if let Err(error) = state
                .compositor
                .frame_submitted()
                .map_err(Into::into)
                .and_then(|_| state.draw())
            {
                tracing::error!(%error, "draw failed");
            }
        }
        DrmEvent::Error(error) => tracing::error!(%error, "drm error"),
    })?;

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &socket_name) };
    tracing::info!(socket = %socket_name.to_string_lossy(), "listening");

    let mut state = State {
        display_handle,
        renderer,
        compositor,
    };
    state.draw()?;
    event_loop.run(None, &mut state, |state| {
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}
