use forge_components::TINT_STRONG_FILL_ALPHA;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, ConfirmTone, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, InputEvent,
    OverlayPosition, Radius, SettingRow, Spacing, TextInput, body_family, confirm_modal, error_row,
    error_row_with_retry, ghost_button, mono_family, overlay, radius, scaled, setting_block,
    setting_groups, setting_row, setting_rows, spacing, toggle, tr, with_alpha,
};
use forge_hotkey::{DEFAULT_HOLD_CEILING_SECS, HotkeyClient};
use forge_storage::{DataProvider, SettingsRepo, StorageError};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Keystroke, Pixels, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::actions::{SHORTCUTS, ShortcutEntry, shortcut_entry};
use crate::async_bridge::{self, Loadable, SnapshotWriter, WriteOutcome};
use crate::navigation::SettingsSection;
use crate::settings_pane_header::SettingsPaneHeader;
use crate::shortcut_overrides::{ChordVerdict, ShortcutOverrides, load_overrides, save_overrides};
use forge_components::ActivePresentation;
use forge_hotkey::{load_hold_ceiling, save_hold_ceiling};

const CEILING_MIN_SECS: u64 = 1;
const CEILING_MAX_SECS: u64 = 3600;
const CEILING_INPUT_W: Pixels = px(72.0);
const UNKNOWN_CHORD: &str = "-";
const CHIP_PAD_H: Pixels = px(8.0);
const CHIP_PAD_V: Pixels = px(3.0);
const CAPTURE_BORDER_ALPHA: f32 = 0.5;

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
            load_overrides(repo),
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
            load_overrides(repo),
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
        result: Result<ShortcutOverrides, StorageError>,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = &result {
            tracing::warn!(error = %e, "failed to load keyboard shortcuts");
        }
        self.overrides = Loadable::settled(result);
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
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
        let density = cx.density();
        if let Some(reason) = self.overrides.failure() {
            return Some(
                error_row_with_retry(
                    "shortcut-load-retry",
                    tr!("settings_shortcuts_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_load(cx)),
                    density,
                )
                .into_any_element(),
            );
        }
        let message = self.rebind_error.as_ref().or(self.save_error.as_ref())?;
        Some(error_row(message.clone(), palette, density).into_any_element())
    }

    fn chord_chip(
        &self,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .px(scaled(CHIP_PAD_H, density))
            .py(scaled(CHIP_PAD_V, density))
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
                        .px(scaled(CHIP_PAD_H, density))
                        .py(scaled(CHIP_PAD_V, density))
                        .rounded(radius(Radius::Sm))
                        .bg(with_alpha(palette.brand, TINT_STRONG_FILL_ALPHA))
                        .border(BORDER_THIN)
                        .border_color(with_alpha(palette.brand, CAPTURE_BORDER_ALPHA))
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
                    controls = controls.child(self.chord_chip(chord.to_owned(), palette, density))
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
                .child(self.chord_chip(UNKNOWN_CHORD, palette, density))
                .child(ghost_button(tr!("settings_shortcuts_rebind"), palette).disabled(true));
        }

        setting_row(tr!(entry.label_key), None, controls, palette, density)
    }

    fn fixed_section(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let rows = setting_rows(palette)
            .row(setting_row(
                tr!("settings_shortcuts_fixed_enter"),
                None,
                self.chord_chip("Enter", palette, density),
                palette,
                density,
            ))
            .row(setting_row(
                tr!("settings_shortcuts_fixed_escape"),
                None,
                self.chord_chip("Esc", palette, density),
                palette,
                density,
            ));

        setting_block(
            tr!("settings_shortcuts_fixed_section"),
            Some(tr!("settings_shortcuts_fixed_note").into()),
            rows,
            palette,
            density,
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
            row = row.hint_color(palette.danger);
        }

        setting_block(
            tr!("settings_hold_ceiling_section"),
            None,
            setting_rows(palette).row(row),
            palette,
            density,
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
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
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
        let body = setting_groups(groups, density);

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

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, TestAppContext};
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::test_support::{
        ReadFault, SETTINGS_READ_FAILED, SETTINGS_WRITE_FAILED, SettingWrite, TestBackend,
        install_presentation, pump, runtime, test_backend, writes_to,
    };
    use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;

    const HOME: &str = "nav.home";
    const HOME_DEFAULT: &str = "ctrl-1";
    const CHAT: &str = "nav.chat";
    const CHAT_DEFAULT: &str = "ctrl-2";
    const STORED: &str = r#"{"nav.chat":{"chord":"ctrl-8"}}"#;
    const SETTLE_ROUNDS: usize = 3;

    struct Harness {
        rt: tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
        writes: UnboundedReceiver<SettingWrite>,
    }

    impl Harness {
        fn new(stored: &str) -> Self {
            let (backend, writes) = test_backend();
            backend.seed(KEYBOARD_SHORTCUTS, stored);
            Self {
                rt: runtime(),
                backend,
                writes,
            }
        }

        fn open(&self, cx: &mut TestAppContext) -> Entity<SettingsShortcutsView> {
            install_presentation(cx);
            let backend = Arc::clone(&self.backend) as Arc<dyn DataProvider>;
            let handle = self.rt.handle().clone();
            let view =
                cx.update(|cx| cx.new(|cx| SettingsShortcutsView::new(backend, handle, None, cx)));
            self.settle(cx);
            view
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn shortcut_writes(&mut self) -> Vec<String> {
            writes_to(&mut self.writes, KEYBOARD_SHORTCUTS)
        }
    }

    fn ctrl(key: &str) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            key: key.to_owned(),
            key_char: None,
        }
    }

    fn shown_chord(
        view: &Entity<SettingsShortcutsView>,
        id: &str,
        cx: &mut TestAppContext,
    ) -> Option<String> {
        let entry = shortcut_entry(id).unwrap();
        view.read_with(cx, |v, _| {
            v.overrides
                .loaded()
                .and_then(|overrides| overrides.chord_of(entry).map(str::to_owned))
        })
    }

    type Edit = fn(&mut SettingsShortcutsView, &mut Context<SettingsShortcutsView>);

    fn edit_paths() -> [(&'static str, Edit); 5] {
        [
            ("rebind by capture", |v, cx| {
                v.start_rebind(HOME, cx);
                v.on_capture_keystroke(ctrl("9"), cx);
            }),
            ("a keystroke for an already armed capture", |v, cx| {
                v.rebinding = Some(HOME);
                v.on_capture_keystroke(ctrl("9"), cx);
            }),
            ("steal a taken chord", |v, cx| {
                v.conflict = Some(ShortcutConflict {
                    target_id: HOME,
                    owner_id: CHAT,
                    chord: CHAT_DEFAULT.to_owned(),
                });
                v.conflict_steal(cx);
            }),
            ("reset one shortcut", |v, cx| v.reset_entry(CHAT, cx)),
            ("reset every shortcut", |v, cx| v.reset_all(cx)),
        ]
    }

    #[gpui::test]
    fn no_edit_after_a_failed_load_writes_over_the_stored_shortcuts(cx: &mut TestAppContext) {
        let unreachable = StorageError::Connection {
            reason: SETTINGS_READ_FAILED.to_owned(),
        }
        .to_string();
        let undecodable = format!("stored setting '{KEYBOARD_SHORTCUTS}' cannot be decoded");
        for (fault, stored, failing_read, reason) in [
            ("unreachable store", STORED, true, unreachable),
            ("undecodable value", "not json", false, undecodable),
        ] {
            for (case, edit) in edit_paths() {
                let mut harness = Harness::new(stored);
                if failing_read {
                    harness
                        .backend
                        .fault_reads(KEYBOARD_SHORTCUTS, ReadFault::Fails);
                }
                let view = harness.open(cx);

                view.update(cx, edit);
                harness.settle(cx);

                let failure = view.read_with(cx, |v, _| v.overrides.failure().map(str::to_owned));
                assert_eq!(
                    (harness.shortcut_writes(), failure),
                    (Vec::<String>::new(), Some(reason.clone())),
                    "{fault}: {case}"
                );
            }
        }
    }

    #[gpui::test]
    fn no_edit_while_the_load_is_pending_writes_and_the_load_still_lands_intact(
        cx: &mut TestAppContext,
    ) {
        for (case, edit) in edit_paths() {
            let mut harness = Harness::new(STORED);
            let gate = Arc::new(tokio::sync::Notify::new());
            harness
                .backend
                .fault_reads(KEYBOARD_SHORTCUTS, ReadFault::Held(Arc::clone(&gate)));
            let view = harness.open(cx);

            view.update(cx, edit);
            harness.settle(cx);
            let writes_while_pending = harness.shortcut_writes();
            harness.backend.heal_reads(KEYBOARD_SHORTCUTS);
            gate.notify_one();
            harness.settle(cx);

            assert_eq!(
                (writes_while_pending, shown_chord(&view, CHAT, cx)),
                (Vec::<String>::new(), Some("ctrl-8".to_owned())),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn retry_after_a_failed_load_restores_the_stored_shortcuts_and_a_rebind_keeps_them(
        cx: &mut TestAppContext,
    ) {
        let mut harness = Harness::new(STORED);
        harness
            .backend
            .fault_reads(KEYBOARD_SHORTCUTS, ReadFault::Fails);
        let view = harness.open(cx);
        harness.backend.heal_reads(KEYBOARD_SHORTCUTS);

        view.update(cx, |v, cx| v.retry_load(cx));
        harness.settle(cx);
        view.update(cx, |v, cx| {
            v.start_rebind(HOME, cx);
            v.on_capture_keystroke(ctrl("9"), cx);
        });
        harness.settle(cx);

        let written: Vec<(Option<String>, Option<String>)> = harness
            .shortcut_writes()
            .iter()
            .map(|raw| {
                let restored = crate::test_support::stored_shortcut_overrides(Some(raw)).unwrap();
                let chord_of = |id| {
                    restored
                        .chord_of(shortcut_entry(id).unwrap())
                        .map(str::to_owned)
                };
                (chord_of(HOME), chord_of(CHAT))
            })
            .collect();
        assert_eq!(
            written,
            vec![(Some("ctrl-9".to_owned()), Some("ctrl-8".to_owned()))]
        );
    }

    #[gpui::test]
    fn a_failed_write_reports_the_error_and_rolls_back_to_the_stored_shortcuts(
        cx: &mut TestAppContext,
    ) {
        let harness = Harness::new(STORED);
        harness.backend.fail_writes(KEYBOARD_SHORTCUTS);
        let view = harness.open(cx);

        view.update(cx, |v, cx| {
            v.start_rebind(HOME, cx);
            v.on_capture_keystroke(ctrl("9"), cx);
        });
        harness.settle(cx);

        let save_error = view.read_with(cx, |v, _| v.save_error.clone());
        let read_only = StorageError::Connection {
            reason: SETTINGS_WRITE_FAILED.to_owned(),
        }
        .to_string();
        assert_eq!(
            (save_error, shown_chord(&view, HOME, cx)),
            (Some(read_only), Some(HOME_DEFAULT.to_owned()))
        );
    }
}
