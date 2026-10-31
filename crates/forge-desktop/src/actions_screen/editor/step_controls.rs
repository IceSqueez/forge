use super::*;
use forge_components::{
    ForgePalette, Icon, MenuPlacement, PushToast, Spacing, menu_button, menu_divider, menu_item,
    spacing, tr,
};
use gpui::{AnyElement, App, ClickEvent, Context, SharedString, div};

struct RemovedStep {
    action: ActionId,
    path: Vec<nav::NavFrame>,
    owners: Vec<String>,
    index: usize,
    item: nav::ChainItem,
}

impl ScreenActionsView {
    fn move_step_up(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i > 0 && i < chain.len() {
                    let step = chain.remove(i);
                    chain.insert(i - 1, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn move_step_down(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i + 1 < chain.len() {
                    let step = chain.remove(i);
                    chain.insert(i + 1, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn move_step_top(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i != 0 && i < chain.len() {
                    let step = chain.remove(i);
                    chain.insert(0, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn move_step_bottom(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                let len = chain.len();
                if len > 0 && i < len - 1 {
                    let step = chain.remove(i);
                    chain.insert(len - 1, step);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn duplicate_step(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if i < chain.len() {
                    let clone = chain[i].clone();
                    chain.insert(i + 1, clone);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn remove_step(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        let removed = self.detail.as_ref().and_then(|detail| {
            let chain = nav::resolve_chain(&detail.action.sub_actions, &self.nav_path);
            Some(RemovedStep {
                action: detail.action.id,
                path: self.nav_path.clone(),
                owners: nav::owner_kinds(&detail.action.sub_actions, &self.nav_path)?,
                index: i,
                item: chain.get(i)?.clone(),
            })
        });
        self.persist_chain_mutation(
            move |chain| {
                if i < chain.len() {
                    chain.remove(i);
                }
            },
            cx,
        );
        if let Some(removed) = removed {
            self.raise_step_undo_toast(removed, cx);
        }
        cx.notify();
    }

    fn raise_step_undo_toast(&self, removed: RemovedStep, cx: &mut Context<Self>) {
        let view = cx.entity();
        let removed = std::cell::Cell::new(Some(removed));
        cx.push_undo_toast(
            tr!("actions_step_removed_toast"),
            move |_window, app: &mut App| {
                if let Some(removed) = removed.take() {
                    view.update(app, |this, cx| this.restore_step(removed, cx));
                }
            },
        );
    }

    fn restore_step(&mut self, removed: RemovedStep, cx: &mut Context<Self>) {
        let RemovedStep {
            action,
            path,
            owners,
            index,
            item,
        } = removed;
        self.write_action(
            action,
            action_writes::ActionWrite::InsertStep {
                path,
                owners,
                index,
                item,
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_step_menu(&mut self, i: usize, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.step_menu_open == Some(i) {
            self.step_menu_open = None;
        } else {
            self.step_menu_open = Some(i);
            self.menu_click_pos = Some(position);
        }
        cx.notify();
    }

    fn close_step_menu(&mut self, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        cx.notify();
    }

    fn set_step_enabled(&mut self, i: usize, enabled: bool, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(i).and_then(nav::ChainItem::step_mut) {
                    step.enabled = enabled;
                }
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_step_controls(
        &self,
        i: usize,
        total: usize,
        enabled: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let menu_open = self.step_menu_open == Some(i);
        let menu_pos = if menu_open { self.menu_click_pos } else { None };
        let view = cx.entity();

        let move_up = step_icon_btn(
            SharedString::from(format!("actions-step-up-{i}")),
            Icon::ArrowUp,
            i == 0,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_up(i, cx)),
        );
        let move_down = step_icon_btn(
            SharedString::from(format!("actions-step-down-{i}")),
            Icon::ArrowDown,
            i + 1 >= total,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_down(i, cx)),
        );

        let menu = menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(menu_pos)
            .items(vec![
                menu_item(
                    SharedString::from(format!("actions-step-edit-{i}")),
                    tr!("action_editor_step_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_edit_sub_action(i, cx)
                    }),
                )
                .icon(Icon::InfoCircle)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-dup-{i}")),
                    tr!("action_editor_step_menu_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate_step(i, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-top-{i}")),
                    tr!("action_editor_step_menu_move_top"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_top(i, cx)),
                )
                .icon(Icon::ArrowBarUp)
                .disabled(i == 0)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-bottom-{i}")),
                    tr!("action_editor_step_menu_move_bottom"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_bottom(i, cx)),
                )
                .icon(Icon::ArrowBarDown)
                .disabled(i + 1 >= total)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-enabled-{i}")),
                    if enabled {
                        tr!("actions_step_disable")
                    } else {
                        tr!("actions_step_enable")
                    },
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_step_enabled(i, !enabled, cx)
                    }),
                )
                .icon(if enabled { Icon::EyeOff } else { Icon::Eye })
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-del-{i}")),
                    tr!("action_editor_step_menu_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_step(i, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.danger)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("actions-step-menu-{i}")),
                cx.listener(move |this, ev: &ClickEvent, _, cx| {
                    this.toggle_step_menu(i, ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_step_menu(cx));
            });

        div()
            .id(SharedString::from(format!("actions-step-controls-{i}")))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(move_up)
            .child(move_down)
            .child(menu)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{ToastAction, ToastKind, Toasts};
    use gpui::TestAppContext;

    use super::super::super::action_writes::tests::{FIRST, SECOND, Screen, THIRD, kinds};
    use super::*;
    use crate::test_support::{blank_window, press_undo, step};

    const INNER_KEY: &str = "then";

    fn stored(screen: &Screen, id: ActionId) -> Vec<String> {
        kinds(&screen.store.stored(id).sub_actions)
    }

    fn remove(screen: &Screen, cx: &mut TestAppContext, index: usize) {
        screen
            .view
            .update(cx, |view, cx| view.remove_step(index, cx));
        screen.settle(cx);
    }

    fn undo(screen: &Screen, cx: &mut TestAppContext) {
        press_undo(cx);
        screen.settle(cx);
    }

    fn undo_toasts(cx: &mut TestAppContext) -> usize {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .filter(|toast| toast.kind == ToastKind::Undo)
                .count()
        })
    }

    fn take_undo(cx: &mut TestAppContext) -> ToastAction {
        cx.update(|cx| {
            let toasts = cx.global_mut::<Toasts>();
            let id = toasts.items().first().expect("an undo toast").id;
            toasts
                .take(id)
                .and_then(|toast| toast.action)
                .expect("an undo action")
        })
    }

    fn expected(steps: &[&str]) -> Vec<String> {
        steps.iter().map(|kind| (*kind).to_owned()).collect()
    }

    #[gpui::test]
    fn undo_puts_a_removed_step_back_where_it_was(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        remove(&screen, cx, 1);
        assert_eq!(stored(&screen, screen.alpha), expected(&[FIRST, THIRD]));

        undo(&screen, cx);

        assert_eq!(
            stored(&screen, screen.alpha),
            expected(&[FIRST, SECOND, THIRD])
        );
    }

    #[gpui::test]
    fn undo_run_twice_restores_the_step_once(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        remove(&screen, cx, 0);
        let undo = take_undo(cx);
        let vcx = cx.add_empty_window();

        for _ in 0..2 {
            vcx.update(|window, cx| (undo.on_action)(window, cx));
            screen.settle(vcx);
        }

        assert_eq!(
            stored(&screen, screen.alpha),
            expected(&[FIRST, SECOND, THIRD])
        );
    }

    #[gpui::test]
    fn undo_inserts_into_a_chain_that_changed_after_the_removal(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        remove(&screen, cx, 1);
        screen.reorder_shown(cx);
        screen.settle(cx);

        undo(&screen, cx);

        assert_eq!(
            stored(&screen, screen.alpha),
            expected(&[THIRD, SECOND, FIRST])
        );
    }

    #[gpui::test]
    fn undo_into_a_chain_shorter_than_the_old_position_appends(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        remove(&screen, cx, 2);
        let older_undo = take_undo(cx);
        remove(&screen, cx, 0);
        remove(&screen, cx, 0);

        let vcx = cx.add_empty_window();
        vcx.update(|window, cx| (older_undo.on_action)(window, cx));
        screen.settle(vcx);

        assert_eq!(stored(&screen, screen.alpha), expected(&[THIRD]));
    }

    #[gpui::test]
    fn undo_after_moving_to_another_action_restores_the_first_and_leaves_the_second(
        cx: &mut TestAppContext,
    ) {
        let screen = Screen::open(cx);
        blank_window(cx);
        remove(&screen, cx, 1);
        let bravo = screen.bravo;
        screen.view.update(cx, |view, cx| view.select(bravo, cx));
        screen.settle(cx);

        undo(&screen, cx);

        assert_eq!(
            (
                stored(&screen, screen.alpha),
                stored(&screen, screen.bravo),
                screen.shown(cx).id
            ),
            (
                expected(&[FIRST, SECOND, THIRD]),
                expected(&[FIRST, SECOND, THIRD]),
                bravo
            )
        );
    }

    #[gpui::test]
    fn undo_after_moving_away_keeps_edits_made_after_the_removal(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        remove(&screen, cx, 1);
        screen.reorder_shown(cx);
        screen.settle(cx);
        let bravo = screen.bravo;
        screen.view.update(cx, |view, cx| view.select(bravo, cx));
        screen.settle(cx);

        undo(&screen, cx);

        assert_eq!(
            stored(&screen, screen.alpha),
            expected(&[THIRD, SECOND, FIRST])
        );
    }

    #[gpui::test]
    fn undo_restores_a_nested_step_after_navigating_back_to_the_root(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        let path = vec![nav::NavFrame {
            step_index: 0,
            chain_key: INNER_KEY.to_owned(),
            case_index: None,
        }];
        let inner = [SECOND, THIRD]
            .map(|kind| nav::ChainItem::fresh(step(kind, true)))
            .to_vec();
        screen.write(
            cx,
            screen.alpha,
            action_writes::ActionWrite::SetChain {
                path: path.clone(),
                chain: inner,
            },
        );
        screen.settle(cx);
        let nested = path.clone();
        screen.view.update(cx, |view, _| view.nav_path = nested);
        remove(&screen, cx, 0);
        screen.view.update(cx, |view, _| view.nav_path = Vec::new());

        undo(&screen, cx);

        let chain = nav::resolve_chain(&screen.store.stored(screen.alpha).sub_actions, &path);
        let restored: Vec<&str> = chain
            .iter()
            .filter_map(nav::ChainItem::step)
            .map(|step| step.kind_id.as_str())
            .collect();
        assert_eq!(restored, [SECOND, THIRD]);
    }

    #[gpui::test]
    fn removing_past_the_end_of_the_chain_offers_no_undo(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);

        remove(&screen, cx, 3);

        assert_eq!(
            (undo_toasts(cx), stored(&screen, screen.alpha)),
            (0, expected(&[FIRST, SECOND, THIRD]))
        );
    }
}
