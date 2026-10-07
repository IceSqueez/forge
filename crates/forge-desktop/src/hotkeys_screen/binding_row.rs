use forge_components::{
    FONT_XXS, ForgePalette, Icon, MenuPlacement, badge, body_family, card, icon, menu_button,
    menu_divider, menu_item, mono_family, toggle, tr,
};
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

use super::HotkeysScreenView;
use super::capture::Capture;
use super::list::RowKey;
use super::row_style::{
    ACCENT_DOT, ARROW_GLYPH, KEYCAPS_MIN_W, ROW_GAP, ROW_OFF_OPACITY, ROW_PAD_H, ROW_PAD_V,
    SCOPE_BADGE_FS, TARGET_FS,
};
use crate::hotkey_action_modal::keycaps;
use crate::hotkey_bindings::{BindingRow, HotkeyEdge};

const STOP_TARGET_MAX_W: Pixels = px(180.0);

impl HotkeysScreenView {
    pub(super) fn render_binding(
        &self,
        index: usize,
        row: &BindingRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = row.key;
        if self.capture == Capture::Rebind(key) {
            return self.render_capture_row(palette, cx);
        }
        let enabled = row.enabled();
        let (target_text, target_ink) = match row.primary_action() {
            Some((_, name)) => (name.clone(), palette.text_primary),
            None => (tr!("hotkeys_unassigned"), palette.text_faint),
        };
        let (scope_label, scope_ink) = if row.registered {
            (tr!("hotkeys_scope_global"), palette.success)
        } else {
            (tr!("hotkeys_scope_unregistered"), palette.text_faint)
        };
        let dot_color = if row.primary_action().is_some() {
            palette.brand
        } else {
            palette.text_faint
        };
        let edge_badge = if row.is_hold() {
            Some(badge(
                palette.surface_overlay,
                palette.warning,
                tr!("hotkeys_badge_hold"),
                false,
                SCOPE_BADGE_FS,
            ))
        } else if row.is_release_edge() {
            Some(badge(
                palette.surface_overlay,
                palette.info,
                tr!("hotkeys_badge_release_edge"),
                false,
                SCOPE_BADGE_FS,
            ))
        } else {
            None
        };
        let stop_target = row
            .is_hold()
            .then(|| row.half(HotkeyEdge::Release))
            .flatten()
            .map(|half| match half.action.as_ref() {
                Some((_, name)) => tr!("hotkeys_row_stop", action = name.as_str()),
                None => tr!("hotkeys_row_stop_unassigned"),
            });

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .child(
                div()
                    .id(("hotkeys-row-combo", index))
                    .flex_none()
                    .min_w(KEYCAPS_MIN_W)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if event.click_count() >= 2 {
                            this.start_capture(Capture::Rebind(key), cx);
                        }
                    }))
                    .child(keycaps(&row.combo, palette)),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    scope_ink,
                    scope_label,
                    false,
                    SCOPE_BADGE_FS,
                )
                .flex_none(),
            )
            .children(edge_badge.map(|badge| badge.flex_none()))
            .child(icon(Icon::ArrowRight, ARROW_GLYPH, palette.text_faint))
            .child(
                div()
                    .flex_none()
                    .size(ACCENT_DOT)
                    .rounded_full()
                    .bg(dot_color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(TARGET_FS)
                    .text_color(target_ink)
                    .child(target_text),
            )
            .children(stop_target.map(|text| {
                div()
                    .flex_none()
                    .max_w(STOP_TARGET_MAX_W)
                    .truncate()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(text)
            }))
            .child(toggle(enabled, palette).on_click(
                ("hotkeys-row-toggle", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_binding(key, cx)),
            ))
            .child(self.render_row_menu(index, row, palette, cx));

        let mut wrapper = div().w_full().child(
            card(body, palette)
                .padding_xy(ROW_PAD_V, ROW_PAD_H)
                .full_width(),
        );
        if !enabled {
            wrapper = wrapper.opacity(ROW_OFF_OPACITY);
        }
        wrapper.into_any_element()
    }

    fn render_row_menu(
        &self,
        index: usize,
        row: &BindingRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = RowKey::Global(row.key);
        let open = self.menu_open == Some(key);
        let view = cx.entity();
        let paired = row.is_hold();
        let mut items = Vec::new();
        for (edge, half) in row.halves() {
            let instance_id = half.instance_id;
            let label = match (paired, edge) {
                (false, _) => tr!("hotkeys_menu_edit"),
                (true, HotkeyEdge::Press) => tr!("hotkeys_menu_edit_press"),
                (true, HotkeyEdge::Release) => tr!("hotkeys_menu_edit_release"),
            };
            items.push(
                menu_item(
                    (
                        "hotkeys-menu-edit",
                        index * 2 + usize::from(edge == HotkeyEdge::Release),
                    ),
                    label,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_half(instance_id, cx)),
                )
                .icon(Icon::Edit)
                .into(),
            );
        }
        if let Some(edge) = row.free_edge() {
            let row_key = row.key;
            let label = match edge {
                HotkeyEdge::Press => tr!("hotkeys_menu_add_press"),
                HotkeyEdge::Release => tr!("hotkeys_menu_add_release"),
            };
            items.push(
                menu_item(
                    ("hotkeys-menu-add-half", index),
                    label,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.add_half(row_key, cx)),
                )
                .icon(Icon::Plus)
                .into(),
            );
        }
        if paired {
            items.push(menu_divider());
            for (edge, half) in row.halves() {
                let instance_id = half.instance_id;
                let label = match edge {
                    HotkeyEdge::Press => tr!("hotkeys_menu_remove_press"),
                    HotkeyEdge::Release => tr!("hotkeys_menu_remove_release"),
                };
                items.push(
                    menu_item(
                        (
                            "hotkeys-menu-remove-half",
                            index * 2 + usize::from(edge == HotkeyEdge::Release),
                        ),
                        label,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.prompt_delete_half(instance_id, cx)
                        }),
                    )
                    .icon(Icon::Eraser)
                    .into(),
                );
            }
        }
        items.push(menu_divider());
        let row_key = row.key;
        items.push(
            menu_item(
                ("hotkeys-menu-delete", index),
                tr!("common_delete"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.prompt_delete(row_key, cx)),
            )
            .icon(Icon::Trash)
            .color(palette.random)
            .into(),
        );

        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(items)
            .on_toggle(
                ("hotkeys-menu-trigger", index),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.toggle_menu(key, event.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }
}
