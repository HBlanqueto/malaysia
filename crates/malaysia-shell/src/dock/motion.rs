use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use bk::platform::Timers;
use bk::prelude::*;
use bk::shell::ToplevelInfo;
use ctdesign::{DOCK_MINIMIZE_MS, DOCK_SLOT_MS};

const SLOT: Duration = Duration::from_millis(DOCK_SLOT_MS);
const FRAME: Duration = Duration::from_millis(16);

// A minimized window's tile: its slot opens while the genie flows in and closes while it flows out,
// so the Dock keeps the tile of a window that just left until its slot is shut.
pub(crate) struct Motion<T> {
    tiles: Vec<Tile<T>>,
    started: bool,
}

struct Tile<T> {
    id: u64,
    window: T,
    from: f32,
    to: f32,
    since: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown<T> {
    pub window: T,
    pub presence: f32,
    // The picture replaces the genie's last frames only once the effect is over.
    pub arrived: bool,
}

impl<T> Default for Motion<T> {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            started: false,
        }
    }
}

fn smooth(share: f32) -> f32 {
    share * share * (3.0 - 2.0 * share)
}

impl<T> Tile<T> {
    fn presence(&self, now: Instant) -> f32 {
        let share = now.saturating_duration_since(self.since).as_secs_f32() / SLOT.as_secs_f32();
        self.from + (self.to - self.from) * smooth(share.clamp(0.0, 1.0))
    }

    fn retarget(&mut self, to: f32, now: Instant) {
        if self.to != to {
            self.from = self.presence(now);
            self.to = to;
            self.since = now;
        }
    }
}

impl<T: Clone> Motion<T> {
    // `minimized` are the windows that have a tile now. The first call finds the Dock already
    // running with tiles, which sit there without opening.
    pub(crate) fn update(&mut self, minimized: &[(u64, T)], now: Instant) {
        let settled = !self.started;
        self.started = true;
        for tile in &mut self.tiles {
            let present = minimized.iter().find(|(id, _)| *id == tile.id);
            tile.retarget(if present.is_some() { 1.0 } else { 0.0 }, now);
            if let Some((_, window)) = present {
                tile.window = window.clone();
            }
        }
        for (id, window) in minimized {
            if self.tiles.iter().all(|tile| tile.id != *id) {
                self.tiles.push(Tile {
                    id: *id,
                    window: window.clone(),
                    from: if settled { 1.0 } else { 0.0 },
                    to: 1.0,
                    since: now,
                });
            }
        }
        self.tiles
            .retain(|tile| tile.to > 0.0 || tile.presence(now) > 0.0);
    }

    pub(crate) fn shown(&self, now: Instant) -> Vec<Shown<T>> {
        self.tiles
            .iter()
            .map(|tile| Shown {
                window: tile.window.clone(),
                presence: tile.presence(now),
                arrived: tile.to == 1.0
                    && (tile.from == 1.0
                        || now.saturating_duration_since(tile.since)
                            >= Duration::from_millis(DOCK_MINIMIZE_MS)),
            })
            .collect()
    }

    pub(crate) fn moving(&self, now: Instant) -> bool {
        let longest = SLOT.max(Duration::from_millis(DOCK_MINIMIZE_MS));
        self.tiles
            .iter()
            .any(|tile| now.saturating_duration_since(tile.since) < longest && tile.from != tile.to)
    }
}

// Re-renders the Dock every frame while a slot opens or closes.
pub(crate) fn use_tiles(windows: &[ToplevelInfo], into_app: bool) -> Vec<Shown<ToplevelInfo>> {
    let frame = use_state(|| 0u64);
    let motion = use_hook(|| Rc::new(RefCell::new(Motion::<ToplevelInfo>::default())));
    let scheduled = use_hook(|| Rc::new(Cell::new(false)));
    let _ = *frame.read();
    let now = Instant::now();
    let minimized: Vec<(u64, ToplevelInfo)> = windows
        .iter()
        .filter(|window| window.minimized && !into_app)
        .map(|window| (window.id, window.clone()))
        .collect();
    let mut motion = motion.borrow_mut();
    motion.update(&minimized, now);
    if motion.moving(now) && !scheduled.replace(true) {
        Timers::get().after(FRAME, move || {
            scheduled.set(false);
            let mut frame = frame;
            *frame.write() += 1;
        });
    }
    motion.shown(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    fn presence(motion: &Motion<&str>, now: Instant) -> Vec<f32> {
        motion.shown(now).iter().map(|tile| tile.presence).collect()
    }

    #[test]
    fn tiles_already_there_when_the_dock_starts_do_not_open() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.update(&[(1, "a")], start);
        assert_eq!(presence(&motion, start), [1.0]);
        assert!(motion.shown(start)[0].arrived);
        assert!(!motion.moving(at(start, 1)));
    }

    #[test]
    fn a_new_tile_opens_its_slot_and_only_then_shows_its_picture() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.update(&[], start);
        motion.update(&[(1, "a")], at(start, 10));
        assert_eq!(presence(&motion, at(start, 10)), [0.0]);
        let middle = presence(&motion, at(start, 10 + DOCK_SLOT_MS / 2))[0];
        assert!(middle > 0.0 && middle < 1.0);
        assert_eq!(presence(&motion, at(start, 10 + DOCK_SLOT_MS)), [1.0]);
        assert!(!motion.shown(at(start, 10 + DOCK_SLOT_MS))[0].arrived);
        assert!(motion.shown(at(start, 10 + DOCK_MINIMIZE_MS))[0].arrived);
    }

    #[test]
    fn a_tile_that_leaves_stays_until_its_slot_is_shut_and_never_shows_its_picture() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.update(&[(1, "a")], start);
        motion.update(&[], at(start, 100));
        assert_eq!(presence(&motion, at(start, 100)), [1.0]);
        assert!(!motion.shown(at(start, 100))[0].arrived);
        motion.update(&[], at(start, 100 + DOCK_SLOT_MS / 2));
        assert_eq!(motion.shown(at(start, 100 + DOCK_SLOT_MS / 2)).len(), 1);
        motion.update(&[], at(start, 100 + DOCK_SLOT_MS));
        assert!(motion.shown(at(start, 100 + DOCK_SLOT_MS)).is_empty());
    }

    #[test]
    fn a_window_restored_and_minimized_again_does_not_jump() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.update(&[(1, "a")], start);
        motion.update(&[], at(start, 100));
        let before = presence(&motion, at(start, 100 + DOCK_SLOT_MS / 2))[0];
        motion.update(&[(1, "a")], at(start, 100 + DOCK_SLOT_MS / 2));
        let after = presence(&motion, at(start, 100 + DOCK_SLOT_MS / 2))[0];
        assert!((before - after).abs() < 1e-6, "{before} {after}");
    }

    #[test]
    fn tiles_keep_the_order_in_which_they_came() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.update(&[(2, "b")], start);
        motion.update(&[(2, "b"), (1, "a")], at(start, 5));
        let order: Vec<_> = motion
            .shown(at(start, 5))
            .iter()
            .map(|tile| tile.window)
            .collect();
        assert_eq!(order, ["b", "a"]);
    }
}
