use std::collections::BTreeMap;

use forge_components::Platform;
use forge_types::{SubActionStep, Variant};

use super::platform_gate::platform_integration;
use crate::chat_author::{AuthorHandle, AuthorKey};

const TWITCH_SHOUTOUT_KIND: &str = "twitch.channel.send_shoutout";
const TWITCH_WHISPER_KIND: &str = "twitch.chat.send_whisper";
const TWITCH_TIMEOUT_KIND: &str = "twitch.moderation.timeout_user";
const TWITCH_BAN_KIND: &str = "twitch.moderation.ban_user";
const YOUTUBE_TIMEOUT_KIND: &str = "youtube.moderation.timeout_user";
const YOUTUBE_BAN_KIND: &str = "youtube.moderation.ban_user";
const KICK_TIMEOUT_KIND: &str = "kick.moderation.timeout";
const KICK_BAN_KIND: &str = "kick.moderation.ban";

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
        if self.name.is_empty() {
            return None;
        }
        let login = Variant::String(self.name.clone());
        match action {
            ViewerAction::Shoutout => Some((
                TWITCH_SHOUTOUT_KIND,
                BTreeMap::from([("to_broadcaster_login".to_owned(), login)]),
            )),
            ViewerAction::Whisper(message) => Some((
                TWITCH_WHISPER_KIND,
                BTreeMap::from([
                    ("to_user_login".to_owned(), login),
                    ("message".to_owned(), Variant::String(message.clone())),
                ]),
            )),
            ViewerAction::Timeout { seconds } => {
                (1..=TWITCH_MAX_TIMEOUT_SECONDS).contains(seconds).then(|| {
                    (
                        TWITCH_TIMEOUT_KIND,
                        BTreeMap::from([
                            ("target_user_login".to_owned(), login),
                            ("duration_seconds".to_owned(), Variant::Int(*seconds)),
                        ]),
                    )
                })
            }
            ViewerAction::Ban => Some((
                TWITCH_BAN_KIND,
                BTreeMap::from([("target_user_login".to_owned(), login)]),
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
            ViewerAction::Shoutout | ViewerAction::Whisper(_) => None,
        }
    }
}
