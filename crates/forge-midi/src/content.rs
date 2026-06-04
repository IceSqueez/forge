use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DetailSection, KeyValueRow, SectionIcon,
    TrailingToken,
};

use crate::client::MidiClient;
use crate::events::{MidiPortInfo, PortDirection};

#[derive(Default)]
pub(crate) struct MidiContentSnapshot {
    pub input_ports: Vec<MidiPortInfo>,
    pub output_ports: Vec<MidiPortInfo>,
    pub events_per_input: HashMap<String, u64>,
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

        let input_items: Vec<ContentListItem> = snap
            .input_ports
            .iter()
            .map(|p| {
                let count = snap.events_per_input.get(&p.name).copied().unwrap_or(0);
                ContentListItem {
                    icon: SectionIcon::new("plug"),
                    name: p.name.clone(),
                    monospace_name: true,
                    active: true,
                    active_label: Some("SUBSCRIBED".to_owned()),
                    trailing: vec![TrailingToken::Label(count.to_string())],
                    enabled: true,
                }
            })
            .collect();

        let output_items: Vec<ContentListItem> = snap
            .output_ports
            .iter()
            .map(|p| ContentListItem {
                icon: SectionIcon::new("plug"),
                name: p.name.clone(),
                monospace_name: true,
                active: false,
                active_label: None,
                trailing: vec![],
                enabled: true,
            })
            .collect();

        let input_count = input_items.len().to_string();
        let output_count = output_items.len().to_string();

        vec![
            DetailSection::TwoColumnLists {
                left: ContentList {
                    title: "Input Ports".to_owned(),
                    icon: SectionIcon::new("plug"),
                    count_label: Some(input_count),
                    items: input_items,
                    footer: None,
                },
                right: ContentList {
                    title: "Output Ports".to_owned(),
                    icon: SectionIcon::new("plug"),
                    count_label: Some(output_count),
                    items: output_items,
                    footer: None,
                },
            },
            DetailSection::KeyValueList {
                title: "Status".to_owned(),
                icon: SectionIcon::new("info"),
                items: vec![
                    KeyValueRow {
                        icon: SectionIcon::new("tag"),
                        name: "Client name".to_owned(),
                        tag: Some(client_name),
                        action: None,
                    },
                    KeyValueRow {
                        icon: SectionIcon::new("clock"),
                        name: "Poll interval".to_owned(),
                        tag: Some("2 s".to_owned()),
                        action: None,
                    },
                    KeyValueRow {
                        icon: SectionIcon::new("bolt"),
                        name: "Events received".to_owned(),
                        tag: Some(total.to_string()),
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
#[allow(clippy::unwrap_used)]
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
