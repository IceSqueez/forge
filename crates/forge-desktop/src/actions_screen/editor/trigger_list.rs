use super::cards::{add_row_button, empty_placeholder_card};
use super::*;
use crate::async_bridge;
use crate::integration_switch::{category_inactive_badge, inactive_badge};
use crate::repo_failure::RepoFailure;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, PushToast, Radius, Spacing,
    ToastAction, ToastKind, UNDO_TOAST_DURATION, body_family, icon, mono_family, radius, row_card,
    spacing, status_dot, tr,
};
use forge_registry::TriggerKindDescriptor;
use forge_types::{TriggerInstance, TriggerInstanceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, FontWeight, SharedString, Window, div,
};

fn trigger_unlink_btn(
    id: impl Into<ElementId>,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
) -> AnyElement {
    let solid = palette.danger;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .p(spacing(Spacing::Xxs, density))
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .hover(move |s| s.bg(solid))
        .on_click(handler)
        .child(icon(Icon::X, UNLINK_GLYPH, palette.text_faint))
        .into_any_element()
}

impl ScreenActionsView {
    fn unlink_trigger(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .unlink_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |this, result, cx| match result {
                Ok(()) => {
                    this.reload_detail(cx);
                    this.raise_unlink_undo_toast(action_id, instance_id, cx);
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn raise_unlink_undo_toast(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        cx.push_toast_full(
            ToastKind::Undo,
            tr!("actions_trigger_unlinked_toast"),
            None,
            Some(ToastAction::new(
                tr!("common_undo"),
                move |_window, app: &mut App| {
                    view.update(app, |this, cx| {
                        this.relink_trigger(action_id, instance_id, cx)
                    });
                },
            )),
            UNDO_TOAST_DURATION,
        );
    }

    fn relink_trigger(
        &mut self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .link_trigger_instance(action_id, instance_id)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| match result {
                Ok(()) => this.reload_detail(cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_triggers_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let label = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!(
                "action_editor_section_triggers_count",
                count = detail.trigger_instances.len() as i64
            ));

        let mut col = div().flex().flex_col().gap(spacing(Spacing::Xs, density));
        if detail.trigger_instances.is_empty() {
            col = col.child(empty_placeholder_card(
                Icon::Bolt,
                palette.warning,
                tr!("action_editor_no_triggers"),
                palette,
                density,
            ));
        } else {
            for instance in &detail.trigger_instances {
                col = col.child(self.render_trigger_card(instance, palette, cx));
            }
        }
        col = col.child(add_row_button(
            "actions-add-trigger",
            Icon::Plus,
            tr!("action_editor_add_trigger"),
            palette.warning,
            palette,
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_trigger_picker(window, cx)),
            density,
        ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(label)
            .child(col)
            .into_any_element()
    }

    fn render_trigger_card(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let descriptor = self.trigger_registry.get(&instance.kind_id);
        let switched_off_owner = self.switched_off_trigger_owner(&instance.kind_id);
        let uncovered_category = self.uncovered_trigger_category(&instance.kind_id);
        let accent = if instance.enabled {
            palette.brand
        } else {
            palette.disabled
        };
        let name_color = if instance.enabled {
            palette.text_primary
        } else {
            palette.text_faint
        };
        let kind_label = descriptor
            .map(crate::kind_copy::trigger_name)
            .unwrap_or_else(|| instance.kind_id.clone());
        let mut condition =
            crate::triggers_screen::condition_display(descriptor, &instance.overrides);
        if crate::triggers_screen::cooldown_applies(descriptor) {
            condition.push_str(&crate::triggers_screen::cooldown_suffix(
                instance.cooldown_secs,
                instance.cooldown_global,
            ));
        }
        if let Some(permission) = descriptor
            .and_then(TriggerKindDescriptor::chat_trigger_family)
            .and_then(|_| crate::triggers_screen::permission_suffix(instance.permission_rung))
        {
            condition.push_str(&permission);
        }
        let glyph = Icon::from_name(
            descriptor
                .map(TriggerKindDescriptor::icon_name)
                .unwrap_or("bolt"),
        );

        let leading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(accent, TRIGGER_DOT))
            .child(icon(glyph, TRIGGER_GLYPH, accent));

        let title = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .flex_none()
                    .text_color(name_color)
                    .child(crate::kind_copy::trigger_instance_name(
                        &instance.name,
                        descriptor,
                    )),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .flex_none()
                    .text_color(palette.text_faint)
                    .child(kind_label),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .min_w_0()
                    .truncate()
                    .text_color(palette.accent_peach)
                    .child(condition),
            )
            .children(
                switched_off_owner
                    .as_ref()
                    .map(|owner| inactive_badge(owner, palette, density)),
            )
            .children(
                uncovered_category
                    .map(|category| category_inactive_badge(category, palette, density)),
            );
        let dimmed =
            !instance.enabled || switched_off_owner.is_some() || uncovered_category.is_some();

        let instance_id = instance.id;
        let unlink = trigger_unlink_btn(
            SharedString::from(format!("actions-trigger-unlink-{instance_id}")),
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.unlink_trigger(instance_id, cx)
            }),
            density,
        );

        let card = row_card(title, palette)
            .leading(leading)
            .trailing(unlink)
            .trailing_reveal(SharedString::from(format!(
                "actions-trigger-row-{instance_id}"
            )))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .on_click(
                SharedString::from(format!("actions-trigger-open-{instance_id}")),
                cx.listener(move |_this, _: &ClickEvent, _, cx| {
                    cx.emit(NavRequested(Screen::Triggers(Some(instance_id))))
                }),
            );
        div()
            .w_full()
            .when(dimmed, |row| row.opacity(TRIGGER_DIMMED_OPACITY))
            .child(card)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_components::{ToastKind, Toasts};
    use forge_storage::queue::MockQueueRepo;
    use forge_storage::{MockTriggerInstanceRepo, StorageError};
    use gpui::TestAppContext;

    use super::super::super::action_writes::tests::Screen;
    use super::*;
    use crate::test_support::{StubHistory, blank_window, error_toasts, press_undo};

    type Links = Arc<Mutex<Vec<(ActionId, TriggerInstanceId)>>>;

    fn refused() -> StorageError {
        StorageError::NotFound {
            key: "link".to_owned(),
        }
    }

    #[derive(Clone, Copy)]
    struct Outcomes {
        unlink_works: bool,
        link_works: bool,
    }

    const BOTH_WORK: Outcomes = Outcomes {
        unlink_works: true,
        link_works: true,
    };

    fn links_recorded(screen: &Screen, cx: &mut TestAppContext, outcomes: Outcomes) -> Links {
        let linked = Links::default();
        let recorded = Arc::clone(&linked);
        let mut triggers = MockTriggerInstanceRepo::new();
        triggers.expect_list_all().returning(|| Ok(Vec::new()));
        triggers
            .expect_list_for_action()
            .returning(|_| Ok(Vec::new()));
        triggers
            .expect_actions_by_instance()
            .returning(|| Ok(Default::default()));
        triggers.expect_unlink_action().returning(move |_, _| {
            if outcomes.unlink_works {
                Ok(true)
            } else {
                Err(refused())
            }
        });
        triggers
            .expect_link_action()
            .returning(move |action, instance| {
                recorded.lock().unwrap().push((action, instance));
                if outcomes.link_works {
                    Ok(())
                } else {
                    Err(refused())
                }
            });
        let mut queues = MockQueueRepo::new();
        queues.expect_list().returning(|| Ok(Vec::new()));
        let actions = Arc::clone(&screen.store) as Arc<dyn forge_storage::action::ActionRepo>;
        screen.view.update(cx, |view, _| {
            view.actions_service = Arc::new(ActionsService::new(
                actions,
                Arc::new(queues),
                Arc::new(StubHistory),
                Arc::new(triggers),
            ));
        });
        linked
    }

    fn unlink(screen: &Screen, cx: &mut TestAppContext, instance: TriggerInstanceId) {
        screen
            .view
            .update(cx, |view, cx| view.unlink_trigger(instance, cx));
        screen.settle(cx);
    }

    fn undo_messages(cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .filter(|toast| toast.kind == ToastKind::Undo)
                .map(|toast| toast.message.to_string())
                .collect()
        })
    }

    #[gpui::test]
    fn undo_links_the_trigger_back_to_the_action_it_left(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        let linked = links_recorded(&screen, cx, BOTH_WORK);
        let instance = TriggerInstanceId::new();
        unlink(&screen, cx, instance);
        assert_eq!(undo_messages(cx), [tr!("actions_trigger_unlinked_toast")]);

        press_undo(cx);
        screen.settle(cx);

        assert_eq!(*linked.lock().unwrap(), [(screen.alpha, instance)]);
    }

    #[gpui::test]
    fn undo_after_selecting_another_action_still_targets_the_first(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        let linked = links_recorded(&screen, cx, BOTH_WORK);
        let instance = TriggerInstanceId::new();
        unlink(&screen, cx, instance);
        let bravo = screen.bravo;
        screen.view.update(cx, |view, cx| view.select(bravo, cx));
        screen.settle(cx);

        press_undo(cx);
        screen.settle(cx);

        assert_eq!(*linked.lock().unwrap(), [(screen.alpha, instance)]);
    }

    #[gpui::test]
    fn a_refused_unlink_offers_no_undo_and_reports_the_error(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        links_recorded(
            &screen,
            cx,
            Outcomes {
                unlink_works: false,
                link_works: true,
            },
        );

        unlink(&screen, cx, TriggerInstanceId::new());

        assert!(undo_messages(cx).is_empty());
        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn a_refused_relink_reports_the_error(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        blank_window(cx);
        links_recorded(
            &screen,
            cx,
            Outcomes {
                unlink_works: true,
                link_works: false,
            },
        );
        unlink(&screen, cx, TriggerInstanceId::new());

        press_undo(cx);
        screen.settle(cx);

        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }
}
