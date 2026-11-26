pub mod layout;

use std::cell::Cell;
use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::desktop::{Space, Window};
use smithay::utils::{FrameExtents, IsAlive, Logical, Point, Rectangle, SERIAL_COUNTER};

use crate::spring::{rests, spring};
use crate::state::State;
use layout::arrange;

const OMEGA: f64 = ctdesign::EXPANSION_OMEGA as f64;
const DAMPING: f64 = ctdesign::EXPANSION_DAMPING as f64;

#[derive(Default)]
pub struct Expansion {
    pub open: bool,
    pub pressed: bool,
    progress: f64,
    motion: Option<Motion>,
    scale: f64,
    frames: Vec<Rectangle<f64, Logical>>,
    slots: Vec<Rectangle<f64, Logical>>,
    pushes: Vec<(Point<f64, Logical>, u32)>,
}

struct Motion {
    from: f64,
    velocity: f64,
    started: Duration,
}

#[derive(Default)]
struct Slot(Cell<Point<f64, Logical>>);

fn area(output: Rectangle<i32, Logical>) -> Rectangle<f64, Logical> {
    let side = ctdesign::EXPANSION_MARGIN_SIDE;
    let top = ctdesign::SPACES_STRIP_HEIGHT + ctdesign::EXPANSION_MARGIN_TOP;
    (output - FrameExtents::new(side, side, top, ctdesign::EXPANSION_MARGIN_BOTTOM)).to_f64()
}

fn laid_out(
    desktop: &Space<Window>,
) -> impl DoubleEndedIterator<Item = (&Window, Rectangle<i32, Logical>)> {
    desktop
        .elements()
        .filter(|window| window.alive())
        .filter_map(|window| {
            desktop
                .element_geometry(window)
                .filter(|frame| !frame.size.is_empty())
                .map(|frame| (window, frame))
        })
}

impl Expansion {
    pub fn drawn(
        &self,
        window: &Window,
        frame: Rectangle<i32, Logical>,
    ) -> (Point<f64, Logical>, f64) {
        let own = frame.loc.to_f64();
        match window.user_data().get::<Slot>().map(|slot| slot.0.get()) {
            Some(slot) => (
                own + (slot - own).upscale(self.progress),
                1.0 + (self.scale - 1.0) * self.progress,
            ),
            None => (own, 1.0),
        }
    }

    fn at(&self, now: Duration) -> (f64, f64) {
        let target = f64::from(self.open);
        match &self.motion {
            Some(motion) => {
                let (offset, velocity) = spring(
                    OMEGA,
                    DAMPING,
                    motion.from - target,
                    motion.velocity,
                    now.saturating_sub(motion.started).as_secs_f64(),
                );
                (target + offset, velocity)
            }
            None => (self.progress, 0.0),
        }
    }
}

impl State {
    pub fn advance_expansion(&mut self, now: Duration) {
        if self.expansion.motion.is_none() {
            return;
        }
        let (progress, velocity) = self.expansion.at(now);
        let target = f64::from(self.expansion.open);
        let longest_side = self
            .output
            .current_mode()
            .map_or(0, |mode| mode.size.w.max(mode.size.h));
        if rests(OMEGA, progress - target, velocity, f64::from(longest_side)) {
            self.expansion.progress = target;
            self.expansion.motion = None;
        } else {
            self.expansion.progress = progress;
            self.needs_redraw = true;
        }
    }

    fn aim_expansion(&mut self, open: bool) {
        let now = self.start_time.elapsed();
        let (from, velocity) = self.expansion.at(now);
        self.expansion.open = open;
        self.expansion.motion = Some(Motion {
            from,
            velocity,
            started: now,
        });
        self.needs_redraw = true;
    }

    pub fn open_expansion(&mut self, time: InputTime) {
        let pointer = self.seat.get_pointer().unwrap();
        let serial = SERIAL_COUNTER.next_serial();
        pointer.unset_grab(self, serial, time);
        self.dismiss_popups();
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, None, serial);
        self.aim_expansion(true);
        self.expansion.frames.clear();
        self.arrange_expansion();
        self.move_pointer(pointer.current_location(), time);
    }

    pub fn close_expansion(&mut self, picked: Option<Window>, time: InputTime) {
        self.aim_expansion(false);
        let focused = picked.or_else(|| self.top_window());
        if let Some(window) = focused {
            self.focus_window(&window, SERIAL_COUNTER.next_serial());
        }
        let location = self.seat.get_pointer().unwrap().current_location();
        self.move_pointer(location, time);
    }

    pub fn pick_in_expansion(&mut self, time: InputTime) {
        let location = self.seat.get_pointer().unwrap().current_location();
        let picked = laid_out(&self.desktops[self.current_desktop])
            .rev()
            .find(|(window, frame)| {
                let (loc, factor) = self.expansion.drawn(window, *frame);
                Rectangle::new(loc, frame.size.to_f64().upscale(factor)).contains(location)
            })
            .map(|(window, _)| window.clone());
        self.close_expansion(picked, time);
    }

    pub fn arrange_expansion(&mut self) {
        if !self.expansion.open {
            return;
        }
        let desktop = &self.desktops[self.current_desktop];
        let Some(output) = desktop.output_geometry(&self.output) else {
            return;
        };
        let expansion = &mut self.expansion;
        if laid_out(desktop)
            .map(|(_, frame)| frame.to_f64())
            .eq(expansion.frames.iter().copied())
        {
            return;
        }
        expansion.frames.clear();
        expansion
            .frames
            .extend(laid_out(desktop).map(|(_, frame)| frame.to_f64()));
        expansion.scale = arrange(
            area(output),
            &expansion.frames,
            &mut expansion.slots,
            &mut expansion.pushes,
        );
        for ((window, _), slot) in laid_out(desktop).zip(&expansion.slots) {
            window
                .user_data()
                .get_or_insert(Slot::default)
                .0
                .set(slot.loc);
        }
        self.needs_redraw = true;
    }
}
