use super::*;
use forge_components::{
    FONT_LG, FONT_XS, FONT_XXS, ForgePalette, Icon, KeyActivated, MenuPlacement, Spacing,
    body_family, ghost_button_with_icon, menu_button, menu_divider, menu_item, scaled, spacing,
    status_dot, tr,
};
use gpui::{AnyElement, ClickEvent, Context, FontWeight, SharedString, div, px};

const STATUS_PILL_PAD_V: gpui::Pixels = px(1.0);

impl ActionEditor {
    pub(super) fn render_editor_header(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let action = &detail.action;
        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(action.name.clone()),
            )
            .child(self.render_status_pill(action, palette, cx));

        let desc = action
            .description
            .clone()
            .unwrap_or_else(|| tr!("action_editor_no_description"));
        let desc_line = div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(desc);

        let header_left = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(title_row)
            .child(desc_line);

        let btn_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button_with_icon(Icon::PlayerPlay, tr!("action_editor_test_run"), palette)
                    .height(HEADER_ACTION_H)
                    .on_click(
                        "actions-editor-test",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.start_test_run(cx)),
                    ),
            )
            .child(
                ghost_button_with_icon(Icon::Edit, tr!("action_editor_edit"), palette)
                    .height(HEADER_ACTION_H)
                    .on_click(
                        "actions-editor-edit",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_edit_modal(cx)),
                    ),
            )
            .child(self.render_header_menu(action.id, palette, cx));

        let header_row = div()
            .flex()
            .items_start()
            .justify_between()
            .child(header_left)
            .child(btn_row);

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(header_row);
        if let Some(telemetry) = &self.telemetry {
            col = col.child(self.render_stats_row(telemetry, palette, cx));
        }
        col.into_any_element()
    }

    fn render_status_pill(
        &self,
        action: &Action,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let (pill_color, pill_label) = if action.enabled {
            (palette.success, tr!("action_editor_enabled"))
        } else {
            (palette.text_faint, tr!("action_editor_disabled"))
        };
        let pill_id = action.id;
        let pill_enabled = action.enabled;
        div()
            .id("actions-editor-status-pill")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(scaled(STATUS_PILL_PAD_V, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(PILL_RADIUS)
            .bg(palette.surface_overlay)
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.click_count() >= 2 {
                    this.set_enabled(pill_id, !pill_enabled, cx);
                }
            }))
            .child(status_dot(pill_color, PILL_DOT))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(pill_color)
                    .child(pill_label),
            )
    }

    fn render_header_menu(
        &self,
        id: ActionId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let menu_open = self.header_open;
        let view = cx.entity();
        menu_button(Icon::DotsVertical, menu_open.is_some(), palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(menu_open)
            .items(vec![
                menu_item(
                    SharedString::from("actions-header-menu-dup"),
                    tr!("action_editor_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_item(
                    SharedString::from("actions-header-menu-history"),
                    tr!("action_editor_run_history"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_history(cx)),
                )
                .icon(Icon::History)
                .into(),
                menu_item(
                    SharedString::from("actions-header-menu-export"),
                    tr!("action_editor_export"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.export_pack(cx)),
                )
                .icon(Icon::Download)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from("actions-header-menu-del"),
                    tr!("action_editor_menu_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.danger)
                .into(),
            ])
            .on_toggle(
                SharedString::from("actions-header-menu-trigger"),
                cx.listener(|this, ev: &ClickEvent, _, cx| {
                    this.toggle_header_menu(ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_header_menu(cx));
            })
    }

    fn toggle_header_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.header_open = if self.header_open.is_some() {
            None
        } else {
            Some(position)
        };
        cx.notify();
    }

    fn close_header_menu(&mut self, cx: &mut Context<Self>) {
        self.header_open = None;
        cx.notify();
    }

    fn open_edit_modal(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        cx.emit(ActionEditorEvent::EditAction(detail.action.clone()));
        self.header_open = None;
        cx.notify();
    }

    pub(super) fn open_history(&mut self, cx: &mut Context<Self>) {
        self.header_open = None;
        cx.emit(ActionEditorEvent::OpenHistory);
        cx.notify();
    }

    fn export_pack(&mut self, cx: &mut Context<Self>) {
        self.header_open = None;
        if let Some(detail) = self.detail.as_ref() {
            cx.emit(ActionEditorEvent::ExportPack(detail.action.id));
        }
        cx.notify();
    }

    fn request_delete(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.header_open = None;
        cx.emit(ActionEditorEvent::RequestDelete(id));
        cx.notify();
    }

    pub(super) fn set_enabled(&mut self, id: ActionId, enabled: bool, cx: &mut Context<Self>) {
        self.write_action(id, ActionWrite::SetEnabled(enabled), cx);
    }

    fn duplicate(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.header_open = None;
        self.write_action(id, ActionWrite::duplicate_copy(), cx);
    }
}
