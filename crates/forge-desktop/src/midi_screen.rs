use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, FONT_XS, ForgePalette, Icon, MenuPlacement, ToastAction,
    ToastKind, badge, body_family, card, ghost_button_with_icon, header_status, icon, menu_button,
    menu_divider, menu_item, mono_family, page_frame, status_dot, toggle, tr,
};
use forge_midi::{MidiClient, MidiMonitorEvent};
use forge_storage::{
    ActionRepo, SettingsRepo, TriggerInstanceRepo, get_json_setting, set_bool_setting,
    set_json_setting,
};
use forge_types::{TriggerConfig, TriggerInstance, TriggerInstanceId, Variant};
use futures_util::StreamExt;
use gpui::{
    AnyElement, App, ClickEvent, Context, Pixels, Point, Rgba, SharedString, Task, Window, div,
    prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::builtin_sections::grow_cell;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

pub const MIDI_ENABLED_KEY: &str = "midi.enabled";
pub const MIDI_KNOWN_DEVICES_KEY: &str = "midi.known_devices";

const MIDI_INPUT_PREFIX: &str = "midi.input.";
const NOTE_ON_KIND: &str = "midi.input.note_on";
const NOTE_OFF_KIND: &str = "midi.input.note_off";
const CONTROL_CHANGE_KIND: &str = "midi.input.control_change";
const PITCH_BEND_KIND: &str = "midi.input.pitch_bend";
const PROGRAM_CHANGE_KIND: &str = "midi.input.program_change";

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

const PORT_POLL_INTERVAL: Duration = Duration::from_secs(2);
const UNDO_TOAST_MS: u64 = 6000;

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Middle C (MIDI note 60) is named C4.
fn note_name(note: i64) -> String {
    let clamped = note.clamp(0, 127);
    let octave = clamped / 12 - 1;
    let name = NOTE_NAMES[(clamped % 12) as usize];
    format!("{name}{octave}")
}

fn config_int(config: &TriggerConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(n)) => Some(*n),
        _ => None,
    }
}

fn config_text(config: &TriggerConfig, key: &str) -> Option<String> {
    match config.get(key) {
        Some(Variant::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn selector_of(kind_id: &str, config: &TriggerConfig) -> Option<i64> {
    match kind_id {
        NOTE_ON_KIND | NOTE_OFF_KIND => config_int(config, "note"),
        CONTROL_CHANGE_KIND => config_int(config, "controller"),
        PROGRAM_CHANGE_KIND => config_int(config, "program"),
        _ => None,
    }
}

fn kind_color(kind_id: &str, palette: &ForgePalette) -> Rgba {
    match kind_id {
        NOTE_ON_KIND => palette.success,
        NOTE_OFF_KIND => palette.text_faint,
        CONTROL_CHANGE_KIND | PITCH_BEND_KIND => palette.info,
        PROGRAM_CHANGE_KIND => palette.brand,
        _ => palette.text_faint,
    }
}

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
    kind_id: String,
    name: String,
    selector: Option<i64>,
    channel: Option<i64>,
    device: Option<String>,
    action_name: Option<String>,
}

impl MappingRow {
    fn signature(&self) -> String {
        let value = match self.selector {
            Some(n) if self.kind_id == NOTE_ON_KIND || self.kind_id == NOTE_OFF_KIND => {
                note_name(n)
            }
            Some(n) => n.to_string(),
            None => tr!("midi_value_any"),
        };
        match self.kind_id.as_str() {
            NOTE_ON_KIND => format!("Note {value}"),
            NOTE_OFF_KIND => format!("NoteOff {value}"),
            CONTROL_CHANGE_KIND => format!("CC {value}"),
            PROGRAM_CHANGE_KIND => format!("PC {value}"),
            PITCH_BEND_KIND => "Pitch".to_owned(),
            other => other.to_owned(),
        }
    }

    fn channel_label(&self) -> String {
        let value = match self.channel {
            Some(channel) => channel.to_string(),
            None => tr!("midi_value_any"),
        };
        format!("ch {value}")
    }
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
        let mut action_name = None;
        if let Some(first) = linked.first()
            && let Some(action) = actions.get(*first).await.map_err(|e| e.to_string())?
        {
            action_name = Some(action.name);
        }
        rows.push(MappingRow {
            selector: selector_of(&instance.kind_id, &instance.overrides),
            channel: config_int(&instance.overrides, "channel"),
            device: config_text(&instance.overrides, "device"),
            id: instance.id,
            kind_id: instance.kind_id,
            name: instance.name,
            action_name,
        });
    }
    Ok(rows)
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
    monitor: Vec<MidiMonitorEvent>,
    mappings: Vec<MappingRow>,
    menu_open: Option<TriggerInstanceId>,
    menu_click_pos: Option<Point<Pixels>>,
    _monitor_bridge: Task<()>,
    _port_poll: Task<()>,
}

impl MidiScreenView {
    pub fn new(
        client: Arc<MidiClient>,
        trigger_repo: Arc<dyn TriggerInstanceRepo>,
        action_repo: Arc<dyn ActionRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let monitor_bridge = Self::spawn_monitor_bridge(&client, &rt_handle, cx);
        let port_poll = Self::spawn_port_poll(cx);
        let mut view = Self {
            enabled: client.is_enabled(),
            live_ports: client.connected_input_ports(),
            client,
            trigger_repo,
            action_repo,
            settings_repo,
            rt_handle,
            known_devices: Vec::new(),
            monitor: Vec::new(),
            mappings: Vec::new(),
            menu_open: None,
            menu_click_pos: None,
            _monitor_bridge: monitor_bridge,
            _port_poll: port_poll,
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

    fn spawn_port_poll(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PORT_POLL_INTERVAL).await;
                if this.update(cx, |this, cx| this.refresh_ports(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.settings_repo);
        let triggers = Arc::clone(&self.trigger_repo);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let known =
                    get_json_setting::<Vec<String>>(settings.as_ref(), MIDI_KNOWN_DEVICES_KEY)
                        .await
                        .unwrap_or_default();
                (known, load_mappings(&*triggers, &*actions).await)
            },
            |this, (known, mappings), cx| {
                this.known_devices = known;
                this.remember_live_ports(cx);
                this.apply_mappings(mappings, cx);
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
        self.monitor.insert(0, event);
        self.monitor.truncate(MONITOR_ROWS);
        cx.notify();
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
        self.enabled = !self.enabled;
        let enabled = self.enabled;
        let client = Arc::clone(&self.client);
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::report_failure(
            &self.rt_handle,
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
            ErrorSink::Toast,
            tr!("midi_toggle_failed"),
            cx,
        );
        cx.notify();
    }

    fn rescan(&mut self, cx: &mut Context<Self>) {
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
            .filter(|row| row.device.as_deref() == Some(port))
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

        if self.monitor.is_empty() {
            lines = lines.child(
                div()
                    .text_color(palette.text_faint)
                    .child(tr!("midi_monitor_empty")),
            );
        } else {
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
            .child(list.child(self.render_add_bar(palette)))
            .into_any_element()
    }

    fn render_mapping(
        &self,
        index: usize,
        row: &MappingRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (target_text, target_ink) = match row.action_name.as_ref() {
            Some(name) => (name.clone(), palette.text_primary),
            None => (tr!("midi_unassigned"), palette.text_faint),
        };
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(MAP_GAP)
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
                    .child(row.signature()),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    row.channel_label(),
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
                    .bg(kind_color(&row.kind_id, palette)),
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
                    |_, _, _| {},
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

    fn render_add_bar(&self, palette: &ForgePalette) -> AnyElement {
        div()
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
    }
}
