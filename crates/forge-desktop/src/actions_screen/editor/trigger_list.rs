use super::cards::{add_row_button, empty_placeholder_card};
use super::*;
use crate::async_bridge;
use crate::integration_switch::{category_inactive_badge, inactive_badge};
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, body_family,
    icon, mono_family, radius, row_card, spacing, status_dot, tr,
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
) -> AnyElement {
    let solid = palette.random;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .p(spacing(Spacing::Xxs, Density::Cozy))
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
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| match result {
                Ok(()) => this.reload_detail(cx),
                Err(message) => this.on_repo_error(&message, cx),
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
        let label = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!(
                "action_editor_section_triggers_count",
                count = detail.trigger_instances.len() as i64
            ));

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        if detail.trigger_instances.is_empty() {
            col = col.child(empty_placeholder_card(
                Icon::Bolt,
                palette.warning,
                tr!("action_editor_no_triggers"),
                palette,
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
        ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
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
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| instance.kind_id.clone());
        let mut condition = crate::triggers_screen::condition_display(
            descriptor,
            &instance.kind_id,
            &instance.overrides,
        );
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
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(accent, TRIGGER_DOT))
            .child(icon(glyph, TRIGGER_GLYPH, accent));

        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(instance.name.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(kind_label),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.bits)
                    .child(condition),
            )
            .children(
                switched_off_owner
                    .as_ref()
                    .map(|owner| inactive_badge(owner, palette)),
            )
            .children(
                uncovered_category.map(|category| category_inactive_badge(category, palette)),
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
