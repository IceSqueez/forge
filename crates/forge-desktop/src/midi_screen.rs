use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, FONT_XS, ForgePalette, Icon, MenuPlacement, ToastAction,
    ToastKind, badge, body_family, card, ghost_button_with_icon, header_status, icon, menu_button,
    menu_divider, menu_item, mono_family, page_frame, status_dot, toggle, tr,
};
use forge_midi::{MidiClient, MidiMonitorEvent};
use forge_runtime::EventBus;
use forge_storage::{
    ActionRepo, SettingsRepo, TriggerInstanceRepo, set_bool_setting, set_json_setting,
};
use forge_types::{ActionId, PlatformScope, TriggerInstance, TriggerInstanceId};
use futures_util::StreamExt;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Point, Rgba, SharedString, Subscription,
    Task, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, BridgeFlow, ErrorSink, drain_events};
use crate::builtin_sections::grow_cell;
use crate::midi_mapping_modal::{
    MappingDraft, MappingModalLaunch, MidiMappingModal, MidiMappingModalEvent,
};
use crate::midi_signal::{MIDI_INPUT_PREFIX, MidiSignal, kind_color, note_name};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

pub const MIDI_ENABLED_KEY: &str = "midi.enabled";
pub const MIDI_KNOWN_DEVICES_KEY: &str = "midi.known_devices";

const MIDI_PORT_PREFIX: &str = "midi.port.";

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

const COLUMNS_GAP: Pixels = px(12.0);
const LEFT_COLUMN_FLEX: f32 = 1.0;
const RIGHT_COLUMN_FLEX: f32 = 1.7;

const SECTION_LABEL_FS: Pixels = px(9.5);
const SECTION_LABEL_MT: Pixels = px(4.0);
const SECTION_LABEL_MB: Pixels = px(8.0);
const LIST_GAP: Pixels = px(6.0);

const DEVICE_PAD_V: Pixels = px(10.0);
const DEVICE_PAD_H: Pixels = px(12.0);
const DEVICE_GAP: Pixels = px(10.0);
const DEVICE_GLYPH: Pixels = px(14.0);
const DEVICE_NAME_FS: Pixels = px(12.5);
const DEVICE_BADGE_FS: Pixels = px(9.5);
const OFFLINE_OPACITY: f32 = 0.6;
const RESCAN_MT: Pixels = px(2.0);

const MONITOR_MT: Pixels = px(12.0);
const MONITOR_PAD: Pixels = px(12.0);
const MONITOR_FS: Pixels = px(10.5);
const MONITOR_LINE_H: Pixels = px(18.9);
const MONITOR_GAP: Pixels = px(8.0);
const MONITOR_KIND_W: Pixels = px(54.0);
const MONITOR_ROWS: usize = 4;

const BINDINGS_FS: Pixels = px(10.0);
const MAP_PAD_V: Pixels = px(9.0);
const MAP_PAD_H: Pixels = px(12.0);
const MAP_GAP: Pixels = px(10.0);
const SIG_FS: Pixels = px(11.5);
const SIG_RADIUS: Pixels = px(5.0);
const SIG_PAD_V: Pixels = px(3.0);
const SIG_PAD_H: Pixels = px(8.0);
const SIG_MIN_W: Pixels = px(66.0);
const CH_BADGE_FS: Pixels = px(9.0);
const ARROW_GLYPH: Pixels = px(13.0);
const ACCENT_DOT: Pixels = px(6.0);
const TARGET_FS: Pixels = px(12.0);

const ADD_BAR_PAD_V: Pixels = px(9.0);
const ADD_BAR_PAD_H: Pixels = px(12.0);
const ADD_BAR_RADIUS: Pixels = px(9.0);
const ADD_BAR_GAP: Pixels = px(6.0);
const ADD_BAR_GLYPH: Pixels = px(13.0);
const ADD_BAR_FS: Pixels = px(12.0);
const KBD_FS: Pixels = px(10.0);
const KBD_ML: Pixels = px(6.0);
const KBD_PAD_V: Pixels = px(1.0);
const KBD_PAD_H: Pixels = px(6.0);
const KBD_RADIUS: Pixels = px(4.0);

const FOOTER_FS: Pixels = px(10.5);
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);

const LEARN_PAD_V: Pixels = px(11.0);
const LEARN_GAP: Pixels = px(8.0);
const LEARN_GLYPH: Pixels = px(14.0);
const LEARN_CANCEL_ML: Pixels = px(6.0);

const UNDO_TOAST_MS: u64 = 6000;

fn monitor_color(kind: &str, palette: &ForgePalette) -> Rgba {
    match kind {
        "note_on" => palette.success,
        "note_off" => palette.text_faint,
        "control_change" | "pitch_bend" => palette.info,
        "program_change" => palette.brand,
        _ => palette.text_secondary,
    }
}

fn monitor_kind_label(kind: &str) -> &str {
    match kind {
        "note_on" => "Note On",
        "note_off" => "Note Off",
        "control_change" => "CC",
        "pitch_bend" => "Pitch",
        "program_change" => "PC",
        other => other,
    }
}

fn monitor_value_label(event: &MidiMonitorEvent) -> String {
    match event.kind.as_str() {
        "note_on" => match (event.number, event.value) {
            (Some(note), Some(velocity)) => {
                format!("{} vel {velocity}", note_name(i64::from(note)))
            }
            (Some(note), None) => note_name(i64::from(note)),
            _ => String::new(),
        },
        "note_off" => event
            .number
            .map(|note| note_name(i64::from(note)))
            .unwrap_or_default(),
        "control_change" => match (event.number, event.value) {
            (Some(controller), Some(value)) => format!("{controller} \u{2192} {value}"),
            _ => String::new(),
        },
        "pitch_bend" => event.value.map(|v| v.to_string()).unwrap_or_default(),
        "program_change" => event.number.map(|n| n.to_string()).unwrap_or_default(),
        _ => String::new(),
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
) -> Result<Vec<MappingRow>, String> {
    let instances = triggers.list_all().await.map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for instance in instances {
        if !instance.kind_id.starts_with(MIDI_INPUT_PREFIX) {
            continue;
        }
        let linked = triggers
            .actions_using(instance.id)
            .await
            .map_err(|e| e.to_string())?;
        let mut action = None;
        if let Some(first) = linked.first()
            && let Some(found) = actions.get(*first).await.map_err(|e| e.to_string())?
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
) -> Result<Vec<MappingRow>, String> {
    let overrides = draft.signal.overrides();
    let instance_id = match draft.instance_id {
        Some(id) => {
            let source = triggers
                .get(id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "trigger instance not found".to_owned())?;
            let updated = TriggerInstance {
                kind_id: draft.signal.kind_id.clone(),
                name: draft.name.clone(),
                overrides,
                ..source
            };
            triggers.save(&updated).await.map_err(|e| e.to_string())?;
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
            };
            triggers.save(&instance).await.map_err(|e| e.to_string())?;
            instance.id
        }
    };

    let linked = triggers
        .actions_using(instance_id)
        .await
        .map_err(|e| e.to_string())?;
    if !linked.contains(&draft.action_id) {
        for previous in linked {
            triggers
                .unlink_action(previous, instance_id)
                .await
                .map_err(|e| e.to_string())?;
        }
        let position = triggers
            .list_for_action(draft.action_id)
            .await
            .map_err(|e| e.to_string())?
            .len() as i64;
        triggers
            .link_action(draft.action_id, instance_id, position)
            .await
            .map_err(|e| e.to_string())?;
    }

    load_mappings(triggers, actions).await
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
    client: Arc<MidiClient>,
    trigger_repo: Arc<dyn TriggerInstanceRepo>,
    action_repo: Arc<dyn ActionRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    enabled: bool,
    live_ports: Vec<String>,
    known_devices: Vec<String>,
    known_devices_loaded: bool,
    monitor: Vec<MidiMonitorEvent>,
    mappings: Vec<MappingRow>,
    capture: Capture,
    modal: Option<OpenModal>,
    menu_open: Option<TriggerInstanceId>,
    menu_click_pos: Option<Point<Pixels>>,
    _monitor_bridge: Task<()>,
    _port_bridge: Task<()>,
}

impl MidiScreenView {
    pub fn new(
        client: Arc<MidiClient>,
        trigger_repo: Arc<dyn TriggerInstanceRepo>,
        action_repo: Arc<dyn ActionRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let monitor_bridge = Self::spawn_monitor_bridge(&client, &rt_handle, cx);
        let port_bridge = Self::spawn_port_bridge(bus, cx);
        let mut view = Self {
            enabled: client.is_enabled(),
            live_ports: client.connected_input_ports(),
            client,
            trigger_repo,
            action_repo,
            settings_repo,
            rt_handle,
            known_devices: Vec::new(),
            known_devices_loaded: false,
            monitor: Vec::new(),
            mappings: Vec::new(),
            capture: Capture::Off,
            modal: None,
            menu_open: None,
            menu_click_pos: None,
            _monitor_bridge: monitor_bridge,
            _port_bridge: port_bridge,
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

    fn spawn_port_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                let touched = batch
                    .iter()
                    .any(|event| event.kind.starts_with(MIDI_PORT_PREFIX));
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
        let settings = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                settings
                    .get_string(MIDI_KNOWN_DEVICES_KEY)
                    .await
                    .map(|stored| {
                        stored
                            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
                            .unwrap_or_default()
                    })
                    .map_err(|e| e.to_string())
            },
            |this, result: Result<Vec<String>, String>, cx| match result {
                Ok(known) => {
                    this.known_devices = known;
                    this.known_devices_loaded = true;
                    this.remember_live_ports(cx);
                    cx.notify();
                }
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
    }

    fn apply_mappings(&mut self, result: Result<Vec<MappingRow>, String>, cx: &mut Context<Self>) {
        match result {
            Ok(rows) => {
                self.mappings = rows;
                cx.notify();
            }
            Err(message) => self.on_repo_error(&message, cx),
        }
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: midi operation failed: {message}");
        cx.push_toast(ToastKind::Error, tr!("midi_toast_error", message = message));
        cx.notify();
    }

    fn push_monitor(&mut self, event: MidiMonitorEvent, cx: &mut Context<Self>) {
        if self.capture != Capture::Off
            && let Some(signal) = MidiSignal::from_monitor(&event)
        {
            self.on_capture(signal, cx);
        }
        self.monitor.insert(0, event);
        self.monitor.truncate(MONITOR_ROWS);
        cx.notify();
    }

    fn on_capture(&mut self, signal: MidiSignal, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
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
        cx.notify();
    }

    fn cancel_learn(&mut self, cx: &mut Context<Self>) {
        self.capture = Capture::Off;
        cx.notify();
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
            devices: self.known_devices.clone(),
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
        cx.notify();
    }

    fn persist_mapping(&mut self, draft: MappingDraft, cx: &mut Context<Self>) {
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { save_mapping(&*triggers, &*actions, draft).await },
            |this, result, cx| this.apply_mappings(result, cx),
            cx,
        );
    }

    fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        let ports = self.client.connected_input_ports();
        if ports == self.live_ports {
            return;
        }
        self.live_ports = ports;
        self.remember_live_ports(cx);
        cx.notify();
    }

    fn remember_live_ports(&mut self, cx: &mut Context<Self>) {
        if !self.known_devices_loaded {
            return;
        }
        let discovered: Vec<String> = self
            .live_ports
            .iter()
            .filter(|port| !self.known_devices.contains(port))
            .cloned()
            .collect();
        if discovered.is_empty() {
            return;
        }
        self.known_devices.extend(discovered);
        self.known_devices.sort();
        let repo = Arc::clone(&self.settings_repo);
        let devices = self.known_devices.clone();
        async_bridge::report_failure(
            &self.rt_handle,
            async move { set_json_setting(repo.as_ref(), MIDI_KNOWN_DEVICES_KEY, &devices).await },
            ErrorSink::Silent,
            "midi known devices",
            cx,
        );
    }

    fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
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
                    client.enable_input().await.map_err(|e| e.to_string())?;
                } else {
                    client.disable_input().await.map_err(|e| e.to_string())?;
                }
                set_bool_setting(repo.as_ref(), MIDI_ENABLED_KEY, enabled)
                    .await
                    .map_err(|e| e.to_string())
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
        if !self.known_devices_loaded {
            self.load_known_devices(cx);
        }
        let client = Arc::clone(&self.client);
        async_bridge::run_async(
            &self.rt_handle,
            async move { client.rescan_ports().await.map_err(|e| e.to_string()) },
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => this.refresh_ports(cx),
                Err(message) => this.on_repo_error(&message, cx),
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
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "trigger instance not found".to_owned())?;
                let copy = TriggerInstance {
                    id: TriggerInstanceId::new(),
                    name: copy_name,
                    user_defined: true,
                    ..source
                };
                triggers.save(&copy).await.map_err(|e| e.to_string())?;
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
                triggers.archive(id).await.map_err(|e| e.to_string())?;
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
                            triggers.restore(id).await.map_err(|e| e.to_string())?;
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
                    .gap(HERO_TOGGLE_GAP)
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
            .mb(HERO_MARGIN_B)
            .child(
                card(body, palette)
                    .padding_xy(HERO_PAD_V, HERO_PAD_H)
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_devices(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let cards: Vec<AnyElement> = self
            .known_devices
            .iter()
            .map(|name| {
                let live_index = self.live_ports.iter().position(|port| port == name);
                self.render_device(name, live_index, palette)
            })
            .collect();

        let mut list = div().w_full().flex().flex_col().gap(LIST_GAP);
        if cards.is_empty() {
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("midi_devices_empty")),
            );
        } else {
            list = list.children(cards);
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_label(&tr!("midi_section_devices"), palette, None))
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
        name: &str,
        live_index: Option<usize>,
        palette: &ForgePalette,
    ) -> AnyElement {
        let online = live_index.is_some();
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
            .gap(DEVICE_GAP)
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

        let mut wrapper = div().w_full();
        if !online {
            wrapper = wrapper.opacity(OFFLINE_OPACITY);
        }
        wrapper
            .child(
                card(row, palette)
                    .padding_xy(DEVICE_PAD_V, DEVICE_PAD_H)
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_monitor(&self, palette: &ForgePalette) -> AnyElement {
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
                    .gap(MONITOR_GAP)
                    .child(
                        div()
                            .w(MONITOR_KIND_W)
                            .flex_shrink_0()
                            .text_color(palette.text_faint)
                            .child(monitor_kind_label(&event.kind).to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(monitor_color(&event.kind, palette))
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
            .child(section_label(&tr!("midi_section_monitor"), palette, None))
            .child(lines);

        div()
            .w_full()
            .mt(MONITOR_MT)
            .child(card(body, palette).padding(MONITOR_PAD).full_width())
            .into_any_element()
    }

    fn render_mappings(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let count = div()
            .font_family(mono_family())
            .text_size(BINDINGS_FS)
            .text_color(palette.text_faint)
            .child(tr!(
                "midi_bindings_count",
                count = self.mappings.len() as i64
            ))
            .into_any_element();

        let mut list = div().w_full().flex().flex_col().gap(LIST_GAP);
        if self.mappings.is_empty() {
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("midi_mappings_empty")),
            );
        } else {
            for (index, row) in self.mappings.iter().enumerate() {
                list = list.child(self.render_mapping(index, row, palette, cx));
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_label(
                &tr!("midi_section_mappings"),
                palette,
                Some(count),
            ))
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
        let (target_text, target_ink) = match row.action.as_ref() {
            Some((_, name)) => (name.clone(), palette.text_primary),
            None => (tr!("midi_unassigned"), palette.text_faint),
        };
        let id = row.id;
        let content = div()
            .id(("midi-mapping-row", index))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(MAP_GAP)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_mapping(id, cx)))
            .child(
                div()
                    .flex_none()
                    .min_w(SIG_MIN_W)
                    .py(SIG_PAD_V)
                    .px(SIG_PAD_H)
                    .rounded(SIG_RADIUS)
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .text_align(gpui::TextAlign::Center)
                    .font_family(mono_family())
                    .text_size(SIG_FS)
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
            )
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
            .gap(MAP_GAP)
            .child(content)
            .child(self.render_row_menu(index, row.id, palette, cx));

        div()
            .w_full()
            .child(
                card(body, palette)
                    .padding_xy(MAP_PAD_V, MAP_PAD_H)
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
                .color(palette.random)
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
        if self.capture == Capture::Learn {
            return self.render_learn_row(palette, cx);
        }
        div()
            .id("midi-add-learn")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(ADD_BAR_GAP)
            .py(ADD_BAR_PAD_V)
            .px(ADD_BAR_PAD_H)
            .rounded(ADD_BAR_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .bg(palette.shell)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_learn(cx)))
            .child(icon(Icon::Plus, ADD_BAR_GLYPH, palette.info))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(palette.info)
                    .child(tr!("midi_add_learn")),
            )
            .child(
                div()
                    .ml(KBD_ML)
                    .py(KBD_PAD_V)
                    .px(KBD_PAD_H)
                    .rounded(KBD_RADIUS)
                    .bg(palette.surface_overlay)
                    .font_family(mono_family())
                    .text_size(KBD_FS)
                    .text_color(palette.text_faint)
                    .child(tr!("midi_add_learn_kbd")),
            )
            .into_any_element()
    }

    fn render_learn_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let (accent, prompt) = if self.enabled {
            (palette.info, tr!("midi_learn_prompt"))
        } else {
            (palette.warning, tr!("midi_learn_input_disabled"))
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(LEARN_GAP)
            .py(LEARN_PAD_V)
            .px(ADD_BAR_PAD_H)
            .rounded(ADD_BAR_RADIUS)
            .border(BORDER_THIN)
            .border_color(accent)
            .bg(palette.surface_overlay)
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
                    .ml(LEARN_CANCEL_ML)
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.text_faint)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_learn(cx)))
                    .child(tr!("common_cancel")),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette) -> AnyElement {
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
            .py(FOOTER_PAD_V)
            .px(FOOTER_PAD_H)
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
                    .gap(px(6.0))
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

fn section_label(
    label: &str,
    palette: &ForgePalette,
    right: Option<AnyElement>,
) -> impl IntoElement {
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
        .children(right)
}

impl Render for MidiScreenView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = header_status(
            palette.success,
            tr!(
                "midi_header_summary",
                devices = self.live_ports.len() as i64,
                mappings = self.mappings.len() as i64
            ),
        );

        let left = grow_cell(
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(self.render_devices(&palette, cx))
                .child(self.render_monitor(&palette)),
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
                    .py(SCROLL_PAD_Y)
                    .px(SCROLL_PAD_X)
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, cx))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(COLUMNS_GAP)
                            .child(left)
                            .child(right),
                    )
                    .child(self.render_footer(&palette)),
            );

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("midi_breadcrumb_builtin")),
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_storage::{ActionTelemetry, ExecutionStatus, StorageError};
    use forge_types::{Action, ExecutionMode, QueueId, TriggerConfig, Variant};
    use time::OffsetDateTime;

    use super::*;
    use crate::midi_signal::{CONTROL_CHANGE_KIND, NOTE_ON_KIND};

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

        async fn list_by_group<'a>(
            &'a self,
            _group: Option<&'a str>,
        ) -> Result<Vec<Action>, StorageError> {
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
        }
    }

    fn note_signal(selector: i64) -> MidiSignal {
        MidiSignal {
            kind_id: NOTE_ON_KIND.to_owned(),
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
        assert_eq!(created.kind_id, NOTE_ON_KIND);
        assert_eq!(created.overrides.get("note"), Some(&Variant::Int(60)));
        assert_eq!(created.overrides.get("channel"), Some(&Variant::Int(3)));
        assert_eq!(
            created.overrides.get("device"),
            Some(&Variant::String("Keys".to_owned()))
        );
    }

    #[tokio::test]
    async fn save_mapping_appends_the_new_link_after_the_actions_existing_triggers() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        for _ in 0..2 {
            let existing = stored_instance("core.manual");
            triggers.seed_link(target.id, existing.id, 0);
            triggers.seed(existing);
        }

        save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id))
            .await
            .unwrap();

        let created = triggers.instances()[2].id;
        assert!(
            triggers.links().contains(&(target.id, created, 2)),
            "the new link must land after the two existing ones"
        );
    }

    #[tokio::test]
    async fn save_mapping_renames_from_the_draft_and_keeps_cooldowns_when_editing() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        let existing = stored_instance(NOTE_ON_KIND);
        let existing_id = existing.id;
        triggers.seed(existing);

        let mut signal = note_signal(7);
        signal.kind_id = CONTROL_CHANGE_KIND.to_owned();
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
        assert_eq!(saved.kind_id, CONTROL_CHANGE_KIND);
        assert_eq!(saved.overrides.get("controller"), Some(&Variant::Int(7)));
    }

    #[tokio::test]
    async fn save_mapping_leaves_the_link_untouched_when_the_action_did_not_change() {
        let triggers = FakeTriggerRepo::default();
        let target = action("Play sound");
        let actions = FakeActionRepo::with(vec![target.clone()]);
        let existing = stored_instance(NOTE_ON_KIND);
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
        let existing = stored_instance(NOTE_ON_KIND);
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
        let midi = stored_instance(NOTE_ON_KIND);
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
        let midi = stored_instance(NOTE_ON_KIND);
        let midi_id = midi.id;
        triggers.seed(midi);
        triggers.seed_link(ActionId::new(), midi_id, 0);

        let rows = load_mappings(&triggers, &actions).await.unwrap();

        assert!(rows[0].action.is_none());
    }
}
