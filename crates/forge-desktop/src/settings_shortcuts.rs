use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, Density, FONT_LG, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    InputEvent, OverlayPosition, Radius, Spacing, TextInput, body_family, confirm_modal,
    drive_overlay_focus, ghost_button, icon, mono_family, overlay, radius, spacing, toggle, tr,
    with_alpha,
};
use forge_hotkey::{DEFAULT_HOLD_CEILING_SECS, HotkeyClient};
use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_storage::{DataProvider, SettingsRepo};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, FontWeight, Keystroke, Pixels,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::actions::{SHORTCUTS, ShortcutEntry, shortcut_entry};
use crate::async_bridge;
use crate::hotkey_bindings::{load_hold_ceiling, save_hold_ceiling};
use crate::presentation::ActivePresentation;
use crate::shortcut_overrides::{ChordVerdict, ShortcutOverrides, save_overrides};

const CEILING_MIN_SECS: u64 = 1;
const CEILING_MAX_SECS: u64 = 3600;
const CEILING_INPUT_W: Pixels = px(72.0);

struct ShortcutConflict {
    target_id: &'static str,
    owner_id: &'static str,
    chord: String,
}

pub struct SettingsShortcutsView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    hotkey_client: Option<Arc<HotkeyClient>>,
    overrides: ShortcutOverrides,
    rebinding: Option<&'static str>,
    rebind_error: Option<String>,
    conflict: Option<ShortcutConflict>,
    save_error: Option<String>,
    hold_ceiling_secs: u64,
    hold_ceiling_on: bool,
    hold_ceiling_input: Entity<TextInput>,
    hold_ceiling_invalid: bool,
    hold_ceiling_debounce: async_bridge::Debounced,
    capture_sub: Option<Subscription>,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    _subs: Vec<Subscription>,
}

impl SettingsShortcutsView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        hotkey_client: Option<Arc<HotkeyClient>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let hold_ceiling_input = cx.new(|cx| {
            let mut input = TextInput::new(tr!("settings_hold_ceiling_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM);
            input.set_content(DEFAULT_HOLD_CEILING_SECS.to_string(), cx);
            input
        });
        let subs = vec![cx.subscribe(
            &hold_ceiling_input,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Changed(text) | InputEvent::Submitted(text) => {
                    this.commit_hold_ceiling(text.as_ref(), cx)
                }
                InputEvent::Cancelled => {}
            },
        )];

        let mut view = Self {
            backend,
            rt_handle,
            hotkey_client,
            overrides: ShortcutOverrides::default(),
            rebinding: None,
            rebind_error: None,
            conflict: None,
            save_error: None,
            hold_ceiling_secs: DEFAULT_HOLD_CEILING_SECS,
            hold_ceiling_on: true,
            hold_ceiling_input,
            hold_ceiling_invalid: false,
            hold_ceiling_debounce: async_bridge::Debounced::new(
                async_bridge::SLIDER_PERSIST_DEBOUNCE,
            ),
            capture_sub: None,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            _subs: subs,
        };
        view.load(cx);
        view.load_hold_ceiling(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.get_string(KEYBOARD_SHORTCUTS).await },
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
    }

    fn load_hold_ceiling(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { Ok::<_, String>(load_hold_ceiling(repo.as_ref()).await) },
            |this, result: Result<Option<u64>, String>, cx| {
                if let Ok(stored) = result {
                    this.apply_hold_ceiling(stored, cx);
                }
            },
            cx,
        );
    }

    fn apply_hold_ceiling(&mut self, stored: Option<u64>, cx: &mut Context<Self>) {
        self.hold_ceiling_on = stored.is_some();
        self.hold_ceiling_secs = stored.unwrap_or(DEFAULT_HOLD_CEILING_SECS);
        self.hold_ceiling_invalid = false;
        let text = self.hold_ceiling_secs.to_string();
        self.hold_ceiling_input.update(cx, |input, cx| {
            input.set_invalid(false, cx);
            input.set_content(text, cx);
        });
        cx.notify();
    }

    fn effective_hold_ceiling(&self) -> Option<u64> {
        self.hold_ceiling_on.then_some(self.hold_ceiling_secs)
    }

    fn push_hold_ceiling(&mut self, cx: &mut Context<Self>) {
        let ceiling = self.effective_hold_ceiling();
        if let Some(client) = &self.hotkey_client {
            client.set_hold_ceiling(ceiling);
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.hold_ceiling_debounce
            .schedule(&self.rt_handle, "hotkey hold ceiling", async move {
                save_hold_ceiling(repo.as_ref(), ceiling).await
            });
        cx.notify();
    }

    fn toggle_hold_ceiling(&mut self, cx: &mut Context<Self>) {
        self.hold_ceiling_on = !self.hold_ceiling_on;
        self.push_hold_ceiling(cx);
    }

    fn commit_hold_ceiling(&mut self, text: &str, cx: &mut Context<Self>) {
        let parsed = text
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|secs| (CEILING_MIN_SECS..=CEILING_MAX_SECS).contains(secs));
        self.hold_ceiling_invalid = parsed.is_none();
        let invalid = self.hold_ceiling_invalid;
        self.hold_ceiling_input
            .update(cx, |input, cx| input.set_invalid(invalid, cx));
        match parsed {
            Some(secs) => {
                self.hold_ceiling_secs = secs;
                self.push_hold_ceiling(cx);
            }
            None => cx.notify(),
        }
    }

    fn apply_loaded(
        &mut self,
        result: Result<Option<String>, forge_storage::StorageError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(raw) => self.overrides.replace_stored(raw.as_deref()),
            Err(e) => {
                tracing::warn!(error = %e, "failed to load keyboard shortcuts");
                self.save_error = Some(e.to_string());
            }
        }
        cx.notify();
    }

    fn start_rebind(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.rebinding = Some(id);
        self.rebind_error = None;
        self.conflict = None;
        let weak = cx.entity().downgrade();
        let sub = cx.intercept_keystrokes(move |event, _window, cx| {
            let keystroke = event.keystroke.clone();
            let _ = weak.update(cx, |this, cx| this.on_capture_keystroke(keystroke, cx));
            cx.stop_propagation();
        });
        self.capture_sub = Some(sub);
        cx.notify();
    }

    fn end_capture(&mut self) {
        self.rebinding = None;
        self.capture_sub = None;
    }

    fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        self.end_capture();
        cx.notify();
    }

    fn on_capture_keystroke(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) {
        let Some(id) = self.rebinding else {
            return;
        };
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.cancel_capture(cx);
            return;
        }
        match self.overrides.verdict(&keystroke, id) {
            ChordVerdict::Unusable => {}
            ChordVerdict::NeedsModifier => {
                self.rebind_error = Some(tr!("settings_shortcuts_error_needs_modifier"));
                self.end_capture();
                cx.notify();
            }
            ChordVerdict::Taken { owner_id, chord } => {
                self.conflict = Some(ShortcutConflict {
                    target_id: id,
                    owner_id,
                    chord,
                });
                self.end_capture();
                cx.notify();
            }
            ChordVerdict::Free(chord) => {
                self.rebind_error = None;
                self.end_capture();
                self.overrides.bind(id, chord);
                self.persist_and_apply(cx);
            }
        }
    }

    fn conflict_steal(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        self.overrides.unbind(conflict.owner_id);
        self.overrides.bind(conflict.target_id, conflict.chord);
        self.rebind_error = None;
        self.persist_and_apply(cx);
    }

    fn conflict_cancel(&mut self, cx: &mut Context<Self>) {
        self.conflict = None;
        cx.notify();
    }

    fn reset_entry(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.overrides.reset(id);
        self.rebind_error = None;
        self.persist_and_apply(cx);
    }

    fn reset_all(&mut self, cx: &mut Context<Self>) {
        self.overrides.reset_all();
        self.rebind_error = None;
        self.conflict = None;
        self.end_capture();
        self.persist_and_apply(cx);
    }

    fn persist_and_apply(&mut self, cx: &mut Context<Self>) {
        self.overrides.apply(cx);

        let map = self.overrides.snapshot();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            save_overrides(repo, map),
            |this, result, cx| this.apply_save_result(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_save_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.save_error = None,
            Err(message) => {
                tracing::warn!(error = %message, "failed to persist keyboard shortcuts");
                self.save_error = Some(message);
            }
        }
        cx.notify();
    }

    fn header(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::Keyboard, px(18.0), palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_LG)
                            .text_color(palette.text_primary)
                            .child(tr!("settings_shortcuts_title")),
                    ),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_shortcuts_subtitle")),
            )
    }

    fn error_banner(&self, palette: &ForgePalette) -> Option<AnyElement> {
        let message = self.rebind_error.as_ref().or(self.save_error.as_ref())?;
        Some(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .px(spacing(Spacing::Sm, Density::Cozy))
                .py(px(6.0))
                .rounded(radius(Radius::Sm))
                .bg(with_alpha(palette.random, 0.1))
                .border(BORDER_THIN)
                .border_color(palette.random)
                .child(icon(Icon::AlertTriangle, px(13.0), palette.random))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.random)
                        .child(message.clone()),
                )
                .into_any_element(),
        )
    }

    fn chord_chip(&self, label: impl Into<SharedString>, palette: &ForgePalette) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(label.into())
            .into_any_element()
    }

    fn entry_row(
        &self,
        entry: &'static ShortcutEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let label = div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_primary)
            .child(tr!(entry.label_key));

        let mut controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));

        if self.rebinding == Some(entry.id) {
            controls = controls
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(radius(Radius::Sm))
                        .bg(with_alpha(palette.brand, 0.12))
                        .border(BORDER_THIN)
                        .border_color(with_alpha(palette.brand, 0.5))
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.brand)
                        .child(tr!("settings_shortcuts_capture_prompt")),
                )
                .child(ghost_button(tr!("common_cancel"), palette).on_click(
                    SharedString::from(format!("shortcut-cancel-{}", entry.id)),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_capture(cx)),
                ));
        } else {
            match self.overrides.chord_of(entry) {
                Some(chord) => {
                    controls = controls.child(self.chord_chip(chord.to_owned(), palette))
                }
                None => {
                    controls = controls.child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_faint)
                            .child(tr!("settings_shortcuts_unbound")),
                    );
                }
            }
            let id = entry.id;
            controls = controls.child(
                ghost_button(tr!("settings_shortcuts_rebind"), palette).on_click(
                    SharedString::from(format!("shortcut-rebind-{id}")),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.start_rebind(id, cx)),
                ),
            );
            if self.overrides.is_overridden(id) {
                controls = controls.child(
                    ghost_button(tr!("settings_shortcuts_reset"), palette).on_click(
                        SharedString::from(format!("shortcut-reset-{id}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_entry(id, cx)),
                    ),
                );
            }
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .py(px(6.0))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(label)
            .child(controls)
    }

    fn fixed_section(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let heading = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("settings_shortcuts_fixed_section"));

        let fixed_row = |label: SharedString, chord: &'static str, this: &Self| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.text_secondary)
                        .child(label),
                )
                .child(this.chord_chip(chord, palette))
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(heading)
            .child(fixed_row(
                tr!("settings_shortcuts_fixed_enter").into(),
                "Enter",
                self,
            ))
            .child(fixed_row(
                tr!("settings_shortcuts_fixed_escape").into(),
                "Esc",
                self,
            ))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("settings_shortcuts_fixed_note")),
            )
    }

    fn hold_ceiling_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let heading = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("settings_hold_ceiling_section"));

        let mut controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(toggle(self.hold_ceiling_on, palette).on_click(
                "hotkey-hold-ceiling-toggle",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_hold_ceiling(cx)),
            ));
        if self.hold_ceiling_on {
            controls = controls
                .child(
                    div()
                        .w(CEILING_INPUT_W)
                        .child(self.hold_ceiling_input.clone()),
                )
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("settings_hold_ceiling_unit")),
                );
        } else {
            controls = controls.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("settings_hold_ceiling_off")),
            );
        }

        let hint = if self.hold_ceiling_invalid {
            tr!(
                "settings_hold_ceiling_invalid",
                min = CEILING_MIN_SECS as i64,
                max = CEILING_MAX_SECS as i64
            )
        } else {
            tr!("settings_hold_ceiling_hint")
        };
        let hint_ink = if self.hold_ceiling_invalid {
            palette.random
        } else {
            palette.text_faint
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(heading)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("settings_hold_ceiling_label")),
                    )
                    .child(controls),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(hint_ink)
                    .child(hint),
            )
    }

    fn conflict_overlay(
        &self,
        conflict: &ShortcutConflict,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner_label = shortcut_entry(conflict.owner_id)
            .map(|entry| tr!(entry.label_key))
            .unwrap_or_default();

        let card = confirm_modal(
            tr!("settings_shortcuts_conflict_title"),
            tr!(
                "settings_shortcuts_conflict_body",
                chord = conflict.chord.as_str(),
                owner = owner_label
            ),
            ConfirmTone::Destructive,
            palette,
        )
        .on_cancel(
            "shortcut-conflict-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_cancel(cx)),
        )
        .on_confirm(
            "shortcut-conflict-steal",
            tr!("settings_shortcuts_conflict_steal"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_steal(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("shortcut-conflict-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.conflict_cancel(cx));
            })
            .into_any_element()
    }
}

impl Render for SettingsShortcutsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        drive_overlay_focus(
            self.conflict.is_some(),
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let mut list = div().flex().flex_col();
        for entry in SHORTCUTS {
            list = list.child(self.entry_row(entry, &palette, density, cx));
        }

        let reset_all = ghost_button(tr!("settings_shortcuts_reset_all"), &palette).on_click(
            "shortcut-reset-all",
            cx.listener(|this, _: &ClickEvent, _, cx| this.reset_all(cx)),
        );

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.header(&palette, density));

        if let Some(banner) = self.error_banner(&palette) {
            body = body.child(banner);
        }

        body = body
            .child(list)
            .child(self.fixed_section(&palette, density))
            .child(self.hold_ceiling_section(&palette, density, cx))
            .child(div().child(reset_all));

        let mut root = div().relative().size_full().child(body);
        if let Some(conflict) = &self.conflict {
            let overlay = self.conflict_overlay(conflict, &palette, cx);
            root = root.child(overlay);
        }
        root
    }
}
