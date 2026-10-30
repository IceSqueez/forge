use forge_components::Density;
use forge_components::FONT_BADGE;
use forge_components::FONT_HERO;
use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::FONT_ROW;
use forge_components::FONT_SECTION_LABEL;
use forge_components::FONT_XXS;
use forge_components::Spacing;
use forge_components::spacing;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, FONT_XS, ForgePalette, Icon, MenuPlacement, Radius, SectionLabel,
    ToastAction, ToastKind, badge, body_family, card, error_row_with_retry, ghost_button_with_icon,
    header_status, icon, menu_button, menu_divider, menu_item, mono_family, pad_tile, page_frame,
    radius, scaled, status_dot, toggle, tr,
};
use forge_midi::vocabulary::event_kinds::INPUT_PREFIX;
use forge_midi::{MidiClient, MidiMonitorEvent, MidiSignalKind, SwitchableMidiSink};
use forge_runtime::EventBus;
use forge_storage::{
    ActionRepo, JsonDocument, Setting, SettingsRepo, StorageError, TriggerInstanceRepo,
    read_stored_setting, write_setting,
};
use forge_types::{ActionId, PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId};
use futures_util::StreamExt;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Point, Subscription, Task,
    Window, div, prelude::*, px,
};

use crate::async_bridge::{
    self, BridgeFlow, ErrorSink, Loadable, SnapshotWriter, WriteOutcome, drain_events,
};
use crate::hub_crumb::hub_crumb;
use crate::integrations::BuiltinObject;
use crate::midi_mapping_modal::{
    MappingDraft, MappingModalLaunch, MidiMappingModal, MidiMappingModalEvent,
};
use crate::midi_signal::{MidiSignal, kind_color, note_name, short_kind_label};
use crate::navigation::NavRequested;
use crate::repo_failure::RepoFailure;
use crate::runtime_event;
use forge_components::ActivePresentation;
use forge_components::PushToast;
use forge_components::grow_cell;
use forge_runtime::{IntegrationSlot, LifecycleState};

pub const MIDI_KNOWN_DEVICES: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "midi.known_devices",
    JsonDocument::new(),
    forge_storage::SettingPortability::MachineLocal,
    forge_storage::SettingDisclosure::Withheld,
);

const UNKNOWN_VALUE: &str = "-";

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const HERO_PAD_V: Pixels = px(14.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(14.0);
const HERO_MARGIN_B: Pixels = px(14.0);
const HERO_TILE: Pixels = px(40.0);
const HERO_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = FONT_HERO;
const HERO_BLURB_MT: Pixels = px(1.0);
const HERO_TOGGLE_GAP: Pixels = px(8.0);
const LABEL_FS: Pixels = FONT_LABEL;

const COLUMNS_GAP: Pixels = px(12.0);
const LEFT_COLUMN_FLEX: f32 = 1.0;
const RIGHT_COLUMN_FLEX: f32 = 1.7;

const DEVICE_PAD_H: Pixels = px(12.0);
const DEVICE_GLYPH: Pixels = px(14.0);
const DEVICE_NAME_FS: Pixels = FONT_ROW;
const DEVICE_BADGE_FS: Pixels = FONT_SECTION_LABEL;
const OFFLINE_OPACITY: f32 = 0.6;
const RESCAN_MT: Pixels = px(2.0);

const MONITOR_MT: Pixels = px(12.0);
const MONITOR_PAD: Pixels = px(12.0);
const MONITOR_FS: Pixels = FONT_XXS;
const MONITOR_LINE_H: Pixels = px(18.9);
const MONITOR_GAP: Pixels = px(8.0);
const MONITOR_KIND_W: Pixels = px(54.0);
const MONITOR_ROWS: usize = 4;

const BINDINGS_FS: Pixels = FONT_MICRO;
const MAP_PAD_V: Pixels = px(9.0);
const MAP_PAD_H: Pixels = px(12.0);
const SIG_FS: Pixels = FONT_LABEL;
const SIG_RADIUS: Pixels = px(5.0);
const SIG_PAD_V: Pixels = px(3.0);
const SIG_PAD_H: Pixels = px(8.0);
const SIG_MIN_W: Pixels = px(66.0);
const SIG_LINE_H: Pixels = px(14.0);
const CH_BADGE_FS: Pixels = FONT_BADGE;
const ARROW_GLYPH: Pixels = px(13.0);
const ACCENT_DOT: Pixels = px(6.0);
const TARGET_FS: Pixels = FONT_XS;

const ADD_BAR_PAD_H: Pixels = px(12.0);
const ADD_BAR_RADIUS: Pixels = radius(Radius::Md);
const ADD_BAR_GLYPH: Pixels = px(13.0);
const ADD_BAR_FS: Pixels = FONT_XS;
const KBD_FS: Pixels = FONT_MICRO;
const KBD_PAD_V: Pixels = px(1.0);
const KBD_RADIUS: Pixels = px(4.0);

const FOOTER_FS: Pixels = FONT_XXS;
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);

const LEARN_PAD_V: Pixels = px(11.0);
const LEARN_GAP: Pixels = px(8.0);
const LEARN_GLYPH: Pixels = px(14.0);

const UNDO_TOAST_MS: u64 = 6000;

fn monitor_value_label(event: &MidiMonitorEvent) -> String {
    match event.kind {
        MidiSignalKind::NoteOn => match (event.number, event.value) {
            (Some(note), Some(velocity)) => {
                format!("{} vel {velocity}", note_name(i64::from(note)))
            }
            (Some(note), None) => note_name(i64::from(note)),
            _ => String::new(),
        },
        MidiSignalKind::NoteOff => event
            .number
            .map(|note| note_name(i64::from(note)))
            .unwrap_or_default(),
        MidiSignalKind::ControlChange => match (event.number, event.value) {
            (Some(controller), Some(value)) => format!("{controller} \u{2192} {value}"),
            _ => String::new(),
        },
        MidiSignalKind::PitchBend => event.value.map(|v| v.to_string()).unwrap_or_default(),
        MidiSignalKind::ProgramChange => event.number.map(|n| n.to_string()).unwrap_or_default(),
    }
}

struct MappingRow {
    id: TriggerInstanceId,
    name: String,
    signal: MidiSignal,
    action: Option<(ActionId, String)>,
}

async fn load_mappings(
    triggers: &dyn TriggerInstanceRepo,
    actions: &dyn ActionRepo,
) -> Result<Vec<MappingRow>, RepoFailure> {
    let instances = triggers.list_all().await.map_err(RepoFailure::from)?;
    let mut rows = Vec::new();
    for instance in instances {
        if !instance.kind_id.starts_with(INPUT_PREFIX) {
            continue;
        }
        let linked = triggers
            .actions_using(instance.id)
            .await
            .map_err(RepoFailure::from)?;
        if !instance.user_defined && linked.is_empty() {
            continue;
        }
        let mut action = None;
        if let Some(first) = linked.first()
            && let Some(found) = actions.get(*first).await.map_err(RepoFailure::from)?
        {
            action = Some((found.id, found.name));
        }
        rows.push(MappingRow {
            signal: MidiSignal::from_instance(&instance.kind_id, &instance.overrides),
            id: instance.id,
            name: instance.name,
            action,
        });
    }
    Ok(rows)
}

async fn save_mapping(
    triggers: &dyn TriggerInstanceRepo,
    actions: &dyn ActionRepo,
    draft: MappingDraft,
) -> Result<Vec<MappingRow>, RepoFailure> {
    let overrides = draft.signal.overrides();
    let instance_id = match draft.instance_id {
        Some(id) => {
            let source = triggers
                .get(id)
                .await
                .map_err(RepoFailure::from)?
                .ok_or_else(RepoFailure::trigger_gone)?;
            let updated = TriggerInstance {
                kind_id: draft.signal.kind_id.clone(),
                name: draft.name.clone(),
                overrides,
                ..source
            };
            triggers.save(&updated).await.map_err(RepoFailure::from)?;
            id
        }
        None => {
            let instance = TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id: draft.signal.kind_id.clone(),
                name: draft.name,
                overrides,
                enabled: true,
                user_defined: true,
                platform_scope: PlatformScope::Any,
                cooldown_secs: 0,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            };
            triggers.save(&instance).await.map_err(RepoFailure::from)?;
            instance.id
        }
    };

    let linked = triggers
        .actions_using(instance_id)
        .await
        .map_err(RepoFailure::from)?;
    if !linked.contains(&draft.action_id) {
        for previous in linked {
            triggers
                .unlink_action(previous, instance_id)
                .await
                .map_err(RepoFailure::from)?;
        }
        triggers
            .link_action(draft.action_id, instance_id)
            .await
            .map_err(RepoFailure::from)?;
    }

    load_mappings(triggers, actions).await
}

fn resolve_selection(known: &[String], current: Option<&str>) -> Option<String> {
    match current {
        Some(name) if known.iter().any(|k| k == name) => Some(name.to_owned()),
        _ => known.first().cloned(),
    }
}

fn follow_device(known: &[String], current: Option<&str>, saved: Option<&str>) -> Option<String> {
    let saved = saved?;
    if current == Some(saved) {
        return None;
    }
    known.iter().any(|k| k == saved).then(|| saved.to_owned())
}

fn visible_mappings<'a>(rows: &'a [MappingRow], selected: Option<&str>) -> Vec<&'a MappingRow> {
    let Some(selected) = selected else {
        return rows.iter().collect();
    };
    rows.iter()
        .filter(|row| {
            row.signal
                .device
                .as_deref()
                .is_none_or(|device| device == selected)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    Off,
    Learn,
    Relearn,
}

struct OpenModal {
    view: Entity<MidiMappingModal>,
    _sub: Subscription,
}

pub struct MidiScreenView {
    sink: Arc<SwitchableMidiSink>,
    client: Option<Arc<MidiClient>>,
    integration: Option<IntegrationSlot<BuiltinObject>>,
    trigger_repo: Arc<dyn TriggerInstanceRepo>,
    action_repo: Arc<dyn ActionRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    enabled: bool,
    live_ports: Vec<String>,
    known_devices: Loadable<Vec<String>>,
    device_writer: SnapshotWriter,
    selected_device: Option<String>,
    monitor: Vec<MidiMonitorEvent>,
    mappings: Vec<MappingRow>,
    capture: Capture,
    escape_sub: Option<Subscription>,
    modal: Option<OpenModal>,
    menu_open: Option<TriggerInstanceId>,
    menu_click_pos: Option<Point<Pixels>>,
    _monitor_bridge: Option<Task<()>>,
    _port_bridge: Task<()>,
    _lifecycle_bridge: Option<Task<()>>,
}

impl MidiScreenView {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
    pub fn new(
        sink: Arc<SwitchableMidiSink>,
        integration: Option<IntegrationSlot<BuiltinObject>>,
        trigger_repo: Arc<dyn TriggerInstanceRepo>,
        action_repo: Arc<dyn ActionRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let client = sink.live();
        let monitor_bridge = client
            .as_ref()
            .map(|client| Self::spawn_monitor_bridge(client, &rt_handle, cx));
        let port_bridge = Self::spawn_port_bridge(bus, cx);
        let lifecycle_bridge = integration
            .as_ref()
            .map(|integration| Self::spawn_lifecycle_bridge(integration, cx));
        let mut view = Self {
            sink,
            enabled: client.is_some(),
            live_ports: client
                .as_ref()
                .map(|client| client.connected_input_ports())
                .unwrap_or_default(),
            client,
            integration,
            trigger_repo,
            action_repo,
            settings_repo,
            device_writer: SnapshotWriter::new(&rt_handle),
            rt_handle,
            known_devices: Loadable::Loading,
            selected_device: None,
            monitor: Vec::new(),
            mappings: Vec::new(),
            capture: Capture::Off,
            escape_sub: None,
            modal: None,
            menu_open: None,
            menu_click_pos: None,
            _monitor_bridge: monitor_bridge,
            _port_bridge: port_bridge,
            _lifecycle_bridge: lifecycle_bridge,
        };
        view.load(cx);
        view
    }

    fn spawn_monitor_bridge(
        client: &Arc<MidiClient>,
        rt_handle: &tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut stream = client.monitor_stream();
        rt_handle.spawn(async move {
            while let Some(event) = stream.next().await {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                if this
                    .update(cx, |this, cx| this.push_monitor(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    fn spawn_lifecycle_bridge(
        integration: &IntegrationSlot<BuiltinObject>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let id = integration.id().clone();
        let mut lifecycle = integration.watch();
        let mut last = lifecycle.current().remove(&id);
        cx.spawn(async move |this, cx| {
            while let Some(mut states) = lifecycle.changed().await {
                let state = states.remove(&id);
                if state == last {
                    continue;
                }
                last = state;
                if this.update(cx, |this, cx| this.follow_engine(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn follow_engine(&mut self, cx: &mut Context<Self>) {
        match self.integration.as_ref().map(IntegrationSlot::state) {
            Some(LifecycleState::Running) => self.enabled = true,
            Some(LifecycleState::Disabled | LifecycleState::Failed(_)) => self.enabled = false,
            Some(LifecycleState::Starting | LifecycleState::Stopping) | None => {}
        }
        let live = self.sink.live();
        let unchanged = match (&self.client, &live) {
            (Some(current), Some(live)) => Arc::ptr_eq(current, live),
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self._monitor_bridge = live
                .as_ref()
                .map(|client| Self::spawn_monitor_bridge(client, &self.rt_handle, cx));
            self.client = live;
            self.refresh_ports(cx);
        }
        cx.notify();
    }

    fn spawn_port_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                let touched = batch.iter().any(runtime_event::is_midi_port_event);
                if !touched {
                    return BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.refresh_ports(cx)) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        })
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.load_known_devices(cx);
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { load_mappings(&*triggers, &*actions).await },
            |this, mappings, cx| this.apply_mappings(mappings, cx),
            cx,
        );
    }

    fn load_known_devices(&mut self, cx: &mut Context<Self>) {
        self.known_devices = Loadable::Loading;
        let settings = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                read_stored_setting(settings.as_ref(), &MIDI_KNOWN_DEVICES)
                    .await
                    .map(Option::unwrap_or_default)
            },
            |this, result: Result<Vec<String>, StorageError>, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to load known MIDI devices");
                }
                this.known_devices = Loadable::settled(result);
                this.remember_live_ports(cx);
                this.ensure_device_selection();
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn known(&self) -> &[String] {
        self.known_devices.loaded().map_or(&[], Vec::as_slice)
    }

    fn device_write_settled(&mut self, outcome: WriteOutcome, _: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            self.known_devices = Loadable::Failed(error);
        }
    }

    fn apply_mappings(
        &mut self,
        result: Result<Vec<MappingRow>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(rows) => {
                self.mappings = rows;
                cx.notify();
            }
            Err(failure) => self.on_repo_error(&failure, cx),
        }
    }

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::warn!(error = %failure, "midi operation failed");
        let message = failure.text();
        cx.push_toast(
            ToastKind::Error,
            tr!("midi_toast_error", message = message.as_str()),
        );
        cx.notify();
    }

    fn push_monitor(&mut self, event: MidiMonitorEvent, cx: &mut Context<Self>) {
        if self.capture != Capture::Off {
            self.on_capture(MidiSignal::from_monitor(&event), cx);
        }
        self.monitor.insert(0, event);
        self.monitor.truncate(MONITOR_ROWS);
        cx.notify();
    }

    fn on_capture(&mut self, signal: MidiSignal, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
        self.escape_sub = None;
        match capture {
            Capture::Off => {}
            Capture::Learn => self.open_modal(None, signal, None, cx),
            Capture::Relearn => {
                if let Some(open) = &self.modal {
                    open.view
                        .update(cx, |modal, cx| modal.apply_capture(signal, cx));
                }
            }
        }
    }

    fn start_learn(&mut self, cx: &mut Context<Self>) {
        self.capture = Capture::Learn;
        let weak = cx.entity().downgrade();
        self.escape_sub = Some(cx.intercept_keystrokes(move |event, _window, cx| {
            if event.keystroke.key != forge_components::ESCAPE_KEY
                || event.keystroke.modifiers.modified()
            {
                return;
            }
            let handled = weak
                .update(cx, |this, cx| {
                    let armed = this.capture == Capture::Learn;
                    if armed {
                        this.cancel_learn(cx);
                    }
                    armed
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        cx.notify();
    }

    fn cancel_learn(&mut self, cx: &mut Context<Self>) {
        self.capture = Capture::Off;
        self.escape_sub = None;
        cx.notify();
    }

    fn select_device(&mut self, name: String, cx: &mut Context<Self>) {
        if self.selected_device.as_deref() == Some(name.as_str()) {
            return;
        }
        self.selected_device = Some(name);
        cx.notify();
    }

    fn ensure_device_selection(&mut self) {
        self.selected_device = resolve_selection(self.known(), self.selected_device.as_deref());
    }

    fn follow_saved_device(&mut self, device: Option<&str>) {
        if let Some(next) = follow_device(self.known(), self.selected_device.as_deref(), device) {
            self.selected_device = Some(next);
        }
    }

    fn visible_mappings(&self) -> Vec<&MappingRow> {
        visible_mappings(&self.mappings, self.selected_device.as_deref())
    }

    fn open_modal(
        &mut self,
        instance_id: Option<TriggerInstanceId>,
        signal: MidiSignal,
        linked_action: Option<ActionId>,
        cx: &mut Context<Self>,
    ) {
        let launch = MappingModalLaunch {
            instance_id,
            signal,
            linked_action,
            devices: self.known().to_vec(),
            input_enabled: self.enabled,
        };
        let action_repo = Arc::clone(&self.action_repo);
        let rt_handle = self.rt_handle.clone();
        let view = cx.new(|cx| MidiMappingModal::new(launch, action_repo, rt_handle, cx));
        let sub = cx.subscribe(&view, Self::on_modal_event);
        self.modal = Some(OpenModal { view, _sub: sub });
        cx.notify();
    }

    fn edit_mapping(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        let Some(row) = self.mappings.iter().find(|row| row.id == id) else {
            cx.notify();
            return;
        };
        let signal = row.signal.clone();
        let linked = row.action.as_ref().map(|(action_id, _)| *action_id);
        self.open_modal(Some(id), signal, linked, cx);
    }

    fn on_modal_event(
        &mut self,
        _view: Entity<MidiMappingModal>,
        event: &MidiMappingModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            MidiMappingModalEvent::Relearn => {
                self.capture = Capture::Relearn;
                cx.notify();
            }
            MidiMappingModalEvent::Cancel => self.close_modal(cx),
            MidiMappingModalEvent::Delete(id) => {
                let id = *id;
                self.close_modal(cx);
                self.delete(id, cx);
            }
            MidiMappingModalEvent::Save(draft) => {
                let draft = MappingDraft {
                    instance_id: draft.instance_id,
                    signal: draft.signal.clone(),
                    action_id: draft.action_id,
                    name: draft.name.clone(),
                };
                self.close_modal(cx);
                self.persist_mapping(draft, cx);
            }
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self.capture = Capture::Off;
        self.escape_sub = None;
        cx.notify();
    }

    fn persist_mapping(&mut self, draft: MappingDraft, cx: &mut Context<Self>) {
        let saved_device = draft.signal.device.clone();
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { save_mapping(&*triggers, &*actions, draft).await },
            move |this, result, cx| {
                if result.is_ok() {
                    this.follow_saved_device(saved_device.as_deref());
                }
                this.apply_mappings(result, cx);
            },
            cx,
        );
    }

    fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        let ports = self
            .client
            .as_ref()
            .map(|client| client.connected_input_ports())
            .unwrap_or_default();
        if ports == self.live_ports {
            return;
        }
        self.live_ports = ports;
        self.remember_live_ports(cx);
        cx.notify();
    }

    fn remember_live_ports(&mut self, cx: &mut Context<Self>) {
        let Some(known) = self.known_devices.loaded_mut() else {
            return;
        };
        let discovered: Vec<String> = self
            .live_ports
            .iter()
            .filter(|port| !known.contains(port))
            .cloned()
            .collect();
        if discovered.is_empty() {
            return;
        }
        known.extend(discovered);
        known.sort();
        let devices = known.clone();
        self.ensure_device_selection();
        let repo = Arc::clone(&self.settings_repo);
        self.device_writer.write(
            async move { write_setting(repo.as_ref(), &MIDI_KNOWN_DEVICES, &devices).await },
            Self::device_write_settled,
            cx,
        );
    }

    fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        let previous = self.enabled;
        self.enabled = !previous;
        let enabled = self.enabled;
        let integration = self.integration.clone();
        async_bridge::optimistic(
            &self.rt_handle,
            previous,
            async move {
                let integration =
                    integration.ok_or_else(|| RepoFailure::localized(tr!("midi_toggle_failed")))?;
                match integration
                    .set_enabled(enabled)
                    .await
                    .map_err(RepoFailure::from)?
                {
                    LifecycleState::Failed(error) => Err(RepoFailure::operation(error)),
                    _ => Ok(()),
                }
            },
            |this, previous, _message, cx| {
                this.enabled = previous;
                ErrorSink::Toast.report(tr!("midi_toggle_failed"), cx);
            },
            cx,
        );
        cx.notify();
    }

    fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.known_devices.failure().is_some() {
            self.load_known_devices(cx);
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { client.rescan_ports().await.map_err(RepoFailure::operation) },
            |this, result: Result<(), RepoFailure>, cx| match result {
                Ok(()) => this.refresh_ports(cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn toggle_menu(
        &mut self,
        id: TriggerInstanceId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = if self.menu_open == Some(id) {
            None
        } else {
            self.menu_click_pos = Some(position);
            Some(id)
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn duplicate(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let Some(copy_name) = self
            .mappings
            .iter()
            .find(|row| row.id == id)
            .map(|row| tr!("triggers_template_copy_name", name = row.name.as_str()))
        else {
            return;
        };
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let source = triggers
                    .get(id)
                    .await
                    .map_err(RepoFailure::from)?
                    .ok_or_else(RepoFailure::trigger_gone)?;
                let copy = TriggerInstance {
                    id: TriggerInstanceId::new(),
                    name: copy_name,
                    user_defined: true,
                    ..source
                };
                triggers.save(&copy).await.map_err(RepoFailure::from)?;
                load_mappings(&*triggers, &*actions).await
            },
            |this, result, cx| this.apply_mappings(result, cx),
            cx,
        );
    }

    fn delete(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let Some(name) = self
            .mappings
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.name.clone())
        else {
            return;
        };
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                triggers.archive(id).await.map_err(RepoFailure::from)?;
                load_mappings(&*triggers, &*actions).await
            },
            move |this, result, cx| {
                let archived = result.is_ok();
                this.apply_mappings(result, cx);
                if archived {
                    this.raise_undo_toast(id, name, cx);
                }
            },
            cx,
        );
    }

    fn raise_undo_toast(&self, id: TriggerInstanceId, name: String, cx: &mut Context<Self>) {
        let view = cx.entity();
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        let rt_handle = self.rt_handle.clone();
        cx.push_toast_full(
            ToastKind::Undo,
            tr!("triggers_toast_deleted", name = name.as_str()),
            None,
            Some(ToastAction::new(
                tr!("common_undo"),
                move |_window, app: &mut App| {
                    let triggers = Arc::clone(&triggers);
                    let actions = Arc::clone(&actions);
                    async_bridge::run_async_entity(
                        &rt_handle,
                        view.clone(),
                        async move {
                            triggers.restore(id).await.map_err(RepoFailure::from)?;
                            load_mappings(&*triggers, &*actions).await
                        },
                        |this, result, cx| this.apply_mappings(result, cx),
                        app,
                    );
                },
            )),
            Duration::from_millis(UNDO_TOAST_MS),
        );
    }

    fn maps_for_device(&self, port: &str) -> usize {
        self.mappings
            .iter()
            .filter(|row| row.signal.device.as_deref() == Some(port))
            .count()
    }

    fn render_hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let label_color = if self.enabled {
            palette.success
        } else {
            palette.text_faint
        };
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(HERO_GAP, density))
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Piano, HERO_GLYPH, palette.info)),
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
                            .child("MIDI"),
                    )
                    .child(
                        div()
                            .mt(HERO_BLURB_MT)
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("midi_hero_blurb")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(HERO_TOGGLE_GAP, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(label_color)
                            .child(if self.enabled {
                                tr!("midi_hero_enabled")
                            } else {
                                tr!("midi_hero_disabled")
                            }),
                    )
                    .child(toggle(self.enabled, palette).on_click(
                        "midi-enabled",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
                    )),
            );

        div()
            .w_full()
            .mb(scaled(HERO_MARGIN_B, density))
            .child(
                card(body, palette)
                    .padding_xy(scaled(HERO_PAD_V, density), scaled(HERO_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_devices(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let known = self.known();
        let mut cards: Vec<AnyElement> = Vec::with_capacity(known.len());
        for (index, name) in known.iter().enumerate() {
            let live_index = self.live_ports.iter().position(|port| port == name);
            cards.push(self.render_device(index, name, live_index, palette, cx));
        }

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if let Some(reason) = self.known_devices.failure() {
            list = list.child(error_row_with_retry(
                "midi-devices-retry",
                tr!("midi_devices_load_failed", reason = reason),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.load_known_devices(cx)),
                density,
            ));
        } else if cards.is_empty() {
            let note = if self.known_devices.is_loaded() {
                tr!("midi_devices_empty")
            } else {
                UNKNOWN_VALUE.to_owned()
            };
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(note),
            );
        } else {
            list = list.children(cards);
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("midi_section_devices"), palette)
                    .row()
                    .trailing(None),
            )
            .child(
                list.child(
                    div().w_full().mt(RESCAN_MT).child(
                        ghost_button_with_icon(Icon::Refresh, tr!("midi_rescan_ports"), palette)
                            .full_width()
                            .on_click(
                                "midi-rescan",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.rescan(cx)),
                            ),
                    ),
                ),
            )
            .into_any_element()
    }

    fn render_device(
        &self,
        index: usize,
        name: &str,
        live_index: Option<usize>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let online = live_index.is_some();
        let selected = self.selected_device.as_deref() == Some(name);
        let glyph_color = if online {
            palette.success
        } else {
            palette.text_faint
        };
        let badge_ink = if online {
            palette.info
        } else {
            palette.text_faint
        };
        let port_line = match live_index {
            Some(index) => tr!("midi_device_port", index = index as i64),
            None => tr!("midi_device_offline"),
        };
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(icon(Icon::Plug, DEVICE_GLYPH, glyph_color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .font_family(body_family())
                            .text_size(DEVICE_NAME_FS)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(MONITOR_FS)
                            .text_color(palette.text_faint)
                            .child(port_line),
                    ),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    badge_ink,
                    tr!(
                        "midi_device_maps",
                        count = self.maps_for_device(name) as i64
                    ),
                    true,
                    DEVICE_BADGE_FS,
                )
                .flex_none(),
            );

        let (surface, border) = if selected {
            (palette.surface_overlay, palette.brand)
        } else {
            (palette.elevated, palette.border_regular)
        };
        let hover_surface = palette.surface_overlay;
        let target = name.to_owned();
        let mut wrapper = div()
            .id(("midi-device", index))
            .w_full()
            .py(spacing(Spacing::Sm, density))
            .px(scaled(DEVICE_PAD_H, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .bg(surface)
            .text_color(palette.text_primary)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_device(target.clone(), cx)
            }))
            .child(row);
        if !selected {
            wrapper = wrapper.hover(move |style| style.bg(hover_surface));
        }
        if !online {
            wrapper = wrapper.opacity(OFFLINE_OPACITY);
        }
        wrapper.into_any_element()
    }

    fn render_monitor(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let mut lines = div()
            .w_full()
            .font_family(mono_family())
            .text_size(MONITOR_FS)
            .line_height(MONITOR_LINE_H)
            .text_color(palette.text_secondary);

        for event in &self.monitor {
            lines = lines.child(
                div()
                    .w_full()
                    .flex()
                    .gap(scaled(MONITOR_GAP, density))
                    .child(
                        div()
                            .w(MONITOR_KIND_W)
                            .flex_shrink_0()
                            .text_color(palette.text_faint)
                            .child(short_kind_label(event.kind)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(kind_color(event.kind.event_kind(), palette))
                            .child(monitor_value_label(event)),
                    ),
            );
        }

        if !self.enabled {
            lines = lines.child(
                div()
                    .text_color(palette.text_faint)
                    .child(tr!("midi_monitor_disabled")),
            );
        } else if self.monitor.is_empty() {
            lines = lines.child(
                div()
                    .text_color(palette.text_faint)
                    .child(tr!("midi_monitor_empty")),
            );
        }

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("midi_section_monitor"), palette)
                    .row()
                    .trailing(None),
            )
            .child(lines);

        div()
            .w_full()
            .mt(MONITOR_MT)
            .child(
                card(body, palette)
                    .padding(scaled(MONITOR_PAD, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_mappings(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let visible = self.visible_mappings();
        let count = div()
            .font_family(mono_family())
            .text_size(BINDINGS_FS)
            .text_color(palette.text_faint)
            .child(tr!("midi_bindings_count", count = visible.len() as i64))
            .into_any_element();

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if visible.is_empty() {
            let empty = if self.mappings.is_empty() {
                tr!("midi_mappings_empty")
            } else {
                tr!("midi_mappings_empty_device")
            };
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(empty),
            );
        } else {
            for (index, row) in visible.iter().enumerate() {
                list = list.child(self.render_mapping(index, row, palette, cx));
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("midi_section_mappings"), palette)
                    .row()
                    .trailing(Some(count)),
            )
            .child(list.child(self.render_add_bar(palette, cx)))
            .into_any_element()
    }

    fn render_mapping(
        &self,
        index: usize,
        row: &MappingRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let (target_text, target_ink) = match row.action.as_ref() {
            Some((_, name)) => (name.clone(), palette.text_primary),
            None => (tr!("midi_unassigned"), palette.text_faint),
        };
        let id = row.id;
        let mut content = div()
            .id(("midi-mapping-row", index))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_mapping(id, cx)))
            .child(
                div()
                    .flex_none()
                    .min_w(SIG_MIN_W)
                    .py(scaled(SIG_PAD_V, density))
                    .px(scaled(SIG_PAD_H, density))
                    .rounded(SIG_RADIUS)
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .text_align(gpui::TextAlign::Center)
                    .font_family(mono_family())
                    .text_size(SIG_FS)
                    .line_height(SIG_LINE_H)
                    .text_color(palette.text_primary)
                    .child(row.signal.label()),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    row.signal.channel_label(),
                    true,
                    CH_BADGE_FS,
                )
                .flex_none(),
            );

        if row.signal.device.is_none() {
            content = content.child(
                badge(
                    palette.surface_overlay,
                    palette.text_faint,
                    tr!("midi_mapping_any_device"),
                    true,
                    CH_BADGE_FS,
                )
                .flex_none(),
            );
        }

        let content = content
            .child(icon(Icon::ArrowRight, ARROW_GLYPH, palette.text_faint))
            .child(
                div()
                    .flex_none()
                    .size(ACCENT_DOT)
                    .rounded_full()
                    .bg(kind_color(&row.signal.kind_id, palette)),
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
            );

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(content)
            .child(self.render_row_menu(index, row.id, palette, cx));

        div()
            .w_full()
            .child(
                card(body, palette)
                    .padding_xy(scaled(MAP_PAD_V, density), scaled(MAP_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        index: usize,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.menu_open == Some(id);
        let view = cx.entity();
        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(vec![
                menu_item(
                    ("midi-menu-edit", index),
                    tr!("midi_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_mapping(id, cx)),
                )
                .icon(Icon::Edit)
                .into(),
                menu_item(
                    ("midi-menu-duplicate", index),
                    tr!("common_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    ("midi-menu-delete", index),
                    tr!("common_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.delete(id, cx)),
                )
                .icon(Icon::Trash)
                .color(palette.danger)
                .into(),
            ])
            .on_toggle(
                ("midi-menu-trigger", index),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.toggle_menu(id, event.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        if self.capture == Capture::Learn {
            return self.render_learn_row(palette, cx);
        }
        let title = div()
            .flex()
            .items_center()
            .child(tr!("midi_add_learn"))
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
                    .child(tr!("midi_add_learn_kbd")),
            );

        pad_tile(
            "midi-add-learn",
            icon(Icon::Plus, ADD_BAR_GLYPH, palette.info),
            title,
            palette,
        )
        .bar(palette)
        .title_color(palette.info)
        .hover_border(palette.info)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_learn(cx)))
        .into_any_element()
    }

    fn render_learn_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let (accent, prompt) = if self.enabled {
            (palette.info, tr!("midi_learn_prompt"))
        } else {
            (palette.warning, tr!("midi_learn_input_disabled"))
        };
        let cancel_hover = palette.text_secondary;
        div()
            .id("midi-learn-row")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(scaled(LEARN_GAP, density))
            .py(scaled(LEARN_PAD_V, density))
            .px(scaled(ADD_BAR_PAD_H, density))
            .rounded(ADD_BAR_RADIUS)
            .border(BORDER_THIN)
            .border_color(accent)
            .bg(palette.surface_overlay)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_learn(cx)))
            .child(icon(Icon::Antenna, LEARN_GLYPH, accent))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(accent)
                    .child(prompt),
            )
            .child(
                div()
                    .id("midi-learn-cancel")
                    .ml(spacing(Spacing::Xs, density))
                    .flex()
                    .items_center()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.text_faint)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(cancel_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.cancel_learn(cx);
                    }))
                    .child(tr!("common_cancel")),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (dot, status_text) = if self.enabled {
            (palette.success, tr!("midi_engine_running"))
        } else {
            (palette.text_faint, tr!("midi_engine_stopped"))
        };
        div()
            .w_full()
            .mt(FOOTER_MT)
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(FOOTER_PAD_V, density))
            .px(scaled(FOOTER_PAD_H, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FOOTER_FS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "midi_footer_left",
                        devices = self.live_ports.len() as i64,
                        mappings = self.mappings.len() as i64
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FOOTER_FS)
                            .text_color(palette.text_faint)
                            .child(status_text),
                    ),
            )
            .into_any_element()
    }
}

impl EventEmitter<NavRequested> for MidiScreenView {}

impl Render for MidiScreenView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_ink = if self.enabled {
            palette.success
        } else {
            palette.text_faint
        };
        let header_right = header_status(
            header_ink,
            tr!(
                "midi_header_summary",
                devices = self.live_ports.len() as i64,
                mappings = self.mappings.len() as i64
            ),
            density,
        );

        let left = grow_cell(
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(self.render_devices(&palette, cx))
                .child(self.render_monitor(&palette, density)),
            LEFT_COLUMN_FLEX,
        );

        let right = grow_cell(
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(self.render_mappings(&palette, cx)),
            RIGHT_COLUMN_FLEX,
        );

        let body = div()
            .id("midi-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(SCROLL_PAD_Y, density))
                    .px(scaled(SCROLL_PAD_X, density))
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, cx))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(scaled(COLUMNS_GAP, density))
                            .child(left)
                            .child(right),
                    )
                    .child(self.render_footer(&palette, density)),
            );

        let frame = page_frame(
            vec![
                hub_crumb(Some(forge_midi::MIDI_INTEGRATION.category), cx),
                BreadcrumbCrumb::leaf("MIDI"),
            ],
            &palette,
        )
        .header_right(header_right)
        .density(density)
        .body(body);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(self.modal.as_ref().map(|open| open.view.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_storage::{ActionTelemetry, ExecutionStatus, StorageError};
    use forge_types::{Action, ExecutionMode, QueueId, TriggerConfig, Variant};
    use time::OffsetDateTime;

    use super::*;
    use forge_midi::vocabulary::event_kinds::{CONTROL_CHANGE, NOTE_ON};

    #[derive(Default)]
    struct TriggerStore {
        instances: Vec<TriggerInstance>,
        links: Vec<(ActionId, TriggerInstanceId, i64)>,
        unlinks: usize,
        fail_save: bool,
    }

    #[derive(Default)]
    struct FakeTriggerRepo {
        state: Mutex<TriggerStore>,
    }

    impl FakeTriggerRepo {
        fn seed(&self, instance: TriggerInstance) {
            self.lock().instances.push(instance);
        }

        fn seed_link(&self, action_id: ActionId, instance_id: TriggerInstanceId, position: i64) {
            self.lock().links.push((action_id, instance_id, position));
        }

        fn failing_save() -> Self {
            let repo = Self::default();
            repo.lock().fail_save = true;
            repo
        }

        fn lock(&self) -> std::sync::MutexGuard<'_, TriggerStore> {
            self.state.lock().unwrap_or_else(|p| p.into_inner())
        }

        fn instances(&self) -> Vec<TriggerInstance> {
            self.lock().instances.clone()
        }

        fn links(&self) -> Vec<(ActionId, TriggerInstanceId, i64)> {
            self.lock().links.clone()
        }

        fn unlinks(&self) -> usize {
            self.lock().unlinks
        }
    }

    #[async_trait]
    impl TriggerInstanceRepo for FakeTriggerRepo {
        async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
            Ok(self.instances())
        }

        async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
            Err(StorageError::NotReady)
        }

        async fn list_for_action(
            &self,
            action_id: ActionId,
        ) -> Result<Vec<TriggerInstance>, StorageError> {
            let store = self.lock();
            Ok(store
                .links
                .iter()
                .filter(|(action, _, _)| *action == action_id)
                .filter_map(|(_, instance_id, _)| {
                    store
                        .instances
                        .iter()
                        .find(|i| i.id == *instance_id)
                        .cloned()
                })
                .collect())
        }

        async fn actions_using(
            &self,
            instance_id: TriggerInstanceId,
        ) -> Result<Vec<ActionId>, StorageError> {
            Ok(self
                .lock()
                .links
                .iter()
                .filter(|(_, linked, _)| *linked == instance_id)
                .map(|(action, _, _)| *action)
                .collect())
        }

        async fn link_action(
            &self,
            action_id: ActionId,
            instance_id: TriggerInstanceId,
        ) -> Result<(), StorageError> {
            let mut store = self.lock();
            let position = store
                .links
                .iter()
                .filter(|(action, _, _)| *action == action_id)
                .count() as i64;
            store.links.push((action_id, instance_id, position));
            Ok(())
        }

        async fn link_action_at(
            &self,
            action_id: ActionId,
            instance_id: TriggerInstanceId,
            position: i64,
        ) -> Result<(), StorageError> {
            self.lock().links.push((action_id, instance_id, position));
            Ok(())
        }

        async fn unlink_action(
            &self,
            action_id: ActionId,
            instance_id: TriggerInstanceId,
        ) -> Result<bool, StorageError> {
            let mut store = self.lock();
            store.unlinks += 1;
            let before = store.links.len();
            store
                .links
                .retain(|(action, linked, _)| !(*action == action_id && *linked == instance_id));
            Ok(store.links.len() != before)
        }

        async fn get(
            &self,
            id: TriggerInstanceId,
        ) -> Result<Option<TriggerInstance>, StorageError> {
            Ok(self.lock().instances.iter().find(|i| i.id == id).cloned())
        }

        async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
            let mut store = self.lock();
            if store.fail_save {
                return Err(StorageError::Connection {
                    reason: "forced write failure".to_owned(),
                });
            }
            match store.instances.iter_mut().find(|i| i.id == instance.id) {
                Some(existing) => *existing = instance.clone(),
                None => store.instances.push(instance.clone()),
            }
            Ok(())
        }

        async fn delete(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
            Err(StorageError::NotReady)
        }

        async fn upsert_default(
            &self,
            _kind_id: &str,
            _name: &str,
        ) -> Result<TriggerInstanceId, StorageError> {
            Err(StorageError::NotReady)
        }

        async fn set_enabled(
            &self,
            _id: TriggerInstanceId,
            _enabled: bool,
        ) -> Result<(), StorageError> {
            Err(StorageError::NotReady)
        }
    }

    #[derive(Default)]
    struct FakeActionRepo {
        actions: Mutex<Vec<Action>>,
    }

    impl FakeActionRepo {
        fn with(actions: Vec<Action>) -> Self {
            Self {
                actions: Mutex::new(actions),
            }
        }
    }

    #[async_trait]
    impl ActionRepo for FakeActionRepo {
        async fn list(&self) -> Result<Vec<Action>, StorageError> {
            Ok(self
                .actions
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone())
        }

        async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
            Ok(self
                .actions
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .find(|a| a.id == id)
                .cloned())
        }

        async fn save(&self, _action: &Action) -> Result<(), StorageError> {
            Err(StorageError::NotReady)
        }

        async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
            Err(StorageError::NotReady)
        }

        async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
            Err(StorageError::NotReady)
        }

        async fn record_execution(
            &self,
            _action_id: ActionId,
            _started_at: OffsetDateTime,
            _duration_ms: u64,
            _status: ExecutionStatus,
        ) -> Result<(), StorageError> {
            Err(StorageError::NotReady)
        }

        async fn prune_executions_before(
            &self,
            _cutoff: OffsetDateTime,
        ) -> Result<u64, StorageError> {
            Err(StorageError::NotReady)
        }
    }

    fn action(name: &str) -> Action {
        Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::default(),
            description: None,
            sub_actions: vec![],
        }
    }

    fn stored_instance(kind_id: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: "Stored name".to_owned(),
            overrides: TriggerConfig::new(),
            enabled: false,
            user_defined: false,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 45,
            cooldown_global: false,
            permission_rung: PermissionRung::Everyone,
        }
    }

    fn note_signal(selector: i64) -> MidiSignal {
        MidiSignal {
            kind_id: NOTE_ON.to_owned(),
            selector: Some(selector),
            channel: Some(3),
            device: Some("Keys".to_owned()),
        }
    }

    fn draft(
        instance_id: Option<TriggerInstanceId>,
        signal: MidiSignal,
        action_id: ActionId,
    ) -> MappingDraft {
        MappingDraft {
            instance_id,
            signal,
            action_id,
            name: "Draft name".to_owned(),
        }
    }

    #[tokio::test]
    async fn save_mapping_creates_an_enabled_user_defined_instance_for_a_new_draft() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);

        save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id))
            .await
            .unwrap();

        let stored = triggers.instances();
        assert_eq!(stored.len(), 1);
        let created = &stored[0];
        assert!(created.enabled, "a fresh mapping must be active");
        assert!(created.user_defined, "a fresh mapping must be user defined");
        assert_eq!(created.name, "Draft name");
        assert_eq!(created.kind_id, NOTE_ON);
        assert_eq!(created.overrides.get("note"), Some(&Variant::Int(60)));
        assert_eq!(created.overrides.get("channel"), Some(&Variant::Int(3)));
        assert_eq!(
            created.overrides.get("device"),
            Some(&Variant::String("Keys".to_owned()))
        );
    }

    #[tokio::test]
    async fn save_mapping_links_a_new_mapping_to_the_drafted_action() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        let existing = stored_instance("core.manual");
        triggers.seed_link(target.id, existing.id, 0);
        triggers.seed(existing);

        save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id))
            .await
            .unwrap();

        let created = triggers.instances()[1].id;
        assert!(
            triggers
                .links()
                .iter()
                .any(|(action, instance, _)| *action == target.id && *instance == created),
            "the new mapping must be linked to the drafted action"
        );
    }

    #[tokio::test]
    async fn save_mapping_renames_from_the_draft_and_keeps_cooldowns_when_editing() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        let existing = stored_instance(NOTE_ON);
        let existing_id = existing.id;
        triggers.seed(existing);

        let mut signal = note_signal(7);
        signal.kind_id = CONTROL_CHANGE.to_owned();
        save_mapping(
            &triggers,
            &actions,
            draft(Some(existing_id), signal, target.id),
        )
        .await
        .unwrap();

        let saved = triggers.instances().remove(0);
        assert_eq!(saved.name, "Draft name");
        assert_eq!(saved.cooldown_secs, 45);
        assert!(!saved.cooldown_global);
        assert!(
            !saved.enabled,
            "editing must not silently re-enable a mapping"
        );
        assert_eq!(saved.kind_id, CONTROL_CHANGE);
        assert_eq!(saved.overrides.get("controller"), Some(&Variant::Int(7)));
    }

    #[tokio::test]
    async fn save_mapping_leaves_the_link_untouched_when_the_action_did_not_change() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        let existing = stored_instance(NOTE_ON);
        let existing_id = existing.id;
        triggers.seed(existing);
        triggers.seed_link(target.id, existing_id, 4);

        save_mapping(
            &triggers,
            &actions,
            draft(Some(existing_id), note_signal(61), target.id),
        )
        .await
        .unwrap();

        assert_eq!(
            triggers.unlinks(),
            0,
            "an unchanged action must not be relinked"
        );
        assert_eq!(triggers.links(), vec![(target.id, existing_id, 4)]);
    }

    #[tokio::test]
    async fn save_mapping_moves_the_link_when_the_draft_points_at_another_action() {
        let triggers = FakeTriggerRepo::default();
        let previous = action("Old target");
        let next = action("New target");
        let actions = FakeActionRepo::with(vec![previous.clone(), next.clone()]);
        let existing = stored_instance(NOTE_ON);
        let existing_id = existing.id;
        triggers.seed(existing);
        triggers.seed_link(previous.id, existing_id, 0);

        save_mapping(
            &triggers,
            &actions,
            draft(Some(existing_id), note_signal(61), next.id),
        )
        .await
        .unwrap();

        assert_eq!(triggers.links(), vec![(next.id, existing_id, 0)]);
    }

    #[tokio::test]
    async fn save_mapping_reports_a_missing_instance_instead_of_recreating_it() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);

        let result = save_mapping(
            &triggers,
            &actions,
            draft(Some(TriggerInstanceId::new()), note_signal(60), target.id),
        )
        .await;

        assert!(result.is_err());
        assert!(triggers.instances().is_empty());
    }

    #[tokio::test]
    async fn save_mapping_surfaces_a_repo_write_failure_as_an_error() {
        let triggers = FakeTriggerRepo::failing_save();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);

        let result =
            save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id)).await;

        assert!(result.is_err());
        assert!(
            triggers.links().is_empty(),
            "a failed write must not link the action"
        );
    }

    #[tokio::test]
    async fn load_mappings_returns_only_midi_input_rows_with_their_linked_action() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        let other_family = stored_instance("twitch.chat");
        triggers.seed(other_family);
        let midi = stored_instance(NOTE_ON);
        let midi_id = midi.id;
        triggers.seed(midi);
        triggers.seed_link(target.id, midi_id, 0);

        let rows = load_mappings(&triggers, &actions).await.unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, midi_id);
        assert_eq!(
            rows[0].action.as_ref().map(|(_, name)| name.as_str()),
            Some("Play sound")
        );
    }

    #[tokio::test]
    async fn load_mappings_reports_a_row_whose_linked_action_no_longer_exists_as_unassigned() {
        let triggers = FakeTriggerRepo::default();
        let actions = FakeActionRepo::default();
        let midi = stored_instance(NOTE_ON);
        let midi_id = midi.id;
        triggers.seed(midi);
        triggers.seed_link(ActionId::new(), midi_id, 0);

        let rows = load_mappings(&triggers, &actions).await.unwrap();

        assert!(rows[0].action.is_none());
    }

    fn mapping_row(name: &str, device: Option<&str>) -> MappingRow {
        MappingRow {
            id: TriggerInstanceId::new(),
            name: name.to_owned(),
            signal: MidiSignal {
                kind_id: NOTE_ON.to_owned(),
                selector: Some(60),
                channel: Some(1),
                device: device.map(str::to_owned),
            },
            action: None,
        }
    }

    fn devices(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn visible_mappings_shows_unbound_rows_next_to_the_selected_devices_own_rows() {
        let rows = vec![
            mapping_row("keys", Some("Keys")),
            mapping_row("pad", Some("Pad")),
            mapping_row("any", None),
        ];

        for (selected, expected) in [
            (Some("Keys"), vec!["keys", "any"]),
            (Some("Pad"), vec!["pad", "any"]),
            (Some("Ghost"), vec!["any"]),
            (None, vec!["keys", "pad", "any"]),
        ] {
            let names: Vec<&str> = visible_mappings(&rows, selected)
                .iter()
                .map(|row| row.name.as_str())
                .collect();
            assert_eq!(names, expected, "selected {selected:?}");
        }
    }

    #[test]
    fn resolve_selection_keeps_a_listed_device_and_otherwise_falls_back_to_the_first() {
        for (known, current, expected) in [
            (devices(&["A", "B"]), None, Some("A")),
            (devices(&["A", "B"]), Some("B"), Some("B")),
            (devices(&["A", "B"]), Some("Gone"), Some("A")),
            (devices(&[]), Some("A"), None),
            (devices(&[]), None, None),
        ] {
            assert_eq!(
                resolve_selection(&known, current).as_deref(),
                expected,
                "known {known:?} current {current:?}"
            );
        }
    }

    #[test]
    fn follow_device_switches_only_to_a_known_device_that_is_not_already_selected() {
        let known = devices(&["A", "B"]);

        for (current, saved, expected) in [
            (Some("A"), Some("B"), Some("B")),
            (None, Some("B"), Some("B")),
            (Some("A"), Some("A"), None),
            (Some("A"), Some("Ghost"), None),
            (Some("A"), None, None),
        ] {
            assert_eq!(
                follow_device(&known, current, saved).as_deref(),
                expected,
                "current {current:?} saved {saved:?}"
            );
        }
    }

    #[gpui::test]
    fn the_crumb_leads_back_to_the_midi_category_on_the_hub(cx: &mut gpui::TestAppContext) {
        let rt = crate::test_support::runtime();
        let bus = crate::test_support::quiet_bus(&rt);
        let storage = rt.block_on(crate::test_support::sandboxed_provider());
        let (settings, _writes) = crate::test_support::test_backend();
        let trigger_repo = storage.trigger_instance_repo();
        let action_repo = storage.action_repo();
        let handle = rt.handle().clone();

        let targets = crate::test_support::hub_crumb_targets(cx, |_, cx| {
            MidiScreenView::new(
                SwitchableMidiSink::new(),
                None,
                trigger_repo,
                action_repo,
                settings as Arc<dyn SettingsRepo>,
                bus,
                handle,
                cx,
            )
        });

        let category = forge_midi::MIDI_INTEGRATION.category;
        assert!(!targets.is_empty(), "no click reached the crumb");
        assert!(
            targets
                .iter()
                .all(|screen| *screen == crate::navigation::Screen::Integrations(Some(category))),
            "{targets:?}"
        );
    }

    const STORED_DEVICES: &str = r#"["Keys B"]"#;
    const KNOWN_PORT: &str = "Keys B";
    const NEW_PORT: &str = "Pads A";

    struct DeviceHarness {
        rt: tokio::runtime::Runtime,
        backend: Arc<crate::test_support::TestBackend>,
        writes: tokio::sync::mpsc::UnboundedReceiver<crate::test_support::SettingWrite>,
    }

    impl DeviceHarness {
        fn new(stored: &str, cx: &mut gpui::TestAppContext) -> Self {
            crate::test_support::install_presentation(cx);
            cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
            let (backend, writes) = crate::test_support::test_backend();
            backend.seed_raw(&MIDI_KNOWN_DEVICES, stored);
            Self {
                rt: crate::test_support::runtime(),
                backend,
                writes,
            }
        }

        fn open(&self, cx: &mut gpui::TestAppContext) -> Entity<MidiScreenView> {
            let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
            let bus = crate::test_support::quiet_bus(&self.rt);
            let handle = self.rt.handle().clone();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    MidiScreenView::new(
                        SwitchableMidiSink::new(),
                        None,
                        Arc::new(FakeTriggerRepo::default()),
                        Arc::new(FakeActionRepo::with(Vec::new())),
                        settings,
                        bus,
                        handle,
                        cx,
                    )
                })
            });
            self.settle(cx);
            view
        }

        fn settle(&self, cx: &mut gpui::TestAppContext) {
            for _ in 0..3 {
                crate::test_support::pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn device_writes(&mut self) -> Vec<Vec<String>> {
            crate::test_support::writes_to(&mut self.writes, MIDI_KNOWN_DEVICES.name())
                .iter()
                .map(|raw| serde_json::from_str(raw).unwrap())
                .collect()
        }
    }

    fn plug_in(view: &Entity<MidiScreenView>, cx: &mut gpui::TestAppContext) {
        view.update(cx, |view, cx| {
            view.live_ports = vec![NEW_PORT.to_owned()];
            view.remember_live_ports(cx);
        });
    }

    fn device_failure(
        view: &Entity<MidiScreenView>,
        cx: &mut gpui::TestAppContext,
    ) -> Option<String> {
        view.read_with(cx, |view, _| view.known_devices.failure())
    }

    fn merged() -> Vec<String> {
        vec![KNOWN_PORT.to_owned(), NEW_PORT.to_owned()]
    }

    #[gpui::test]
    fn a_live_port_after_a_failed_load_writes_nothing_over_the_known_devices(
        cx: &mut gpui::TestAppContext,
    ) {
        let unreachable = crate::test_support::shown_in_english(StorageError::Connection {
            reason: crate::test_support::SETTINGS_READ_FAILED.to_owned(),
        });
        let undecodable = crate::test_support::shown_in_english(StorageError::UndecodableSetting {
            key: MIDI_KNOWN_DEVICES.name().to_owned(),
        });
        for (fault, stored, failing_read, reason) in [
            ("unreachable store", STORED_DEVICES, true, unreachable),
            ("undecodable value", "not json", false, undecodable),
        ] {
            let mut harness = DeviceHarness::new(stored, cx);
            if failing_read {
                harness.backend.fault_reads(
                    MIDI_KNOWN_DEVICES.name(),
                    crate::test_support::ReadFault::Fails,
                );
            }
            let view = harness.open(cx);

            plug_in(&view, cx);
            harness.settle(cx);

            assert_eq!(
                (harness.device_writes(), device_failure(&view, cx)),
                (Vec::<Vec<String>>::new(), Some(reason)),
                "{fault}"
            );
        }
    }

    #[gpui::test]
    fn a_port_seen_while_the_load_is_pending_is_merged_and_written_once_the_load_lands(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut harness = DeviceHarness::new(STORED_DEVICES, cx);
        let gate = Arc::new(tokio::sync::Notify::new());
        harness.backend.fault_reads(
            MIDI_KNOWN_DEVICES.name(),
            crate::test_support::ReadFault::Held(Arc::clone(&gate)),
        );
        let view = harness.open(cx);

        plug_in(&view, cx);
        harness.settle(cx);
        let while_pending = harness.device_writes();
        harness.backend.heal_reads(MIDI_KNOWN_DEVICES.name());
        gate.notify_one();
        harness.settle(cx);

        assert_eq!(
            (while_pending, harness.device_writes()),
            (Vec::<Vec<String>>::new(), vec![merged()])
        );
    }

    #[gpui::test]
    fn retry_after_a_failed_load_restores_the_known_devices_and_a_new_port_keeps_them(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut harness = DeviceHarness::new(STORED_DEVICES, cx);
        harness.backend.fault_reads(
            MIDI_KNOWN_DEVICES.name(),
            crate::test_support::ReadFault::Fails,
        );
        let view = harness.open(cx);
        harness.backend.heal_reads(MIDI_KNOWN_DEVICES.name());

        view.update(cx, |view, cx| view.load_known_devices(cx));
        harness.settle(cx);
        plug_in(&view, cx);
        harness.settle(cx);

        assert_eq!(harness.device_writes(), vec![merged()]);
    }

    #[gpui::test]
    fn a_failed_device_write_shows_the_error_without_reloading_into_another_write(
        cx: &mut gpui::TestAppContext,
    ) {
        let read_only = crate::test_support::shown_in_english(StorageError::Connection {
            reason: crate::test_support::SETTINGS_WRITE_FAILED.to_owned(),
        });
        let harness = DeviceHarness::new(STORED_DEVICES, cx);
        harness.backend.fail_writes(MIDI_KNOWN_DEVICES.name());
        let view = harness.open(cx);
        let reload_gate = Arc::new(tokio::sync::Notify::new());
        harness.backend.fault_reads(
            MIDI_KNOWN_DEVICES.name(),
            crate::test_support::ReadFault::Held(reload_gate),
        );

        plug_in(&view, cx);
        harness.settle(cx);

        assert_eq!(device_failure(&view, cx), Some(read_only));
    }
}
