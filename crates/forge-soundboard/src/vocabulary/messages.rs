use forge_audio::AudioError;
use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::error::SoundboardError;

const NAMESPACE: &str = "soundboard";

const CLIP_ARG: &str = "clip";
const ROUTE_ARG: &str = "route";
const DESTINATION_ARG: &str = "destination";

const CLIP_NOT_FOUND: MessageCode = MessageCode::from_static(NAMESPACE, "clip_not_found");
const SOURCE_MISSING: MessageCode = MessageCode::from_static(NAMESPACE, "source_missing");
const IMPORT_REFUSED: MessageCode = MessageCode::from_static(NAMESPACE, "import_refused");
const MASTER_VOLUME_NOT_SAVED: MessageCode =
    MessageCode::from_static(NAMESPACE, "master_volume_not_saved");
const STORAGE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "storage_failed");
const PLAYBACK_TASK_FAILED: MessageCode =
    MessageCode::from_static(NAMESPACE, "playback_task_failed");
const NO_OUTPUT_DEVICE: MessageCode = MessageCode::from_static(NAMESPACE, "no_output_device");
const AUDIO_HOST_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_host_failed");
const AUDIO_DEVICE_LOST: MessageCode = MessageCode::from_static(NAMESPACE, "audio_device_lost");
const AUDIO_CONVERSION_FAILED: MessageCode =
    MessageCode::from_static(NAMESPACE, "audio_conversion_failed");
const AUDIO_DECODE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_decode_failed");
const AUDIO_IO_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_io_failed");
const NO_AUDIO_ROUTE: MessageCode = MessageCode::from_static(NAMESPACE, "no_audio_route");
const AUDIO_ROUTE_UNKNOWN: MessageCode = MessageCode::from_static(NAMESPACE, "audio_route_unknown");
const AUDIO_ROUTES_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_routes_failed");
const AUDIO_REMOTE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "audio_remote_failed");
const NO_OVERLAY_LISTENER: MessageCode = MessageCode::from_static(NAMESPACE, "no_overlay_listener");

pub const SOUNDBOARD_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        CLIP_NOT_FOUND,
        SOURCE_MISSING,
        IMPORT_REFUSED,
        MASTER_VOLUME_NOT_SAVED,
        STORAGE_FAILED,
        PLAYBACK_TASK_FAILED,
        NO_OUTPUT_DEVICE,
        AUDIO_HOST_FAILED,
        AUDIO_DEVICE_LOST,
        AUDIO_CONVERSION_FAILED,
        AUDIO_DECODE_FAILED,
        AUDIO_IO_FAILED,
        NO_AUDIO_ROUTE,
        AUDIO_ROUTE_UNKNOWN,
        AUDIO_ROUTES_FAILED,
        AUDIO_REMOTE_FAILED,
        NO_OVERLAY_LISTENER,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

fn with_detail(message: MessageRef, detail: &str) -> MessageRef {
    message.with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn audio_failure(error: &AudioError) -> MessageRef {
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

pub(crate) fn failure(error: &SoundboardError) -> MessageRef {
    match error {
        SoundboardError::ClipNotFound(clip) => {
            MessageRef::new(CLIP_NOT_FOUND).with_arg(CLIP_ARG, VerbatimText::new(clip))
        }
        SoundboardError::SourceMissing(clip) => {
            MessageRef::new(SOURCE_MISSING).with_arg(CLIP_ARG, VerbatimText::new(clip))
        }
        SoundboardError::ImportRefused(source) => {
            with_detail(MessageRef::new(IMPORT_REFUSED), &source.to_string())
        }
        SoundboardError::Audio(source) => audio_failure(source),
        SoundboardError::MasterVolumeNotSaved(source) => with_detail(
            MessageRef::new(MASTER_VOLUME_NOT_SAVED),
            &source.to_string(),
        ),
        SoundboardError::Storage(detail) => with_detail(MessageRef::new(STORAGE_FAILED), detail),
        SoundboardError::JoinError(detail) => {
            with_detail(MessageRef::new(PLAYBACK_TASK_FAILED), detail)
        }
    }
}
#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use forge_storage::StorageError;
    use forge_types::MessageArg;

    use super::*;

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

    fn storage_error() -> StorageError {
        StorageError::Connection {
            reason: "db".to_owned(),
        }
    }

    fn every_audio_error_with_its_code() -> Vec<(AudioError, &'static str)> {
        vec![
            (AudioError::NoDefaultDevice, "soundboard.no_output_device"),
            (AudioError::Host("h".into()), "soundboard.audio_host_failed"),
            (
                AudioError::DeviceLost("d".into()),
                "soundboard.audio_device_lost",
            ),
            (
                AudioError::Resample("r".into()),
                "soundboard.audio_conversion_failed",
            ),
            (
                AudioError::Decode("x".into()),
                "soundboard.audio_decode_failed",
            ),
            (
                AudioError::Io(std::io::Error::other("io")),
                "soundboard.audio_io_failed",
            ),
            (
                AudioError::JoinFailed("j".into()),
                "soundboard.playback_task_failed",
            ),
            (AudioError::NoRoute, "soundboard.no_audio_route"),
            (
                AudioError::UnknownRoute("r".into()),
                "soundboard.audio_route_unknown",
            ),
            (
                AudioError::AllRoutesFailed("a".into()),
                "soundboard.audio_routes_failed",
            ),
            (
                AudioError::RemoteDestination("r".into()),
                "soundboard.audio_remote_failed",
            ),
            (
                AudioError::NoRemoteListener {
                    destination: "ov".into(),
                    live_players: 0,
                },
                "soundboard.no_overlay_listener",
            ),
        ]
    }

    fn every_step_error_with_its_code() -> Vec<(SoundboardError, &'static str)> {
        vec![
            (
                SoundboardError::ClipNotFound("c".into()),
                "soundboard.clip_not_found",
            ),
            (
                SoundboardError::SourceMissing("c".into()),
                "soundboard.source_missing",
            ),
            (
                SoundboardError::ImportRefused(storage_error()),
                "soundboard.import_refused",
            ),
            (
                SoundboardError::MasterVolumeNotSaved(storage_error()),
                "soundboard.master_volume_not_saved",
            ),
            (
                SoundboardError::Storage("s".into()),
                "soundboard.storage_failed",
            ),
            (
                SoundboardError::JoinError("j".into()),
                "soundboard.playback_task_failed",
            ),
        ]
    }

    #[test]
    fn every_audio_error_maps_to_its_own_code() {
        for (error, expected) in every_audio_error_with_its_code() {
            assert_eq!(
                audio_failure(&error).code().to_string(),
                expected,
                "{error:?}"
            );
        }
    }

    #[test]
    fn every_soundboard_error_maps_to_its_own_code() {
        for (error, expected) in every_step_error_with_its_code() {
            assert_eq!(failure(&error).code().to_string(), expected, "{error:?}");
        }
    }

    #[test]
    fn an_audio_error_wrapped_in_a_soundboard_error_maps_exactly_as_the_bare_audio_error() {
        for (error, _) in every_audio_error_with_its_code() {
            let bare = audio_failure(&error);

            assert_eq!(failure(&SoundboardError::Audio(error)), bare);
        }
    }

    #[test]
    fn clip_and_route_and_destination_are_carried_as_verbatim_arguments() {
        let verbatim = |text: &str| MessageArg::Verbatim(VerbatimText::new(text));

        assert_eq!(
            failure(&SoundboardError::ClipNotFound("kick".into())).arg("clip"),
            Some(&verbatim("kick"))
        );
        assert_eq!(
            failure(&SoundboardError::SourceMissing("kick".into())).arg("clip"),
            Some(&verbatim("kick"))
        );
        assert_eq!(
            audio_failure(&AudioError::UnknownRoute("main".into())).arg("route"),
            Some(&verbatim("main"))
        );
        assert_eq!(
            audio_failure(&AudioError::NoRemoteListener {
                destination: "ov".into(),
                live_players: 3,
            })
            .arg("destination"),
            Some(&verbatim("ov"))
        );
    }

    #[test]
    fn upstream_text_is_carried_as_detail_for_the_errors_that_have_it() {
        for (error, detail) in [
            (AudioError::Host("host-text".into()), "host-text"),
            (AudioError::Decode("decode-text".into()), "decode-text"),
            (
                AudioError::AllRoutesFailed("routes-text".into()),
                "routes-text",
            ),
            (AudioError::Io(std::io::Error::other("io-text")), "io-text"),
        ] {
            let mapped = audio_failure(&error);

            assert_eq!(mapped.detail().map(|d| d.as_str()), Some(detail));
            assert_eq!(mapped.args().count(), 0);
        }
        assert_eq!(
            failure(&SoundboardError::Storage("db-text".into()))
                .detail()
                .map(|d| d.as_str()),
            Some("db-text")
        );
    }

    #[test]
    fn errors_with_no_upstream_text_carry_no_detail() {
        assert!(
            audio_failure(&AudioError::NoDefaultDevice)
                .detail()
                .is_none()
        );
        assert!(audio_failure(&AudioError::NoRoute).detail().is_none());
        assert!(
            failure(&SoundboardError::ClipNotFound("c".into()))
                .detail()
                .is_none()
        );
    }

    fn every_constructor() -> Vec<MessageRef> {
        every_audio_error_with_its_code()
            .iter()
            .map(|(error, _)| audio_failure(error))
            .chain(
                every_step_error_with_its_code()
                    .iter()
                    .map(|(error, _)| failure(error)),
            )
            .collect()
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(SOUNDBOARD_MESSAGES.en_ftl);
        let uk = ftl_variables(SOUNDBOARD_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                SOUNDBOARD_MESSAGES.codes.contains(code),
                "{code} not catalogued"
            );
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
        let catalogued: BTreeSet<MessageCode> = SOUNDBOARD_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(SOUNDBOARD_MESSAGES.en_ftl),
            ftl_variables(SOUNDBOARD_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_never_an_argument() {
        for message in every_constructor() {
            assert!(
                message.args().all(|(_, value)| matches!(
                    value,
                    MessageArg::Verbatim(text) if !text.as_str().is_empty()
                )),
                "{message:?}"
            );
        }
    }
}
