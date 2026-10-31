use gpui::{App, Context, FocusHandle, InteractiveElement, Window};

use crate::actions::{LIST_CONTEXT, ListActivate, ListSelectNext, ListSelectPrev};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ListStep {
    Prev,
    Next,
}

pub(crate) fn stepped_index(current: Option<usize>, len: usize, step: ListStep) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match (current, step) {
        (None, ListStep::Next) => 0,
        (None, ListStep::Prev) => last,
        (Some(index), ListStep::Prev) => index.min(last).saturating_sub(1),
        (Some(index), ListStep::Next) => (index + 1).min(last),
    })
}

pub(crate) trait NavigableList: Sized + 'static {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>);
    fn activate_list(&mut self, window: &mut Window, cx: &mut Context<Self>);
}

pub(crate) fn navigable_list<V: NavigableList, E: InteractiveElement>(
    list: E,
    focus: &FocusHandle,
    cx: &mut Context<V>,
) -> E {
    list.track_focus(focus)
        .key_context(LIST_CONTEXT)
        .on_action(
            cx.listener(|this: &mut V, _: &ListSelectPrev, _, cx| {
                this.step_list(ListStep::Prev, cx)
            }),
        )
        .on_action(
            cx.listener(|this: &mut V, _: &ListSelectNext, _, cx| {
                this.step_list(ListStep::Next, cx)
            }),
        )
        .on_action(
            cx.listener(|this: &mut V, _: &ListActivate, window, cx| {
                this.activate_list(window, cx)
            }),
        )
}

pub(crate) struct ListCursor<Id> {
    focus: FocusHandle,
    cursor: Option<Id>,
    focus_claimed: bool,
}

impl<Id: Clone + PartialEq> ListCursor<Id> {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            focus: cx.focus_handle(),
            cursor: None,
            focus_claimed: false,
        }
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(crate) fn claim_focus_once(&mut self, window: &mut Window, cx: &mut App) {
        if !self.focus_claimed {
            self.focus_claimed = true;
            window.focus(&self.focus, cx);
        }
    }

    pub(crate) fn is_at(&self, id: &Id) -> bool {
        self.cursor.as_ref() == Some(id)
    }

    pub(crate) fn step(
        &mut self,
        rows: &[Id],
        selected: Option<&Id>,
        step: ListStep,
    ) -> Option<usize> {
        let current = self
            .cursor
            .as_ref()
            .or(selected)
            .and_then(|id| rows.iter().position(|row| row == id));
        let next = stepped_index(current, rows.len(), step)?;
        self.cursor = rows.get(next).cloned();
        Some(next)
    }

    pub(crate) fn activation_target(&self, rows: &[Id]) -> Option<Id> {
        self.cursor.clone().filter(|id| rows.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{IntoElement, ParentElement, Render, Styled, TestAppContext, div};

    struct KeyedList {
        focus: FocusHandle,
        steps: Vec<ListStep>,
        activations: usize,
    }

    impl NavigableList for KeyedList {
        fn step_list(&mut self, step: ListStep, _cx: &mut Context<Self>) {
            self.steps.push(step);
        }

        fn activate_list(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
            self.activations += 1;
        }
    }

    impl Render for KeyedList {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let focus = self.focus.clone();
            navigable_list(div(), &focus, cx).size_full().child("row")
        }
    }

    #[gpui::test]
    fn arrows_and_enter_reach_the_focused_list(cx: &mut TestAppContext) {
        cx.update(crate::actions::register_key_bindings);
        let (list, vcx) = cx.add_window_view(|_window, cx| KeyedList {
            focus: cx.focus_handle(),
            steps: Vec::new(),
            activations: 0,
        });
        vcx.run_until_parked();
        let focus = list.read_with(vcx, |list, _| list.focus.clone());
        vcx.update(|window, cx| window.focus(&focus, cx));
        vcx.run_until_parked();

        vcx.simulate_keystrokes("down up enter");

        list.read_with(vcx, |list, _| {
            assert_eq!(list.steps, [ListStep::Next, ListStep::Prev]);
            assert_eq!(list.activations, 1);
        });
    }

    #[test]
    fn stepping_stays_inside_the_list() {
        use ListStep::{Next, Prev};
        for (current, len, step, expected) in [
            (None, 0, Next, None),
            (Some(2), 0, Prev, None),
            (None, 3, Next, Some(0)),
            (None, 3, Prev, Some(2)),
            (Some(0), 3, Prev, Some(0)),
            (Some(2), 3, Next, Some(2)),
            (Some(1), 3, Next, Some(2)),
            (Some(1), 3, Prev, Some(0)),
            (Some(9), 3, Prev, Some(1)),
            (Some(9), 3, Next, Some(2)),
        ] {
            assert_eq!(
                stepped_index(current, len, step),
                expected,
                "{current:?} {len} {step:?}"
            );
        }
    }

    #[gpui::test]
    fn the_cursor_starts_at_the_selection_and_falls_back_when_its_row_is_gone(
        cx: &mut TestAppContext,
    ) {
        let mut cursor = cx.update(ListCursor::<u32>::new);

        assert_eq!(cursor.step(&[1, 2, 3], Some(&2), ListStep::Next), Some(2));
        assert!(cursor.is_at(&3));

        assert_eq!(cursor.step(&[1, 2], None, ListStep::Next), Some(0));
        assert!(cursor.is_at(&1));
    }

    #[gpui::test]
    fn activation_ignores_a_cursor_whose_row_was_filtered_out(cx: &mut TestAppContext) {
        let mut cursor = cx.update(ListCursor::<u32>::new);
        cursor.step(&[7], None, ListStep::Next);

        assert_eq!(cursor.activation_target(&[7, 8]), Some(7));
        assert_eq!(cursor.activation_target(&[8]), None);
    }
}
