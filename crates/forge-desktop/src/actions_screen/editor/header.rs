use super::*;
use forge_components::{
    Density, FONT_LG, FONT_XS, FONT_XXS, ForgePalette, Icon, MenuPlacement, Spacing, body_family,
    ghost_button_with_icon, menu_button, menu_divider, menu_item, spacing, status_dot, tr,
};
use gpui::{AnyElement, ClickEvent, Context, FontWeight, SharedString, Window, div, px};

impl ScreenActionsView {
    pub(super) fn render_editor_header(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action = &detail.action;
        let (pill_color, pill_label) = if action.enabled {
            (palette.success, tr!("action_editor_enabled"))
        } else {
            (palette.text_faint, tr!("action_editor_disabled"))
        };
        let pill_id = action.id;
        let pill_enabled = action.enabled;
        let pill = div()
            .id("actions-editor-status-pill")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(px(1.0))
            .px(px(6.0))
            .rounded(PILL_RADIUS)
            .bg(palette.surface_overlay)
            .cursor_pointer()
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
            );

        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(action.name.clone()),
            )
            .child(pill);

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
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(title_row)
            .child(desc_line);

        let id = action.id;
        let menu_open = self.header_menu_open;
        let view = cx.entity();
        let header_menu = menu_button(Icon::DotsVertical, menu_open.is_some(), palette)
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
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_history_modal(cx)),
                )
                .icon(Icon::History)
                .into(),
                menu_item(
                    SharedString::from("actions-header-menu-export"),
                    tr!("action_editor_export"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.export_json(cx)),
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
                .color(palette.random)
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
            });

        let btn_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
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
                        cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.open_edit_modal(window, cx)
                        }),
                    ),
            )
            .child(header_menu);

        let header_row = div()
            .flex()
            .items_start()
            .justify_between()
            .child(header_left)
            .child(btn_row);

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(header_row);
        if let Some(telemetry) = &self.telemetry {
            col = col.child(self.render_stats_row(telemetry, palette, cx));
        }
        col.into_any_element()
    }

    fn toggle_header_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.header_menu_open = if self.header_menu_open.is_some() {
            None
        } else {
            Some(position)
        };
        cx.notify();
    }

    fn close_header_menu(&mut self, cx: &mut Context<Self>) {
        self.header_menu_open = None;
        cx.notify();
    }

    fn open_edit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let action = detail.action.clone();
        self.header_menu_open = None;
        self.open_action_modal(Some(action), window, cx);
    }
}
