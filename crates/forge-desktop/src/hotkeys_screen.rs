use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    MenuPlacement, OverlayPosition, ToastKind, badge, body_family, card, confirm_modal,
    drive_overlay_focus, fmt_relative_time, icon, menu_button, menu_divider, menu_item,
    mono_family, overlay, pad_tile, page_frame, status_dot, toggle, tr,
};
use forge_events::Event;
use forge_hotkey::HotkeyClient;
use forge_runtime::EventBus;
use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_storage::{DataProvider, SettingsRepo, StorageError, set_bool_setting};
use forge_types::{ActionId, TriggerInstanceId};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, Keystroke, Pixels, Point, SharedString,
    Subscription, Task, Window, div, prelude::*, px,
};
use time::OffsetDateTime;

use crate::actions::{SHORTCUTS, ShortcutEntry, chord_caps, shortcut_entry};
use crate::app_shortcut_modal::{AppShortcutModal, AppShortcutModalEvent};
use crate::async_bridge::{self, BridgeFlow, ErrorSink, drain_events};
use crate::hotkey_action_modal::{
    ActionModalLaunch, BindingDraft, HotkeyActionModal, HotkeyActionModalEvent, keycaps,
};
use crate::hotkey_bindings::{
    BindingRow, HOTKEY_ENABLED_KEY, HOTKEY_EVENT_PREFIX, HOTKEY_PRESSED_KIND, conflict_count,
    delete_binding, do_bind, keystroke_to_combo, load_bindings, rebind_combo, registered_combos,
    relink_action, set_binding_enabled,
};
use crate::presentation::ActivePresentation;
use crate::shortcut_overrides::{ChordVerdict, ShortcutOverrides, save_overrides};
use crate::toasts::PushToast;

const ENABLE_FAILED_KIND: &str = "hotkey.engine.enable_failed";
const COMBO_FIELD: &str = "combo";
const COMBOS_FIELD: &str = "combos";

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const HERO_PAD_V: Pixels = px(14.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(14.0);
const HERO_MARGIN_B: Pixels = px(14.0);
const HERO_TILE: Pixels = px(40.0);
const HERO_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = px(15.0);
const HERO_BLURB_MT: Pixels = px(1.0);
const HERO_TOGGLE_GAP: Pixels = px(8.0);
const LABEL_FS: Pixels = px(11.5);

const STAT_GAP: Pixels = px(10.0);
const STAT_MARGIN_B: Pixels = px(14.0);
const STAT_PAD_V: Pixels = px(10.0);
const STAT_PAD_H: Pixels = px(12.0);
const STAT_LABEL_MB: Pixels = px(4.0);
const STAT_HINT_MT: Pixels = px(2.0);

const SECTION_LABEL_FS: Pixels = px(9.5);
const SECTION_LABEL_MT: Pixels = px(4.0);
const SECTION_LABEL_MB: Pixels = px(8.0);
const SECTION_HINT_FS: Pixels = px(10.0);
const LIST_GAP: Pixels = px(6.0);

const ROW_PAD_V: Pixels = px(9.0);
const ROW_PAD_H: Pixels = px(12.0);
const ROW_GAP: Pixels = px(10.0);
const KEYCAPS_MIN_W: Pixels = px(130.0);
const SCOPE_BADGE_FS: Pixels = px(9.0);
const ARROW_GLYPH: Pixels = px(13.0);
const ACCENT_DOT: Pixels = px(6.0);
const TARGET_FS: Pixels = px(12.0);
const ROW_OFF_OPACITY: f32 = 0.55;
const UNBOUND_FS: Pixels = px(11.5);
const UNBOUND_RADIUS: Pixels = px(5.0);
const UNBOUND_PAD_V: Pixels = px(3.0);
const UNBOUND_PAD_H: Pixels = px(8.0);

const ADD_BAR_PAD_H: Pixels = px(12.0);
const ADD_BAR_RADIUS: Pixels = px(9.0);
const ADD_BAR_GLYPH: Pixels = px(13.0);
const ADD_BAR_FS: Pixels = px(12.0);
const KBD_FS: Pixels = px(10.0);
const KBD_ML: Pixels = px(6.0);
const KBD_PAD_V: Pixels = px(1.0);
const KBD_PAD_H: Pixels = px(6.0);
const KBD_RADIUS: Pixels = px(4.0);

const CAPTURE_PAD_V: Pixels = px(11.0);
const CAPTURE_GAP: Pixels = px(8.0);
const CAPTURE_GLYPH: Pixels = px(14.0);
const CAPTURE_CANCEL_ML: Pixels = px(6.0);

const FOOTER_FS: Pixels = px(10.5);
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_GAP: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);
const FOOTER_SEPARATOR: &str = "·";

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);

const NO_VALUE: &str = "-";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    Off,
    Add,
    Rebind(TriggerInstanceId),
    Modal(Option<TriggerInstanceId>),
    App(&'static str),
    AppModal(&'static str),
}

impl Capture {
    fn target(self) -> Option<TriggerInstanceId> {
        match self {
            Capture::Rebind(id) | Capture::Modal(Some(id)) => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowKey {
    Global(TriggerInstanceId),
    App(&'static str),
}

struct ConflictPrompt {
    combo: String,
    holder: String,
    capture: Capture,
    app_owner: Option<&'static str>,
}

struct DeletePrompt {
    combo: String,
    action: Option<String>,
}

struct LastFired {
    combo: String,
    at: OffsetDateTime,
}

struct OpenModal {
    view: Entity<HotkeyActionModal>,
    editing: Option<TriggerInstanceId>,
    _sub: Subscription,
}

struct OpenAppModal {
    id: &'static str,
    view: Entity<AppShortcutModal>,
    _sub: Subscription,
}

pub struct HotkeysScreenView {
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    enabled: bool,
    bindings: Vec<BindingRow>,
    shortcuts: ShortcutOverrides,
    conflicts: usize,
    last_fired: Option<LastFired>,
    capture: Capture,
    capture_sub: Option<Subscription>,
    conflict: Option<ConflictPrompt>,
    delete_prompt: Option<DeletePrompt>,
    modal: Option<OpenModal>,
    app_modal: Option<OpenAppModal>,
    menu_open: Option<RowKey>,
    menu_click_pos: Option<Point<Pixels>>,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    _bus_bridge: Task<()>,
}

impl HotkeysScreenView {
    pub fn new(
        client: Arc<HotkeyClient>,
        backend: Arc<dyn DataProvider>,
        settings_repo: Arc<dyn SettingsRepo>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let bus_bridge = Self::spawn_bus_bridge(bus, cx);
        let mut view = Self {
            enabled: client.is_enabled(),
            conflicts: conflict_count(&client),
            client,
            backend,
            settings_repo,
            rt_handle,
            bindings: Vec::new(),
            shortcuts: ShortcutOverrides::default(),
            last_fired: None,
            capture: Capture::Off,
            capture_sub: None,
            conflict: None,
            delete_prompt: None,
            modal: None,
            app_modal: None,
            menu_open: None,
            menu_click_pos: None,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            _bus_bridge: bus_bridge,
        };
        view.load(cx);
        view.load_shortcuts(cx);
        view
    }

    fn spawn_bus_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                if !batch
                    .iter()
                    .any(|event| event.kind.starts_with(HOTKEY_EVENT_PREFIX))
                {
                    return BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.on_hotkey_events(batch, cx)) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        })
    }

    fn on_hotkey_events(&mut self, batch: &[Event], cx: &mut Context<Self>) {
        for event in batch {
            match event.kind.as_str() {
                HOTKEY_PRESSED_KIND => {
                    if let Some(combo) = event.payload.get(COMBO_FIELD).and_then(|v| v.as_str()) {
                        self.last_fired = Some(LastFired {
                            combo: combo.to_owned(),
                            at: event.timestamp,
                        });
                    }
                }
                ENABLE_FAILED_KIND => {
                    let failed = event
                        .payload
                        .get(COMBOS_FIELD)
                        .and_then(|v| v.as_array())
                        .map(|combos| combos.len())
                        .unwrap_or(0);
                    if failed > 0 {
                        cx.push_toast(
                            ToastKind::Warn,
                            tr!("hotkeys_toast_enable_partial", count = failed as i64),
                        );
                    }
                }
                _ => {}
            }
        }
        self.refresh_from_client(cx);
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let backend = Arc::clone(&self.backend);
        let registered = registered_combos(&self.client);
        async_bridge::run_async(
            &self.rt_handle,
            load_bindings(backend, registered),
            |this, result, cx| this.apply_bindings(result, cx),
            cx,
        );
    }

    fn apply_bindings(&mut self, result: Result<Vec<BindingRow>, String>, cx: &mut Context<Self>) {
        match result {
            Ok(rows) => {
                self.bindings = rows;
                self.refresh_from_client(cx);
            }
            Err(message) => self.on_repo_error(&message, cx),
        }
    }

    fn load_shortcuts(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.get_string(KEYBOARD_SHORTCUTS).await },
            |this, result, cx| this.apply_shortcuts(result, cx),
            cx,
        );
    }

    fn apply_shortcuts(
        &mut self,
        result: Result<Option<String>, StorageError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(raw) => self.shortcuts.replace_stored(raw.as_deref()),
            Err(e) => self.on_repo_error(&e.to_string(), cx),
        }
        cx.notify();
    }

    fn persist_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.apply(cx);
        let repo = Arc::clone(&self.settings_repo);
        let map = self.shortcuts.snapshot();
        async_bridge::run_async(
            &self.rt_handle,
            save_overrides(repo, map),
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => cx.notify(),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn refresh_from_client(&mut self, cx: &mut Context<Self>) {
        let registered = registered_combos(&self.client);
        for row in &mut self.bindings {
            row.registered = registered.iter().any(|(_, combo)| combo == &row.combo);
        }
        self.conflicts = conflict_count(&self.client);
        self.enabled = self.client.is_enabled();
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(error = %message, "hotkey binding operation failed");
        cx.push_toast(
            ToastKind::Error,
            tr!("hotkeys_toast_error", message = message),
        );
        cx.notify();
    }

    fn enabled_count(&self) -> usize {
        self.bindings.iter().filter(|row| row.enabled).count()
    }

    fn global_count(&self) -> usize {
        self.bindings.iter().filter(|row| row.registered).count()
    }

    fn total_count(&self) -> usize {
        self.bindings.len() + SHORTCUTS.len()
    }

    fn active_count(&self) -> usize {
        self.enabled_count() + self.shortcuts.bound_count()
    }

    fn toggle_engine(&mut self, cx: &mut Context<Self>) {
        let previous = self.enabled;
        self.enabled = !previous;
        let enabled = self.enabled;
        let client = Arc::clone(&self.client);
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::optimistic(
            &self.rt_handle,
            previous,
            async move {
                if enabled {
                    client.enable().await.map_err(|e| e.to_string())?;
                } else {
                    client.disable().await.map_err(|e| e.to_string())?;
                }
                set_bool_setting(repo.as_ref(), HOTKEY_ENABLED_KEY, enabled)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, previous, _message, cx| {
                this.enabled = previous;
                ErrorSink::Toast.report(tr!("hotkeys_toggle_failed"), cx);
            },
            cx,
        );
        cx.notify();
    }

    fn start_capture(&mut self, capture: Capture, cx: &mut Context<Self>) {
        self.capture = capture;
        self.menu_open = None;
        let weak = cx.entity().downgrade();
        self.capture_sub = Some(cx.intercept_keystrokes(move |event, _window, cx| {
            let keystroke = event.keystroke.clone();
            let handled = weak
                .update(cx, |this, cx| this.on_capture_keystroke(keystroke, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        cx.notify();
    }

    fn on_capture_keystroke(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) -> bool {
        if self.capture == Capture::Off {
            return false;
        }
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.cancel_capture(cx);
            return true;
        }
        if let Capture::App(id) | Capture::AppModal(id) = self.capture {
            self.on_app_capture(id, &keystroke, cx);
            return true;
        }
        let Some(combo) = keystroke_to_combo(&keystroke) else {
            return true;
        };
        self.on_capture_combo(combo, cx);
        true
    }

    fn on_app_capture(&mut self, id: &'static str, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let capture = self.capture;
        match self.shortcuts.verdict(keystroke, id) {
            ChordVerdict::Unusable => {}
            ChordVerdict::NeedsModifier => {
                self.end_capture();
                self.stop_app_modal_capture(cx);
                cx.push_toast(
                    ToastKind::Warn,
                    tr!("settings_shortcuts_error_needs_modifier"),
                );
                cx.notify();
            }
            ChordVerdict::Taken { owner_id, chord } => {
                self.end_capture();
                self.conflict = Some(ConflictPrompt {
                    combo: chord,
                    holder: shortcut_entry(owner_id)
                        .map(|entry| tr!(entry.label_key))
                        .unwrap_or_default(),
                    capture,
                    app_owner: Some(owner_id),
                });
                cx.notify();
            }
            ChordVerdict::Free(chord) => {
                self.end_capture();
                self.apply_capture(capture, chord, cx);
            }
        }
    }

    fn end_capture(&mut self) {
        self.capture = Capture::Off;
        self.capture_sub = None;
    }

    fn stop_app_modal_capture(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = &self.app_modal {
            open.view.update(cx, |modal, cx| modal.cancel_capture(cx));
        }
    }

    fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
        self.capture_sub = None;
        if matches!(capture, Capture::Modal(_))
            && let Some(open) = &self.modal
        {
            open.view.update(cx, |modal, cx| modal.cancel_capture(cx));
        }
        if matches!(capture, Capture::AppModal(_)) {
            self.stop_app_modal_capture(cx);
        }
        cx.notify();
    }

    fn on_capture_combo(&mut self, combo: String, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
        self.capture_sub = None;
        let holder = self
            .bindings
            .iter()
            .find(|row| row.combo == combo && Some(row.instance_id) != capture.target())
            .map(|row| match row.action.as_ref() {
                Some((_, name)) => name.clone(),
                None => tr!("hotkeys_conflict_holder_unassigned"),
            });
        match holder {
            Some(holder) => {
                self.conflict = Some(ConflictPrompt {
                    combo,
                    holder,
                    capture,
                    app_owner: None,
                });
                cx.notify();
            }
            None => self.apply_capture(capture, combo, cx),
        }
    }

    fn apply_capture(&mut self, capture: Capture, combo: String, cx: &mut Context<Self>) {
        match capture {
            Capture::Off => cx.notify(),
            Capture::Add => self.open_modal(None, combo, None, cx),
            Capture::Rebind(id) => self.rebind(id, combo, cx),
            Capture::Modal(_) => {
                if let Some(open) = &self.modal {
                    open.view
                        .update(cx, |modal, cx| modal.apply_capture(combo, cx));
                }
                cx.notify();
            }
            Capture::App(id) => {
                self.shortcuts.bind(id, combo);
                self.persist_shortcuts(cx);
            }
            Capture::AppModal(_) => {
                if let Some(open) = &self.app_modal {
                    open.view
                        .update(cx, |modal, cx| modal.apply_capture(combo, cx));
                }
                cx.notify();
            }
        }
    }

    fn conflict_replace(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.conflict.take() else {
            return;
        };
        let ConflictPrompt {
            combo,
            capture,
            app_owner,
            ..
        } = prompt;
        if let Some(owner) = app_owner {
            self.shortcuts.unbind(owner);
            self.apply_capture(capture, combo, cx);
            return;
        }
        let client = Arc::clone(&self.client);
        let backend = Arc::clone(&self.backend);
        let doomed = combo.clone();
        async_bridge::run_async(
            &self.rt_handle,
            delete_binding(client, backend, doomed),
            move |this, result: Result<(), String>, cx| match result {
                Ok(()) => {
                    if capture.target().is_none() {
                        this.load(cx);
                    }
                    this.apply_capture(capture, combo, cx);
                }
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn conflict_cancel(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.conflict.take() else {
            return;
        };
        if matches!(prompt.capture, Capture::Modal(_))
            && let Some(open) = &self.modal
        {
            open.view.update(cx, |modal, cx| modal.cancel_capture(cx));
        }
        if matches!(prompt.capture, Capture::AppModal(_)) {
            self.stop_app_modal_capture(cx);
        }
        cx.notify();
    }

    fn open_modal(
        &mut self,
        instance_id: Option<TriggerInstanceId>,
        combo: String,
        linked_action: Option<ActionId>,
        cx: &mut Context<Self>,
    ) {
        let launch = ActionModalLaunch {
            instance_id,
            combo,
            linked_action,
        };
        let action_repo = self.backend.action_repo();
        let rt_handle = self.rt_handle.clone();
        let view = cx.new(|cx| HotkeyActionModal::new(launch, action_repo, rt_handle, cx));
        let sub = cx.subscribe(&view, Self::on_modal_event);
        self.modal = Some(OpenModal {
            view,
            editing: instance_id,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _view: Entity<HotkeyActionModal>,
        event: &HotkeyActionModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HotkeyActionModalEvent::Recapture => {
                let editing = self.modal.as_ref().and_then(|open| open.editing);
                self.start_capture(Capture::Modal(editing), cx);
            }
            HotkeyActionModalEvent::Cancel => self.close_modal(cx),
            HotkeyActionModalEvent::Save(draft) => {
                let draft = BindingDraft {
                    instance_id: draft.instance_id,
                    combo: draft.combo.clone(),
                    action_id: draft.action_id,
                };
                self.close_modal(cx);
                self.persist_draft(draft, cx);
            }
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self.capture = Capture::Off;
        self.capture_sub = None;
        cx.notify();
    }

    fn persist_draft(&mut self, draft: BindingDraft, cx: &mut Context<Self>) {
        let client = Arc::clone(&self.client);
        let backend = Arc::clone(&self.backend);
        let action_id = draft.action_id;
        let combo = draft.combo;
        let Some(instance_id) = draft.instance_id else {
            self.run_reload(do_bind(client, backend, combo, action_id), cx);
            return;
        };
        let previous = self
            .bindings
            .iter()
            .find(|row| row.instance_id == instance_id)
            .map(|row| row.combo.clone())
            .unwrap_or_else(|| combo.clone());
        self.run_reload(
            async move {
                rebind_combo(
                    Arc::clone(&client),
                    Arc::clone(&backend),
                    instance_id,
                    previous,
                    combo,
                )
                .await?;
                relink_action(backend, instance_id, action_id).await
            },
            cx,
        );
    }

    fn rebind(&mut self, instance_id: TriggerInstanceId, combo: String, cx: &mut Context<Self>) {
        let Some(previous) = self
            .bindings
            .iter()
            .find(|row| row.instance_id == instance_id)
            .map(|row| row.combo.clone())
        else {
            cx.notify();
            return;
        };
        let client = Arc::clone(&self.client);
        let backend = Arc::clone(&self.backend);
        self.run_reload(
            rebind_combo(client, backend, instance_id, previous, combo),
            cx,
        );
    }

    fn change_action(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        let Some(row) = self
            .bindings
            .iter()
            .find(|row| row.instance_id == instance_id)
        else {
            cx.notify();
            return;
        };
        let combo = row.combo.clone();
        let linked = row.action.as_ref().map(|(id, _)| *id);
        self.open_modal(Some(instance_id), combo, linked, cx);
    }

    fn prompt_delete(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.delete_prompt = self
            .bindings
            .iter()
            .find(|row| row.instance_id == instance_id)
            .map(|row| DeletePrompt {
                combo: row.combo.clone(),
                action: row.action.as_ref().map(|(_, name)| name.clone()),
            });
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.delete_prompt = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.delete_prompt.take() else {
            return;
        };
        let client = Arc::clone(&self.client);
        let backend = Arc::clone(&self.backend);
        self.run_reload(delete_binding(client, backend, prompt.combo), cx);
    }

    fn run_reload(
        &mut self,
        work: impl Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            work,
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => this.load(cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_binding(&mut self, instance_id: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(row) = self
            .bindings
            .iter_mut()
            .find(|row| row.instance_id == instance_id)
        else {
            return;
        };
        let previous = row.enabled;
        row.enabled = !previous;
        let enabled = row.enabled;
        let backend = Arc::clone(&self.backend);
        async_bridge::optimistic(
            &self.rt_handle,
            previous,
            set_binding_enabled(backend, instance_id, enabled),
            move |this, previous, _message, cx| {
                if let Some(row) = this
                    .bindings
                    .iter_mut()
                    .find(|row| row.instance_id == instance_id)
                {
                    row.enabled = previous;
                }
                ErrorSink::Toast.report(tr!("hotkeys_toggle_binding_failed"), cx);
            },
            cx,
        );
        cx.notify();
    }

    fn reset_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.shortcuts.reset(id);
        self.persist_shortcuts(cx);
    }

    fn toggle_app_binding(&mut self, id: &'static str, cx: &mut Context<Self>) {
        let enabled = !self.shortcuts.is_enabled(id);
        self.shortcuts.set_enabled(id, enabled);
        self.persist_shortcuts(cx);
    }

    fn edit_app_binding(&mut self, entry: &'static ShortcutEntry, cx: &mut Context<Self>) {
        self.menu_open = None;
        let chord = self.shortcuts.chord_of(entry).map(str::to_owned);
        let view = cx.new(|_| AppShortcutModal::new(entry, chord));
        let sub = cx.subscribe(&view, Self::on_app_modal_event);
        self.app_modal = Some(OpenAppModal {
            id: entry.id,
            view,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_app_modal_event(
        &mut self,
        _view: Entity<AppShortcutModal>,
        event: &AppShortcutModalEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.app_modal.as_ref().map(|open| open.id) else {
            return;
        };
        match event {
            AppShortcutModalEvent::Recapture => self.start_capture(Capture::AppModal(id), cx),
            AppShortcutModalEvent::Cancel => self.close_app_modal(cx),
            AppShortcutModalEvent::Save(chord) => {
                let chord = chord.clone();
                self.close_app_modal(cx);
                match chord {
                    Some(chord) => self.shortcuts.bind(id, chord),
                    None => self.shortcuts.unbind(id),
                }
                self.persist_shortcuts(cx);
            }
        }
    }

    fn close_app_modal(&mut self, cx: &mut Context<Self>) {
        self.app_modal = None;
        if matches!(self.capture, Capture::AppModal(_)) {
            self.end_capture();
        }
        cx.notify();
    }

    fn toggle_menu(&mut self, key: RowKey, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open == Some(key) {
            None
        } else {
            self.menu_click_pos = Some(position);
            Some(key)
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn render_hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let label_color = if self.enabled {
            palette.success
        } else {
            palette.text_faint
        };
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(HERO_GAP)
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Keyboard, HERO_GLYPH, palette.success)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(HERO_TITLE_FS)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child(tr!("hotkeys_hero_title")),
                    )
                    .child(
                        div()
                            .mt(HERO_BLURB_MT)
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("hotkeys_hero_blurb")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(HERO_TOGGLE_GAP)
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(label_color)
                            .child(if self.enabled {
                                tr!("hotkeys_hero_enabled")
                            } else {
                                tr!("hotkeys_hero_disabled")
                            }),
                    )
                    .child(toggle(self.enabled, palette).on_click(
                        "hotkeys-enabled",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_engine(cx)),
                    )),
            );

        div()
            .w_full()
            .mb(HERO_MARGIN_B)
            .child(
                card(body, palette)
                    .padding_xy(HERO_PAD_V, HERO_PAD_H)
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_stats(&self, palette: &ForgePalette) -> AnyElement {
        let active = self.active_count();
        let conflicts = self.conflicts;
        let (conflicts_ink, conflicts_hint, conflicts_hint_ink) = if conflicts == 0 {
            (
                palette.success,
                tr!("hotkeys_stat_conflicts_none"),
                palette.success,
            )
        } else {
            (
                palette.warning,
                tr!("hotkeys_stat_conflicts_hint"),
                palette.text_faint,
            )
        };
        let (fired_value, fired_hint) = match &self.last_fired {
            Some(last) => (last.combo.clone(), fmt_relative_time(Some(last.at))),
            None => (NO_VALUE.to_owned(), tr!("hotkeys_stat_last_fired_none")),
        };

        div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(STAT_GAP)
            .mb(STAT_MARGIN_B)
            .child(stat_card(
                tr!("hotkeys_stat_bindings"),
                self.total_count().to_string(),
                palette.text_primary,
                tr!("hotkeys_stat_bindings_hint", count = active as i64),
                palette.success,
                palette,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_global"),
                self.global_count().to_string(),
                palette.text_primary,
                tr!("hotkeys_stat_global_hint"),
                palette.text_faint,
                palette,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_conflicts"),
                conflicts.to_string(),
                conflicts_ink,
                conflicts_hint,
                conflicts_hint_ink,
                palette,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_last_fired"),
                fired_value,
                palette.text_primary,
                fired_hint,
                palette.text_faint,
                palette,
            ))
            .into_any_element()
    }

    fn render_bindings(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(tr!("hotkeys_section_hint"))
            .into_any_element();

        let mut list = div().w_full().flex().flex_col().gap(LIST_GAP);
        if self.bindings.is_empty() && self.capture != Capture::Add {
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("hotkeys_bindings_empty")),
            );
        }
        for (index, row) in self.bindings.iter().enumerate() {
            list = list.child(self.render_binding(index, row, palette, cx));
        }
        list = list.child(self.render_add_bar(palette, cx));
        for (index, entry) in SHORTCUTS.iter().enumerate() {
            list = list.child(self.render_app_binding(index, entry, palette, cx));
        }

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

    fn render_app_binding(
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
            palette.brand
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

    fn render_binding(
        &self,
        index: usize,
        row: &BindingRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let instance_id = row.instance_id;
        if self.capture == Capture::Rebind(instance_id) {
            return self.render_capture_row(palette, cx);
        }
        let (target_text, target_ink) = match row.action.as_ref() {
            Some((_, name)) => (name.clone(), palette.text_primary),
            None => (tr!("hotkeys_unassigned"), palette.text_faint),
        };
        let (scope_label, scope_ink) = if row.registered {
            (tr!("hotkeys_scope_global"), palette.success)
        } else {
            (tr!("hotkeys_scope_unregistered"), palette.text_faint)
        };
        let dot_color = if row.action.is_some() {
            palette.brand
        } else {
            palette.text_faint
        };

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
                            this.start_capture(Capture::Rebind(instance_id), cx);
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
            .child(toggle(row.enabled, palette).on_click(
                ("hotkeys-row-toggle", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.toggle_binding(instance_id, cx)
                }),
            ))
            .child(self.render_row_menu(index, instance_id, palette, cx));

        let mut wrapper = div().w_full().child(
            card(body, palette)
                .padding_xy(ROW_PAD_V, ROW_PAD_H)
                .full_width(),
        );
        if !row.enabled {
            wrapper = wrapper.opacity(ROW_OFF_OPACITY);
        }
        wrapper.into_any_element()
    }

    fn render_row_menu(
        &self,
        index: usize,
        instance_id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = RowKey::Global(instance_id);
        let open = self.menu_open == Some(key);
        let view = cx.entity();
        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(vec![
                menu_item(
                    ("hotkeys-menu-edit", index),
                    tr!("hotkeys_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.change_action(instance_id, cx)
                    }),
                )
                .icon(Icon::Edit)
                .into(),
                menu_divider(),
                menu_item(
                    ("hotkeys-menu-delete", index),
                    tr!("common_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.prompt_delete(instance_id, cx)
                    }),
                )
                .icon(Icon::Trash)
                .color(palette.random)
                .into(),
            ])
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

    fn render_capture_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let cancel_hover = palette.text_secondary;
        div()
            .id("hotkeys-capture-row")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(CAPTURE_GAP)
            .py(CAPTURE_PAD_V)
            .px(ADD_BAR_PAD_H)
            .rounded(ADD_BAR_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.surface_overlay)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_capture(cx)))
            .child(icon(Icon::Keyboard, CAPTURE_GLYPH, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.success)
                    .child(tr!("hotkeys_capture_prompt")),
            )
            .child(
                div()
                    .id("hotkeys-capture-cancel")
                    .ml(CAPTURE_CANCEL_ML)
                    .flex()
                    .items_center()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.text_faint)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(cancel_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.cancel_capture(cx);
                    }))
                    .child(tr!("common_cancel")),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette) -> AnyElement {
        let listener = if self.enabled {
            tr!("hotkeys_footer_listening")
        } else {
            tr!("hotkeys_footer_stopped")
        };
        let (dot, right) = if self.conflicts == 0 {
            (palette.success, tr!("hotkeys_footer_no_conflicts"))
        } else {
            (
                palette.warning,
                tr!("hotkeys_footer_conflicts", count = self.conflicts as i64),
            )
        };
        div()
            .w_full()
            .mt(FOOTER_MT)
            .flex()
            .items_center()
            .justify_between()
            .py(FOOTER_PAD_V)
            .px(FOOTER_PAD_H)
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(FOOTER_GAP)
                    .child(footer_text(
                        tr!("hotkeys_footer_bindings", count = self.total_count() as i64),
                        palette,
                    ))
                    .child(footer_text(FOOTER_SEPARATOR.to_owned(), palette))
                    .child(footer_text(listener, palette)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(FOOTER_GAP)
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(footer_text(right, palette)),
            )
            .into_any_element()
    }

    fn render_delete_confirm(
        &self,
        prompt: &DeletePrompt,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = match prompt.action.as_deref() {
            Some(action) => tr!("hotkeys_confirm_delete_body", action = action),
            None => tr!("hotkeys_confirm_delete_body_unassigned"),
        };
        let card = confirm_modal(
            tr!("hotkeys_confirm_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.combo.clone())
        .on_cancel(
            "hotkeys-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "hotkeys-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("hotkeys-delete-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }

    fn render_conflict(
        &self,
        prompt: &ConflictPrompt,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = confirm_modal(
            tr!("hotkeys_conflict_title"),
            tr!("hotkeys_conflict_body", holder = prompt.holder.as_str()),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(match prompt.capture {
            Capture::App(_) => chord_caps(&prompt.combo),
            _ => prompt.combo.clone(),
        })
        .on_cancel(
            "hotkeys-conflict-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_cancel(cx)),
        )
        .on_confirm(
            "hotkeys-conflict-replace",
            tr!("hotkeys_conflict_replace"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.conflict_replace(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("hotkeys-conflict-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.conflict_cancel(cx));
            })
            .into_any_element()
    }
}

fn footer_text(text: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FOOTER_FS)
        .text_color(palette.text_faint)
        .child(text)
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

fn stat_card(
    label: String,
    value: String,
    value_color: gpui::Rgba,
    hint: String,
    hint_color: gpui::Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(STAT_LABEL_MB)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .font_family(body_family())
                .text_size(FONT_SM)
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .mt(STAT_HINT_MT)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint),
        );

    let mut cell = div().min_w(px(0.0)).child(
        card(body, palette)
            .padding_xy(STAT_PAD_V, STAT_PAD_H)
            .full_width()
            .full_height(),
    );
    let style = cell.style();
    style.flex_grow = Some(1.0);
    style.flex_basis = Some(gpui::relative(0.0).into());
    cell
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

impl Render for HotkeysScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        drive_overlay_focus(
            self.conflict.is_some() || self.delete_prompt.is_some(),
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let header_right = div()
            .flex()
            .items_center()
            .gap(HEADER_GAP)
            .child(icon(Icon::Keyboard, HEADER_GLYPH, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "hotkeys_header_summary",
                        count = self.enabled_count() as i64
                    )),
            );

        let body = div()
            .id("hotkeys-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(SCROLL_PAD_Y)
                    .px(SCROLL_PAD_X)
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, cx))
                    .child(self.render_stats(&palette))
                    .child(self.render_bindings(&palette, cx))
                    .child(self.render_footer(&palette)),
            );

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("hotkeys_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("hotkeys_hero_title")),
            ],
            &palette,
        )
        .header_right(header_right)
        .density(density)
        .body(body);

        let prompt = if let Some(conflict) = &self.conflict {
            Some(self.render_conflict(conflict, &palette, cx))
        } else {
            self.delete_prompt
                .as_ref()
                .map(|prompt| self.render_delete_confirm(prompt, &palette, cx))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(self.modal.as_ref().map(|open| open.view.clone()))
            .children(self.app_modal.as_ref().map(|open| open.view.clone()))
            .children(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_target_names_the_binding_a_capture_must_not_conflict_with() {
        // Both edit flows carry the row they are editing so on_capture_combo can exclude it.
        // Without it, recapturing a binding's own combo reports the binding as its own
        // conflict, and confirming Replace deletes the row being edited.
        let id = TriggerInstanceId::new();
        let cases = [
            ("idle", Capture::Off, None),
            ("adding a new binding", Capture::Add, None),
            ("rebinding from the row menu", Capture::Rebind(id), Some(id)),
            (
                "recapturing inside an edit modal",
                Capture::Modal(Some(id)),
                Some(id),
            ),
            (
                "recapturing inside an add modal",
                Capture::Modal(None),
                None,
            ),
        ];

        for (case, capture, expected) in cases {
            assert_eq!(capture.target(), expected, "wrong exclusion while {case}");
        }
    }
}
