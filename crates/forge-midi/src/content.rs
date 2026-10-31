use forge_types::{RecoverMutex, UserText, VerbatimText};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DEFAULT_ROW_PADDING_Y_PX, DetailSection,
    KeyValueRow, SectionIcon, TrailingToken,
};

use crate::client::MidiClient;
use crate::events::MidiPortInfo;
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
        let snap = self.content_state().lock_recover();
        let client_name = self.config().client_name.clone();
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
                    row_padding_y_px: DEFAULT_ROW_PADDING_Y_PX,
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
                    row_padding_y_px: DEFAULT_ROW_PADDING_Y_PX,
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

    fn content_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
        None
    }
}

impl MidiContentSnapshot {
    pub(crate) fn set_ports(
        &mut self,
        inputs: Vec<MidiPortInfo>,
        outputs: Vec<MidiPortInfo>,
        subscribed: HashSet<String>,
    ) {
        self.input_ports = inputs;
        self.output_ports = outputs;
        self.subscribed_inputs = subscribed;
    }
}

pub(crate) fn record_midi_event(snap: &mut MidiContentSnapshot, port: &str) {
    *snap.events_per_input.entry(port.to_owned()).or_insert(0) += 1;
    snap.total_events += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{input_port, output_port};

    #[test]
    fn set_ports_replaces_every_port_list_and_the_subscribed_set_together() {
        let snap_arc = make_content_state();
        let mut snap = snap_arc.lock().unwrap();
        snap.set_ports(
            vec![input_port("Piano")],
            vec![output_port("Synth")],
            HashSet::from(["Piano".to_owned()]),
        );
        snap.set_ports(vec![input_port("Pads")], Vec::new(), HashSet::new());

        assert_eq!(
            snap.input_ports
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["Pads"]
        );
        assert!(snap.output_ports.is_empty());
        assert!(snap.subscribed_inputs.is_empty());
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
