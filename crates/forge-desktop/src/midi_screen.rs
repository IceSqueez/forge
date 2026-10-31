mod bridges;
mod devices_column;
mod mapping_edits;
mod mapping_store;
mod mappings_column;
mod selection;
#[cfg(test)]
mod tests;

pub(crate) use self::mapping_store::MIDI_KNOWN_DEVICES;
use crate::async_bridge::{Loadable, SnapshotWriter};
use crate::hub_crumb::hub_crumb;
use crate::integrations::BuiltinObject;
use crate::midi_mapping_modal::MidiMappingModal;
use crate::midi_signal::MidiSignal;
use crate::navigation::NavRequested;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, grow_cell, header_status, page_frame, scaled, tr,
};
use forge_midi::{MidiClient, MidiMonitorEvent, SwitchableMidiSink};
use forge_runtime::{EventBus, IntegrationSlot};
use forge_storage::{ActionRepo, SettingsRepo, TriggerInstanceRepo};
use forge_types::{ActionId, TriggerInstanceId};
use gpui::{
    Context, Entity, EventEmitter, Pixels, Point, Subscription, Task, Window, div, prelude::*, px,
};
use std::sync::Arc;

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const COLUMNS_GAP: Pixels = px(12.0);
const LEFT_COLUMN_FLEX: f32 = 1.0;
const RIGHT_COLUMN_FLEX: f32 = 1.7;

struct MappingRow {
    id: TriggerInstanceId,
    name: String,
    signal: MidiSignal,
    action: Option<(ActionId, String)>,
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

pub(crate) struct MidiScreenView {
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
    pub(crate) fn new(
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
                devices = i64::try_from(self.live_ports.len()).unwrap_or(i64::MAX),
                mappings = i64::try_from(self.mappings.len()).unwrap_or(i64::MAX)
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
