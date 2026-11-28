use std::os::fd::AsFd;
use std::sync::atomic::Ordering;

use ctappearance::watch::{AppearanceEvent, AppearanceWatch};
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, generic::Generic};

use crate::spring::REDUCED_MOTION;
use crate::state::{Backend, State};

pub fn watch_appearance(
    loop_handle: &LoopHandle<'static, State>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut watch = AppearanceWatch::new()?;
    let fd = watch.as_fd().try_clone_to_owned()?;
    loop_handle.insert_source(
        Generic::new(fd, Interest::READ, Mode::Level),
        move |_, _, state| match watch.drain() {
            Ok(events) => {
                if events.iter().any(AppearanceEvent::restyles) {
                    state.restyle();
                }
                Ok(PostAction::Continue)
            }
            Err(error) => {
                tracing::warn!(%error, "the appearance watch stopped");
                Ok(PostAction::Remove)
            }
        },
    )?;
    Ok(())
}

impl State {
    pub fn restyle(&mut self) {
        let config = ctappearance::load_appearance();
        let dark = config.color_scheme.is_dark();
        let ring = ctappearance::resolve(&config, ctappearance::load_dominant(), false).ring_accent;
        REDUCED_MOTION.store(config.reduced_motion, Ordering::Relaxed);
        let effects = match &mut self.backend {
            Backend::Drm(drm) => &mut drm.effects,
            Backend::Winit(winit) => &mut winit.effects,
        };
        effects.restyle(dark, ring);
        self.needs_redraw = true;
    }
}
