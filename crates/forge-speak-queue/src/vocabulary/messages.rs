use crate::SpeakError;
use crate::vocabulary::{RejectReason, SkipReason};
use forge_audio::AudioError;
use forge_speech_types::{EngineId, VoiceId};
use forge_tts_core::TtsError;
use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

const NAMESPACE: &str = "speak";

const ENGINE_ARG: &str = "engine";
const VOICE_ARG: &str = "voice";
const SECONDS_ARG: &str = "seconds";
const ROUTE_ARG: &str = "route";
const DESTINATION_ARG: &str = "destination";

const LIMIT_ARG: &str = "limit";
const RULE_ARG: &str = "rule";
const UNKNOWN_RULE: &str = "other";

const MILLIS_PER_SECOND: u64 = 1000;

const ENGINE_NOT_REGISTERED: MessageCode =
    MessageCode::from_static(NAMESPACE, "engine_not_registered");
const SYNTHESIS_TIMED_OUT: MessageCode = MessageCode::from_static(NAMESPACE, "synthesis_timed_out");
const ENGINE_AUTH_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "engine_auth_failed");
const ENGINE_UNAVAILABLE: MessageCode = MessageCode::from_static(NAMESPACE, "engine_unavailable");
const ENGINE_RATE_LIMITED: MessageCode = MessageCode::from_static(NAMESPACE, "engine_rate_limited");
const VOICE_NOT_RECOGNIZED: MessageCode =
    MessageCode::from_static(NAMESPACE, "voice_not_recognized");
const NETWORK_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "network_failed");
const SSML_UNSUPPORTED: MessageCode = MessageCode::from_static(NAMESPACE, "ssml_unsupported");
const ENGINE_IO_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "engine_io_failed");
const QUOTA_EXHAUSTED: MessageCode = MessageCode::from_static(NAMESPACE, "quota_exhausted");
const SYNTHESIS_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "synthesis_failed");
const NO_OUTPUT_DEVICE: MessageCode = MessageCode::from_static(NAMESPACE, "no_output_device");
const AUDIO_HOST_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_host_failed");
const AUDIO_DEVICE_LOST: MessageCode = MessageCode::from_static(NAMESPACE, "audio_device_lost");
const AUDIO_CONVERSION_FAILED: MessageCode =
    MessageCode::from_static(NAMESPACE, "audio_conversion_failed");
const AUDIO_DECODE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_decode_failed");
const AUDIO_IO_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_io_failed");
const PLAYBACK_TASK_FAILED: MessageCode =
    MessageCode::from_static(NAMESPACE, "playback_task_failed");
const NO_AUDIO_ROUTE: MessageCode = MessageCode::from_static(NAMESPACE, "no_audio_route");
const AUDIO_ROUTE_UNKNOWN: MessageCode = MessageCode::from_static(NAMESPACE, "audio_route_unknown");
const AUDIO_ROUTES_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_routes_failed");
const AUDIO_REMOTE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_remote_failed");
const AUDIO_START_TIMED_OUT: MessageCode =
    MessageCode::from_static(NAMESPACE, "audio_start_timed_out");
const NO_OVERLAY_LISTENER: MessageCode = MessageCode::from_static(NAMESPACE, "no_overlay_listener");

const SKIPPED_RULE_MATCHED: MessageCode =
    MessageCode::from_static(NAMESPACE, "skipped_rule_matched");
const SKIPPED_WORD_FILTER: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_word_filter");
const SKIPPED_EMPTY: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_empty");
const SKIPPED_ALIAS: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_alias");
const SKIPPED_NO_VOICES: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_no_voices");
const SKIPPED_VOICE_OVERRIDE_MISSING: MessageCode =
    MessageCode::from_static(NAMESPACE, "skipped_voice_override_missing");
const SKIPPED_VOICE_UNRESOLVED: MessageCode =
    MessageCode::from_static(NAMESPACE, "skipped_voice_unresolved");
const SKIPPED_BY_USER: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_by_user");
const SKIPPED_PROMOTED: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_promoted");
const SKIPPED_CANCELLED: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_cancelled");
const SKIPPED_CLEARED: MessageCode = MessageCode::from_static(NAMESPACE, "skipped_cleared");
const REJECTED_QUEUE_FULL: MessageCode = MessageCode::from_static(NAMESPACE, "rejected_queue_full");
const REJECTED_PER_USER_LIMIT: MessageCode =
    MessageCode::from_static(NAMESPACE, "rejected_per_user_limit");
const QUEUE_STOPPED: MessageCode = MessageCode::from_static(NAMESPACE, "queue_stopped");
const QUEUE_LAGGING: MessageCode = MessageCode::from_static(NAMESPACE, "queue_lagging");
const WAIT_CANCELLED: MessageCode = MessageCode::from_static(NAMESPACE, "wait_cancelled");
const EVENT_STREAM_CLOSED: MessageCode = MessageCode::from_static(NAMESPACE, "event_stream_closed");
const OUTCOME_UNKNOWN: MessageCode = MessageCode::from_static(NAMESPACE, "outcome_unknown");
const WAIT_TIMED_OUT: MessageCode = MessageCode::from_static(NAMESPACE, "wait_timed_out");
const REMOVED_FROM_QUEUE: MessageCode = MessageCode::from_static(NAMESPACE, "removed_from_queue");
const SHOW_SPEECH_UNAVAILABLE: MessageCode =
    MessageCode::from_static(NAMESPACE, "show_speech_unavailable");

pub const SPEAK_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        ENGINE_NOT_REGISTERED,
        SYNTHESIS_TIMED_OUT,
        ENGINE_AUTH_FAILED,
        ENGINE_UNAVAILABLE,
        ENGINE_RATE_LIMITED,
        VOICE_NOT_RECOGNIZED,
        NETWORK_FAILED,
        SSML_UNSUPPORTED,
        ENGINE_IO_FAILED,
        QUOTA_EXHAUSTED,
        SYNTHESIS_FAILED,
        NO_OUTPUT_DEVICE,
        AUDIO_HOST_FAILED,
        AUDIO_DEVICE_LOST,
        AUDIO_CONVERSION_FAILED,
        AUDIO_DECODE_FAILED,
        AUDIO_IO_FAILED,
        PLAYBACK_TASK_FAILED,
        NO_AUDIO_ROUTE,
        AUDIO_ROUTE_UNKNOWN,
        AUDIO_ROUTES_FAILED,
        AUDIO_REMOTE_FAILED,
        AUDIO_START_TIMED_OUT,
        NO_OVERLAY_LISTENER,
        SKIPPED_RULE_MATCHED,
        SKIPPED_WORD_FILTER,
        SKIPPED_EMPTY,
        SKIPPED_ALIAS,
        SKIPPED_NO_VOICES,
        SKIPPED_VOICE_OVERRIDE_MISSING,
        SKIPPED_VOICE_UNRESOLVED,
        SKIPPED_BY_USER,
        SKIPPED_PROMOTED,
        SKIPPED_CANCELLED,
        SKIPPED_CLEARED,
        REJECTED_QUEUE_FULL,
        REJECTED_PER_USER_LIMIT,
        QUEUE_STOPPED,
        QUEUE_LAGGING,
        WAIT_CANCELLED,
        EVENT_STREAM_CLOSED,
        OUTCOME_UNKNOWN,
        WAIT_TIMED_OUT,
        REMOVED_FROM_QUEUE,
        SHOW_SPEECH_UNAVAILABLE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

fn engine_arg(message: MessageRef, engine: &EngineId) -> MessageRef {
    message.with_arg(ENGINE_ARG, VerbatimText::new(engine.0.as_str()))
}

fn with_detail(message: MessageRef, detail: &str) -> MessageRef {
    message.with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn engine_not_registered(engine: &EngineId) -> MessageRef {
    engine_arg(MessageRef::new(ENGINE_NOT_REGISTERED), engine)
}

pub(crate) fn synthesis_timed_out(millis: u64) -> MessageRef {
    MessageRef::new(SYNTHESIS_TIMED_OUT).with_arg(SECONDS_ARG, millis / MILLIS_PER_SECOND)
}

fn voice_not_recognized(voice: &VoiceId) -> MessageRef {
    MessageRef::new(VOICE_NOT_RECOGNIZED).with_arg(VOICE_ARG, VerbatimText::new(voice.0.as_str()))
}

pub fn tts_failure(error: &TtsError) -> MessageRef {
    match error {
        TtsError::Timeout { ms } => synthesis_timed_out(*ms),
        TtsError::AuthFailed { reason } => with_detail(MessageRef::new(ENGINE_AUTH_FAILED), reason),
        TtsError::EngineUnavailable { id, detail } => {
            with_detail(engine_arg(MessageRef::new(ENGINE_UNAVAILABLE), id), detail)
        }
        TtsError::RateLimited { retry_after_secs } => {
            MessageRef::new(ENGINE_RATE_LIMITED).with_arg(SECONDS_ARG, *retry_after_secs)
        }
        TtsError::InvalidVoice { id } => voice_not_recognized(id),
        TtsError::NetworkFailed(detail) => with_detail(MessageRef::new(NETWORK_FAILED), detail),
        TtsError::SsmlUnsupported { id } => engine_arg(MessageRef::new(SSML_UNSUPPORTED), id),
        TtsError::Io(source) => with_detail(MessageRef::new(ENGINE_IO_FAILED), &source.to_string()),
        TtsError::QuotaExceeded { id, detail } => {
            with_detail(engine_arg(MessageRef::new(QUOTA_EXHAUSTED), id), detail)
        }
        TtsError::SynthesisFailed { id, detail } => {
            with_detail(engine_arg(MessageRef::new(SYNTHESIS_FAILED), id), detail)
        }
    }
}

pub(crate) fn audio_start_timed_out(seconds: u64) -> MessageRef {
    MessageRef::new(AUDIO_START_TIMED_OUT).with_arg(SECONDS_ARG, seconds)
}

pub fn audio_failure(error: &AudioError) -> MessageRef {
    match error {
        AudioError::NoDefaultDevice => MessageRef::new(NO_OUTPUT_DEVICE),
        AudioError::Host(detail) => with_detail(MessageRef::new(AUDIO_HOST_FAILED), detail),
        AudioError::DeviceLost(detail) => with_detail(MessageRef::new(AUDIO_DEVICE_LOST), detail),
        AudioError::Resample(detail) => {
            with_detail(MessageRef::new(AUDIO_CONVERSION_FAILED), detail)
        }
        AudioError::Decode(detail) => with_detail(MessageRef::new(AUDIO_DECODE_FAILED), detail),
        AudioError::Io(source) => {
            with_detail(MessageRef::new(AUDIO_IO_FAILED), &source.to_string())
        }
        AudioError::JoinFailed(detail) => {
            with_detail(MessageRef::new(PLAYBACK_TASK_FAILED), detail)
        }
        AudioError::NoRoute => MessageRef::new(NO_AUDIO_ROUTE),
        AudioError::UnknownRoute(route) => {
            MessageRef::new(AUDIO_ROUTE_UNKNOWN).with_arg(ROUTE_ARG, VerbatimText::new(route))
        }
        AudioError::AllRoutesFailed(detail) => {
            with_detail(MessageRef::new(AUDIO_ROUTES_FAILED), detail)
        }
        AudioError::RemoteDestination(detail) => {
            with_detail(MessageRef::new(AUDIO_REMOTE_FAILED), detail)
        }
        AudioError::NoRemoteListener { destination, .. } => MessageRef::new(NO_OVERLAY_LISTENER)
            .with_arg(DESTINATION_ARG, VerbatimText::new(destination)),
    }
}

pub fn skip_message(reason: SkipReason, detail: Option<&str>) -> MessageRef {
    let code = match reason {
        SkipReason::SkipRuleMatched => SKIPPED_RULE_MATCHED,
        SkipReason::BlockedByWordFilter => SKIPPED_WORD_FILTER,
        SkipReason::EmptyAfterProcessing => SKIPPED_EMPTY,
        SkipReason::BlockedByAlias => SKIPPED_ALIAS,
        SkipReason::NoVoicesAvailable => SKIPPED_NO_VOICES,
        SkipReason::VoiceOverrideNotFound => SKIPPED_VOICE_OVERRIDE_MISSING,
        SkipReason::VoiceResolutionFailed => SKIPPED_VOICE_UNRESOLVED,
        SkipReason::UserSkip => SKIPPED_BY_USER,
        SkipReason::Promoted => SKIPPED_PROMOTED,
        SkipReason::Cancelled => SKIPPED_CANCELLED,
        SkipReason::Cleared => SKIPPED_CLEARED,
    };
    let message = MessageRef::new(code);
    match (reason, detail) {
        (SkipReason::SkipRuleMatched, rule) => {
            message.with_arg(RULE_ARG, VerbatimText::new(rule.unwrap_or(UNKNOWN_RULE)))
        }
        (_, Some(detail)) => with_detail(message, detail),
        (_, None) => message,
    }
}

pub fn reject_message(reason: RejectReason, limit: usize) -> MessageRef {
    let code = match reason {
        RejectReason::QueueFull => REJECTED_QUEUE_FULL,
        RejectReason::PerUserLimit => REJECTED_PER_USER_LIMIT,
    };
    MessageRef::new(code).with_arg(LIMIT_ARG, limit)
}

pub fn queue_failure(error: &SpeakError) -> MessageRef {
    match error {
        SpeakError::ActorGone => MessageRef::new(QUEUE_STOPPED),
        SpeakError::LaggingReceiver => MessageRef::new(QUEUE_LAGGING),
    }
}

pub fn wait_cancelled() -> MessageRef {
    MessageRef::new(WAIT_CANCELLED)
}

pub fn event_stream_closed() -> MessageRef {
    MessageRef::new(EVENT_STREAM_CLOSED)
}

pub fn outcome_unknown() -> MessageRef {
    MessageRef::new(OUTCOME_UNKNOWN)
}

pub fn wait_timed_out() -> MessageRef {
    MessageRef::new(WAIT_TIMED_OUT)
}

pub fn removed_from_queue() -> MessageRef {
    MessageRef::new(REMOVED_FROM_QUEUE)
}
#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use forge_types::MessageArg;

    use super::*;

    const ALL_SKIP_REASONS: [SkipReason; 11] = [
        SkipReason::SkipRuleMatched,
        SkipReason::BlockedByWordFilter,
        SkipReason::EmptyAfterProcessing,
        SkipReason::BlockedByAlias,
        SkipReason::NoVoicesAvailable,
        SkipReason::VoiceOverrideNotFound,
        SkipReason::VoiceResolutionFailed,
        SkipReason::UserSkip,
        SkipReason::Promoted,
        SkipReason::Cancelled,
        SkipReason::Cleared,
    ];

    const ALL_SKIP_RULES: [forge_tts_pipeline::SkipRule; 8] = [
        forge_tts_pipeline::SkipRule::ContainsUrl,
        forge_tts_pipeline::SkipRule::StartsWithPrefix,
        forge_tts_pipeline::SkipRule::FromBotAccount,
        forge_tts_pipeline::SkipRule::ExceedsMaxLength,
        forge_tts_pipeline::SkipRule::RepeatsRecentMessage,
        forge_tts_pipeline::SkipRule::EmoteOnly,
        forge_tts_pipeline::SkipRule::MostlyNonLatin,
        forge_tts_pipeline::SkipRule::CustomRegex,
    ];

    type Shape = (String, Vec<(String, MessageArg)>, Option<String>);

    fn ftl_variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn engine() -> EngineId {
        EngineId("piper".into())
    }

    fn io_error() -> std::io::Error {
        std::io::Error::other("disk gone")
    }

    fn every_tts_error() -> Vec<TtsError> {
        vec![
            TtsError::Timeout { ms: 12_500 },
            TtsError::AuthFailed { reason: "r".into() },
            TtsError::EngineUnavailable {
                id: engine(),
                detail: "d".into(),
            },
            TtsError::RateLimited {
                retry_after_secs: 7,
            },
            TtsError::InvalidVoice {
                id: VoiceId("v".into()),
            },
            TtsError::NetworkFailed("n".into()),
            TtsError::SsmlUnsupported { id: engine() },
            TtsError::Io(io_error()),
            TtsError::QuotaExceeded {
                id: engine(),
                detail: "d".into(),
            },
            TtsError::SynthesisFailed {
                id: engine(),
                detail: "d".into(),
            },
        ]
    }

    fn every_audio_error() -> Vec<AudioError> {
        vec![
            AudioError::NoDefaultDevice,
            AudioError::Host("h".into()),
            AudioError::DeviceLost("l".into()),
            AudioError::Resample("r".into()),
            AudioError::Decode("d".into()),
            AudioError::Io(io_error()),
            AudioError::JoinFailed("j".into()),
            AudioError::NoRoute,
            AudioError::UnknownRoute("main".into()),
            AudioError::AllRoutesFailed("a".into()),
            AudioError::RemoteDestination("r".into()),
            AudioError::NoRemoteListener {
                destination: "overlay".into(),
                live_players: 0,
            },
        ]
    }

    fn every_constructor() -> Vec<MessageRef> {
        let mut all: Vec<MessageRef> = every_tts_error().iter().map(tts_failure).collect();
        all.extend(every_audio_error().iter().map(audio_failure));
        all.extend(
            ALL_SKIP_REASONS
                .into_iter()
                .map(|reason| skip_message(reason, None)),
        );
        all.extend([
            engine_not_registered(&engine()),
            audio_start_timed_out(5),
            reject_message(RejectReason::QueueFull, 5),
            reject_message(RejectReason::PerUserLimit, 5),
            queue_failure(&SpeakError::ActorGone),
            queue_failure(&SpeakError::LaggingReceiver),
            wait_cancelled(),
            event_stream_closed(),
            outcome_unknown(),
            wait_timed_out(),
            removed_from_queue(),
            MessageRef::new(SHOW_SPEECH_UNAVAILABLE),
        ]);
        all
    }

    fn shape(message: &MessageRef) -> Shape {
        (
            message.code().to_string(),
            message
                .args()
                .map(|(name, value)| (name.to_owned(), value.clone()))
                .collect(),
            message.detail().map(|d| d.as_str().to_owned()),
        )
    }

    fn verbatim(text: &str) -> MessageArg {
        MessageArg::Verbatim(VerbatimText::new(text))
    }

    fn expected(code: &str, args: &[(&str, MessageArg)], detail: Option<&str>) -> Shape {
        (
            code.to_owned(),
            args.iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
            detail.map(str::to_owned),
        )
    }

    #[test]
    fn catalog_is_well_formed() {
        assert_eq!(SPEAK_MESSAGES.check(), Ok(()));
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(SPEAK_MESSAGES.en_ftl);
        let uk = ftl_variables(SPEAK_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(SPEAK_MESSAGES.codes.contains(code), "{code} not catalogued");
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = SPEAK_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(SPEAK_MESSAGES.en_ftl),
            ftl_variables(SPEAK_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        let causes = [
            tts_failure(&TtsError::AuthFailed {
                reason: "token abc rejected".into(),
            }),
            tts_failure(&TtsError::NetworkFailed("token abc rejected".into())),
            audio_failure(&AudioError::Host("token abc rejected".into())),
            skip_message(SkipReason::BlockedByAlias, Some("token abc rejected")),
        ];
        for message in causes {
            assert_eq!(
                message.detail().map(|d| d.as_str()),
                Some("token abc rejected"),
                "{}",
                message.code()
            );
            assert!(
                message
                    .args()
                    .all(|(_, v)| !format!("{v:?}").contains("abc")),
                "{}",
                message.code()
            );
        }
    }

    #[test]
    fn tts_errors_map_to_their_codes_arguments_and_detail() {
        let rows = [
            (
                TtsError::Timeout { ms: 12_500 },
                expected("speak.synthesis_timed_out", &[("seconds", 12.into())], None),
            ),
            (
                TtsError::Timeout { ms: 1_000 },
                expected("speak.synthesis_timed_out", &[("seconds", 1.into())], None),
            ),
            (
                TtsError::AuthFailed { reason: "r".into() },
                expected("speak.engine_auth_failed", &[], Some("r")),
            ),
            (
                TtsError::EngineUnavailable {
                    id: engine(),
                    detail: "d".into(),
                },
                expected(
                    "speak.engine_unavailable",
                    &[("engine", verbatim("piper"))],
                    Some("d"),
                ),
            ),
            (
                TtsError::RateLimited {
                    retry_after_secs: 7,
                },
                expected("speak.engine_rate_limited", &[("seconds", 7.into())], None),
            ),
            (
                TtsError::InvalidVoice {
                    id: VoiceId("v".into()),
                },
                expected(
                    "speak.voice_not_recognized",
                    &[("voice", verbatim("v"))],
                    None,
                ),
            ),
            (
                TtsError::NetworkFailed("n".into()),
                expected("speak.network_failed", &[], Some("n")),
            ),
            (
                TtsError::SsmlUnsupported { id: engine() },
                expected(
                    "speak.ssml_unsupported",
                    &[("engine", verbatim("piper"))],
                    None,
                ),
            ),
            (
                TtsError::Io(io_error()),
                expected("speak.engine_io_failed", &[], Some("disk gone")),
            ),
            (
                TtsError::QuotaExceeded {
                    id: engine(),
                    detail: "d".into(),
                },
                expected(
                    "speak.quota_exhausted",
                    &[("engine", verbatim("piper"))],
                    Some("d"),
                ),
            ),
            (
                TtsError::SynthesisFailed {
                    id: engine(),
                    detail: "d".into(),
                },
                expected(
                    "speak.synthesis_failed",
                    &[("engine", verbatim("piper"))],
                    Some("d"),
                ),
            ),
        ];
        for (error, want) in rows {
            assert_eq!(shape(&tts_failure(&error)), want, "{error:?}");
        }
    }

    #[test]
    fn audio_errors_map_to_their_codes_arguments_and_detail() {
        let rows = [
            (
                AudioError::NoDefaultDevice,
                expected("speak.no_output_device", &[], None),
            ),
            (
                AudioError::Host("h".into()),
                expected("speak.audio_host_failed", &[], Some("h")),
            ),
            (
                AudioError::DeviceLost("l".into()),
                expected("speak.audio_device_lost", &[], Some("l")),
            ),
            (
                AudioError::Resample("r".into()),
                expected("speak.audio_conversion_failed", &[], Some("r")),
            ),
            (
                AudioError::Decode("d".into()),
                expected("speak.audio_decode_failed", &[], Some("d")),
            ),
            (
                AudioError::Io(io_error()),
                expected("speak.audio_io_failed", &[], Some("disk gone")),
            ),
            (
                AudioError::JoinFailed("j".into()),
                expected("speak.playback_task_failed", &[], Some("j")),
            ),
            (
                AudioError::NoRoute,
                expected("speak.no_audio_route", &[], None),
            ),
            (
                AudioError::UnknownRoute("main".into()),
                expected(
                    "speak.audio_route_unknown",
                    &[("route", verbatim("main"))],
                    None,
                ),
            ),
            (
                AudioError::AllRoutesFailed("a".into()),
                expected("speak.audio_routes_failed", &[], Some("a")),
            ),
            (
                AudioError::RemoteDestination("r".into()),
                expected("speak.audio_remote_failed", &[], Some("r")),
            ),
            (
                AudioError::NoRemoteListener {
                    destination: "overlay".into(),
                    live_players: 0,
                },
                expected(
                    "speak.no_overlay_listener",
                    &[("destination", verbatim("overlay"))],
                    None,
                ),
            ),
        ];
        for (error, want) in rows {
            assert_eq!(shape(&audio_failure(&error)), want, "{error:?}");
        }
    }

    #[test]
    fn each_skip_reason_has_its_own_code_and_the_detail_rides_only_as_detail() {
        let codes: BTreeSet<String> = ALL_SKIP_REASONS
            .into_iter()
            .map(|reason| skip_message(reason, None).code().to_string())
            .collect();
        assert_eq!(codes.len(), ALL_SKIP_REASONS.len());
        for reason in ALL_SKIP_REASONS
            .into_iter()
            .filter(|reason| *reason != SkipReason::SkipRuleMatched)
        {
            let bare = skip_message(reason, None);
            let detailed = skip_message(reason, Some("no urls"));
            assert_eq!(
                (
                    shape(&bare),
                    detailed.code().to_string(),
                    detailed.args().count(),
                    detailed.detail().map(|d| d.as_str().to_owned()),
                ),
                (
                    expected(&bare.code().to_string(), &[], None),
                    bare.code().to_string(),
                    0,
                    Some("no urls".to_owned()),
                ),
                "{reason:?}"
            );
        }
    }

    #[test]
    fn a_skip_rule_message_carries_the_rule_token_as_argument_and_never_as_detail() {
        for (detail, rule) in [
            (Some("contains_url"), "contains_url"),
            (Some("custom_regex"), "custom_regex"),
            (None, UNKNOWN_RULE),
        ] {
            assert_eq!(
                shape(&skip_message(SkipReason::SkipRuleMatched, detail)),
                expected(
                    "speak.skipped_rule_matched",
                    &[("rule", verbatim(rule))],
                    None
                ),
                "{detail:?}"
            );
        }
    }

    #[test]
    fn every_skip_rule_token_has_its_own_branch_in_both_languages() {
        for (language, ftl) in [("en", SPEAK_MESSAGES.en_ftl), ("uk", SPEAK_MESSAGES.uk_ftl)] {
            let entry = ftl
                .split("msg-speak-skipped_rule_matched")
                .nth(1)
                .and_then(|rest| rest.split("\nmsg-").next())
                .unwrap_or_else(|| panic!("skipped_rule_matched missing in {language}"));
            for rule in ALL_SKIP_RULES {
                let key = format!("[{}]", rule.token());
                assert!(entry.contains(&key), "{language}: no branch for {key}");
            }
        }
    }

    #[test]
    fn rejections_carry_the_limit_and_queue_failures_have_distinct_codes() {
        assert_eq!(
            [
                shape(&reject_message(RejectReason::QueueFull, 5)),
                shape(&reject_message(RejectReason::PerUserLimit, 0)),
                shape(&queue_failure(&SpeakError::ActorGone)),
                shape(&queue_failure(&SpeakError::LaggingReceiver)),
            ],
            [
                expected("speak.rejected_queue_full", &[("limit", 5.into())], None),
                expected(
                    "speak.rejected_per_user_limit",
                    &[("limit", 0.into())],
                    None
                ),
                expected("speak.queue_stopped", &[], None),
                expected("speak.queue_lagging", &[], None),
            ]
        );
    }
}
