use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DetailSection, KeyValueRow, SectionIcon,
    TrailingToken,
};

use forge_types::{UserText, VerbatimText};

use crate::client::MidiClient;
use crate::events::{MidiPortInfo, PortDirection};
use crate::vocabulary::labels;

#[derive(Default)]
pub(crate) struct MidiContentSnapshot {
    pub input_ports: Vec<MidiPortInfo>,
    pub output_ports: Vec<MidiPortInfo>,
    pub events_per_input: HashMap<String, u64>,
    pub subscribed_inputs: HashSet<String>,
    pub total_events: u64,
}

pub(crate) fn make_content_state() -> Arc<Mutex<MidiContentSnapshot>> {
    Arc::new(Mutex::new(MidiContentSnapshot::default()))
}

impl BuiltinContent for MidiClient {
    fn sections(&self) -> Vec<DetailSection> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        let client_name = self.config.client_name.clone();
        let total = snap.total_events;
        let engine_enabled = self.is_enabled();

        let input_items: Vec<ContentListItem> = snap
            .input_ports
            .iter()
            .map(|p| {
                let count = snap.events_per_input.get(&p.name).copied().unwrap_or(0);
                let subscribed = snap.subscribed_inputs.contains(&p.name);
                let status_label = match (subscribed, engine_enabled) {
                    (true, _) => labels::SUBSCRIBED,
                    (false, true) => labels::OPEN_FAILED,
                    (false, false) => labels::DISABLED,
                };
                ContentListItem {
                    icon: SectionIcon::new("plug"),
                    icon_tint: None,
                    name: p.name.clone(),
                    monospace_name: true,
                    active: subscribed,
                    active_label: Some(status_label.into()),
                    trailing: vec![TrailingToken::Label(
                        VerbatimText::new(count.to_string()).into(),
                    )],
                    enabled: true,
                    on_click: None,
                }
            })
            .collect();

        let output_items: Vec<ContentListItem> = snap
            .output_ports
            .iter()
            .map(|p| ContentListItem {
                icon: SectionIcon::new("plug"),
                icon_tint: None,
                name: p.name.clone(),
                monospace_name: true,
                active: false,
                active_label: None,
                trailing: vec![],
                enabled: true,
                on_click: None,
            })
            .collect();

        let input_count: UserText = VerbatimText::new(input_items.len().to_string()).into();
        let output_count: UserText = VerbatimText::new(output_items.len().to_string()).into();

        vec![
            DetailSection::TwoColumnLists {
                left: Box::new(ContentList {
                    title: labels::INPUT_PORTS.into(),
                    icon: SectionIcon::new("plug"),
                    inline_label: None,
                    count_label: Some(input_count),
                    visible_rows: None,
                    row_padding_y_px: 7,
                    refreshable: false,
                    items: input_items,
                    footer: None,
                }),
                right: Box::new(ContentList {
                    title: labels::OUTPUT_PORTS.into(),
                    icon: SectionIcon::new("plug"),
                    inline_label: None,
                    count_label: Some(output_count),
                    visible_rows: None,
                    row_padding_y_px: 7,
                    refreshable: false,
                    items: output_items,
                    footer: None,
                }),
            },
            DetailSection::KeyValueList {
                title: labels::STATUS.into(),
                icon: SectionIcon::new("info"),
                items: vec![
                    KeyValueRow {
                        icon: SectionIcon::new("tag"),
                        name: labels::CLIENT_NAME.into(),
                        tag: Some(VerbatimText::new(client_name).into()),
                        action: None,
                    },
                    KeyValueRow {
                        icon: SectionIcon::new("clock"),
                        name: labels::POLL_INTERVAL.into(),
                        tag: Some(labels::POLL_INTERVAL_VALUE.into()),
                        action: None,
                    },
                    KeyValueRow {
                        icon: SectionIcon::new("bolt"),
                        name: labels::EVENTS_RECEIVED.into(),
                        tag: Some(VerbatimText::new(total.to_string()).into()),
                        action: None,
                    },
                ],
            },
        ]
    }
}

pub(crate) fn record_port_added(snap: &mut MidiContentSnapshot, port: MidiPortInfo) {
    match port.direction {
        PortDirection::Input => {
            if !snap.input_ports.iter().any(|p| p.name == port.name) {
                snap.input_ports.push(port);
            }
        }
        PortDirection::Output => {
            if !snap.output_ports.iter().any(|p| p.name == port.name) {
                snap.output_ports.push(port);
            }
        }
    }
}

pub(crate) fn record_port_removed(
    snap: &mut MidiContentSnapshot,
    name: &str,
    direction: PortDirection,
) {
    match direction {
        PortDirection::Input => snap.input_ports.retain(|p| p.name != name),
        PortDirection::Output => snap.output_ports.retain(|p| p.name != name),
    }
}

pub(crate) fn record_midi_event(snap: &mut MidiContentSnapshot, port: &str) {
    *snap.events_per_input.entry(port.to_owned()).or_insert(0) += 1;
    snap.total_events += 1;
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{BuiltinContent, DetailSection};

    use super::*;
    use crate::client::MidiClient;

    #[test]
    fn sections_returns_two_sections() {
        let c = MidiClient::new_for_test();
        let content: &dyn BuiltinContent = &*c;
        let sections = content.sections();
        assert_eq!(sections.len(), 2);
        assert!(matches!(sections[0], DetailSection::TwoColumnLists { .. }));
        assert!(matches!(sections[1], DetailSection::KeyValueList { .. }));
    }

    #[test]
    fn record_port_added_adds_input_port() {
        let snap_arc = make_content_state();
        let mut snap = snap_arc.lock().unwrap();
        record_port_added(
            &mut snap,
            MidiPortInfo {
                name: "Piano".to_owned(),
                direction: PortDirection::Input,
            },
        );
        assert_eq!(snap.input_ports.len(), 1);
        assert_eq!(snap.input_ports[0].name, "Piano");
    }

    #[test]
    fn record_port_removed_removes_input_port() {
        let snap_arc = make_content_state();
        let mut snap = snap_arc.lock().unwrap();
        record_port_added(
            &mut snap,
            MidiPortInfo {
                name: "Piano".to_owned(),
                direction: PortDirection::Input,
            },
        );
        record_port_removed(&mut snap, "Piano", PortDirection::Input);
        assert!(snap.input_ports.is_empty());
    }

    #[test]
    fn record_midi_event_increments_counters() {
        let snap_arc = make_content_state();
        let mut snap = snap_arc.lock().unwrap();
        record_midi_event(&mut snap, "Piano");
        record_midi_event(&mut snap, "Piano");
        assert_eq!(*snap.events_per_input.get("Piano").unwrap(), 2);
        assert_eq!(snap.total_events, 2);
    }
}
