use crate::scene::WindowId;

pub const MAX_DESKTOPS: usize = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(u64);

impl SpaceId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceKind {
    Desktop,
    Focus { owner: WindowId, home: SpaceId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub id: SpaceId,
    pub kind: SpaceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacesError {
    Full,
    UnknownWindow,
    AlreadyFocused,
    NotFocused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assigned {
    pub space: SpaceId,
    pub needs_switch: bool,
}

#[derive(Debug, Clone)]
pub struct Spaces {
    list: Vec<Space>,
    current: usize,
    next_id: u64,
    windows: Vec<(WindowId, SpaceId)>,
}

impl Default for Spaces {
    fn default() -> Self {
        Self::new()
    }
}

impl Spaces {
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

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Space> + '_ {
        self.list.iter()
    }

    pub fn desktop_count(&self) -> usize {
        self.list
            .iter()
            .filter(|space| space.kind == SpaceKind::Desktop)
            .count()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn current(&self) -> Space {
        self.list[self.current]
    }

    pub fn index_of(&self, id: SpaceId) -> Option<usize> {
        self.list.iter().position(|space| space.id == id)
    }

    pub fn get(&self, id: SpaceId) -> Option<Space> {
        self.list.iter().copied().find(|space| space.id == id)
    }

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

    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.list.len() || index == self.current {
            return false;
        }
        self.current = index;
        true
    }

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

    pub fn remove_window(&mut self, window: WindowId) -> bool {
        let before = self.windows.len();
        if self.focus_space_of(window).is_some() {
            let _ = self.exit_focus(window);
        }
        self.windows.retain(|(entry, _)| *entry != window);
        self.windows.len() != before
    }

    pub fn space_of(&self, window: WindowId) -> Option<SpaceId> {
        self.windows
            .iter()
            .find(|(entry, _)| *entry == window)
            .map(|(_, space)| *space)
    }

    pub fn is_shown(&self, window: WindowId) -> bool {
        self.space_of(window)
            .is_none_or(|space| space == self.current().id)
    }

    pub fn focus_space_of(&self, window: WindowId) -> Option<SpaceId> {
        self.list.iter().find_map(|space| match space.kind {
            SpaceKind::Focus { owner, .. } if owner == window => Some(space.id),
            _ => None,
        })
    }

    pub fn enter_focus(&mut self, window: WindowId) -> Result<usize, SpacesError> {
        let space = self.space_of(window).ok_or(SpacesError::UnknownWindow)?;
        if self.focus_space_of(window).is_some() {
            return Err(SpacesError::AlreadyFocused);
        }
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
    fn windows_open_on_the_shown_desktop() {
        let mut spaces = Spaces::new();
        let second = spaces.add_desktop().unwrap();
        spaces.switch_to(1);
        let assigned = spaces.assign(w(1), None);
        assert_eq!(assigned.space, second);
        assert!(!assigned.needs_switch);
        assert!(spaces.is_shown(w(1)));
        spaces.switch_to(0);
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
        spaces.switch_to(1);
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
        spaces.switch_to(1);
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
