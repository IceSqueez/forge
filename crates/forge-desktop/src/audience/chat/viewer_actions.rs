use forge_components::tr;
use forge_host::integration_catalog::viewer_step;
pub(crate) use forge_platform_core::ViewerAction;
use forge_platform_core::ViewerRef;
use forge_speak_queue::AliasKey;
use forge_types::{PlatformId, SubActionStep};

use crate::audience::chat_author::{AuthorHandle, AuthorKey};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewerTarget {
    pub platform: PlatformId,
    pub name: String,
    pub viewer_id: Option<String>,
}

impl ViewerTarget {
    pub(crate) fn new(key: &AuthorKey, name: impl Into<String>) -> Self {
        let viewer_id = match &key.handle {
            AuthorHandle::ViewerId(id) => Some(id.to_string()),
            AuthorHandle::Name(_) => None,
        };
        Self {
            platform: key.platform,
            name: name.into(),
            viewer_id,
        }
    }

    pub(crate) fn tts_alias_key(&self) -> Option<String> {
        let id = self
            .viewer_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())?;
        Some(AliasKey::new(Some(self.platform), id).to_stored())
    }

    pub(crate) fn builtin_id(&self) -> String {
        self.platform.as_str().to_owned()
    }

    pub(crate) fn supports(&self, action: &ViewerAction) -> bool {
        self.step(action).is_some()
    }

    pub(crate) fn can_whisper(&self) -> bool {
        self.supports(&ViewerAction::Whisper(String::new()))
    }

    pub(crate) fn label(&self, action: &ViewerAction) -> String {
        let name = self.name.as_str();
        match action {
            ViewerAction::Shoutout => tr!("viewer_step_shoutout", name = name),
            ViewerAction::Whisper(_) => tr!("viewer_step_whisper", name = name),
            ViewerAction::Timeout { .. } => tr!("viewer_step_timeout", name = name),
            ViewerAction::Ban => tr!("viewer_step_ban", name = name),
            ViewerAction::Unban => tr!("viewer_step_unban", name = name),
        }
    }

    pub(crate) fn step(&self, action: &ViewerAction) -> Option<SubActionStep> {
        let viewer = ViewerRef {
            name: &self.name,
            viewer_id: self.viewer_id.as_deref(),
        };
        let step = viewer_step(self.platform, viewer, action)?;
        Some(SubActionStep {
            kind_id: step.kind_id.to_owned(),
            config: step.config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: Some(self.label(action)),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::PlatformId;
    use forge_types::Variant;

    use std::sync::Arc;

    use forge_runtime::{SpeakDispatcher, SpeechOrigin, SpeechRequest};
    use forge_speak_queue::SpeakEvent;
    use forge_speech_types::{AliasId, AliasState, EngineId, VoiceAlias, VoiceId};
    use forge_types::ArgStack;

    use super::{ViewerAction, ViewerTarget};
    use crate::audience::chat_author::AuthorKey;
    use crate::test_support::{spawn_speak_queue, within_hang_guard};
    use forge_host::speak_bridge::SpeakBridge;

    const ONE_WEEK_SECONDS: i64 = 604_800;
    const TWO_WEEKS_SECONDS: i64 = 1_209_600;
    const ONE_DAY_SECONDS: i64 = 86_400;

    fn by_id(platform: PlatformId, viewer_id: &str) -> ViewerTarget {
        ViewerTarget::new(&AuthorKey::by_viewer_id(platform, viewer_id), "alice")
    }

    fn by_name(platform: PlatformId, name: &str) -> ViewerTarget {
        ViewerTarget::new(&AuthorKey::by_name(platform, name), name)
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn config(entries: &[(&str, Variant)]) -> BTreeMap<String, Variant> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn kind_and_config(
        target: &ViewerTarget,
        action: &ViewerAction,
    ) -> Option<(String, BTreeMap<String, Variant>)> {
        target.step(action).map(|step| (step.kind_id, step.config))
    }

    fn every_action() -> [ViewerAction; 5] {
        [
            ViewerAction::Shoutout,
            ViewerAction::Whisper("hi".to_owned()),
            ViewerAction::Timeout { seconds: 600 },
            ViewerAction::Ban,
            ViewerAction::Unban,
        ]
    }

    #[test]
    fn each_platform_builds_its_own_moderation_step_against_the_viewer_identity_it_needs() {
        let ten_minutes = ViewerAction::Timeout { seconds: 600 };
        for (target, action, kind, expected) in [
            (
                by_id(PlatformId::Twitch, "1001"),
                ViewerAction::Shoutout,
                "twitch.channel.send_shoutout",
                config(&[("to_broadcaster_id", text("1001"))]),
            ),
            (
                by_id(PlatformId::Twitch, "1001"),
                ViewerAction::Whisper("hi there".to_owned()),
                "twitch.chat.send_whisper",
                config(&[("to_user_id", text("1001")), ("message", text("hi there"))]),
            ),
            (
                by_id(PlatformId::Twitch, "1001"),
                ten_minutes.clone(),
                "twitch.moderation.timeout_user",
                config(&[
                    ("target_user_id", text("1001")),
                    ("duration_seconds", Variant::Int(600)),
                ]),
            ),
            (
                by_id(PlatformId::Twitch, "1001"),
                ViewerAction::Ban,
                "twitch.moderation.ban_user",
                config(&[("target_user_id", text("1001"))]),
            ),
            (
                by_id(PlatformId::Twitch, "1001"),
                ViewerAction::Unban,
                "twitch.moderation.unban_user",
                config(&[("target_user_id", text("1001"))]),
            ),
            (
                by_id(PlatformId::YouTube, "UCabc"),
                ten_minutes.clone(),
                "youtube.moderation.timeout_user",
                config(&[
                    ("channel_id", text("UCabc")),
                    ("display_name", text("alice")),
                    ("duration_seconds", Variant::Int(600)),
                ]),
            ),
            (
                by_id(PlatformId::YouTube, "UCabc"),
                ViewerAction::Ban,
                "youtube.moderation.ban_user",
                config(&[
                    ("channel_id", text("UCabc")),
                    ("display_name", text("alice")),
                ]),
            ),
            (
                by_id(PlatformId::YouTube, "UCabc"),
                ViewerAction::Unban,
                "youtube.moderation.unban_user",
                config(&[("channel_id", text("UCabc"))]),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                ten_minutes,
                "kick.moderation.timeout",
                config(&[
                    ("user_id", text("4242")),
                    ("display_name", text("alice")),
                    ("duration_minutes", Variant::Int(10)),
                ]),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                ViewerAction::Ban,
                "kick.moderation.ban",
                config(&[("user_id", text("4242")), ("display_name", text("alice"))]),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                ViewerAction::Unban,
                "kick.moderation.unban",
                config(&[("user_id", text("4242"))]),
            ),
        ] {
            assert_eq!(
                kind_and_config(&target, &action),
                Some((kind.to_owned(), expected)),
                "{:?} {action:?}",
                target.platform
            );
        }
    }

    #[test]
    fn a_nameless_viewer_still_gets_ban_and_timeout_steps_with_a_blank_display_name() {
        for (platform, viewer_id) in [(PlatformId::YouTube, "UCabc"), (PlatformId::Kick, "4242")] {
            let target = ViewerTarget::new(&AuthorKey::by_viewer_id(platform, viewer_id), "");
            for action in [ViewerAction::Ban, ViewerAction::Timeout { seconds: 600 }] {
                assert_eq!(
                    target
                        .step(&action)
                        .and_then(|step| step.config.get("display_name").cloned()),
                    Some(text("")),
                    "{platform:?} {action:?}"
                );
            }
        }
    }

    #[test]
    fn a_viewer_without_the_identity_its_platform_needs_gets_no_step_for_any_action() {
        for target in [
            by_name(PlatformId::YouTube, "alice"),
            by_name(PlatformId::Kick, "alice"),
            by_id(PlatformId::YouTube, ""),
            by_id(PlatformId::Kick, "alice"),
            by_id(PlatformId::Kick, "-5"),
            by_id(PlatformId::Kick, ""),
            by_name(PlatformId::Twitch, "alice"),
            by_id(PlatformId::Twitch, ""),
            by_id(PlatformId::Twitch, "  "),
        ] {
            for action in every_action() {
                assert_eq!(
                    (target.step(&action), target.supports(&action)),
                    (None, false),
                    "{target:?} {action:?}"
                );
            }
        }
    }

    #[test]
    fn shoutout_and_whisper_are_offered_only_on_twitch() {
        for (target, offered) in [
            (by_id(PlatformId::Twitch, "1001"), true),
            (by_id(PlatformId::YouTube, "UCabc"), false),
            (by_id(PlatformId::Kick, "4242"), false),
        ] {
            assert_eq!(
                [
                    target.supports(&ViewerAction::Shoutout),
                    target.supports(&ViewerAction::Whisper("hi".to_owned())),
                ],
                [offered, offered],
                "{:?}",
                target.platform
            );
        }
    }

    #[test]
    fn a_timeout_step_exists_only_within_each_platforms_duration_bounds() {
        for (target, seconds, expected) in [
            (by_id(PlatformId::Twitch, "1001"), -1, None),
            (by_id(PlatformId::Twitch, "1001"), 0, None),
            (
                by_id(PlatformId::Twitch, "1001"),
                1,
                Some(("duration_seconds", 1)),
            ),
            (
                by_id(PlatformId::Twitch, "1001"),
                TWO_WEEKS_SECONDS,
                Some(("duration_seconds", TWO_WEEKS_SECONDS)),
            ),
            (
                by_id(PlatformId::Twitch, "1001"),
                TWO_WEEKS_SECONDS + 1,
                None,
            ),
            (by_id(PlatformId::YouTube, "UCabc"), 0, None),
            (
                by_id(PlatformId::YouTube, "UCabc"),
                1,
                Some(("duration_seconds", 1)),
            ),
            (
                by_id(PlatformId::YouTube, "UCabc"),
                ONE_DAY_SECONDS,
                Some(("duration_seconds", ONE_DAY_SECONDS)),
            ),
            (
                by_id(PlatformId::YouTube, "UCabc"),
                ONE_DAY_SECONDS + 1,
                None,
            ),
            (by_id(PlatformId::Kick, "4242"), -1, None),
            (by_id(PlatformId::Kick, "4242"), 0, None),
            (
                by_id(PlatformId::Kick, "4242"),
                1,
                Some(("duration_minutes", 1)),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                60,
                Some(("duration_minutes", 1)),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                61,
                Some(("duration_minutes", 2)),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                600,
                Some(("duration_minutes", 10)),
            ),
            (
                by_id(PlatformId::Kick, "4242"),
                ONE_WEEK_SECONDS,
                Some(("duration_minutes", 10_080)),
            ),
            (by_id(PlatformId::Kick, "4242"), ONE_WEEK_SECONDS + 1, None),
            (by_id(PlatformId::Kick, "4242"), TWO_WEEKS_SECONDS, None),
            (by_id(PlatformId::Kick, "4242"), i64::MAX, None),
        ] {
            let duration = target
                .step(&ViewerAction::Timeout { seconds })
                .and_then(|step| {
                    step.config
                        .into_iter()
                        .find(|(key, _)| key.starts_with("duration_"))
                });
            assert_eq!(
                duration,
                expected.map(|(key, value)| (key.to_owned(), Variant::Int(value))),
                "{:?} {seconds}s",
                target.platform
            );
        }
    }

    #[test]
    fn the_step_runs_on_the_integration_of_the_viewers_platform() {
        for (target, builtin) in [
            (by_name(PlatformId::Twitch, "alice"), "twitch"),
            (by_id(PlatformId::YouTube, "UCabc"), "youtube"),
            (by_id(PlatformId::Kick, "4242"), "kick"),
        ] {
            assert_eq!(target.builtin_id(), builtin);
        }
    }
    #[test]
    fn a_viewer_without_a_usable_id_has_no_tts_alias_key() {
        for target in [
            by_name(PlatformId::Twitch, "alice"),
            by_name(PlatformId::YouTube, "alice"),
            by_name(PlatformId::Kick, "alice"),
            by_id(PlatformId::Twitch, ""),
            by_id(PlatformId::Kick, "   "),
        ] {
            assert_eq!(target.tts_alias_key(), None, "{target:?}");
        }
    }

    async fn first_outcome(stream: &mut forge_speak_queue::SpeakEventStream) -> SpeakEvent {
        within_hang_guard(async {
            loop {
                if let Ok(
                    event @ (SpeakEvent::Skipped { .. }
                    | SpeakEvent::Failed { .. }
                    | SpeakEvent::Finished { .. }),
                ) = stream.recv().await
                {
                    return event;
                }
            }
        })
        .await
    }

    #[tokio::test]
    async fn the_tts_alias_key_is_the_identity_the_speech_queue_sees_for_that_viewer() {
        for (platform, viewer_id, ingest_platform, ingest_id) in [
            (PlatformId::Twitch, " 141981764 ", "twitch", "141981764"),
            (
                PlatformId::YouTube,
                "UCx9-abc_DEF",
                "youtube",
                "UCx9-abc_DEF",
            ),
            (PlatformId::Kick, "4242", "kick", "4242"),
        ] {
            let target = by_id(platform, viewer_id);
            let key = target
                .tts_alias_key()
                .expect("an id-keyed viewer has a key");
            let (speak, mut stream, _resolver) = spawn_speak_queue(vec![VoiceAlias {
                id: AliasId::generate(),
                alias_key: key.clone(),
                viewer_name: target.name.clone(),
                engine_id: EngineId(String::new()),
                voice_id: VoiceId(String::new()),
                pitch_semitones: None,
                rate_multiplier: None,
                state: AliasState::Blocked,
            }]);
            let bridge = SpeakBridge::new(Arc::new(speak));
            let args = [
                ("user_platform", ingest_platform),
                ("user_id", ingest_id),
                ("user_name", "renamed_since"),
            ]
            .into_iter()
            .fold(ArgStack::new(), |stack, (name, value)| {
                stack.set(name.to_owned(), text(value))
            });

            SpeakDispatcher::speak(
                &bridge,
                SpeechRequest::chosen_voice(
                    "hello chat".to_owned(),
                    None,
                    SpeechOrigin::from_args(&args, None),
                ),
            )
            .await
            .expect("dispatch");

            assert!(
                matches!(
                    first_outcome(&mut stream).await,
                    SpeakEvent::Skipped { ref message, .. }
                        if message.code().to_string() == "speak.skipped_alias"
                ),
                "{key:?} did not match the queued speaker"
            );
        }
    }
}
