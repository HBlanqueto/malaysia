pub mod layout;
pub mod strip;

use std::cell::Cell;
use std::time::Duration;

use smithay::backend::input::InputTime;
use smithay::desktop::{Space, Window};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{FrameExtents, IsAlive, Logical, Point, Rectangle, SERIAL_COUNTER, Size};

use crate::effects::{app_id, with_text};
use crate::spring::{Motion, advance_motion, sample_motion};
use crate::state::State;
use crate::workspaces::desktops::{Direction, move_window, send_changed_state};
use crate::workspaces::full_screen::{full_screen_window, is_full_screen, waiting};
use layout::{arrange, grid, preview};
use strip::{EXPANDED_HEIGHT, Hit, Strip, fitted, miniature, split_shares};

const OMEGA: f64 = ctdesign::EXPANSION_OMEGA as f64;
const DAMPING: f64 = ctdesign::EXPANSION_DAMPING as f64;
const DROP_OMEGA: f64 = ctdesign::EXPANSION_DROP_OMEGA as f64;
const DROP_DAMPING: f64 = ctdesign::EXPANSION_DROP_DAMPING as f64;
const LABEL_FADE_MS: f64 = ctdesign::SPACES_SPLIT_LABEL_FADE_MS as f64;
const DRAG_THRESHOLD: f64 = ctdesign::DRAG.threshold as f64;
const DRAG_FLOOR_SCALE: f64 = ctdesign::EXPANSION_DRAG_FLOOR_SCALE as f64;
const STRIP_ENTRY: (f64, f64) = (
    ctdesign::SPACES_STRIP_ENTRY_OMEGA as f64,
    ctdesign::SPACES_STRIP_ENTRY_DAMPING as f64,
);
const STRIP_EXIT: (f64, f64) = (
    ctdesign::SPACES_STRIP_EXIT_OMEGA as f64,
    ctdesign::SPACES_STRIP_EXIT_DAMPING as f64,
);

#[derive(Default)]
pub struct Expansion {
    pub open: bool,
    pub pressed: bool,
    pub drag_origin: Option<Point<f64, Logical>>,
    pub dragged: Option<Dragged>,
    pub strip: Strip,
    pub desktop: usize,
    pub progress: f64,
    motion: Option<Motion>,
    strip_in: f64,
    strip_motion: Option<Motion>,
    pub held: bool,
    unsettled: f64,
    settle: Option<Motion>,
    openings: u32,
    frames: Vec<Rectangle<f64, Logical>>,
    slots: Vec<Rectangle<f64, Logical>>,
    pushes: Vec<(Point<f64, Logical>, u32)>,
    pub previewed: Option<Window>,
    preview_target: (Point<f64, Logical>, f64),
    enlarged: bool,
    preview: Option<Motion>,
    previewing: f64,
    pub app: Option<Window>,
    pub flight: Option<Flight>,
    pub ghost: Option<Ghost>,
}

pub struct Dragged {
    window: Window,
    grip: Point<f64, Logical>,
    start_y: f64,
    start_scale: f64,
    split: Option<(usize, Direction, f64)>,
}

pub struct Flight {
    window: Window,
    from: (Point<f64, Logical>, f64),
    landing: Landing,
    flown: f64,
    motion: Option<Motion>,
}

#[derive(Clone, Copy)]
enum Landing {
    Slot,
    Desktop(usize),
    Split(usize, Direction, f64),
}

pub struct Lifted<'a> {
    pub window: &'a Window,
    pub entry: Option<usize>,
    pub frame: Rectangle<i32, Logical>,
    pub drawn: (Point<f64, Logical>, f64),
}

pub struct Ghost {
    pub window: Window,
    pub app: Window,
    pub side: Direction,
    since: Duration,
    pub opacity: f64,
}

fn holding(
    desktops: &[Space<Window>],
    window: &Window,
) -> Option<(usize, Rectangle<i32, Logical>)> {
    desktops
        .iter()
        .enumerate()
        .find_map(|(index, entry)| entry.element_geometry(window).map(|frame| (index, frame)))
        .filter(|(_, frame)| !frame.size.is_empty())
}

fn entry_of(desktops: &[Space<Window>], id: usize) -> Option<usize> {
    desktops.iter().position(|entry| entry.id() == id)
}

fn travel(
    (from, from_scale): (Point<f64, Logical>, f64),
    (to, to_scale): (Point<f64, Logical>, f64),
    size: Size<i32, Logical>,
) -> f64 {
    let moved = to - from;
    let longest_side = f64::from(size.w.max(size.h));
    moved
        .x
        .abs()
        .max(moved.y.abs())
        .max((to_scale - from_scale).abs() * longest_side)
}

fn mixed(
    (from, from_scale): (Point<f64, Logical>, f64),
    (to, to_scale): (Point<f64, Logical>, f64),
    progress: f64,
) -> (Point<f64, Logical>, f64) {
    (
        from + (to - from).upscale(progress),
        from_scale + (to_scale - from_scale) * progress,
    )
}

fn held_scale(y: f64, start_y: f64, start_scale: f64) -> f64 {
    let risen = ((start_y - y) / (start_y - EXPANDED_HEIGHT).max(f64::EPSILON)).clamp(0.0, 1.0);
    start_scale + (DRAG_FLOOR_SCALE - start_scale) * risen
}

struct Slot {
    loc: Cell<(Point<f64, Logical>, f64)>,
    before_growth: Cell<(Point<f64, Logical>, f64)>,
    off_slot: Cell<(Point<f64, Logical>, f64)>,
    opening: Cell<u32>,
}

pub fn of_app(window: &Window, app: Option<&Window>) -> bool {
    let Some(app) = app else {
        return true;
    };
    if window == app {
        return true;
    }
    let surface = window.toplevel().unwrap().wl_surface();
    let app_surface = app.toplevel().unwrap().wl_surface();
    with_text(surface, app_id, |id| {
        with_text(app_surface, app_id, |other| other == id) == Some(true)
    })
    .unwrap_or_else(|| {
        with_text(app_surface, app_id, |_| ()).is_none() && surface.client() == app_surface.client()
    })
}

fn laid_out<'a>(
    desktop: &'a Space<Window>,
    app: Option<&Window>,
) -> impl DoubleEndedIterator<Item = (&'a Window, Rectangle<i32, Logical>)> {
    desktop
        .elements()
        .filter(move |window| window.alive() && of_app(window, app))
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
        let Some(slot) = self.slot_of(window) else {
            return (own, 1.0);
        };
        let (loc, scale) = self.aimed(slot, own);
        let (off, scale_off) = slot.off_slot.get();
        let (loc, scale) = (
            loc + off.upscale(self.unsettled),
            scale + scale_off * self.unsettled,
        );
        if self.previewed.as_ref() != Some(window) {
            return (loc, scale);
        }
        mixed((loc, scale), self.preview_target, self.previewing)
    }

    fn slot_of<'a>(&self, window: &'a Window) -> Option<&'a Slot> {
        window
            .user_data()
            .get::<Slot>()
            .filter(|slot| slot.opening.get() == self.openings)
    }

    pub fn area(&self, output: Rectangle<i32, Logical>) -> Rectangle<f64, Logical> {
        let strip_height = if self.app.is_some() {
            0
        } else {
            self.strip.target_height()
        };
        let side = ctdesign::EXPANSION_MARGIN_SIDE;
        let top = strip_height + ctdesign::EXPANSION_MARGIN_TOP;
        (output - FrameExtents::new(side, side, top, ctdesign::EXPANSION_MARGIN_BOTTOM)).to_f64()
    }

    pub fn preview_moving(&self, window: &Window) -> bool {
        self.preview.is_some() && self.previewed.as_ref() == Some(window)
    }

    fn preview_at(&self, now: Duration) -> (f64, f64) {
        sample_motion(
            &self.preview,
            OMEGA,
            DAMPING,
            self.previewing,
            f64::from(self.enlarged),
            now,
        )
    }

    fn clear_preview(&mut self) {
        self.previewed = None;
        self.enlarged = false;
        self.preview = None;
        self.previewing = 0.0;
    }

    fn aimed(&self, slot: &Slot, own: Point<f64, Logical>) -> (Point<f64, Logical>, f64) {
        mixed(
            (own, 1.0),
            mixed(slot.before_growth.get(), slot.loc.get(), self.strip.growth),
            self.progress,
        )
    }

    pub fn at_rest(&self) -> bool {
        !self.held
            && !self.flying()
            && self.motion.is_none()
            && self.strip_motion.is_none()
            && self.settle.is_none()
            && self.strip.at_rest()
    }

    pub fn flying(&self) -> bool {
        self.flight
            .as_ref()
            .is_some_and(|flight| flight.motion.is_some())
    }

    pub fn strip_top(&self) -> Option<f64> {
        (self.strip_in > 0.0).then(|| (self.strip_in - 1.0) * self.strip.height())
    }

    fn strip_spring(&self) -> (f64, f64) {
        if self.open { STRIP_ENTRY } else { STRIP_EXIT }
    }

    fn strip_at(&self, now: Duration) -> (f64, f64) {
        let (omega, damping) = self.strip_spring();
        sample_motion(
            &self.strip_motion,
            omega,
            damping,
            self.strip_in,
            f64::from(self.open),
            now,
        )
    }

    fn held_drawn(
        &self,
        dragged: &Dragged,
        size: Size<i32, Logical>,
        pointer: Point<f64, Logical>,
    ) -> (Point<f64, Logical>, f64) {
        let scale = if self.app.is_some() {
            dragged.start_scale
        } else {
            held_scale(pointer.y, dragged.start_y, dragged.start_scale)
        };
        let size = size.to_f64().upscale(scale);
        let grip = Point::from((dragged.grip.x * size.w, dragged.grip.y * size.h));
        (pointer - grip, scale)
    }

    fn landing(
        &self,
        desktops: &[Space<Window>],
        flight: &Flight,
        frame: Rectangle<i32, Logical>,
        output: Size<f64, Logical>,
    ) -> Option<(Point<f64, Logical>, f64)> {
        let thumbnail = |id| {
            let index = entry_of(desktops, id)?;
            let mut thumbnail = self.strip.thumbnail(index, desktops.len(), output);
            thumbnail.loc.y += self.strip_top()?;
            Some(thumbnail)
        };
        match flight.landing {
            Landing::Slot => Some(self.drawn(&flight.window, frame)),
            Landing::Desktop(id) => {
                thumbnail(id).map(|thumbnail| miniature(thumbnail, frame.loc.to_f64(), output))
            }
            Landing::Split(id, side, left) => thumbnail(id).map(|thumbnail| {
                let (left_share, right_share) = split_shares(thumbnail, left);
                fitted(
                    side.left_right(left_share, right_share).0,
                    frame.size.to_f64(),
                )
            }),
        }
    }

    fn flown(
        &self,
        desktops: &[Space<Window>],
        flight: &Flight,
        frame: Rectangle<i32, Logical>,
        output: Size<f64, Logical>,
    ) -> Option<(Point<f64, Logical>, f64)> {
        let to = self.landing(desktops, flight, frame, output)?;
        Some(mixed(flight.from, to, flight.flown))
    }

    pub fn lifted(
        &self,
        desktops: &[Space<Window>],
        output: Size<f64, Logical>,
        pointer: Point<f64, Logical>,
    ) -> Option<Lifted<'_>> {
        if let Some(dragged) = &self.dragged {
            let frame = desktops[self.desktop].element_geometry(&dragged.window)?;
            return Some(Lifted {
                window: &dragged.window,
                entry: Some(self.desktop),
                frame,
                drawn: self.held_drawn(dragged, frame.size, pointer),
            });
        }
        let flight = self.flight.as_ref()?;
        let (entry, frame) = holding(desktops, &flight.window)?;
        Some(Lifted {
            window: &flight.window,
            entry: matches!(flight.landing, Landing::Slot).then_some(entry),
            frame,
            drawn: self.flown(desktops, flight, frame, output)?,
        })
    }

    pub fn split_preview(
        &self,
        desktops: &[Space<Window>],
    ) -> Option<(usize, &Window, Direction, f64)> {
        if let Some(dragged) = &self.dragged {
            return dragged
                .split
                .map(|(index, side, left)| (index, &dragged.window, side, left));
        }
        let flight = self.flight.as_ref()?;
        let Landing::Split(id, side, left) = flight.landing else {
            return None;
        };
        Some((entry_of(desktops, id)?, &flight.window, side, left))
    }

    pub fn window_at<'a>(
        &self,
        desktops: &'a [Space<Window>],
        location: Point<f64, Logical>,
    ) -> Option<(&'a Window, Point<f64, Logical>, f64)> {
        let desktop = &desktops[self.desktop];
        let previewed = self.previewed.as_ref();
        laid_out(desktop, self.app.as_ref())
            .filter(|(window, _)| Some(*window) == previewed)
            .chain(laid_out(desktop, self.app.as_ref()).rev())
            .find_map(|(window, frame)| {
                let (loc, factor) = self.drawn(window, frame);
                Rectangle::new(loc, frame.size.to_f64().upscale(factor))
                    .contains(location)
                    .then_some((window, loc, factor))
            })
    }

    pub fn at(&self, now: Duration) -> (f64, f64) {
        sample_motion(
            &self.motion,
            OMEGA,
            DAMPING,
            self.progress,
            f64::from(self.open),
            now,
        )
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
        self.settle = Some(Motion::new(1.0, 0.0, now));
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
        self.advance_flight(now);
        let expansion = &mut self.expansion;
        if let Some(ghost) = &mut expansion.ghost {
            let elapsed = now.saturating_sub(ghost.since).as_secs_f64() * 1000.0;
            ghost.opacity = (-elapsed / LABEL_FADE_MS).exp();
            if ghost.opacity < 0.5 / 255.0 {
                expansion.ghost = None;
            }
            self.needs_redraw = true;
        }
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
        if let Some(previewing) = advance_motion(
            &mut expansion.preview,
            OMEGA,
            DAMPING,
            f64::from(expansion.enlarged),
            now,
            longest_side,
        ) {
            expansion.previewing = previewing;
            self.needs_redraw |= expansion.preview.is_some();
            if expansion.preview.is_none() && !expansion.enlarged {
                expansion.previewed = None;
            }
        }
        let target = f64::from(expansion.open);
        let (omega, damping) = expansion.strip_spring();
        if let Some(strip_in) = advance_motion(
            &mut expansion.strip_motion,
            omega,
            damping,
            target,
            now,
            longest_side,
        ) {
            expansion.strip_in = strip_in;
            self.needs_redraw |= expansion.strip_motion.is_some();
        }
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
            expansion.strip_in = 0.0;
            expansion.strip_motion = None;
            expansion.app = None;
            if self.slide.is_none() {
                self.suspend_hidden_desktops(now);
            }
        }
    }

    fn advance_flight(&mut self, now: Duration) {
        let expansion = &self.expansion;
        let Some(flight) = &expansion.flight else {
            return;
        };
        if flight.motion.is_none() {
            if !(flight.window.alive() && waiting(&flight.window)) {
                self.expansion.flight = None;
            }
            return;
        }
        let stride = self
            .output_geometry()
            .zip(holding(&self.desktops, &flight.window))
            .and_then(|(output, (_, frame))| {
                let to = expansion.landing(&self.desktops, flight, frame, output.size.to_f64())?;
                Some(travel(flight.from, to, frame.size))
            });
        let Some(flight) = &mut self.expansion.flight else {
            return;
        };
        if stride.is_none() && !matches!(flight.landing, Landing::Slot) {
            flight.landing = Landing::Slot;
            self.needs_redraw = true;
            return;
        }
        let flown = stride.and_then(|stride| {
            advance_motion(
                &mut flight.motion,
                DROP_OMEGA,
                DROP_DAMPING,
                1.0,
                now,
                stride,
            )
        });
        match flown {
            Some(flown) if flight.motion.is_some() => {
                flight.flown = flown;
                self.needs_redraw = true;
            }
            _ if matches!(flight.landing, Landing::Split(..)) => {
                flight.motion = None;
                flight.flown = 1.0;
                self.needs_redraw = true;
                if let Some(flight) = &self.expansion.flight {
                    self.split_flight(flight);
                }
            }
            _ => {
                flight.motion = None;
                self.land_flight();
            }
        }
    }

    fn land_flight(&mut self) {
        let Some(flight) = self.expansion.flight.take() else {
            return;
        };
        let window = &flight.window;
        let Some((origin, frame)) =
            holding(&self.desktops, window).filter(|_| window.alive() && !is_full_screen(window))
        else {
            return;
        };
        self.needs_redraw = true;
        match flight.landing {
            Landing::Slot => {
                let Some(output) = self.output_geometry() else {
                    return;
                };
                if flight.motion.is_some()
                    && let Some(slot) = window.user_data().get::<Slot>()
                    && let Some(drawn) =
                        self.expansion
                            .flown(&self.desktops, &flight, frame, output.size.to_f64())
                {
                    self.expansion
                        .unsettle(&self.desktops, self.start_time.elapsed());
                    self.expansion.settle_from(slot, frame.loc.to_f64(), drawn);
                }
            }
            Landing::Desktop(id) => {
                let Some(target) = entry_of(&self.desktops, id).filter(|&target| target != origin)
                else {
                    return;
                };
                window.set_activated(false);
                send_changed_state(window);
                let [from, to] = self.desktops.get_disjoint_mut([origin, target]).unwrap();
                move_window(window, from, to);
                self.rearrange_expansion();
            }
            Landing::Split(..) => {
                if flight.motion.is_some() {
                    self.split_flight(&flight);
                }
            }
        }
    }

    fn split_flight(&self, flight: &Flight) {
        if let Landing::Split(id, side, _) = flight.landing
            && flight.window.alive()
            && !is_full_screen(&flight.window)
            && let Some(index) = entry_of(&self.desktops, id)
        {
            self.split_with(&flight.window, index, side);
        }
    }

    fn aim_expansion(&mut self, open: bool) {
        let now = self.start_time.elapsed();
        let (from, velocity) = self.expansion.at(now);
        let (strip_from, strip_velocity) = self.expansion.strip_at(now);
        let entering = open
            && !self.expansion.held
            && self.expansion.motion.is_none()
            && self.expansion.progress == 0.0;
        let strip_start = if entering {
            now + Duration::from_millis(ctdesign::SPACES_STRIP_ENTRY_DELAY_MS)
        } else {
            now
        };
        self.expansion.open = open;
        self.expansion.held = false;
        self.expansion.motion = Some(Motion::new(from, velocity, now));
        self.expansion.strip_motion = Some(Motion::new(strip_from, strip_velocity, strip_start));
        self.needs_redraw = true;
    }

    pub fn hold_expansion(&mut self, progress: f64) {
        self.expansion.progress = progress;
        self.expansion.motion = None;
        self.expansion.strip_in = progress;
        self.expansion.strip_motion = None;
        self.expansion.held = true;
        self.needs_redraw = true;
    }

    pub fn release_expansion(&mut self, open: bool, velocity: f64, time: InputTime) {
        if open {
            self.aim_expansion(true);
        } else {
            self.close_expansion(None, time);
        }
        let expansion = &mut self.expansion;
        for motion in [&mut expansion.motion, &mut expansion.strip_motion]
            .into_iter()
            .flatten()
        {
            motion.velocity = velocity;
        }
    }

    pub fn open_expansion(&mut self, app: Option<Window>, time: InputTime) {
        self.expansion.app = app;
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
        if self.expansion.app.is_some() {
            return None;
        }
        let top = self.expansion.strip_top()?;
        let output = self.output_geometry()?;
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
        self.land_flight();
        match self.strip_hit(origin) {
            None => {
                let Some((window, top_left, factor)) = self
                    .expansion
                    .window_at(&self.desktops, origin)
                    .filter(|(window, ..)| !is_full_screen(window))
                    .map(|(window, top_left, factor)| (window.clone(), top_left, factor))
                else {
                    return;
                };
                if self.expansion.app.is_none() && !self.expansion.strip.grown {
                    self.expansion.strip.grow(self.start_time.elapsed());
                    self.wake_desktops();
                    self.rearrange_expansion();
                }
                let start_scale = self
                    .expansion
                    .slot_of(&window)
                    .map_or(1.0, |slot| slot.loc.get().1);
                let size = window.geometry().size.to_f64().upscale(factor);
                let grip = location - top_left;
                self.expansion.dragged = Some(Dragged {
                    window,
                    grip: Point::from((grip.x / size.w, grip.y / size.h)),
                    start_y: location.y,
                    start_scale,
                    split: None,
                });
            }
            Some(Hit::Thumbnail(index)) => {
                if let Some((point, output)) = self.on_strip(origin) {
                    let strip = &mut self.expansion.strip;
                    let left = strip.thumbnail(index, self.desktops.len(), output).loc.x;
                    strip.moving = Some((index, point.x - left));
                    strip.opening = None;
                }
            }
            Some(_) => {}
        }
    }

    fn split_target(
        &self,
        window: &Window,
        location: Point<f64, Logical>,
    ) -> Option<(usize, Direction, f64)> {
        let (point, output) = self.on_strip(location)?;
        let strip = &self.expansion.strip;
        if point.y >= strip.height() {
            return None;
        }
        let count = self.desktops.len();
        let (index, thumbnail) = (0..count)
            .filter(|&index| !self.is_desktop(index))
            .map(|index| (index, strip.thumbnail(index, count, output)))
            .find(|(_, thumbnail)| {
                (thumbnail.loc.x..thumbnail.loc.x + thumbnail.size.w).contains(&point.x)
            })?;
        let side = if point.x < thumbnail.loc.x + thumbnail.size.w / 2.0 {
            Direction::Left
        } else {
            Direction::Right
        };
        let [(_, _, left), (_, _, right)] = self.joined_tiles(window, index, side)?;
        Some((index, side, f64::from(left) / f64::from(left + right)))
    }

    pub fn hover_expansion(&mut self, location: Point<f64, Logical>) {
        self.expansion
            .dragged
            .take_if(|dragged| !dragged.window.alive());
        self.drag_in_expansion(location);
        let now = self.start_time.elapsed();
        let split = self
            .expansion
            .dragged
            .as_ref()
            .and_then(|dragged| self.split_target(&dragged.window, location));
        if let Some(dragged) = &mut self.expansion.dragged {
            if let Some((index, side, _)) = dragged.split
                && split.is_none_or(|(next, ..)| next != index)
                && let Some(app) = self.desktops.get(index).and_then(full_screen_window)
            {
                self.expansion.ghost = Some(Ghost {
                    window: dragged.window.clone(),
                    app: app.clone(),
                    side,
                    since: now,
                    opacity: 1.0,
                });
            }
            dragged.split = split;
        }
        self.needs_redraw = true;
        let Some((point, output)) = self.on_strip(location) else {
            return;
        };
        let was_grown = self.expansion.strip.grown;
        let was_hovered = self.expansion.strip.hovered;
        let addable = self.new_desktop_index().is_some();
        self.expansion
            .strip
            .hover(point, self.desktops.len(), addable, output, now);
        if self.expansion.dragged.is_some()
            || self.expansion.strip.moving.is_some()
            || self
                .expansion
                .strip
                .hovered
                .is_some_and(|index| !self.is_desktop(index) || self.heir(index).is_none())
        {
            self.expansion.strip.hovered = None;
        }
        if self.expansion.strip.hovered != was_hovered {
            self.delay_close(now);
        }
        if !was_grown && self.expansion.strip.grown {
            self.wake_desktops();
            self.rearrange_expansion();
        }
    }

    fn delay_close(&mut self, now: Duration) {
        let strip = &mut self.expansion.strip;
        strip.closable = false;
        strip.hovered_since = now;
        if strip.hovered.is_none() {
            return;
        }
        let delay = Duration::from_millis(ctdesign::SPACES_CLOSE_DELAY_MS);
        if let Err(error) =
            self.loop_handle
                .insert_source(Timer::from_duration(delay), move |_, _, state| {
                    let strip = &mut state.expansion.strip;
                    if strip.hovered_since == now {
                        strip.closable = true;
                        state.needs_redraw = true;
                    }
                    TimeoutAction::Drop
                })
        {
            tracing::error!(%error, "close timer failed");
        }
    }

    pub fn rearrange_expansion(&mut self) {
        self.expansion.clear_preview();
        self.expansion.frames.clear();
        self.arrange_expansion();
    }

    pub fn close_expansion(&mut self, picked: Option<Window>, time: InputTime) {
        let location = self.seat.get_pointer().unwrap().current_location();
        if let Some(flight) = self
            .expansion
            .dragged
            .take()
            .and_then(|dragged| self.take_off(dragged, location))
        {
            self.expansion.flight = Some(flight);
        }
        self.land_flight();
        self.expansion.ghost = None;
        self.expansion.pressed = false;
        self.expansion.drag_origin = None;
        self.expansion.strip.moving = None;
        self.expansion.clear_preview();
        self.aim_expansion(false);
        let focused = picked.or_else(|| self.top_window());
        if let Some(window) = focused {
            self.focus_window(&window, SERIAL_COUNTER.next_serial());
        }
        self.move_pointer(location, time);
    }

    pub fn toggle_preview(&mut self) {
        if self.expansion.pressed
            || self.expansion.dragged.is_some()
            || self.expansion.flight.is_some()
        {
            return;
        }
        let now = self.start_time.elapsed();
        let location = self.seat.get_pointer().unwrap().current_location();
        let expansion = &mut self.expansion;
        let (mut from, mut velocity) = expansion.preview_at(now);
        if expansion.enlarged {
            expansion.enlarged = false;
        } else {
            let desktop = &self.desktops[expansion.desktop];
            let Some(output) = desktop.output_geometry(&self.output) else {
                return;
            };
            let Some((window, frame, slot)) = expansion
                .window_at(&self.desktops, location)
                .and_then(|(window, ..)| {
                    let frame = desktop.element_geometry(window)?;
                    let slot = expansion.slot_of(window)?;
                    Some((window, frame, slot))
                })
            else {
                return;
            };
            if expansion.previewed.as_ref() != Some(window) {
                (from, velocity) = (0.0, 0.0);
            }
            let (loc, scale) = expansion.aimed(slot, frame.loc.to_f64());
            let size = frame.size.to_f64();
            let target = preview(
                expansion.area(output),
                Rectangle::new(frame.loc.to_f64(), size),
                Rectangle::new(loc, size.upscale(scale)),
            );
            expansion.previewed = Some(window.clone());
            expansion.preview_target = target;
            expansion.enlarged = true;
        }
        expansion.preview = Some(Motion::new(from, velocity, now));
        self.needs_redraw = true;
    }

    pub fn pick_in_expansion(&mut self, time: InputTime) {
        self.land_flight();
        let location = self.seat.get_pointer().unwrap().current_location();
        if self.expansion.strip.moving.is_some() {
            let moved = self.on_strip(location).and_then(|(point, output)| {
                self.expansion
                    .strip
                    .moved(point.x, self.desktops.len(), output)
            });
            self.expansion.strip.moving = None;
            if let Some((from, to, _)) = moved {
                self.move_entry(from, to);
            }
            self.hover_expansion(location);
            return;
        }
        let hit = self.strip_hit(location);
        if let Some(dragged) = self.expansion.dragged.take() {
            self.drop_window(dragged, location, hit);
            self.hover_expansion(location);
            self.needs_redraw = true;
            return;
        }
        match hit {
            Some(Hit::Thumbnail(index)) => {
                self.go_to_desktop(index, time);
                if !self.seat.get_keyboard().unwrap().modifier_state().alt {
                    self.close_expansion(None, time);
                }
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
                    .map(|(window, ..)| window.clone()),
                time,
            ),
        }
    }

    fn take_off(&self, dragged: Dragged, location: Point<f64, Logical>) -> Option<Flight> {
        let frame = self.desktops[self.expansion.desktop]
            .element_geometry(&dragged.window)
            .filter(|_| dragged.window.alive())?;
        Some(Flight {
            from: self.expansion.held_drawn(&dragged, frame.size, location),
            window: dragged.window,
            landing: Landing::Slot,
            flown: 0.0,
            motion: Some(Motion::new(0.0, 0.0, self.start_time.elapsed())),
        })
    }

    fn drop_window(&mut self, dragged: Dragged, location: Point<f64, Logical>, hit: Option<Hit>) {
        let Some(mut flight) = self.take_off(dragged, location) else {
            return;
        };
        let landing = if let Some((index, side, left)) = self.split_target(&flight.window, location)
        {
            Landing::Split(self.desktops[index].id(), side, left)
        } else {
            match hit {
                Some(Hit::Thumbnail(index))
                    if index != self.expansion.desktop && self.is_desktop(index) =>
                {
                    Landing::Desktop(self.desktops[index].id())
                }
                Some(Hit::Add) => self.add_desktop().map_or(Landing::Slot, |index| {
                    Landing::Desktop(self.desktops[index].id())
                }),
                _ => Landing::Slot,
            }
        };
        flight.landing = landing;
        self.expansion.flight = Some(flight);
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
        if laid_out(desktop, expansion.app.as_ref())
            .map(|(_, frame)| frame.to_f64())
            .eq(expansion.frames.iter().copied())
        {
            return;
        }
        expansion.clear_preview();
        expansion.frames.clear();
        expansion
            .frames
            .extend(laid_out(desktop, expansion.app.as_ref()).map(|(_, frame)| frame.to_f64()));
        let area = expansion.area(output);
        let scale = if expansion.app.is_some() {
            grid(area, &expansion.frames, &mut expansion.slots)
        } else {
            arrange(
                area,
                &expansion.frames,
                &mut expansion.slots,
                &mut expansion.pushes,
            )
        };
        if settling {
            expansion.unsettle(&self.desktops, self.start_time.elapsed());
        }
        let before_growth = !expansion.strip.grown;
        for ((window, frame), slot) in
            laid_out(desktop, expansion.app.as_ref()).zip(&expansion.slots)
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_window_shrinks_linearly_with_the_pointers_rise_to_the_floor_at_the_strips_bottom_and_never_past_its_start_scale()
     {
        let floor = 2400.0 * DRAG_FLOOR_SCALE;
        for (start_y, y, width) in [
            (840.0, 1200.0, 1020.0),
            (840.0, 840.0, 1020.0),
            (840.0, 800.0, 972.0),
            (840.0, 600.0, 728.0),
            (840.0, 400.0, 486.0),
            (840.0, 200.0, 242.0),
            (840.0, EXPANDED_HEIGHT, floor),
            (840.0, -10.0, floor),
            (EXPANDED_HEIGHT - 20.0, EXPANDED_HEIGHT - 40.0, floor),
            (EXPANDED_HEIGHT - 20.0, EXPANDED_HEIGHT - 10.0, 1020.0),
        ] {
            let drawn = 2400.0 * held_scale(y, start_y, 1020.0 / 2400.0);
            assert!((drawn - width).abs() < 1.5, "{start_y} {y} {drawn}");
        }
    }
}
