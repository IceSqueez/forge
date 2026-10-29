use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigProblem, ConfigReason, ConfigSpec, Interpolation, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};

use super::byte_field::ByteField;
use crate::events::MidiOutMessage;
use crate::sink::MidiSink;
use crate::vocabulary::OutMessageKind;
use crate::vocabulary::config_keys::send;
use crate::vocabulary::limits::{CHANNEL_MAX, DATA_BYTE_MAX};
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::SEND;
const MIDDLE_C: i64 = 60;
const LOWEST: i64 = 0;
const RAW_BYTES_SHAPE: &str = "expected a list of byte values";

const PORT: TextField = TextField {
    key: send::PORT,
    label: "Output Port",
    editor: TextEditor::Line {
        placeholder: "Synth",
    },
    default: "",
    required: true,
};

const MESSAGE_KIND: ChoiceField<OutMessageKind> = ChoiceField {
    key: send::MESSAGE_KIND,
    label: "Message Kind",
    default: OutMessageKind::NoteOn,
};

pub(crate) const NOTE: ByteField = ByteField {
    key: send::NOTE,
    label: "Note (NoteOn/NoteOff)",
    default: MIDDLE_C,
    max: DATA_BYTE_MAX,
    seeds_default: true,
};

pub(crate) const VELOCITY: ByteField = ByteField {
    key: send::VELOCITY,
    label: "Velocity (NoteOn/NoteOff)",
    default: DATA_BYTE_MAX,
    max: DATA_BYTE_MAX,
    seeds_default: true,
};

const CONTROLLER: ByteField = ByteField {
    key: send::CONTROLLER,
    label: "Controller (CC)",
    default: LOWEST,
    max: DATA_BYTE_MAX,
    seeds_default: false,
};

const VALUE: ByteField = ByteField {
    key: send::VALUE,
    label: "Value (CC)",
    default: LOWEST,
    max: DATA_BYTE_MAX,
    seeds_default: false,
};

const CHANNEL: ByteField = ByteField {
    key: send::CHANNEL,
    label: "Channel (0-15)",
    default: LOWEST,
    max: CHANNEL_MAX,
    seeds_default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &PORT,
    &MESSAGE_KIND,
    &NOTE,
    &VELOCITY,
    &CONTROLLER,
    &VALUE,
    &CHANNEL,
]);

fn read_message(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<MidiOutMessage, ConfigProblem> {
    Ok(match MESSAGE_KIND.read(config, args)? {
        OutMessageKind::NoteOn => MidiOutMessage::NoteOn {
            note: NOTE.read(config, args)?,
            velocity: VELOCITY.read(config, args)?,
            channel: CHANNEL.read(config, args)?,
        },
        OutMessageKind::NoteOff => MidiOutMessage::NoteOff {
            note: NOTE.read(config, args)?,
            velocity: VELOCITY.read(config, args)?,
            channel: CHANNEL.read(config, args)?,
        },
        OutMessageKind::ControlChange => MidiOutMessage::ControlChange {
            controller: CONTROLLER.read(config, args)?,
            value: VALUE.read(config, args)?,
            channel: CHANNEL.read(config, args)?,
        },
        OutMessageKind::Raw => MidiOutMessage::Raw(read_raw_bytes(config)?),
    })
}

fn read_raw_bytes(config: &SubActionConfig) -> Result<Vec<u8>, ConfigProblem> {
    let invalid = |reason| ConfigProblem::field(send::RAW_BYTES, reason);
    let items = match config.get(send::RAW_BYTES) {
        Some(Variant::Array(items)) if !items.is_empty() => items,
        Some(Variant::Array(_)) | None => return Err(invalid(ConfigReason::Required)),
        Some(_) => {
            return Err(invalid(ConfigReason::InvalidFormat(
                RAW_BYTES_SHAPE.to_owned(),
            )));
        }
    };
    items
        .iter()
        .map(|item| match item {
            Variant::Int(number) => u8::try_from(*number).map_err(|_| {
                invalid(ConfigReason::OutOfRange(ValueBounds::between(
                    u8::MIN,
                    u8::MAX,
                )))
            }),
            _ => Err(invalid(ConfigReason::NotANumber)),
        })
        .collect()
}

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
        KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match MESSAGE_KIND.peek(config, interpolation)? {
            Some(OutMessageKind::Raw) => read_raw_bytes(config).map(drop),
            _ => Ok(()),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let read = PORT
            .read(config, ctx.arg_stack)
            .and_then(|port| Ok((port, read_message(config, ctx.arg_stack)?)));
        let (port, message) = match read {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(&self.sink.send_output(&port, &message).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use forge_events::{Event, EventPublisher};
    use forge_registry::{ConfigProblem, ConfigReason, RunContext, SubActionRunner, ValueBounds};
    use forge_types::{ArgStack, EventId, SubActionOutcome, Variant};

    use super::*;
    use crate::backend::tests::MockMidiBackend;
    use crate::client::MidiClient;
    use crate::config::MidiConfig;
    use crate::error::MidiError;
    use crate::events::{MidiPortInfo, PortDirection};
    use crate::sink::MidiSink;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    #[derive(Default)]
    struct RecordingSink {
        sent: Mutex<Vec<(String, MidiOutMessage)>>,
    }

    impl RecordingSink {
        fn sent(&self) -> Vec<(String, MidiOutMessage)> {
            self.sent.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl MidiSink for RecordingSink {
        async fn send_output(
            &self,
            port_name: &str,
            message: &MidiOutMessage,
        ) -> Result<(), MidiError> {
            self.sent
                .lock()
                .unwrap()
                .push((port_name.to_owned(), message.clone()));
            Ok(())
        }
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

    fn step(kind: &str, entries: &[(&str, Variant)]) -> SubActionConfig {
        let mut config = BTreeMap::from([
            ("port".to_owned(), Variant::String("Out".to_owned())),
            ("message_kind".to_owned(), Variant::String(kind.to_owned())),
        ]);
        for (key, value) in entries {
            config.insert((*key).to_owned(), value.clone());
        }
        config
    }

    fn bytes(values: &[i64]) -> Variant {
        Variant::Array(values.iter().copied().map(Variant::Int).collect())
    }

    fn invalid(field: &str, reason: ConfigReason) -> SubActionOutcome {
        SubActionOutcome::Failed(format!(
            "invalid step config: {}",
            ConfigProblem::field(field, reason)
        ))
    }

    async fn run_recorded(
        config: &SubActionConfig,
        stack: &ArgStack,
    ) -> (SubActionOutcome, Vec<(String, MidiOutMessage)>) {
        let sink = Arc::new(RecordingSink::default());
        let runner = MidiSendRunner::new(Arc::clone(&sink) as Arc<dyn MidiSink>);
        let (telemetry, _) = runner.execute(config, &make_run_context(stack)).await;
        (telemetry.outcome, sink.sent())
    }

    #[tokio::test]
    async fn note_on_runner_dispatches_correct_bytes() {
        let (client, backend) = make_sink_with_output_port("Out");
        let runner = MidiSendRunner::new(client as Arc<dyn MidiSink>);
        let config = step(
            "note_on",
            &[
                ("note", Variant::Int(72)),
                ("velocity", Variant::Int(64)),
                ("channel", Variant::Int(0)),
            ],
        );

        let stack = ArgStack::new();
        let (telem, _) = runner.execute(&config, &make_run_context(&stack)).await;

        assert_eq!(telem.outcome, SubActionOutcome::Success);
        assert_eq!(
            backend.sent_outputs(),
            vec![("Out".to_owned(), vec![0x90u8, 72, 64])]
        );
    }

    #[tokio::test]
    async fn cc_runner_dispatches_correct_bytes() {
        let (client, backend) = make_sink_with_output_port("Out");
        let runner = MidiSendRunner::new(client as Arc<dyn MidiSink>);
        let config = step(
            "cc",
            &[
                ("controller", Variant::Int(7)),
                ("value", Variant::Int(100)),
                ("channel", Variant::Int(1)),
            ],
        );

        let stack = ArgStack::new();
        let (telem, _) = runner.execute(&config, &make_run_context(&stack)).await;

        assert_eq!(telem.outcome, SubActionOutcome::Success);
        assert_eq!(
            backend.sent_outputs(),
            vec![("Out".to_owned(), vec![0xB1u8, 7, 100])]
        );
    }

    #[tokio::test]
    async fn raw_invalid_status_returns_failed() {
        let (client, backend) = make_sink_with_output_port("Out");
        let runner = MidiSendRunner::new(client as Arc<dyn MidiSink>);
        let config = step("raw", &[("raw_bytes", bytes(&[0x00]))]);

        let stack = ArgStack::new();
        let (telem, _) = runner.execute(&config, &make_run_context(&stack)).await;

        assert!(matches!(telem.outcome, SubActionOutcome::Failed(_)));
        assert!(backend.sent_outputs().is_empty());
    }

    #[tokio::test]
    async fn sends_the_message_its_config_describes() {
        let note = |note, velocity, channel| MidiOutMessage::NoteOn {
            note,
            velocity,
            channel,
        };
        let stack = ArgStack::new()
            .set("pitch".to_owned(), Variant::Int(64))
            .set("ch".to_owned(), Variant::String("9".to_owned()));
        let cases = [
            (
                "upper bounds",
                step(
                    "note_on",
                    &[
                        ("note", Variant::Int(127)),
                        ("velocity", Variant::Int(127)),
                        ("channel", Variant::Int(15)),
                    ],
                ),
                note(127, 127, 15),
            ),
            (
                "lower bounds",
                step(
                    "note_on",
                    &[
                        ("note", Variant::Int(0)),
                        ("velocity", Variant::Int(0)),
                        ("channel", Variant::Int(0)),
                    ],
                ),
                note(0, 0, 0),
            ),
            (
                "templates and numeric text",
                step(
                    "note_on",
                    &[
                        ("note", Variant::String("%pitch%".to_owned())),
                        ("velocity", Variant::String("100".to_owned())),
                        ("channel", Variant::String("%ch%".to_owned())),
                    ],
                ),
                note(64, 100, 9),
            ),
            (
                "note off without a velocity releases at full velocity",
                step("note_off", &[("note", Variant::Int(61))]),
                MidiOutMessage::NoteOff {
                    note: 61,
                    velocity: 127,
                    channel: 0,
                },
            ),
            (
                "cc without controller or value",
                step("cc", &[]),
                MidiOutMessage::ControlChange {
                    controller: 0,
                    value: 0,
                    channel: 0,
                },
            ),
            (
                "raw bytes at both byte bounds",
                step("raw", &[("raw_bytes", bytes(&[0xF0, 0, 255, 0xF7]))]),
                MidiOutMessage::Raw(vec![0xF0, 0, 255, 0xF7]),
            ),
            (
                "raw kind ignores the note fields",
                step(
                    "raw",
                    &[
                        ("raw_bytes", bytes(&[0x90, 60, 1])),
                        ("note", Variant::Int(500)),
                    ],
                ),
                MidiOutMessage::Raw(vec![0x90, 60, 1]),
            ),
        ];

        for (case, config, expected) in cases {
            let (outcome, sent) = run_recorded(&config, &stack).await;

            assert_eq!(outcome, SubActionOutcome::Success, "{case}");
            assert_eq!(sent, vec![("Out".to_owned(), expected)], "{case}");
        }
    }

    #[tokio::test]
    async fn invalid_number_fields_fail_naming_the_field_without_sending() {
        let data = ConfigReason::OutOfRange(ValueBounds::between(0, 127));
        let channel = ConfigReason::OutOfRange(ValueBounds::between(0, 15));
        let stack = ArgStack::new().set("pitch".to_owned(), Variant::Int(128));
        let cases = [
            ("note_on", "note", Variant::Int(128), data.clone()),
            ("note_on", "note", Variant::Int(-1), data.clone()),
            ("note_on", "velocity", Variant::Int(128), data.clone()),
            ("note_off", "velocity", Variant::Int(128), data.clone()),
            ("note_off", "velocity", Variant::Int(-1), data.clone()),
            ("cc", "controller", Variant::Int(128), data.clone()),
            ("cc", "value", Variant::Int(128), data.clone()),
            ("note_on", "channel", Variant::Int(16), channel.clone()),
            ("cc", "channel", Variant::Int(-1), channel),
            (
                "note_on",
                "note",
                Variant::String("%pitch%".to_owned()),
                data,
            ),
            (
                "note_on",
                "note",
                Variant::String("middle".to_owned()),
                ConfigReason::NotANumber,
            ),
            (
                "note_off",
                "velocity",
                Variant::Float(64.0),
                ConfigReason::NotANumber,
            ),
        ];

        for (kind, key, value, reason) in cases {
            let config = step(kind, &[(key, value.clone())]);
            let (outcome, sent) = run_recorded(&config, &stack).await;

            assert_eq!(outcome, invalid(key, reason), "{kind} {key}={value:?}");
            assert!(sent.is_empty(), "{kind} {key}={value:?}");
        }
    }

    #[tokio::test]
    async fn blank_or_missing_port_fails_without_sending() {
        let mut missing = step("note_on", &[]);
        missing.remove("port");
        let blank = step("note_on", &[("port", Variant::String("  ".to_owned()))]);

        for config in [missing, blank] {
            let (outcome, sent) = run_recorded(&config, &ArgStack::new()).await;

            assert_eq!(outcome, invalid("port", ConfigReason::Required));
            assert!(sent.is_empty());
        }
    }

    #[test]
    fn validation_checks_raw_bytes_only_for_the_raw_kind() {
        let runner = MidiSendRunner::new(Arc::new(RecordingSink::default()));
        let shape = ConfigReason::InvalidFormat(RAW_BYTES_SHAPE.to_owned());
        let byte_range = ConfigReason::OutOfRange(ValueBounds::between(0, 255));
        let cases = [
            ("note_on", None, None),
            ("note_on", Some(Variant::Bool(true)), None),
            ("raw", None, Some(ConfigReason::Required)),
            ("raw", Some(bytes(&[])), Some(ConfigReason::Required)),
            (
                "raw",
                Some(Variant::String("90 3C 7F".to_owned())),
                Some(shape),
            ),
            ("raw", Some(bytes(&[0x90, 256])), Some(byte_range.clone())),
            ("raw", Some(bytes(&[-1])), Some(byte_range)),
            (
                "raw",
                Some(Variant::Array(vec![
                    Variant::Int(0x90),
                    Variant::String("60".to_owned()),
                ])),
                Some(ConfigReason::NotANumber),
            ),
            ("raw", Some(bytes(&[0x90, 60, 127])), None),
        ];

        for (kind, raw, reason) in cases {
            let config = step(kind, &[]);
            let config = match &raw {
                Some(value) => {
                    let mut config = config;
                    config.insert("raw_bytes".to_owned(), value.clone());
                    config
                }
                None => config,
            };
            let problem = runner
                .validate_config(&config)
                .err()
                .map(|error| error.into_config_problem());

            assert_eq!(
                problem,
                reason.map(|reason| ConfigProblem::field("raw_bytes", reason)),
                "{kind} {raw:?}"
            );
        }
    }

    #[tokio::test]
    async fn invalid_raw_bytes_fail_execution_without_sending() {
        let config = step("raw", &[("raw_bytes", bytes(&[0x90, 300]))]);

        let (outcome, sent) = run_recorded(&config, &ArgStack::new()).await;

        assert_eq!(
            outcome,
            invalid(
                "raw_bytes",
                ConfigReason::OutOfRange(ValueBounds::between(0, 255))
            )
        );
        assert!(sent.is_empty());
    }

    #[test]
    fn validation_rejects_an_unknown_message_kind() {
        let runner = MidiSendRunner::new(Arc::new(RecordingSink::default()));

        let problem = runner
            .validate_config(&step("unknown", &[]))
            .unwrap_err()
            .into_config_problem();

        assert_eq!(problem.field.as_deref(), Some("message_kind"));
        assert!(matches!(problem.reason, ConfigReason::InvalidChoice(_)));
    }

    #[test]
    fn validation_defers_number_templates_to_run_time() {
        let runner = MidiSendRunner::new(Arc::new(RecordingSink::default()));
        let config = step(
            "note_on",
            &[
                ("note", Variant::String("%pitch%".to_owned())),
                ("channel", Variant::String("%ch%".to_owned())),
            ],
        );

        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn default_config_matches_the_stored_shape_and_waits_only_for_a_port() {
        let runner = MidiSendRunner::new(Arc::new(RecordingSink::default()));
        let defaults = runner.default_config();
        let stored: serde_json::Value = serde_json::from_str(
            r#"{"channel":{"type":"int","value":0},"message_kind":{"type":"string","value":"note_on"},"note":{"type":"int","value":60},"port":{"type":"string","value":""},"velocity":{"type":"int","value":127}}"#,
        )
        .unwrap();

        assert_eq!(serde_json::to_value(&defaults).unwrap(), stored);
        assert_eq!(
            runner
                .validate_config(&defaults)
                .unwrap_err()
                .into_config_problem(),
            ConfigProblem::field("port", ConfigReason::Required)
        );
    }
}
