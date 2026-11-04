use std::collections::HashSet;
use std::hash::Hash;

use gpui::Modifiers;

const BULK_MINIMUM: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectGesture {
    Single,
    Toggle,
    Range,
}

impl SelectGesture {
    pub fn from_modifiers(modifiers: &Modifiers) -> Self {
        if modifiers.shift {
            SelectGesture::Range
        } else if modifiers.secondary() {
            SelectGesture::Toggle
        } else {
            SelectGesture::Single
        }
    }
}

#[derive(Debug, Clone)]
pub struct MultiSelection<Id> {
    ids: Vec<Id>,
    anchor: Option<Id>,
}

impl<Id> Default for MultiSelection<Id> {
    fn default() -> Self {
        Self {
            ids: Vec::new(),
            anchor: None,
        }
    }
}

impl<Id: Copy + Eq + Hash> MultiSelection<Id> {
    pub fn select_only(&mut self, id: Id) {
        self.ids = vec![id];
        self.anchor = Some(id);
    }

    pub fn apply(&mut self, id: Id, gesture: SelectGesture, visible: &[Id]) {
        match gesture {
            SelectGesture::Single => self.select_only(id),
            SelectGesture::Toggle => {
                match self.ids.iter().position(|held| *held == id) {
                    Some(position) => {
                        self.ids.remove(position);
                    }
                    None => self.ids.push(id),
                }
                self.anchor = Some(id);
            }
            SelectGesture::Range => self.select_range(id, visible),
        }
    }

    fn select_range(&mut self, id: Id, visible: &[Id]) {
        let anchor_at = self
            .anchor
            .and_then(|anchor| visible.iter().position(|row| *row == anchor));
        let target_at = visible.iter().position(|row| *row == id);
        match (anchor_at, target_at) {
            (Some(from), Some(to)) => {
                let (start, end) = (from.min(to), from.max(to));
                self.ids = visible[start..=end].to_vec();
            }
            _ => self.select_only(id),
        }
    }

    pub fn retain_visible(&mut self, visible: &[Id]) {
        let shown: HashSet<Id> = visible.iter().copied().collect();
        self.ids.retain(|id| shown.contains(id));
        if self.anchor.is_some_and(|anchor| !shown.contains(&anchor)) {
            self.anchor = None;
        }
    }

    pub fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }

    pub fn contains(&self, id: &Id) -> bool {
        self.ids.contains(id)
    }

    pub fn ids(&self) -> &[Id] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn is_bulk(&self) -> bool {
        self.ids.len() >= BULK_MINIMUM
    }

    pub fn bulk_contains(&self, id: &Id) -> bool {
        self.is_bulk() && self.contains(id)
    }
}
