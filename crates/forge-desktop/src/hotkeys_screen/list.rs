use forge_components::{ForgePalette, Icon, icon, mono_family, pad_tile, tr};
use forge_types::TriggerInstanceId;
use gpui::{AnyElement, ClickEvent, Context, Pixels, Point, SharedString, div, prelude::*, px};

use super::HotkeysScreenView;
use super::capture::Capture;
use crate::actions::SHORTCUTS;

const SECTION_LABEL_FS: Pixels = px(9.5);
const SECTION_LABEL_MT: Pixels = px(4.0);
const SECTION_LABEL_MB: Pixels = px(8.0);
const SECTION_HINT_FS: Pixels = px(10.0);
const LIST_GAP: Pixels = px(6.0);

const ADD_BAR_GLYPH: Pixels = px(13.0);

const KBD_FS: Pixels = px(10.0);
const KBD_ML: Pixels = px(6.0);
const KBD_PAD_V: Pixels = px(1.0);
const KBD_PAD_H: Pixels = px(6.0);
const KBD_RADIUS: Pixels = px(4.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RowKey {
    Global(TriggerInstanceId),
    App(&'static str),
}

impl HotkeysScreenView {
    pub(super) fn toggle_menu(
        &mut self,
        key: RowKey,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = if self.menu_open == Some(key) {
            None
        } else {
            self.menu_click_pos = Some(position);
            Some(key)
        };
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    pub(super) fn render_bindings(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(tr!("hotkeys_section_hint"))
            .into_any_element();

        let mut list = div().w_full().flex().flex_col().gap(LIST_GAP);
        for (index, row) in self.bindings.iter().enumerate() {
            list = list.child(self.render_binding(index, row, palette, cx));
        }
        for (index, entry) in SHORTCUTS.iter().enumerate() {
            list = list.child(self.render_app_binding(index, entry, palette, cx));
        }
        list = list.child(self.render_add_bar(palette, cx));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_label(
                &tr!("hotkeys_section_bindings"),
                palette,
                hint,
            ))
            .child(list)
            .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        if self.capture == Capture::Add {
            return self.render_capture_row(palette, cx);
        }
        let title = div()
            .flex()
            .items_center()
            .child(tr!("hotkeys_add_binding"))
            .child(
                div()
                    .ml(KBD_ML)
                    .py(KBD_PAD_V)
                    .px(KBD_PAD_H)
                    .rounded(KBD_RADIUS)
                    .bg(palette.surface_overlay)
                    .font_family(mono_family())
                    .text_size(KBD_FS)
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(palette.text_faint)
                    .child(tr!("hotkeys_add_binding_kbd")),
            );

        pad_tile(
            "hotkeys-add",
            icon(Icon::Plus, ADD_BAR_GLYPH, palette.success),
            title,
            palette,
        )
        .bar(palette)
        .title_color(palette.success)
        .hover_border(palette.success)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_capture(Capture::Add, cx)))
        .into_any_element()
    }
}

fn section_label(label: &str, palette: &ForgePalette, right: AnyElement) -> impl IntoElement {
    div()
        .w_full()
        .mt(SECTION_LABEL_MT)
        .mb(SECTION_LABEL_MB)
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(mono_family())
                .text_size(SECTION_LABEL_FS)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(right)
}
