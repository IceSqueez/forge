use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::error::MidiError;
use crate::events::MidiOutMessage;
use crate::sink::MidiSink;

pub struct MidiSendRunner {
    sink: Arc<dyn MidiSink>,
}

impl MidiSendRunner {
    pub fn new(sink: Arc<dyn MidiSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for MidiSendRunner {
    fn id(&self) -> &str {
        "midi.send"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Midi
    }

    fn label(&self) -> &str {
        "Send MIDI Message"
    }

    fn summary(&self) -> &str {
        "Sends a MIDI message (Note On, Note Off, CC, or raw) to an output port."
    }

    fn search_text(&self) -> &str {
        "midi send note on off cc raw output port"
    }

    fn icon_name(&self) -> &str {
        "music"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("port".to_owned(), Variant::String(String::new())),
            (
                "message_kind".to_owned(),
                Variant::String("note_on".to_owned()),
            ),
            ("note".to_owned(), Variant::Int(60)),
            ("velocity".to_owned(), Variant::Int(127)),
            ("channel".to_owned(), Variant::Int(0)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "port",
                label: "Output Port",
                placeholder: "Synth",
            },
            FormField::Select {
                key: "message_kind",
                label: "Message Kind",
                options: &["note_on", "note_off", "cc", "raw"],
            },
            FormField::Integer {
                key: "note",
                label: "Note (NoteOn/NoteOff)",
                min: 0,
                max: 127,
            },
            FormField::Integer {
                key: "velocity",
                label: "Velocity (NoteOn/NoteOff)",
                min: 0,
                max: 127,
            },
            FormField::Integer {
                key: "controller",
                label: "Controller (CC)",
                min: 0,
                max: 127,
            },
            FormField::Integer {
                key: "value",
                label: "Value (CC)",
                min: 0,
                max: 127,
            },
            FormField::Integer {
                key: "channel",
                label: "Channel (0-15)",
                min: 0,
                max: 15,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("port") {
            Some(Variant::String(_)) => {}
            _ => {
                return Err(RegistryError::UnknownKindId(
                    "midi.send: 'port' must be a string".to_owned(),
                ));
            }
        }
        match config.get("message_kind") {
            Some(Variant::String(k))
                if matches!(k.as_str(), "note_on" | "note_off" | "cc" | "raw") => {}
            _ => {
                return Err(RegistryError::UnknownKindId(
                    "midi.send: 'message_kind' must be note_on|note_off|cc|raw".to_owned(),
                ));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let port = ctx.arg_stack.interpolate(
            config
                .get("port")
                .and_then(|v| {
                    if let Variant::String(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        );

        let message_kind = config
            .get("message_kind")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let channel = extract_u8_clamped(config, "channel", 0, 15).unwrap_or(0);

        let message = match message_kind.as_str() {
            "note_on" => {
                let note = match extract_u8_clamped(config, "note", 0, 127) {
                    Some(n) => n,
                    None => {
                        return failed(
                            ctx.index,
                            started_at,
                            start,
                            "midi.send: 'note' must be 0-127",
                        );
                    }
                };
                let velocity = match extract_u8_clamped(config, "velocity", 0, 127) {
                    Some(v) => v,
                    None => {
                        return failed(
                            ctx.index,
                            started_at,
                            start,
                            "midi.send: 'velocity' must be 0-127",
                        );
                    }
                };
                MidiOutMessage::NoteOn {
                    note,
                    velocity,
                    channel,
                }
            }
            "note_off" => {
                let note = match extract_u8_clamped(config, "note", 0, 127) {
                    Some(n) => n,
                    None => {
                        return failed(
                            ctx.index,
                            started_at,
                            start,
                            "midi.send: 'note' must be 0-127",
                        );
                    }
                };
                let velocity = extract_u8_clamped(config, "velocity", 0, 127).unwrap_or(0);
                MidiOutMessage::NoteOff {
                    note,
                    velocity,
                    channel,
                }
            }
            "cc" => {
                let controller = match extract_u8_clamped(config, "controller", 0, 127) {
                    Some(c) => c,
                    None => {
                        return failed(
                            ctx.index,
                            started_at,
                            start,
                            "midi.send: 'controller' must be 0-127",
                        );
                    }
                };
                let value = match extract_u8_clamped(config, "value", 0, 127) {
                    Some(v) => v,
                    None => {
                        return failed(
                            ctx.index,
                            started_at,
                            start,
                            "midi.send: 'value' must be 0-127",
                        );
                    }
                };
                MidiOutMessage::ControlChange {
                    controller,
                    value,
                    channel,
                }
            }
            "raw" => {
                let bytes = match extract_raw_bytes(config) {
                    Ok(b) => b,
                    Err(e) => return failed(ctx.index, started_at, start, &e),
                };
                MidiOutMessage::Raw(bytes)
            }
            _ => {
                return failed(
                    ctx.index,
                    started_at,
                    start,
                    "midi.send: unknown message_kind",
                );
            }
        };

        let outcome = match self.sink.send_output(&port, &message).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => match e {
                MidiError::InvalidStatusByte(_) => {
                    SubActionOutcome::Failed("raw bytes have invalid status byte".to_owned())
                }
                other => SubActionOutcome::Failed(other.to_string()),
            },
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "midi.send".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

fn failed(
    index: usize,
    started_at: OffsetDateTime,
    start: Instant,
    reason: &str,
) -> (SubActionTelemetry, Option<ArgStack>) {
    (
        SubActionTelemetry {
            args_in: ::std::collections::BTreeMap::new(),
            produced: ::std::collections::BTreeMap::new(),
            kind: "midi.send".to_owned(),
            started_at,
            duration_ms: start.elapsed().as_millis() as u64,
            outcome: SubActionOutcome::Failed(reason.to_owned()),
            index,
        },
        None,
    )
}

fn extract_u8_clamped(config: &SubActionConfig, key: &str, min: i64, max: i64) -> Option<u8> {
    match config.get(key) {
        Some(Variant::Int(v)) if *v >= min && *v <= max => Some(*v as u8),
        _ => None,
    }
}

fn extract_raw_bytes(config: &SubActionConfig) -> Result<Vec<u8>, String> {
    match config.get("raw_bytes") {
        Some(Variant::Array(arr)) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for v in arr {
                match v {
                    Variant::Int(n) if *n >= 0 && *n <= 255 => bytes.push(*n as u8),
                    _ => return Err("midi.send: raw_bytes must be Array of Int 0-255".to_owned()),
                }
            }
            Ok(bytes)
        }
        _ => Err("midi.send: 'raw_bytes' must be an Array".to_owned()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_registry::{RunContext, SubActionRunner};
    use forge_types::{ArgStack, EventId, SubActionOutcome, Variant};

    use super::*;
    use crate::backend::tests::MockMidiBackend;
    use crate::client::MidiClient;
    use crate::config::MidiConfig;
    use crate::events::{MidiPortInfo, PortDirection};
    use crate::sink::MidiSink;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_sink_with_output_port(port: &str) -> (Arc<MidiClient>, Arc<MockMidiBackend>) {
        let backend = Arc::new(MockMidiBackend::new(
            vec![],
            vec![MidiPortInfo {
                name: port.to_owned(),
                direction: PortDirection::Output,
            }],
        ));
        let client = MidiClient::start(
            MidiConfig::default(),
            Arc::new(NoopPublisher),
            Arc::clone(&backend) as Arc<dyn crate::backend::MidiBackend>,
        );
        (client, backend)
    }

    fn make_run_context<'a>(stack: &'a ArgStack) -> RunContext<'a> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    #[tokio::test]
    async fn note_on_runner_dispatches_correct_bytes() {
        let (client, backend) = make_sink_with_output_port("Out");
        let sink: Arc<dyn MidiSink> = Arc::clone(&client) as Arc<dyn MidiSink>;
        let runner = MidiSendRunner::new(sink);

        let config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            (
                "message_kind".to_owned(),
                Variant::String("note_on".to_owned()),
            ),
            ("note".to_owned(), Variant::Int(72)),
            ("velocity".to_owned(), Variant::Int(64)),
            ("channel".to_owned(), Variant::Int(0)),
        ]);

        let stack = ArgStack::new();
        let ctx = make_run_context(&stack);
        let (telem, _) = runner.execute(&config, &ctx).await;

        assert_eq!(telem.outcome, SubActionOutcome::Success);
        let sent = backend.sent_outputs();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, vec![0x90u8, 72, 64]);
    }

    #[tokio::test]
    async fn raw_invalid_status_returns_failed() {
        let (client, _backend) = make_sink_with_output_port("Out");
        let sink: Arc<dyn MidiSink> = Arc::clone(&client) as Arc<dyn MidiSink>;
        let runner = MidiSendRunner::new(sink);

        let config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            ("message_kind".to_owned(), Variant::String("raw".to_owned())),
            (
                "raw_bytes".to_owned(),
                Variant::Array(vec![Variant::Int(0x00)]),
            ),
        ]);

        let stack = ArgStack::new();
        let ctx = make_run_context(&stack);
        let (telem, _) = runner.execute(&config, &ctx).await;

        assert!(matches!(telem.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn note_out_of_range_returns_failed() {
        let (client, _backend) = make_sink_with_output_port("Out");
        let sink: Arc<dyn MidiSink> = Arc::clone(&client) as Arc<dyn MidiSink>;
        let runner = MidiSendRunner::new(sink);

        let config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            (
                "message_kind".to_owned(),
                Variant::String("note_on".to_owned()),
            ),
            ("note".to_owned(), Variant::Int(200)),
            ("velocity".to_owned(), Variant::Int(64)),
            ("channel".to_owned(), Variant::Int(0)),
        ]);

        let stack = ArgStack::new();
        let ctx = make_run_context(&stack);
        let (telem, _) = runner.execute(&config, &ctx).await;

        assert!(matches!(telem.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn cc_runner_dispatches_correct_bytes() {
        let (client, backend) = make_sink_with_output_port("Out");
        let sink: Arc<dyn MidiSink> = Arc::clone(&client) as Arc<dyn MidiSink>;
        let runner = MidiSendRunner::new(sink);

        let config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            ("message_kind".to_owned(), Variant::String("cc".to_owned())),
            ("controller".to_owned(), Variant::Int(7)),
            ("value".to_owned(), Variant::Int(100)),
            ("channel".to_owned(), Variant::Int(1)),
        ]);

        let stack = ArgStack::new();
        let ctx = make_run_context(&stack);
        let (telem, _) = runner.execute(&config, &ctx).await;

        assert_eq!(telem.outcome, SubActionOutcome::Success);
        let sent = backend.sent_outputs();
        assert_eq!(sent[0].1, vec![0xB1u8, 7, 100]);
    }

    #[test]
    fn validate_config_accepts_valid_note_on() {
        let runner = MidiSendRunner::new(Arc::new(crate::sink::tests::NoopSink));
        let config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            (
                "message_kind".to_owned(),
                Variant::String("note_on".to_owned()),
            ),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_unknown_kind() {
        let runner = MidiSendRunner::new(Arc::new(crate::sink::tests::NoopSink));
        let config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            (
                "message_kind".to_owned(),
                Variant::String("unknown".to_owned()),
            ),
        ]);
        assert!(runner.validate_config(&config).is_err());
    }
}
