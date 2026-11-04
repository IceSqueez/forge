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

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [u32; 6] = [1, 2, 3, 4, 5, 6];

    fn marked_from(id: u32) -> MultiSelection<u32> {
        let mut marked = MultiSelection::default();
        marked.select_only(id);
        marked
    }

    #[test]
    fn modifiers_map_to_gestures_with_shift_winning_over_the_secondary_key() {
        let shift_and_secondary = Modifiers {
            shift: true,
            ..Modifiers::secondary_key()
        };
        let alt = Modifiers {
            alt: true,
            ..Modifiers::none()
        };
        for (modifiers, expected, case) in [
            (Modifiers::none(), SelectGesture::Single, "none"),
            (Modifiers::shift(), SelectGesture::Range, "shift"),
            (
                Modifiers::secondary_key(),
                SelectGesture::Toggle,
                "secondary",
            ),
            (
                shift_and_secondary,
                SelectGesture::Range,
                "shift + secondary",
            ),
            (alt, SelectGesture::Single, "alt"),
        ] {
            assert_eq!(
                SelectGesture::from_modifiers(&modifiers),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn toggle_adds_an_unmarked_row_and_removes_a_marked_one() {
        let mut marked = marked_from(1);

        marked.apply(3, SelectGesture::Toggle, &ROWS);
        let added = marked.ids().to_vec();
        marked.apply(1, SelectGesture::Toggle, &ROWS);

        assert_eq!((added, marked.ids().to_vec()), (vec![1, 3], vec![3]));
    }

    #[test]
    fn toggling_the_only_marked_row_off_leaves_nothing_marked() {
        let mut marked = marked_from(2);

        marked.apply(2, SelectGesture::Toggle, &ROWS);

        assert!(marked.is_empty());
    }

    #[test]
    fn a_range_after_a_toggle_starts_at_the_toggled_row() {
        let mut marked = marked_from(1);
        marked.apply(3, SelectGesture::Toggle, &ROWS);

        marked.apply(5, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [3, 4, 5]);
    }

    #[test]
    fn a_range_covers_anchor_to_target_in_either_direction() {
        for (anchor, target, expected) in [
            (2, 4, vec![2, 3, 4]),
            (4, 2, vec![2, 3, 4]),
            (3, 3, vec![3]),
            (1, 6, ROWS.to_vec()),
        ] {
            let mut marked = marked_from(anchor);

            marked.apply(target, SelectGesture::Range, &ROWS);

            assert_eq!(marked.ids(), expected, "{anchor} -> {target}");
        }
    }

    #[test]
    fn a_range_spans_only_the_visible_rows_between_anchor_and_target() {
        let visible = [1, 3, 5, 6];
        let mut marked = marked_from(1);

        marked.apply(5, SelectGesture::Range, &visible);

        assert_eq!(marked.ids(), [1, 3, 5]);
    }

    #[test]
    fn a_range_replaces_the_marked_rows_instead_of_adding_to_them() {
        let mut marked = marked_from(6);
        marked.apply(2, SelectGesture::Toggle, &ROWS);

        marked.apply(3, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [2, 3]);
    }

    #[test]
    fn repeated_ranges_keep_measuring_from_the_same_anchor() {
        let mut marked = marked_from(2);
        marked.apply(5, SelectGesture::Range, &ROWS);

        marked.apply(1, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [1, 2]);
    }

    #[test]
    fn a_range_without_a_usable_anchor_marks_only_the_target() {
        let mut never_anchored = MultiSelection::default();
        let mut anchor_off_screen = marked_from(9);
        for (marked, case) in [
            (&mut never_anchored, "no anchor"),
            (&mut anchor_off_screen, "anchor not visible"),
        ] {
            marked.apply(4, SelectGesture::Range, &ROWS);

            assert_eq!(marked.ids(), [4], "{case}");
        }
    }

    #[test]
    fn a_range_target_that_is_not_visible_marks_only_the_target() {
        let mut marked = marked_from(1);

        marked.apply(9, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [9]);
    }

    #[test]
    fn a_single_gesture_replaces_a_bulk_selection_with_the_clicked_row() {
        let mut marked = marked_from(1);
        marked.apply(4, SelectGesture::Range, &ROWS);

        marked.apply(6, SelectGesture::Single, &ROWS);

        assert_eq!(marked.ids(), [6]);
    }

    #[test]
    fn retain_visible_drops_the_hidden_rows_and_keeps_the_order_of_the_rest() {
        let mut marked = marked_from(1);
        marked.apply(5, SelectGesture::Range, &ROWS);

        marked.retain_visible(&[5, 3, 1]);

        assert_eq!(marked.ids(), [1, 3, 5]);
    }

    #[test]
    fn retain_visible_forgets_a_hidden_anchor_so_the_next_range_starts_fresh() {
        let mut marked = marked_from(2);
        marked.apply(4, SelectGesture::Toggle, &ROWS);

        marked.retain_visible(&[1, 2, 3, 5, 6]);
        marked.apply(6, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [6]);
    }

    #[test]
    fn retain_visible_keeps_a_visible_anchor() {
        let mut marked = marked_from(2);
        marked.apply(5, SelectGesture::Toggle, &ROWS);

        marked.retain_visible(&[2, 3, 4, 5]);
        marked.apply(3, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [3, 4, 5]);
    }

    #[test]
    fn clear_forgets_the_anchor_too() {
        let mut marked = marked_from(2);

        marked.clear();
        marked.apply(4, SelectGesture::Range, &ROWS);

        assert_eq!(marked.ids(), [4]);
    }

    #[test]
    fn a_selection_is_bulk_from_two_marked_rows() {
        let mut marked = MultiSelection::default();
        let mut seen = vec![marked.is_bulk()];
        for id in [1, 2, 3] {
            marked.apply(id, SelectGesture::Toggle, &ROWS);
            seen.push(marked.is_bulk());
        }

        assert_eq!(seen, [false, false, true, true]);
    }

    #[test]
    fn bulk_contains_holds_only_for_members_of_a_bulk_selection() {
        let single = marked_from(1);
        let mut bulk = marked_from(1);
        bulk.apply(2, SelectGesture::Toggle, &ROWS);

        assert_eq!(
            (
                single.bulk_contains(&1),
                bulk.bulk_contains(&1),
                bulk.bulk_contains(&3)
            ),
            (false, true, false)
        );
    }
}
