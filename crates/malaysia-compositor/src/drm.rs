use std::error::Error;
use std::fs::OpenOptions;
use std::os::fd::OwnedFd;

use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::{GbmFramebufferExporter, NodeFilter};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{Space, Window};
use smithay::output::{Mode, Output, PhysicalProperties};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::drm::control::{Device, ModeTypeFlags, connector};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::DeviceFd;

use crate::state::{CLEAR_COLOR, State, elements};

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

pub struct Drm {
    renderer: GlesRenderer,
    compositor: Compositor,
}

impl Drm {
    pub fn init(
        path: &str,
        display_handle: &DisplayHandle,
        loop_handle: &LoopHandle<'static, State>,
    ) -> Result<(Drm, Output), Box<dyn Error>> {
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

        let (width_mm, height_mm) = connector.size().unwrap_or((0, 0));
        let output = Output::new(
            format!(
                "{}-{}",
                connector.interface().as_str(),
                connector.interface_id()
            ),
            PhysicalProperties {
                size: (width_mm as i32, height_mm as i32).into(),
                subpixel: connector.subpixel().into(),
                make: String::new(),
                model: String::new(),
                serial_number: String::new(),
            },
        );
        let output_mode = Mode::from(mode);
        output.set_preferred(output_mode);
        output.change_current_state(Some(output_mode), None, None, Some((0, 0).into()));
        output.create_global::<State>(display_handle);

        let surface = drm.create_surface(crtc, mode, &[connector.handle()])?;
        let compositor = Compositor::new(
            &output,
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

        loop_handle.insert_source(notifier, move |event, _, state| match event {
            DrmEvent::VBlank(_) => {
                if let Err(error) = state.backend.compositor.frame_submitted() {
                    tracing::error!(%error, "frame submit failed");
                }
                state.frame_done();
            }
            DrmEvent::Error(error) => tracing::error!(%error, "drm error"),
        })?;
        Ok((
            Drm {
                renderer,
                compositor,
            },
            output,
        ))
    }

    pub fn render(
        &mut self,
        space: &Space<Window>,
        output: &Output,
    ) -> Result<bool, Box<dyn Error>> {
        let elements = elements(&mut self.renderer, space, output);
        let frame = self.compositor.render_frame(
            &mut self.renderer,
            &elements,
            CLEAR_COLOR,
            FrameFlags::DEFAULT,
        )?;
        if frame.is_empty {
            return Ok(false);
        }
        self.compositor.queue_frame(())?;
        Ok(true)
    }
}
