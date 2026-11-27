use std::error::Error;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::desktop::Space;
use smithay::input::keyboard::KeyboardSource;
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Physical, Size, Transform};
use smithay::wayland::fractional_scale::with_fractional_scale;

use crate::effects::Effects;
use crate::state::{Element, Row, State, elements};

pub struct Winit {
    backend: WinitGraphicsBackend<GlesRenderer>,
    effects: Effects,
    damage_tracker: OutputDamageTracker,
    age: usize,
}

struct StopLoopOnDrop(LoopSignal);

impl Drop for StopLoopOnDrop {
    fn drop(&mut self) {
        tracing::info!("the winit backend ended");
        self.0.stop();
    }
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
        dark: bool,
    ) -> Result<(Winit, Output), Box<dyn Error>> {
        let (mut backend, event_loop) = winit::init::<GlesRenderer>()?;
        let effects = Effects::new(backend.renderer(), dark)?;
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

        let stop_loop_on_drop = StopLoopOnDrop(loop_signal);
        loop_handle.insert_source(event_loop, move |event, _, state| {
            let _ = &stop_loop_on_drop;
            match event {
                WinitEvent::Resized { size, scale_factor } => {
                    state.output.change_current_state(
                        Some(mode(size)),
                        None,
                        Some(Scale::Fractional(scale_factor)),
                        None,
                    );
                    for window in state.desktops.iter().flat_map(Space::elements) {
                        window.with_surfaces(|_, states| {
                            with_fractional_scale(states, |fractional| {
                                fractional.set_preferred_scale(scale_factor)
                            })
                        });
                    }
                    state.rearrange_expansion();
                    state.needs_redraw = true;
                }
                WinitEvent::Redraw => state.render(),
                WinitEvent::CloseRequested => {
                    tracing::info!("stopping: the host closed the window");
                    state.loop_signal.stop();
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Focus(false) => {
                    let keyboard = state.seat.get_keyboard().unwrap();
                    keyboard.release_source(state, KeyboardSource::Physical);
                    state.suppressed_keys.clear();
                }
                _ => {}
            }
        })?;
        Ok((
            Winit {
                damage_tracker: OutputDamageTracker::from_output(&output),
                age: 0,
                effects,
                backend,
            },
            output,
        ))
    }

    pub fn request_redraw(&self) {
        self.backend.window().request_redraw();
    }

    pub fn render(
        &mut self,
        row: Row,
        output: &Output,
        frame: &mut Vec<Element>,
    ) -> Result<bool, Box<dyn Error>> {
        let (renderer, mut framebuffer) = self.backend.bind()?;
        elements(renderer, &self.effects, row, output, frame);
        let result = self.damage_tracker.render_output(
            renderer,
            &mut framebuffer,
            self.age,
            frame,
            ctdesign::DESKTOP_BACKGROUND.premultiplied(),
        )?;
        drop(framebuffer);
        let Some(damage) = result.damage else {
            return Ok(false);
        };
        self.backend.submit(Some(damage))?;
        self.age = self.backend.buffer_age().unwrap_or(0);
        Ok(true)
    }
}
