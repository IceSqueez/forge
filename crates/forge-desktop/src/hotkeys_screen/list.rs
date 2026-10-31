use forge_components::ActivePresentation;
use forge_components::FONT_MICRO;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    BORDER_THIN, ForgePalette, Icon, Radius, SectionLabel, error_row_with_retry, icon, mono_family,
    pad_tile, radius, scaled, tr,
};
use forge_types::TriggerInstanceId;
use gpui::{
    AnyElement, ClickEvent, Context, KeyDownEvent, Pixels, Point, Window, div, prelude::*, px,
};

use super::HotkeysScreenView;
use super::capture::Capture;
use crate::actions::SHORTCUTS;
use crate::navigable_list::{ListStep, NavigableList, navigable_list};

const SECTION_HINT_FS: Pixels = FONT_MICRO;

const ADD_BAR_GLYPH: Pixels = px(13.0);

const KBD_FS: Pixels = FONT_MICRO;
const KBD_PAD_V: Pixels = px(1.0);
const KBD_RADIUS: Pixels = px(4.0);
const TOGGLE_ROW_KEY: &str = "space";
const CURSOR_RING_RADIUS: Pixels = radius(Radius::Md);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RowKey {
    Global(TriggerInstanceId),
    RetryShortcuts,
    App(&'static str),
    AddBinding,
}

impl NavigableList for HotkeysScreenView {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        if self.keys_belong_to_the_list() {
            let rows = self.cursor_rows();
            if self.row_keys.step(&rows, None, step).is_some() {
                cx.notify();
            }
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.keys_belong_to_the_list() {
            return;
        }
        match self.row_keys.activation_target(&self.cursor_rows()) {
            Some(RowKey::Global(key)) => self.start_capture(Capture::Rebind(key), cx),
            Some(RowKey::RetryShortcuts) => self.load_shortcuts(cx),
            Some(RowKey::App(id)) => self.start_capture(Capture::App(id), cx),
            Some(RowKey::AddBinding) => self.start_capture(Capture::Add, cx),
            None => {}
        }
    }

    fn open_list_menu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.keys_belong_to_the_list() {
            return;
        }
        let target = self.row_keys.activation_target(&self.cursor_rows());
        if let Some(key @ (RowKey::Global(_) | RowKey::App(_))) = target {
            let position = self
                .row_keys
                .row_bounds()
                .map(|row| row.bottom_right())
                .unwrap_or_default();
            self.toggle_menu(key, position, cx);
        }
    }
}

impl HotkeysScreenView {
    fn keys_belong_to_the_list(&self) -> bool {
        self.capture == Capture::Off
    }

    pub(super) fn cursor_rows(&self) -> Vec<RowKey> {
        let mut rows: Vec<RowKey> = self
            .bindings
            .iter()
            .map(|row| RowKey::Global(row.key))
            .collect();
        if self.shortcuts.failure().is_some() {
            rows.push(RowKey::RetryShortcuts);
        }
        rows.extend(SHORTCUTS.iter().map(|entry| RowKey::App(entry.id)));
        rows.push(RowKey::AddBinding);
        rows
    }

    pub(super) fn point_at_row(&mut self, key: RowKey, cx: &mut Context<Self>) {
        self.row_keys.move_to(key);
        cx.notify();
    }

    fn toggle_cursor_row(&mut self, cx: &mut Context<Self>) {
        if !self.keys_belong_to_the_list() {
            return;
        }
        match self.row_keys.activation_target(&self.cursor_rows()) {
            Some(RowKey::Global(key)) => self.toggle_binding(key, cx),
            Some(RowKey::App(id)) => self.toggle_app_binding(id, cx),
            Some(RowKey::RetryShortcuts | RowKey::AddBinding) | None => {}
        }
    }

    pub(super) fn with_row_keys(
        &self,
        key: RowKey,
        row: gpui::Stateful<gpui::Div>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let row = row
            .relative()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.point_at_row(key, cx)));
        if self.row_keys.is_at(&key) {
            row.child(self.row_keys.row_anchor(Some(&self.scroll)))
                .child(cursor_ring(palette))
        } else {
            row
        }
    }

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
        let density = cx.density();
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(tr!("hotkeys_section_hint"))
            .into_any_element();

        let focus = self.row_keys.focus().clone();
        let mut list = navigable_list(div(), &focus, cx)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == TOGGLE_ROW_KEY && !event.keystroke.modifiers.modified() {
                    this.toggle_cursor_row(cx);
                }
            }))
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        for (index, row) in self.bindings.iter().enumerate() {
            list = list.child(self.render_binding(index, row, palette, cx));
        }
        if let Some(reason) = self.shortcuts.failure() {
            let retry_row = error_row_with_retry(
                "hotkeys-app-load-retry",
                tr!("settings_shortcuts_load_failed", reason = reason),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.load_shortcuts(cx)),
                density,
            );
            list = list.child(self.with_row_keys(
                RowKey::RetryShortcuts,
                div().id("hotkeys-app-load-row").w_full().child(retry_row),
                palette,
                cx,
            ));
        }
        for (index, entry) in SHORTCUTS.iter().enumerate() {
            list = list.child(self.render_app_binding(index, entry, palette, cx));
        }
        list = list.child(self.render_add_bar(palette, cx));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("hotkeys_section_bindings"), palette)
                    .row()
                    .trailing(Some(hint)),
            )
            .child(list)
            .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        if self.capture == Capture::Add {
            return self.render_capture_row(palette, cx);
        }
        let title = div()
            .flex()
            .items_center()
            .child(tr!("hotkeys_add_binding"))
            .child(
                div()
                    .ml(spacing(Spacing::Xs, density))
                    .py(scaled(KBD_PAD_V, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(KBD_RADIUS)
                    .bg(palette.surface_overlay)
                    .font_family(mono_family())
                    .text_size(KBD_FS)
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(palette.text_faint)
                    .child(tr!("hotkeys_add_binding_kbd")),
            );

        let tile = pad_tile(
            "hotkeys-add",
            icon(Icon::Plus, ADD_BAR_GLYPH, palette.success),
            title,
            palette,
        )
        .bar(palette)
        .title_color(palette.success)
        .hover_border(palette.success)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_capture(Capture::Add, cx)));
        self.with_row_keys(
            RowKey::AddBinding,
            div().id("hotkeys-add-row").w_full().child(tile),
            palette,
            cx,
        )
        .into_any_element()
    }
}

fn cursor_ring(palette: &ForgePalette) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .rounded(CURSOR_RING_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.brand)
}
