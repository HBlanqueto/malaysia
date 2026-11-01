//! Spaces: the row of desktops and focus spaces the user slides between.
//!
//! A space is either a **desktop**, where windows float as usual, or a
//! **focus space**, created when a window is maximized: the window gets a space
//! of its own, inserted just right of the desktop it came from, the way macOS
//! full screen does. See ADR 0051.
//!
//! This module only keeps the bookkeeping — which spaces exist, in which order,
//! which one is shown and where each window lives. Drawing, animation and the
//! protocol state of the windows belong to the compositor.

use crate::scene::WindowId;

/// The most desktops the row can hold. Focus spaces do not count.
pub const MAX_DESKTOPS: usize = 99;

/// Identifies one space for as long as it exists.
///
/// Ids are never reused, so an id kept across a change still names the same
/// space or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(u64);

impl SpaceId {
    /// The number behind this id.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// What a space is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceKind {
    /// A desktop.
    Desktop,
    /// The focus space of a maximized window.
    Focus {
        /// The maximized window.
        owner: WindowId,
        /// The desktop the window returns to when it leaves focus.
        home: SpaceId,
    },
}

/// One space in the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    /// The space's id.
    pub id: SpaceId,
    /// Whether it is a desktop or a focus space.
    pub kind: SpaceKind,
}

/// Why a change to the row was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacesError {
    /// The row already has [`MAX_DESKTOPS`] desktops.
    Full,
    /// The window is not tracked.
    UnknownWindow,
    /// The window already has a focus space.
    AlreadyFocused,
    /// The window has no focus space.
    NotFocused,
}

/// Where a newly mapped window was put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assigned {
    /// The space the window now lives on.
    pub space: SpaceId,
    /// Whether that space is not the one shown, so the caller should switch to
    /// it (a new window opened from a focus space goes to its desktop).
    pub needs_switch: bool,
}

/// The row of spaces, left to right, and which one is shown.
#[derive(Debug, Clone)]
pub struct Spaces {
    list: Vec<Space>,
    current: usize,
    next_id: u64,
    /// Every tracked window and the space it lives on.
    windows: Vec<(WindowId, SpaceId)>,
}

impl Default for Spaces {
    fn default() -> Self {
        Self::new()
    }
}

impl Spaces {
    /// A row with one desktop, shown.
    pub fn new() -> Self {
        Self {
            list: vec![Space {
                id: SpaceId(0),
                kind: SpaceKind::Desktop,
            }],
            current: 0,
            next_id: 1,
            windows: Vec::new(),
        }
    }

    /// How many spaces the row has, desktops and focus spaces.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Always false: the row keeps at least one desktop.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The spaces, left to right.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Space> + '_ {
        self.list.iter()
    }

    /// How many desktops the row has.
    pub fn desktop_count(&self) -> usize {
        self.list
            .iter()
            .filter(|space| space.kind == SpaceKind::Desktop)
            .count()
    }

    /// The position of the shown space.
    pub fn current_index(&self) -> usize {
        self.current
    }

    /// The shown space.
    pub fn current(&self) -> Space {
        self.list[self.current]
    }

    /// The position of a space in the row.
    pub fn index_of(&self, id: SpaceId) -> Option<usize> {
        self.list.iter().position(|space| space.id == id)
    }

    /// The space with `id`.
    pub fn get(&self, id: SpaceId) -> Option<Space> {
        self.list.iter().copied().find(|space| space.id == id)
    }

    /// The 1-based number of a desktop among the desktops, as the user sees it
    /// («Escritorio 3»). `None` for a focus space or an unknown id.
    pub fn desktop_number(&self, id: SpaceId) -> Option<usize> {
        let mut number = 0;
        for space in &self.list {
            if space.kind == SpaceKind::Desktop {
                number += 1;
                if space.id == id {
                    return Some(number);
                }
            }
        }
        None
    }

    /// Appends a desktop at the right end of the row. The shown space does
    /// not change.
    pub fn add_desktop(&mut self) -> Result<SpaceId, SpacesError> {
        if self.desktop_count() >= MAX_DESKTOPS {
            return Err(SpacesError::Full);
        }
        let id = self.fresh_id();
        self.list.push(Space {
            id,
            kind: SpaceKind::Desktop,
        });
        Ok(id)
    }

    /// Shows the space at `index`, returning whether the shown space changed.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.list.len() || index == self.current {
            return false;
        }
        self.current = index;
        true
    }

    /// Shows the space to the right, if any. The row does not wrap.
    pub fn show_next(&mut self) -> bool {
        self.switch_to(self.current + 1)
    }

    /// Shows the space to the left, if any. The row does not wrap.
    pub fn show_previous(&mut self) -> bool {
        self.current > 0 && self.switch_to(self.current - 1)
    }

    /// Starts tracking a newly mapped window.
    ///
    /// A window with a parent (a dialog) lives with its parent. Any other
    /// window goes to the shown desktop; if a focus space is shown, it goes to
    /// that space's desktop instead, because a focus space belongs to one
    /// window.
    pub fn assign(&mut self, window: WindowId, parent: Option<WindowId>) -> Assigned {
        if let Some(space) = self.space_of(window) {
            return Assigned {
                space,
                needs_switch: space != self.current().id,
            };
        }
        let current = self.current();
        let space = parent
            .and_then(|parent| self.space_of(parent))
            .unwrap_or(match current.kind {
                SpaceKind::Desktop => current.id,
                SpaceKind::Focus { home, .. } => home,
            });
        self.windows.push((window, space));
        Assigned {
            space,
            needs_switch: space != current.id,
        }
    }

    /// Stops tracking a window. If it owned a focus space, that space is
    /// removed and any window left on it (a dialog) goes back to the desktop.
    pub fn remove_window(&mut self, window: WindowId) -> bool {
        let before = self.windows.len();
        if self.focus_space_of(window).is_some() {
            let _ = self.exit_focus(window);
        }
        self.windows.retain(|(entry, _)| *entry != window);
        self.windows.len() != before
    }

    /// The space a window lives on.
    pub fn space_of(&self, window: WindowId) -> Option<SpaceId> {
        self.windows
            .iter()
            .find(|(entry, _)| *entry == window)
            .map(|(_, space)| *space)
    }

    /// Whether a window lives on the shown space. Untracked windows (not yet
    /// mapped) count as shown, so nothing hides a window before it is placed.
    pub fn is_shown(&self, window: WindowId) -> bool {
        self.space_of(window)
            .is_none_or(|space| space == self.current().id)
    }

    /// The focus space a window owns, if it is maximized.
    pub fn focus_space_of(&self, window: WindowId) -> Option<SpaceId> {
        self.list.iter().find_map(|space| match space.kind {
            SpaceKind::Focus { owner, .. } if owner == window => Some(space.id),
            _ => None,
        })
    }

    /// Creates a focus space for `window` just right of its desktop and moves
    /// the window there. Returns the new space's position; the caller decides
    /// when to show it (the slide and the grow are one animation).
    pub fn enter_focus(&mut self, window: WindowId) -> Result<usize, SpacesError> {
        let space = self.space_of(window).ok_or(SpacesError::UnknownWindow)?;
        if self.focus_space_of(window).is_some() {
            return Err(SpacesError::AlreadyFocused);
        }
        // A dialog inside a focus space has that space's desktop as home.
        let home = match self.get(space).map(|space| space.kind) {
            Some(SpaceKind::Focus { home, .. }) => home,
            _ => space,
        };
        let home_index = self.index_of(home).ok_or(SpacesError::UnknownWindow)?;
        let id = self.fresh_id();
        let index = home_index + 1;
        self.list.insert(
            index,
            Space {
                id,
                kind: SpaceKind::Focus {
                    owner: window,
                    home,
                },
            },
        );
        if self.current >= index {
            self.current += 1;
        }
        self.move_window(window, id);
        Ok(index)
    }

    /// Removes a window's focus space and moves every window on it back to
    /// its desktop. If the focus space was shown, the desktop is shown.
    /// Returns the desktop's position.
    pub fn exit_focus(&mut self, window: WindowId) -> Result<usize, SpacesError> {
        let id = self.focus_space_of(window).ok_or(SpacesError::NotFocused)?;
        let index = self.index_of(id).expect("focus space is in the row");
        let SpaceKind::Focus { home, .. } = self.list[index].kind else {
            unreachable!("focus_space_of returns focus spaces");
        };
        for (_, space) in self.windows.iter_mut() {
            if *space == id {
                *space = home;
            }
        }
        let was_shown = self.current == index;
        self.list.remove(index);
        let home_index = self.index_of(home).expect("a focus space's home exists");
        if was_shown {
            self.current = home_index;
        } else if self.current > index {
            self.current -= 1;
        }
        Ok(home_index)
    }

    fn move_window(&mut self, window: WindowId, space: SpaceId) {
        if let Some(entry) = self.windows.iter_mut().find(|(entry, _)| *entry == window) {
            entry.1 = space;
        }
    }

    fn fresh_id(&mut self) -> SpaceId {
        let id = SpaceId(self.next_id);
        self.next_id += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(n: u64) -> WindowId {
        WindowId::new(n)
    }

    #[test]
    fn starts_with_one_shown_desktop() {
        let spaces = Spaces::new();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces.current_index(), 0);
        assert_eq!(spaces.current().kind, SpaceKind::Desktop);
        assert_eq!(spaces.desktop_number(spaces.current().id), Some(1));
    }

    #[test]
    fn new_desktops_go_to_the_right_without_switching() {
        let mut spaces = Spaces::new();
        let second = spaces.add_desktop().unwrap();
        assert_eq!(spaces.index_of(second), Some(1));
        assert_eq!(spaces.current_index(), 0);
        assert_eq!(spaces.desktop_number(second), Some(2));
    }

    #[test]
    fn the_row_stops_at_ninety_nine_desktops() {
        let mut spaces = Spaces::new();
        for _ in 1..MAX_DESKTOPS {
            spaces.add_desktop().unwrap();
        }
        assert_eq!(spaces.desktop_count(), MAX_DESKTOPS);
        assert_eq!(spaces.add_desktop(), Err(SpacesError::Full));
    }

    #[test]
    fn focus_spaces_do_not_count_toward_the_limit() {
        let mut spaces = Spaces::new();
        spaces.assign(w(1), None);
        spaces.enter_focus(w(1)).unwrap();
        for _ in 1..MAX_DESKTOPS {
            spaces.add_desktop().unwrap();
        }
        assert_eq!(spaces.len(), MAX_DESKTOPS + 1);
    }

    #[test]
    fn switching_does_not_wrap() {
        let mut spaces = Spaces::new();
        spaces.add_desktop().unwrap();
        assert!(!spaces.show_previous());
        assert!(spaces.show_next());
        assert!(!spaces.show_next());
        assert_eq!(spaces.current_index(), 1);
        assert!(spaces.show_previous());
        assert_eq!(spaces.current_index(), 0);
    }

    #[test]
    fn windows_open_on_the_shown_desktop() {
        let mut spaces = Spaces::new();
        let second = spaces.add_desktop().unwrap();
        spaces.show_next();
        let assigned = spaces.assign(w(1), None);
        assert_eq!(assigned.space, second);
        assert!(!assigned.needs_switch);
        assert!(spaces.is_shown(w(1)));
        spaces.show_previous();
        assert!(!spaces.is_shown(w(1)));
    }

    #[test]
    fn untracked_windows_count_as_shown() {
        let spaces = Spaces::new();
        assert!(spaces.is_shown(w(7)));
    }

    #[test]
    fn dialogs_follow_their_parent() {
        let mut spaces = Spaces::new();
        spaces.add_desktop().unwrap();
        spaces.assign(w(1), None);
        spaces.show_next();
        let assigned = spaces.assign(w(2), Some(w(1)));
        assert_eq!(assigned.space, spaces.space_of(w(1)).unwrap());
        assert!(assigned.needs_switch);
    }

    #[test]
    fn focus_space_goes_right_of_its_desktop() {
        let mut spaces = Spaces::new();
        let first = spaces.current().id;
        let second = spaces.add_desktop().unwrap();
        spaces.assign(w(1), None);
        let index = spaces.enter_focus(w(1)).unwrap();
        assert_eq!(index, 1);
        assert_eq!(spaces.index_of(second), Some(2));
        let focus = spaces.focus_space_of(w(1)).unwrap();
        assert_eq!(spaces.space_of(w(1)), Some(focus));
        assert_eq!(
            spaces.get(focus).unwrap().kind,
            SpaceKind::Focus {
                owner: w(1),
                home: first
            }
        );
        assert_eq!(spaces.desktop_number(second), Some(2));
        assert_eq!(spaces.desktop_number(focus), None);
        assert_eq!(spaces.enter_focus(w(1)), Err(SpacesError::AlreadyFocused));
    }

    #[test]
    fn inserting_left_of_the_shown_space_keeps_it_shown() {
        let mut spaces = Spaces::new();
        spaces.assign(w(1), None);
        let second = spaces.add_desktop().unwrap();
        spaces.show_next();
        spaces.enter_focus(w(1)).unwrap();
        assert_eq!(spaces.current().id, second);
    }

    #[test]
    fn exit_focus_returns_to_the_desktop() {
        let mut spaces = Spaces::new();
        let first = spaces.current().id;
        spaces.assign(w(1), None);
        let index = spaces.enter_focus(w(1)).unwrap();
        spaces.switch_to(index);
        spaces.assign(w(2), Some(w(1)));
        assert_eq!(spaces.exit_focus(w(1)), Ok(0));
        assert_eq!(spaces.current().id, first);
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces.space_of(w(1)), Some(first));
        assert_eq!(spaces.space_of(w(2)), Some(first));
        assert_eq!(spaces.exit_focus(w(1)), Err(SpacesError::NotFocused));
    }

    #[test]
    fn a_window_opened_from_a_focus_space_goes_to_its_desktop() {
        let mut spaces = Spaces::new();
        let first = spaces.current().id;
        spaces.assign(w(1), None);
        let index = spaces.enter_focus(w(1)).unwrap();
        spaces.switch_to(index);
        let assigned = spaces.assign(w(2), None);
        assert_eq!(assigned.space, first);
        assert!(assigned.needs_switch);
    }

    #[test]
    fn closing_a_focused_window_removes_its_space() {
        let mut spaces = Spaces::new();
        spaces.assign(w(1), None);
        spaces.add_desktop().unwrap();
        let index = spaces.enter_focus(w(1)).unwrap();
        spaces.switch_to(index + 1);
        assert!(spaces.remove_window(w(1)));
        assert_eq!(spaces.len(), 2);
        assert_eq!(spaces.current_index(), 1);
        assert_eq!(spaces.current().kind, SpaceKind::Desktop);
    }
}
