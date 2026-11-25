use std::error::Error;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::desktop::{Space, Window};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Physical, Size, Transform};

use crate::state::{CLEAR_COLOR, State, elements};

pub struct Winit {
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
}

fn mode(size: Size<i32, Physical>) -> Mode {
    Mode {
        size,
        refresh: 60_000,
    }
}

impl Winit {
    pub fn init(
        display_handle: &DisplayHandle,
        loop_handle: &LoopHandle<'static, State>,
        loop_signal: LoopSignal,
    ) -> Result<(Winit, Output), Box<dyn Error>> {
        let (backend, event_loop) = winit::init::<GlesRenderer>()?;
        let output = Output::new(
            "winit".to_string(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: String::new(),
                model: String::new(),
                serial_number: String::new(),
            },
        );
        let output_mode = mode(backend.window_size());
        output.set_preferred(output_mode);
        output.change_current_state(
            Some(output_mode),
            Some(Transform::Flipped180),
            None,
            Some((0, 0).into()),
        );
        output.create_global::<State>(display_handle);

        loop_handle.insert_source(event_loop, move |event, _, state| match event {
            WinitEvent::Resized { size, .. } => {
                state
                    .output
                    .change_current_state(Some(mode(size)), None, None, None);
                state.needs_redraw = true;
            }
            WinitEvent::Redraw => state.render(),
            WinitEvent::CloseRequested => loop_signal.stop(),
            _ => {}
        })?;
        Ok((
            Winit {
                damage_tracker: OutputDamageTracker::from_output(&output),
                backend,
            },
            output,
        ))
    }

    pub fn request_redraw(&self) {
        self.backend.window().request_redraw();
    }

    pub fn render(&mut self, space: &Space<Window>, output: &Output) -> Result<(), Box<dyn Error>> {
        let age = self.backend.buffer_age().unwrap_or(0);
        let (renderer, mut framebuffer) = self.backend.bind()?;
        let elements = elements(renderer, space, output);
        let result = self.damage_tracker.render_output(
            renderer,
            &mut framebuffer,
            age,
            &elements,
            CLEAR_COLOR,
        )?;
        drop(framebuffer);
        self.backend.submit(result.damage.map(Vec::as_slice))?;
        Ok(())
    }
}
