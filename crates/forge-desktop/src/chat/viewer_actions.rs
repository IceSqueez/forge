use std::collections::BTreeMap;

use forge_components::Platform;
use forge_types::{SubActionStep, Variant};

use super::platform_gate::platform_integration;
use crate::chat_author::{AuthorHandle, AuthorKey};
use crate::voice_alias_form::{alias_key, platform_scope};

const TWITCH_SHOUTOUT_KIND: &str = "twitch.channel.send_shoutout";
const TWITCH_WHISPER_KIND: &str = "twitch.chat.send_whisper";
const TWITCH_TIMEOUT_KIND: &str = "twitch.moderation.timeout_user";
const TWITCH_BAN_KIND: &str = "twitch.moderation.ban_user";
const TWITCH_UNBAN_KIND: &str = "twitch.moderation.unban_user";
const YOUTUBE_TIMEOUT_KIND: &str = "youtube.moderation.timeout_user";
const YOUTUBE_BAN_KIND: &str = "youtube.moderation.ban_user";
const YOUTUBE_UNBAN_KIND: &str = "youtube.moderation.unban_user";
const KICK_TIMEOUT_KIND: &str = "kick.moderation.timeout";
const KICK_BAN_KIND: &str = "kick.moderation.ban";
const KICK_UNBAN_KIND: &str = "kick.moderation.unban";

const TWITCH_SHOUTOUT_TARGET_KEY: &str = "to_broadcaster_id";
const TWITCH_WHISPER_TARGET_KEY: &str = "to_user_id";
const TWITCH_MODERATION_TARGET_KEY: &str = "target_user_id";

const TWITCH_MAX_TIMEOUT_SECONDS: i64 = 1_209_600;
const YOUTUBE_MAX_TIMEOUT_SECONDS: i64 = 86_400;
const KICK_MAX_TIMEOUT_MINUTES: i64 = 10_080;
const SECONDS_PER_MINUTE: u64 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ViewerAction {
    Shoutout,
    Whisper(String),
    Timeout { seconds: i64 },
    Ban,
    Unban,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewerTarget {
    pub platform: Platform,
    pub name: String,
    pub viewer_id: Option<String>,
}

impl ViewerTarget {
    pub fn new(key: &AuthorKey, name: impl Into<String>) -> Self {
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

    pub fn tts_alias_key(&self) -> Option<String> {
        let id = self
            .viewer_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())?;
        Some(alias_key(platform_scope(self.platform), id))
    }

    pub fn builtin_id(&self) -> String {
        platform_integration(self.platform).id_str().to_owned()
    }

    pub fn supports(&self, action: &ViewerAction) -> bool {
        self.step(action).is_some()
    }

    pub fn label(&self, action: &ViewerAction) -> String {
        let name = &self.name;
        match action {
            ViewerAction::Shoutout => format!("Shoutout {name}"),
            ViewerAction::Whisper(_) => format!("Whisper {name}"),
            ViewerAction::Timeout { .. } => format!("Timeout {name}"),
            ViewerAction::Ban => format!("Ban {name}"),
            ViewerAction::Unban => format!("Unban {name}"),
        }
    }

    pub fn step(&self, action: &ViewerAction) -> Option<SubActionStep> {
        let (kind_id, config) = match self.platform {
            Platform::Twitch => self.twitch_step(action)?,
            Platform::YouTube => self.youtube_step(action)?,
            Platform::Kick => self.kick_step(action)?,
        };
        Some(SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: Some(self.label(action)),
        })
    }

    fn twitch_step(
        &self,
        action: &ViewerAction,
    ) -> Option<(&'static str, BTreeMap<String, Variant>)> {
        let user_id = self
            .viewer_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())?;
        let id = Variant::String(user_id.to_owned());
        match action {
            ViewerAction::Shoutout => Some((
                TWITCH_SHOUTOUT_KIND,
                BTreeMap::from([(TWITCH_SHOUTOUT_TARGET_KEY.to_owned(), id)]),
            )),
            ViewerAction::Whisper(message) => Some((
                TWITCH_WHISPER_KIND,
                BTreeMap::from([
                    (TWITCH_WHISPER_TARGET_KEY.to_owned(), id),
                    ("message".to_owned(), Variant::String(message.clone())),
                ]),
            )),
            ViewerAction::Timeout { seconds } => {
                (1..=TWITCH_MAX_TIMEOUT_SECONDS).contains(seconds).then(|| {
                    (
                        TWITCH_TIMEOUT_KIND,
                        BTreeMap::from([
                            (TWITCH_MODERATION_TARGET_KEY.to_owned(), id),
                            ("duration_seconds".to_owned(), Variant::Int(*seconds)),
                        ]),
                    )
                })
            }
            ViewerAction::Ban => Some((
                TWITCH_BAN_KIND,
                BTreeMap::from([(TWITCH_MODERATION_TARGET_KEY.to_owned(), id)]),
            )),
            ViewerAction::Unban => Some((
                TWITCH_UNBAN_KIND,
                BTreeMap::from([(TWITCH_MODERATION_TARGET_KEY.to_owned(), id)]),
            )),
        }
    }

    fn youtube_step(
        &self,
        action: &ViewerAction,
    ) -> Option<(&'static str, BTreeMap<String, Variant>)> {
        let channel_id = self.viewer_id.clone().filter(|id| !id.is_empty())?;
        let channel = ("channel_id".to_owned(), Variant::String(channel_id));
        match action {
            ViewerAction::Timeout { seconds } => (1..=YOUTUBE_MAX_TIMEOUT_SECONDS)
                .contains(seconds)
                .then(|| {
                    (
                        YOUTUBE_TIMEOUT_KIND,
                        BTreeMap::from([
                            channel,
                            ("duration_seconds".to_owned(), Variant::Int(*seconds)),
                        ]),
                    )
                }),
            ViewerAction::Ban => Some((YOUTUBE_BAN_KIND, BTreeMap::from([channel]))),
            ViewerAction::Unban => Some((YOUTUBE_UNBAN_KIND, BTreeMap::from([channel]))),
            ViewerAction::Shoutout | ViewerAction::Whisper(_) => None,
        }
    }

    fn kick_step(
        &self,
        action: &ViewerAction,
    ) -> Option<(&'static str, BTreeMap<String, Variant>)> {
        let user_id = self
            .viewer_id
            .clone()
            .filter(|id| id.parse::<u64>().is_ok())?;
        let user = ("user_id".to_owned(), Variant::String(user_id));
        match action {
            ViewerAction::Timeout { seconds } => {
                let minutes = u64::try_from(*seconds)
                    .ok()
                    .filter(|seconds| *seconds >= 1)?
                    .div_ceil(SECONDS_PER_MINUTE);
                let minutes = i64::try_from(minutes)
                    .ok()
                    .filter(|minutes| *minutes <= KICK_MAX_TIMEOUT_MINUTES)?;
                Some((
                    KICK_TIMEOUT_KIND,
                    BTreeMap::from([user, ("duration_minutes".to_owned(), Variant::Int(minutes))]),
                ))
            }
            ViewerAction::Ban => Some((KICK_BAN_KIND, BTreeMap::from([user]))),
            ViewerAction::Unban => Some((KICK_UNBAN_KIND, BTreeMap::from([user]))),
            ViewerAction::Shoutout | ViewerAction::Whisper(_) => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_components::Platform;
    use forge_types::Variant;

    use std::sync::Arc;
    use std::time::Duration;

    use forge_runtime::{SpeakDispatcher, SpeechOrigin};
    use forge_speak_queue::SpeakEvent;
    use forge_types::ArgStack;
    use forge_voice::{AliasId, AliasState, EngineId, VoiceAlias, VoiceId};

    use super::{ViewerAction, ViewerTarget};
    use crate::chat_author::AuthorKey;
    use crate::speak_bridge::SpeakBridge;
    use crate::test_support::spawn_speak_queue;

    const ONE_WEEK_SECONDS: i64 = 604_800;
    const TWO_WEEKS_SECONDS: i64 = 1_209_600;
    const ONE_DAY_SECONDS: i64 = 86_400;
    const QUEUE_DEADLINE: Duration = Duration::from_secs(5);

    fn by_id(platform: Platform, viewer_id: &str) -> ViewerTarget {
        ViewerTarget::new(&AuthorKey::by_viewer_id(platform, viewer_id), "alice")
    }

    fn by_name(platform: Platform, name: &str) -> ViewerTarget {
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
                by_id(Platform::Twitch, "1001"),
                ViewerAction::Shoutout,
                "twitch.channel.send_shoutout",
                config(&[("to_broadcaster_id", text("1001"))]),
            ),
            (
                by_id(Platform::Twitch, "1001"),
                ViewerAction::Whisper("hi there".to_owned()),
                "twitch.chat.send_whisper",
                config(&[("to_user_id", text("1001")), ("message", text("hi there"))]),
            ),
            (
                by_id(Platform::Twitch, "1001"),
                ten_minutes.clone(),
                "twitch.moderation.timeout_user",
                config(&[
                    ("target_user_id", text("1001")),
                    ("duration_seconds", Variant::Int(600)),
                ]),
            ),
            (
                by_id(Platform::Twitch, "1001"),
                ViewerAction::Ban,
                "twitch.moderation.ban_user",
                config(&[("target_user_id", text("1001"))]),
            ),
            (
                by_id(Platform::Twitch, "1001"),
                ViewerAction::Unban,
                "twitch.moderation.unban_user",
                config(&[("target_user_id", text("1001"))]),
            ),
            (
                by_id(Platform::YouTube, "UCabc"),
                ten_minutes.clone(),
                "youtube.moderation.timeout_user",
                config(&[
                    ("channel_id", text("UCabc")),
                    ("duration_seconds", Variant::Int(600)),
                ]),
            ),
            (
                by_id(Platform::YouTube, "UCabc"),
                ViewerAction::Ban,
                "youtube.moderation.ban_user",
                config(&[("channel_id", text("UCabc"))]),
            ),
            (
                by_id(Platform::YouTube, "UCabc"),
                ViewerAction::Unban,
                "youtube.moderation.unban_user",
                config(&[("channel_id", text("UCabc"))]),
            ),
            (
                by_id(Platform::Kick, "4242"),
                ten_minutes,
                "kick.moderation.timeout",
                config(&[
                    ("user_id", text("4242")),
                    ("duration_minutes", Variant::Int(10)),
                ]),
            ),
            (
                by_id(Platform::Kick, "4242"),
                ViewerAction::Ban,
                "kick.moderation.ban",
                config(&[("user_id", text("4242"))]),
            ),
            (
                by_id(Platform::Kick, "4242"),
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
    fn a_viewer_without_the_identity_its_platform_needs_gets_no_step_for_any_action() {
        for target in [
            by_name(Platform::YouTube, "alice"),
            by_name(Platform::Kick, "alice"),
            by_id(Platform::YouTube, ""),
            by_id(Platform::Kick, "alice"),
            by_id(Platform::Kick, "-5"),
            by_id(Platform::Kick, ""),
            by_name(Platform::Twitch, "alice"),
            by_id(Platform::Twitch, ""),
            by_id(Platform::Twitch, "  "),
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
            (by_id(Platform::Twitch, "1001"), true),
            (by_id(Platform::YouTube, "UCabc"), false),
            (by_id(Platform::Kick, "4242"), false),
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
            (by_id(Platform::Twitch, "1001"), -1, None),
            (by_id(Platform::Twitch, "1001"), 0, None),
            (
                by_id(Platform::Twitch, "1001"),
                1,
                Some(("duration_seconds", 1)),
            ),
            (
                by_id(Platform::Twitch, "1001"),
                TWO_WEEKS_SECONDS,
                Some(("duration_seconds", TWO_WEEKS_SECONDS)),
            ),
            (by_id(Platform::Twitch, "1001"), TWO_WEEKS_SECONDS + 1, None),
            (by_id(Platform::YouTube, "UCabc"), 0, None),
            (
                by_id(Platform::YouTube, "UCabc"),
                1,
                Some(("duration_seconds", 1)),
            ),
            (
                by_id(Platform::YouTube, "UCabc"),
                ONE_DAY_SECONDS,
                Some(("duration_seconds", ONE_DAY_SECONDS)),
            ),
            (by_id(Platform::YouTube, "UCabc"), ONE_DAY_SECONDS + 1, None),
            (by_id(Platform::Kick, "4242"), -1, None),
            (by_id(Platform::Kick, "4242"), 0, None),
            (
                by_id(Platform::Kick, "4242"),
                1,
                Some(("duration_minutes", 1)),
            ),
            (
                by_id(Platform::Kick, "4242"),
                60,
                Some(("duration_minutes", 1)),
            ),
            (
                by_id(Platform::Kick, "4242"),
                61,
                Some(("duration_minutes", 2)),
            ),
            (
                by_id(Platform::Kick, "4242"),
                600,
                Some(("duration_minutes", 10)),
            ),
            (
                by_id(Platform::Kick, "4242"),
                ONE_WEEK_SECONDS,
                Some(("duration_minutes", 10_080)),
            ),
            (by_id(Platform::Kick, "4242"), ONE_WEEK_SECONDS + 1, None),
            (by_id(Platform::Kick, "4242"), TWO_WEEKS_SECONDS, None),
            (by_id(Platform::Kick, "4242"), i64::MAX, None),
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
            (by_name(Platform::Twitch, "alice"), "twitch"),
            (by_id(Platform::YouTube, "UCabc"), "youtube"),
            (by_id(Platform::Kick, "4242"), "kick"),
        ] {
            assert_eq!(target.builtin_id(), builtin);
        }
    }
    #[test]
    fn a_viewer_without_a_usable_id_has_no_tts_alias_key() {
        for target in [
            by_name(Platform::Twitch, "alice"),
            by_name(Platform::YouTube, "alice"),
            by_name(Platform::Kick, "alice"),
            by_id(Platform::Twitch, ""),
            by_id(Platform::Kick, "   "),
        ] {
            assert_eq!(target.tts_alias_key(), None, "{target:?}");
        }
    }

    async fn first_outcome(stream: &mut forge_speak_queue::SpeakEventStream) -> SpeakEvent {
        tokio::time::timeout(QUEUE_DEADLINE, async {
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
        .expect("the queue never decided the speech")
    }

    #[tokio::test]
    async fn the_tts_alias_key_is_the_identity_the_speech_queue_sees_for_that_viewer() {
        for (platform, viewer_id, ingest_platform, ingest_id) in [
            (Platform::Twitch, " 141981764 ", "twitch", "141981764"),
            (Platform::YouTube, "UCx9-abc_DEF", "youtube", "UCx9-abc_DEF"),
            (Platform::Kick, "4242", "kick", "4242"),
        ] {
            let target = by_id(platform, viewer_id);
            let key = target
                .tts_alias_key()
                .expect("an id-keyed viewer has a key");
            let (speak, mut stream, _resolver) = spawn_speak_queue(vec![VoiceAlias {
                id: AliasId::new(),
                viewer_id: key.clone(),
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
                "hello chat".to_owned(),
                None,
                SpeechOrigin::from_args(&args, None),
            )
            .await
            .expect("dispatch");

            assert!(
                matches!(
                    first_outcome(&mut stream).await,
                    SpeakEvent::Skipped { ref reason, .. } if reason.contains("blocked")
                ),
                "{key:?} did not match the queued speaker"
            );
        }
    }
}
