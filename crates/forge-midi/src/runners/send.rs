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
