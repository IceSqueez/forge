use forge_components::{
    BORDER_THIN, ForgePalette, Icon, OverlayPosition, body_family, ghost_button_with_icon, icon,
    modal, mono_family, overlay, primary_button, secondary_button, tr,
};
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, Pixels, SharedString, Window, div, prelude::*,
    px,
};

use crate::actions::{ShortcutEntry, chord_caps};
use crate::hotkey_action_modal::keycaps;
use crate::presentation::ActivePresentation;

const MODAL_W: Pixels = px(440.0);
const BODY_PAD_V: Pixels = px(16.0);
const BODY_PAD_H: Pixels = px(18.0);

const SECTION_LABEL_FS: Pixels = px(9.5);
const SECTION_LABEL_MB: Pixels = px(8.0);
const SIGNAL_GAP: Pixels = px(10.0);
const RECAPTURE_HEIGHT: Pixels = px(32.0);

const DISPLAY_PAD_V: Pixels = px(7.0);
const DISPLAY_PAD_H: Pixels = px(11.0);
const DISPLAY_RADIUS: Pixels = px(7.0);
const DISPLAY_GAP: Pixels = px(7.0);
const LISTEN_GLYPH: Pixels = px(12.0);
const DISPLAY_FS: Pixels = px(12.0);

const FOOTER_GAP: Pixels = px(8.0);

pub enum AppShortcutModalEvent {
    Save(Option<String>),
    Recapture,
    Cancel,
}

pub struct AppShortcutModal {
    entry: &'static ShortcutEntry,
    chord: Option<String>,
    capturing: bool,
}

impl EventEmitter<AppShortcutModalEvent> for AppShortcutModal {}

impl AppShortcutModal {
    pub fn new(entry: &'static ShortcutEntry, chord: Option<String>) -> Self {
        Self {
            entry,
            chord,
            capturing: false,
        }
    }

    pub fn apply_capture(&mut self, chord: String, cx: &mut Context<Self>) {
        self.capturing = false;
        self.chord = Some(chord);
        cx.notify();
    }

    pub fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        if !self.capturing {
            return;
        }
        self.capturing = false;
        cx.notify();
    }

    fn recapture(&mut self, cx: &mut Context<Self>) {
        self.capturing = true;
        cx.emit(AppShortcutModalEvent::Recapture);
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        cx.emit(AppShortcutModalEvent::Save(self.chord.clone()));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AppShortcutModalEvent::Cancel);
    }

    fn render_chord(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let border = if self.capturing {
            palette.success
        } else {
            palette.border_input
        };
        let mut display = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(DISPLAY_GAP)
            .py(DISPLAY_PAD_V)
            .px(DISPLAY_PAD_H)
            .rounded(DISPLAY_RADIUS)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(palette.shell);
        display = match (self.capturing, self.chord.as_deref()) {
            (true, _) => display
                .child(icon(Icon::Keyboard, LISTEN_GLYPH, palette.success))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(DISPLAY_FS)
                        .text_color(palette.success)
                        .child(tr!("hotkeys_capture_prompt")),
                ),
            (false, Some(chord)) => display.child(keycaps(&chord_caps(chord), palette)),
            (false, None) => display.child(
                div()
                    .font_family(mono_family())
                    .text_size(DISPLAY_FS)
                    .text_color(palette.text_faint)
                    .child(tr!("settings_shortcuts_unbound")),
            ),
        };

        let recapture =
            ghost_button_with_icon(Icon::Keyboard, tr!("hotkeys_modal_recapture"), palette)
                .height(RECAPTURE_HEIGHT)
                .on_click(
                    "app-shortcut-recapture",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.recapture(cx)),
                );

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .mb(SECTION_LABEL_MB)
                    .font_family(mono_family())
                    .text_size(SECTION_LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(SharedString::from(
                        tr!("hotkeys_modal_section_combo").to_uppercase(),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(SIGNAL_GAP)
                    .child(display)
                    .child(div().flex_none().child(recapture)),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(FOOTER_GAP)
            .child(secondary_button(tr!("common_cancel"), palette).on_click(
                "app-shortcut-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            ))
            .child(
                primary_button(tr!("hotkeys_modal_save_changes"), palette)
                    .disabled(self.capturing)
                    .on_click(
                        "app-shortcut-save",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for AppShortcutModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let body = div()
            .w_full()
            .py(BODY_PAD_V)
            .px(BODY_PAD_H)
            .child(self.render_chord(&palette, cx));

        let card = modal(tr!(self.entry.label_key), body, &palette)
            .header_icon(Icon::Keyboard, palette.success)
            .subtitle(tr!("hotkeys_app_modal_subtitle"))
            .width(MODAL_W)
            .flush_body()
            .footer(self.render_footer(&palette, cx))
            .on_close(
                "app-shortcut-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("app-shortcut-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}
