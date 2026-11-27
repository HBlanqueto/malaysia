pub mod layout;
pub mod strip;

use std::cell::Cell;
use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::desktop::{Space, Window};
use smithay::utils::{FrameExtents, IsAlive, Logical, Point, Rectangle, SERIAL_COUNTER, Size};

use crate::spring::{Motion, advance_motion};
use crate::state::State;
use crate::workspaces::desktops::{move_window, send_changed_state};
use crate::workspaces::full_screen::is_full_screen;
use layout::arrange;
use strip::{Hit, Strip};

const OMEGA: f64 = ctdesign::EXPANSION_OMEGA as f64;
const DAMPING: f64 = ctdesign::EXPANSION_DAMPING as f64;
const DRAG_THRESHOLD: f64 = ctdesign::DRAG.threshold as f64;

#[derive(Default)]
pub struct Expansion {
    pub open: bool,
    pub pressed: bool,
    pub drag_origin: Option<Point<f64, Logical>>,
    pub dragged: Option<(Window, Point<f64, Logical>)>,
    pub strip: Strip,
    pub desktop: usize,
    progress: f64,
    motion: Option<Motion>,
    pub held: bool,
    unsettled: f64,
    settle: Option<Motion>,
    openings: u32,
    frames: Vec<Rectangle<f64, Logical>>,
    slots: Vec<Rectangle<f64, Logical>>,
    pushes: Vec<(Point<f64, Logical>, u32)>,
}

struct Slot {
    loc: Cell<(Point<f64, Logical>, f64)>,
    before_growth: Cell<(Point<f64, Logical>, f64)>,
    off_slot: Cell<(Point<f64, Logical>, f64)>,
    opening: Cell<u32>,
}

fn area(output: Rectangle<i32, Logical>, strip_height: i32) -> Rectangle<f64, Logical> {
    let side = ctdesign::EXPANSION_MARGIN_SIDE;
    let top = strip_height + ctdesign::EXPANSION_MARGIN_TOP;
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
        let Some(slot) = window
            .user_data()
            .get::<Slot>()
            .filter(|slot| slot.opening.get() == self.openings)
        else {
            return (own, 1.0);
        };
        let (loc, scale) = self.aimed(slot, own);
        let (off, scale_off) = slot.off_slot.get();
        (
            loc + off.upscale(self.unsettled),
            scale + scale_off * self.unsettled,
        )
    }

    fn aimed(&self, slot: &Slot, own: Point<f64, Logical>) -> (Point<f64, Logical>, f64) {
        let growth = self.strip.growth;
        let (before, scale_before) = slot.before_growth.get();
        let (loc, scale) = slot.loc.get();
        let loc = before + (loc - before).upscale(growth);
        let scale = scale_before + (scale - scale_before) * growth;
        (
            own + (loc - own).upscale(self.progress),
            1.0 + (scale - 1.0) * self.progress,
        )
    }

    pub fn at_rest(&self) -> bool {
        !self.held && self.motion.is_none() && self.settle.is_none() && self.strip.motion.is_none()
    }

    pub fn strip_top(&self) -> Option<f64> {
        (self.progress > 0.0).then(|| (self.progress - 1.0) * self.strip.height())
    }

    pub fn window_at<'a>(
        &self,
        desktops: &'a [Space<Window>],
        location: Point<f64, Logical>,
    ) -> Option<(&'a Window, Point<f64, Logical>)> {
        laid_out(&desktops[self.desktop])
            .rev()
            .find_map(|(window, frame)| {
                let (loc, factor) = self.drawn(window, frame);
                Rectangle::new(loc, frame.size.to_f64().upscale(factor))
                    .contains(location)
                    .then_some((window, loc))
            })
    }

    pub fn at(&self, now: Duration) -> (f64, f64) {
        self.motion.as_ref().map_or((self.progress, 0.0), |motion| {
            motion.at(OMEGA, DAMPING, f64::from(self.open), now)
        })
    }

    fn unsettle(&mut self, desktops: &[Space<Window>], now: Duration) {
        for slot in desktops
            .iter()
            .flat_map(Space::elements)
            .filter_map(|window| window.user_data().get::<Slot>())
        {
            let (off, scale_off) = slot.off_slot.get();
            slot.off_slot
                .set((off.upscale(self.unsettled), scale_off * self.unsettled));
        }
        self.unsettled = 1.0;
        self.settle = Some(Motion {
            from: 1.0,
            velocity: 0.0,
            started: now,
        });
    }

    fn settle_from(
        &self,
        slot: &Slot,
        own: Point<f64, Logical>,
        (loc, scale): (Point<f64, Logical>, f64),
    ) {
        let (aimed, aimed_scale) = self.aimed(slot, own);
        slot.off_slot.set((loc - aimed, scale - aimed_scale));
    }
}

impl State {
    pub fn advance_expansion(&mut self, now: Duration) {
        let longest_side = f64::from(
            self.output
                .current_mode()
                .map_or(0, |mode| mode.size.w.max(mode.size.h)),
        );
        if self.expansion.strip.advance(now, longest_side) {
            self.needs_redraw = true;
        }
        let expansion = &mut self.expansion;
        if let Some(unsettled) = advance_motion(
            &mut expansion.settle,
            OMEGA,
            DAMPING,
            0.0,
            now,
            longest_side,
        ) {
            expansion.unsettled = unsettled;
            self.needs_redraw |= expansion.settle.is_some();
        }
        let target = f64::from(expansion.open);
        let Some(progress) = advance_motion(
            &mut expansion.motion,
            OMEGA,
            DAMPING,
            target,
            now,
            longest_side,
        ) else {
            return;
        };
        expansion.progress = progress;
        if expansion.motion.is_some() {
            self.needs_redraw = true;
        } else if !expansion.open {
            expansion.openings += 1;
            expansion.strip = Strip::default();
            if self.slide.is_none() {
                self.suspend_hidden_desktops(now);
            }
        }
    }

    fn aim_expansion(&mut self, open: bool) {
        let now = self.start_time.elapsed();
        let (from, velocity) = self.expansion.at(now);
        self.expansion.open = open;
        self.expansion.held = false;
        self.expansion.motion = Some(Motion {
            from,
            velocity,
            started: now,
        });
        self.needs_redraw = true;
    }

    pub fn hold_expansion(&mut self, progress: f64) {
        self.expansion.progress = progress;
        self.expansion.motion = None;
        self.expansion.held = true;
        self.needs_redraw = true;
    }

    pub fn release_expansion(&mut self, open: bool, velocity: f64, time: InputTime) {
        if open {
            self.aim_expansion(true);
        } else {
            self.close_expansion(None, time);
        }
        if let Some(motion) = &mut self.expansion.motion {
            motion.velocity = velocity;
        }
    }

    pub fn open_expansion(&mut self, time: InputTime) {
        let pointer = self.seat.get_pointer().unwrap();
        let serial = SERIAL_COUNTER.next_serial();
        pointer.unset_grab(self, serial, time);
        self.growth = None;
        self.dismiss_popups();
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, None, serial);
        self.aim_expansion(true);
        self.rearrange_expansion();
        self.move_pointer(pointer.current_location(), time);
    }

    fn on_strip(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(Point<f64, Logical>, Size<f64, Logical>)> {
        let top = self.expansion.strip_top()?;
        let output = self.desktops[self.current_desktop].output_geometry(&self.output)?;
        Some((location - Point::from((0.0, top)), output.size.to_f64()))
    }

    fn strip_hit(&self, location: Point<f64, Logical>) -> Option<Hit> {
        self.on_strip(location).and_then(|(point, output)| {
            self.expansion.strip.hit(point, self.desktops.len(), output)
        })
    }

    fn drag_in_expansion(&mut self, location: Point<f64, Logical>) {
        let Some(origin) = self.expansion.drag_origin else {
            return;
        };
        let moved = location - origin;
        if moved.x.abs().max(moved.y.abs()) <= DRAG_THRESHOLD {
            return;
        }
        self.expansion.drag_origin = None;
        if self.strip_hit(origin).is_none() {
            self.expansion.dragged = self
                .expansion
                .window_at(&self.desktops, origin)
                .filter(|(window, _)| !is_full_screen(window))
                .map(|(window, top_left)| (window.clone(), origin - top_left));
        }
    }

    pub fn hover_expansion(&mut self, location: Point<f64, Logical>) {
        self.drag_in_expansion(location);
        self.needs_redraw = true;
        let Some((point, output)) = self.on_strip(location) else {
            return;
        };
        let was_grown = self.expansion.strip.grown;
        let now = self.start_time.elapsed();
        self.expansion
            .strip
            .hover(point, self.desktops.len(), output, now);
        if self.expansion.dragged.is_some()
            || self
                .expansion
                .strip
                .hovered
                .is_some_and(|index| !self.is_desktop(index) || self.heir(index).is_none())
        {
            self.expansion.strip.hovered = None;
        }
        if !was_grown && self.expansion.strip.grown {
            self.wake_desktops();
            self.rearrange_expansion();
        }
    }

    pub fn rearrange_expansion(&mut self) {
        self.expansion.frames.clear();
        self.arrange_expansion();
    }

    pub fn close_expansion(&mut self, picked: Option<Window>, time: InputTime) {
        self.expansion.dragged = None;
        self.expansion.drag_origin = None;
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
        let hit = self.strip_hit(location);
        if let Some((window, grip)) = self.expansion.dragged.take() {
            self.drop_window(window, location - grip, hit);
            self.hover_expansion(location);
            self.needs_redraw = true;
            return;
        }
        match hit {
            Some(Hit::Thumbnail(index)) => {
                self.go_to_desktop(index, time);
                self.close_expansion(None, time);
            }
            Some(Hit::Add) => {
                self.add_desktop();
                self.needs_redraw = true;
            }
            Some(Hit::Close(index)) => {
                self.remove_entry(index);
                self.hover_expansion(location);
                self.needs_redraw = true;
            }
            Some(Hit::Bar) => {}
            None => self.close_expansion(
                self.expansion
                    .window_at(&self.desktops, location)
                    .map(|(window, _)| window.clone()),
                time,
            ),
        }
    }

    fn drop_window(&mut self, window: Window, top_left: Point<f64, Logical>, hit: Option<Hit>) {
        if !window.alive() {
            return;
        }
        let origin = self.expansion.desktop;
        let target = match hit {
            Some(Hit::Thumbnail(index)) if index != origin && self.is_desktop(index) => Some(index),
            Some(Hit::Add) => self.add_desktop(),
            _ => None,
        };
        if let Some(target) = target {
            window.set_activated(false);
            send_changed_state(&window);
            let [from, to] = self.desktops.get_disjoint_mut([origin, target]).unwrap();
            move_window(&window, from, to);
            self.rearrange_expansion();
        } else if let Some(frame) = self.desktops[origin].element_geometry(&window)
            && let Some(slot) = window.user_data().get::<Slot>()
        {
            let scale = self.expansion.drawn(&window, frame).1;
            self.expansion
                .unsettle(&self.desktops, self.start_time.elapsed());
            self.expansion
                .settle_from(slot, frame.loc.to_f64(), (top_left, scale));
        }
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
        let settling = expansion.desktop == self.current_desktop || self.slide.is_none();
        expansion.desktop = self.current_desktop;
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
        let scale = arrange(
            area(output, expansion.strip.target_height()),
            &expansion.frames,
            &mut expansion.slots,
            &mut expansion.pushes,
        );
        if settling {
            expansion.unsettle(&self.desktops, self.start_time.elapsed());
        }
        let before_growth = !expansion.strip.grown;
        for ((window, frame), slot) in laid_out(desktop).zip(&expansion.slots) {
            let kept = window.user_data().get_or_insert(|| Slot {
                loc: Cell::new((slot.loc, scale)),
                before_growth: Cell::new((slot.loc, scale)),
                off_slot: Cell::default(),
                opening: Cell::default(),
            });
            let was = expansion.drawn(window, frame);
            let arranged_before = kept.opening.replace(expansion.openings) == expansion.openings;
            kept.loc.set((slot.loc, scale));
            if before_growth {
                kept.before_growth.set((slot.loc, scale));
            }
            if arranged_before && settling {
                expansion.settle_from(kept, frame.loc.to_f64(), was);
            }
        }
        self.needs_redraw = true;
    }
}
