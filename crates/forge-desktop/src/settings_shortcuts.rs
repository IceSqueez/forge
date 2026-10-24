use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, InputEvent,
    OverlayPosition, Radius, SettingRow, Spacing, TextInput, body_family, confirm_modal, error_row,
    error_row_with_retry, ghost_button, mono_family, overlay, radius, setting_block,
    setting_groups, setting_row, setting_rows, spacing, toggle, tr, with_alpha,
};
use forge_hotkey::{DEFAULT_HOLD_CEILING_SECS, HotkeyClient};
use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_storage::{DataProvider, SettingsRepo, StorageError};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Keystroke, Pixels, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::actions::{SHORTCUTS, ShortcutEntry, shortcut_entry};
use crate::async_bridge::{self, Loadable, SnapshotWriter, WriteOutcome};
use crate::settings::SettingsSection;
use crate::settings_pane_header::SettingsPaneHeader;
use crate::shortcut_overrides::{ChordVerdict, ShortcutOverrides, save_overrides};
use forge_components::ActivePresentation;
use forge_hotkey::{load_hold_ceiling, save_hold_ceiling};

const CEILING_MIN_SECS: u64 = 1;
const CEILING_MAX_SECS: u64 = 3600;
const CEILING_INPUT_W: Pixels = px(72.0);
const UNKNOWN_CHORD: &str = "-";

struct ShortcutConflict {
    target_id: &'static str,
    owner_id: &'static str,
    chord: String,
}

pub struct SettingsShortcutsView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    hotkey_client: Option<Arc<HotkeyClient>>,
    overrides: Loadable<ShortcutOverrides>,
    writer: SnapshotWriter,
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
                InputEvent::Cancelled | InputEvent::Blurred(_) => {}
            },
        )];

        let mut view = Self {
            backend,
            hotkey_client,
            overrides: Loadable::Loading,
            writer: SnapshotWriter::new(&rt_handle),
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
            rt_handle,
            capture_sub: None,
            _subs: subs,
        };
        view.load(cx);
        view.load_hold_ceiling(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.overrides = Loadable::Loading;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.get_string(KEYBOARD_SHORTCUTS).await },
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
        cx.notify();
    }

    fn rollback(&mut self, cx: &mut Context<Self>) {
        self.overrides = Loadable::Loading;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.get_string(KEYBOARD_SHORTCUTS).await },
            |this, result, cx| {
                this.apply_loaded(result, cx);
                if let Some(overrides) = this.overrides.loaded() {
                    overrides.apply(cx);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn retry_load(&mut self, cx: &mut Context<Self>) {
        self.save_error = None;
        self.load(cx);
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
        result: Result<Option<String>, StorageError>,
        cx: &mut Context<Self>,
    ) {
        let decoded = result
            .map_err(|e| e.to_string())
            .and_then(|raw| ShortcutOverrides::from_stored(raw.as_deref()));
        if let Err(e) = &decoded {
            tracing::warn!(error = %e, "failed to load keyboard shortcuts");
        }
        self.overrides = Loadable::settled(decoded);
        cx.notify();
    }

    fn start_rebind(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if !self.overrides.is_loaded() {
            return;
        }
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
        if keystroke.key == forge_components::ESCAPE_KEY && !keystroke.modifiers.modified() {
            self.cancel_capture(cx);
            return;
        }
        let Some(overrides) = self.overrides.loaded_mut() else {
            self.cancel_capture(cx);
            return;
        };
        match overrides.verdict(&keystroke, id) {
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
                overrides.bind(id, chord);
                self.rebind_error = None;
                self.end_capture();
                self.persist_and_apply(cx);
            }
        }
    }

    fn conflict_steal(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        let Some(overrides) = self.overrides.loaded_mut() else {
            cx.notify();
            return;
        };
        overrides.unbind(conflict.owner_id);
        overrides.bind(conflict.target_id, conflict.chord);
        self.rebind_error = None;
        self.persist_and_apply(cx);
    }

    fn conflict_cancel(&mut self, cx: &mut Context<Self>) {
        self.conflict = None;
        cx.notify();
    }

    fn reset_entry(&mut self, id: &'static str, cx: &mut Context<Self>) {
        let Some(overrides) = self.overrides.loaded_mut() else {
            return;
        };
        overrides.reset(id);
        self.rebind_error = None;
        self.persist_and_apply(cx);
    }

    fn reset_all(&mut self, cx: &mut Context<Self>) {
        let Some(overrides) = self.overrides.loaded_mut() else {
            return;
        };
        overrides.reset_all();
        self.rebind_error = None;
        self.conflict = None;
        self.end_capture();
        self.persist_and_apply(cx);
    }

    fn persist_and_apply(&mut self, cx: &mut Context<Self>) {
        let Some(overrides) = self.overrides.loaded() else {
            return;
        };
        overrides.apply(cx);
        let map = overrides.snapshot();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.writer
            .write(save_overrides(repo, map), Self::apply_save_result, cx);
        cx.notify();
    }

    fn apply_save_result(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        match outcome {
            WriteOutcome::Written => self.save_error = None,
            WriteOutcome::Superseded => {}
            WriteOutcome::Failed(message) => {
                self.save_error = Some(message);
                self.rollback(cx);
            }
        }
    }

    fn error_banner(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(reason) = self.overrides.failure() {
            return Some(
                error_row_with_retry(
                    "shortcut-load-retry",
                    tr!("settings_shortcuts_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_load(cx)),
                )
                .into_any_element(),
            );
        }
        let message = self.rebind_error.as_ref().or(self.save_error.as_ref())?;
        Some(error_row(message.clone(), palette).into_any_element())
    }

    fn chord_chip(&self, label: impl Into<SharedString>, palette: &ForgePalette) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .font_family(mono_family())
            .text_size(FONT_XXS)
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
    ) -> SettingRow {
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
        } else if let Some(overrides) = self.overrides.loaded() {
            match overrides.chord_of(entry) {
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
            if overrides.is_overridden(id) {
                controls = controls.child(
                    ghost_button(tr!("settings_shortcuts_reset"), palette).on_click(
                        SharedString::from(format!("shortcut-reset-{id}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_entry(id, cx)),
                    ),
                );
            }
        } else {
            controls = controls
                .child(self.chord_chip(UNKNOWN_CHORD, palette))
                .child(ghost_button(tr!("settings_shortcuts_rebind"), palette).disabled(true));
        }

        setting_row(tr!(entry.label_key), None, controls, palette, density)
    }

    fn fixed_section(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let rows = setting_rows(palette)
            .row(setting_row(
                tr!("settings_shortcuts_fixed_enter"),
                None,
                self.chord_chip("Enter", palette),
                palette,
                density,
            ))
            .row(setting_row(
                tr!("settings_shortcuts_fixed_escape"),
                None,
                self.chord_chip("Esc", palette),
                palette,
                density,
            ));

        setting_block(
            tr!("settings_shortcuts_fixed_section"),
            Some(tr!("settings_shortcuts_fixed_note").into()),
            rows,
            palette,
        )
    }

    fn hold_ceiling_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

        let mut row = setting_row(
            tr!("settings_hold_ceiling_label"),
            Some(self.hold_ceiling_hint().into()),
            controls,
            palette,
            density,
        );
        if self.hold_ceiling_invalid {
            row = row.hint_color(palette.random);
        }

        setting_block(
            tr!("settings_hold_ceiling_section"),
            None,
            setting_rows(palette).row(row),
            palette,
        )
    }

    fn hold_ceiling_hint(&self) -> String {
        if self.hold_ceiling_invalid {
            tr!(
                "settings_hold_ceiling_invalid",
                min = CEILING_MIN_SECS as i64,
                max = CEILING_MAX_SECS as i64
            )
        } else {
            tr!("settings_hold_ceiling_hint")
        }
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
            .on_dismiss("shortcut-conflict-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.conflict_cancel(cx));
            })
            .into_any_element()
    }
}

impl Render for SettingsShortcutsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let mut list = setting_rows(&palette);
        for entry in SHORTCUTS {
            list = list.row(self.entry_row(entry, &palette, density, cx));
        }

        let reset_all = ghost_button(tr!("settings_shortcuts_reset_all"), &palette)
            .disabled(!self.overrides.is_loaded())
            .on_click(
                "shortcut-reset-all",
                cx.listener(|this, _: &ClickEvent, _, cx| this.reset_all(cx)),
            );

        let mut groups: Vec<AnyElement> = Vec::new();
        if let Some(banner) = self.error_banner(&palette, cx) {
            groups.push(banner);
        }
        groups.push(list.into_any_element());
        groups.push(self.fixed_section(&palette, density).into_any_element());
        groups.push(
            self.hold_ceiling_section(&palette, density, cx)
                .into_any_element(),
        );
        let body = setting_groups(groups);

        let header = SettingsPaneHeader::new(SettingsSection::Shortcuts)
            .subtitle(tr!("settings_shortcuts_subtitle"))
            .trailing(reset_all);
        let mut root = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(body);
        if let Some(conflict) = &self.conflict {
            let overlay = self.conflict_overlay(conflict, &palette, cx);
            root = root.child(overlay);
        }
        root
    }
}
