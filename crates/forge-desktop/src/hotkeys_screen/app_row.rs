use forge_components::{
    BORDER_THIN, ForgePalette, Icon, MenuPlacement, badge, body_family, card, icon, menu_button,
    menu_item, mono_family, toggle, tr,
};
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

use super::HotkeysScreenView;
use super::capture::Capture;
use super::list::RowKey;
use super::row_style::{
    ACCENT_DOT, ARROW_GLYPH, KEYCAPS_MIN_W, ROW_GAP, ROW_OFF_OPACITY, ROW_PAD_H, ROW_PAD_V,
    SCOPE_BADGE_FS, TARGET_FS,
};
use crate::actions::{ShortcutEntry, chord_caps};
use crate::hotkey_action_modal::keycaps;

const UNBOUND_FS: Pixels = px(11.5);
const UNBOUND_RADIUS: Pixels = px(5.0);
const UNBOUND_PAD_V: Pixels = px(3.0);
const UNBOUND_PAD_H: Pixels = px(8.0);

impl HotkeysScreenView {
    pub(super) fn render_app_binding(
        &self,
        index: usize,
        entry: &'static ShortcutEntry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.capture == Capture::App(entry.id) {
            return self.render_capture_row(palette, cx);
        }
        let id = entry.id;
        let enabled = self.shortcuts.is_enabled(id);
        let chord = self.shortcuts.chord_of(entry);
        let dot_color = if chord.is_some() {
            palette.warning
        } else {
            palette.text_faint
        };
        let combo: AnyElement = match chord {
            Some(chord) => keycaps(&chord_caps(chord), palette).into_any_element(),
            None => unbound_chip(palette),
        };

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .child(
                div()
                    .id(("hotkeys-app-combo", index))
                    .flex_none()
                    .min_w(KEYCAPS_MIN_W)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if event.click_count() >= 2 {
                            this.start_capture(Capture::App(id), cx);
                        }
                    }))
                    .child(combo),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    tr!("hotkeys_scope_app"),
                    false,
                    SCOPE_BADGE_FS,
                )
                .flex_none(),
            )
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
                    .text_color(palette.text_primary)
                    .child(tr!(entry.label_key)),
            )
            .child(toggle(enabled, palette).on_click(
                ("hotkeys-app-toggle", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_app_binding(id, cx)),
            ))
            .child(self.render_app_menu(index, entry, palette, cx));

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

    fn render_app_menu(
        &self,
        index: usize,
        entry: &'static ShortcutEntry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.id;
        let key = RowKey::App(id);
        let view = cx.entity();
        let mut items = vec![
            menu_item(
                ("hotkeys-app-edit", index),
                tr!("hotkeys_menu_edit"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_app_binding(entry, cx)),
            )
            .icon(Icon::Edit)
            .into(),
        ];
        if self.shortcuts.chord_of(entry).is_some() {
            items.push(
                menu_item(
                    ("hotkeys-app-unbind", index),
                    tr!("hotkeys_menu_unbind"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.unbind_app_binding(id, cx)),
                )
                .icon(Icon::Eraser)
                .into(),
            );
        }
        if self.shortcuts.is_overridden(id) {
            items.push(
                menu_item(
                    ("hotkeys-app-reset", index),
                    tr!("hotkeys_menu_reset_default"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_app_binding(id, cx)),
                )
                .icon(Icon::Refresh)
                .into(),
            );
        }

        menu_button(Icon::DotsVertical, self.menu_open == Some(key), palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(items)
            .on_toggle(
                ("hotkeys-app-menu-trigger", index),
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

fn unbound_chip(palette: &ForgePalette) -> AnyElement {
    div()
        .flex_none()
        .py(UNBOUND_PAD_V)
        .px(UNBOUND_PAD_H)
        .rounded(UNBOUND_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell)
        .font_family(mono_family())
        .text_size(UNBOUND_FS)
        .text_color(palette.text_faint)
        .child(tr!("settings_shortcuts_unbound"))
        .into_any_element()
}
