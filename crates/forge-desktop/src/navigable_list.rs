use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, FocusHandle, Global, InteractiveElement, IntoElement, Pixels, Point,
    ScrollHandle, Styled, Subscription, WeakFocusHandle, Window, canvas,
};

use crate::actions::{LIST_CONTEXT, ListActivate, ListOpenMenu, ListSelectNext, ListSelectPrev};

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
    fn open_list_menu(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}

#[derive(Default)]
struct RestingList(Option<WeakFocusHandle>);

impl Global for RestingList {}

pub(crate) fn forward_root_focus_to_list<V: 'static>(
    root: &FocusHandle,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Subscription {
    let forwarded_from = root.clone();
    cx.on_focus(root, window, move |_, window, cx| {
        let list = cx
            .default_global::<RestingList>()
            .0
            .as_ref()
            .and_then(WeakFocusHandle::upgrade);
        if let Some(list) = list.filter(|list| forwarded_from.contains(list, window)) {
            window.focus(&list, cx);
        }
    })
}

pub(crate) fn navigable_list<V: NavigableList, E: InteractiveElement>(
    list: E,
    focus: &FocusHandle,
    cx: &mut Context<V>,
) -> E {
    cx.default_global::<RestingList>().0 = Some(focus.downgrade());
    list_keys(list.track_focus(focus), cx)
}

pub(crate) fn list_keys<V: NavigableList, E: InteractiveElement>(
    list: E,
    cx: &mut Context<V>,
) -> E {
    list.key_context(LIST_CONTEXT)
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
        .on_action(
            cx.listener(|this: &mut V, _: &ListOpenMenu, window, cx| {
                this.open_list_menu(window, cx)
            }),
        )
}

fn offset_revealing(
    offset: Point<Pixels>,
    viewport: Bounds<Pixels>,
    row: Bounds<Pixels>,
) -> Point<Pixels> {
    let above = viewport.top() - row.top();
    let below = row.bottom() - viewport.bottom();
    let mut revealed = offset;
    if above > Pixels::ZERO {
        revealed.y += above;
    } else if below > Pixels::ZERO {
        revealed.y -= below;
    }
    revealed
}

pub(crate) struct ListCursor<Id> {
    focus: FocusHandle,
    cursor: Option<Id>,
    focus_claimed: bool,
    row_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    reveal_pending: Rc<Cell<bool>>,
}

impl<Id: Clone + PartialEq> ListCursor<Id> {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            focus: cx.focus_handle(),
            cursor: None,
            focus_claimed: false,
            row_bounds: Rc::default(),
            reveal_pending: Rc::default(),
        }
    }

    pub(crate) fn reachable_by_tab(cx: &mut App) -> Self {
        let cursor = Self::new(cx);
        Self {
            focus: cursor.focus.clone().tab_stop(true),
            ..cursor
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

    pub(crate) fn reclaim_focus(&mut self) {
        self.focus_claimed = false;
    }

    pub(crate) fn row_bounds(&self) -> Option<Bounds<Pixels>> {
        self.row_bounds.get()
    }

    pub(crate) fn row_anchor(&self, scroll: Option<&ScrollHandle>) -> impl IntoElement + use<Id> {
        let row_bounds = Rc::clone(&self.row_bounds);
        let reveal_pending = Rc::clone(&self.reveal_pending);
        let scroll = scroll.cloned();
        canvas(
            move |bounds, window, _| {
                row_bounds.set(Some(bounds));
                if let Some(scroll) = scroll
                    && reveal_pending.replace(false)
                {
                    let revealed = offset_revealing(scroll.offset(), scroll.bounds(), bounds);
                    if revealed != scroll.offset() {
                        scroll.set_offset(revealed);
                        window.request_animation_frame();
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    pub(crate) fn is_at(&self, id: &Id) -> bool {
        self.cursor.as_ref() == Some(id)
    }

    pub(crate) fn move_to(&mut self, id: Id) {
        self.cursor = Some(id);
    }

    pub(crate) fn current(&self) -> Option<&Id> {
        self.cursor.as_ref()
    }

    pub(crate) fn clear(&mut self) {
        self.cursor = None;
    }

    pub(crate) fn step(
        &mut self,
        rows: &[Id],
        selected: Option<&Id>,
        step: ListStep,
    ) -> Option<usize> {
        let row_of = |id: &Id| rows.iter().position(|row| row == id);
        let current = self
            .cursor
            .as_ref()
            .and_then(row_of)
            .or_else(|| selected.and_then(row_of));
        let next = stepped_index(current, rows.len(), step)?;
        self.cursor = rows.get(next).cloned();
        self.reveal_pending.set(true);
        Some(next)
    }

    pub(crate) fn activation_target(&self, rows: &[Id]) -> Option<Id> {
        self.cursor.clone().filter(|id| rows.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::install_presentation;
    use forge_components::{FORGE_DEFAULT, TextInput, focus_inside_modal, overlay};
    use gpui::{
        AppContext, Entity, Modifiers, ParentElement, Render, StatefulInteractiveElement,
        TestAppContext, VisualTestContext, div, point, px, size,
    };

    struct KeyedList {
        focus: FocusHandle,
        steps: Vec<ListStep>,
        activations: usize,
        menus: usize,
    }

    impl NavigableList for KeyedList {
        fn step_list(&mut self, step: ListStep, _cx: &mut Context<Self>) {
            self.steps.push(step);
        }

        fn activate_list(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
            self.activations += 1;
        }

        fn open_list_menu(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
            self.menus += 1;
        }
    }

    impl Render for KeyedList {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let focus = self.focus.clone();
            navigable_list(div(), &focus, cx).size_full().child("row")
        }
    }

    const TOOLBAR_HEIGHT: f32 = 40.0;
    const FOCUS_HOPS_AFTER_A_CLOSE: usize = 3;

    struct ToolbarScreen {
        root: FocusHandle,
        list: Entity<KeyedList>,
        field: Entity<TextInput>,
        modal_open: bool,
        _forward: Subscription,
    }

    impl ToolbarScreen {
        fn open_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let field = self.field.clone();
            focus_inside_modal(window, cx, |window, cx| {
                field.update(cx, |field, cx| field.focus(window, cx))
            });
            self.modal_open = true;
            cx.notify();
        }
    }

    impl Render for ToolbarScreen {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let view = cx.entity();
            let toolbar = div()
                .id("toolbar-button")
                .h(px(TOOLBAR_HEIGHT))
                .w_full()
                .on_click(cx.listener(|this, _, window, cx| this.open_modal(window, cx)));
            div()
                .track_focus(&self.root)
                .size_full()
                .child(toolbar)
                .child(self.list.clone())
                .children(self.modal_open.then(|| {
                    overlay(div().child(self.field.clone()), &FORGE_DEFAULT).on_dismiss(
                        "toolbar-modal",
                        move |_, cx| {
                            view.update(cx, |this, cx| {
                                this.modal_open = false;
                                cx.notify();
                            })
                        },
                    )
                }))
        }
    }

    fn settle(vcx: &mut VisualTestContext) {
        for _ in 0..FOCUS_HOPS_AFTER_A_CLOSE {
            vcx.run_until_parked();
            vcx.update(|_, _| {});
        }
        vcx.run_until_parked();
    }

    #[gpui::test]
    fn arrows_reach_the_list_after_a_modal_opened_by_a_click_outside_it(cx: &mut TestAppContext) {
        cx.update(crate::actions::register_key_bindings);
        cx.update(forge_components::bind_text_input_keys);
        install_presentation(cx);
        let (screen, vcx) = cx.add_window_view(|window, cx| {
            let root = cx.focus_handle();
            let forward = forward_root_focus_to_list(&root, window, cx);
            ToolbarScreen {
                root,
                list: cx.new(|cx| KeyedList {
                    focus: cx.focus_handle(),
                    steps: Vec::new(),
                    activations: 0,
                    menus: 0,
                }),
                field: cx.new(|cx| TextInput::new("name", cx)),
                modal_open: false,
                _forward: forward,
            }
        });
        vcx.update(|window, _| window.activate_window());
        settle(vcx);
        let list = screen.read_with(vcx, |screen, _| screen.list.clone());
        let focus = list.read_with(vcx, |list, _| list.focus.clone());
        vcx.update(|window, cx| window.focus(&focus, cx));
        settle(vcx);

        vcx.simulate_click(
            point(px(TOOLBAR_HEIGHT), px(TOOLBAR_HEIGHT / 2.0)),
            Modifiers::none(),
        );
        settle(vcx);
        assert!(screen.read_with(vcx, |screen, _| screen.modal_open));
        vcx.simulate_keystrokes("escape");
        settle(vcx);
        assert!(!screen.read_with(vcx, |screen, _| screen.modal_open));

        vcx.simulate_keystrokes("down");

        list.read_with(vcx, |list, _| assert_eq!(list.steps, [ListStep::Next]));
    }

    #[gpui::test]
    fn arrows_and_enter_reach_the_focused_list(cx: &mut TestAppContext) {
        cx.update(crate::actions::register_key_bindings);
        let (list, vcx) = cx.add_window_view(|_window, cx| KeyedList {
            focus: cx.focus_handle(),
            steps: Vec::new(),
            activations: 0,
            menus: 0,
        });
        vcx.run_until_parked();
        let focus = list.read_with(vcx, |list, _| list.focus.clone());
        vcx.update(|window, cx| window.focus(&focus, cx));
        vcx.run_until_parked();

        vcx.simulate_keystrokes("down up enter shift-f10 menu");

        list.read_with(vcx, |list, _| {
            assert_eq!(list.steps, [ListStep::Next, ListStep::Prev]);
            assert_eq!(list.activations, 1);
            assert_eq!(list.menus, 2);
        });
    }

    #[test]
    fn a_stepped_row_scrolls_just_far_enough_into_the_viewport() {
        let viewport = Bounds::new(point(px(0.0), px(100.0)), size(px(300.0), px(200.0)));
        let row_at = |top: f32| Bounds::new(point(px(0.0), px(top)), size(px(300.0), px(30.0)));
        for (top, expected) in [
            (150.0, -50.0),
            (100.0, -50.0),
            (270.0, -50.0),
            (70.0, -20.0),
            (280.0, -60.0),
            (0.0, 50.0),
        ] {
            assert_eq!(
                offset_revealing(point(px(0.0), px(-50.0)), viewport, row_at(top)).y,
                px(expected),
                "row at {top}"
            );
        }
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

    #[gpui::test]
    fn a_cursor_off_the_list_steps_from_the_selection_and_a_listed_one_outranks_it(
        cx: &mut TestAppContext,
    ) {
        use ListStep::{Next, Prev};
        for (at, selected, step, expected) in [
            (9, Some(2), Next, 3),
            (9, Some(2), Prev, 1),
            (9, Some(8), Next, 1),
            (9, Some(8), Prev, 3),
            (9, None, Prev, 3),
            (1, Some(3), Next, 2),
        ] {
            let mut cursor = cx.update(ListCursor::<u32>::new);
            cursor.move_to(at);

            cursor.step(&[1, 2, 3], selected.as_ref(), step);

            assert!(cursor.is_at(&expected), "{at} {selected:?} {step:?}");
        }
    }

    #[gpui::test]
    fn an_empty_list_leaves_the_cursor_for_when_its_row_comes_back(cx: &mut TestAppContext) {
        let mut cursor = cx.update(ListCursor::<u32>::new);
        cursor.move_to(2);

        assert_eq!(cursor.step(&[], None, ListStep::Next), None);

        assert_eq!(cursor.step(&[1, 2, 3], None, ListStep::Next), Some(2));
    }
}
