use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{ActorRole, ActorSlot, CancelSignal, CanonicalVariable, message_emote_codes};
use forge_types::{ArgStack, EventId, MessageRef, SubActionOutcome, UserText, Variant};
use tokio::sync::watch;

pub(crate) fn dispatch_outcome<T>(result: &Result<T, MessageRef>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => SubActionOutcome::Failed(UserText::Message(error.clone())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDescriptor {
    pub id: String,
    pub name: String,
    pub locale: String,
    pub engine_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakingViewer {
    pub platform: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeechOrigin {
    pub viewer: Option<SpeakingViewer>,
    pub caused_by: Option<EventId>,
    pub message_emotes: Vec<String>,
}

impl SpeechOrigin {
    pub fn from_args(args: &ArgStack, caused_by: Option<EventId>) -> Self {
        Self {
            viewer: principal_viewer(args),
            caused_by,
            message_emotes: message_emote_codes(args),
        }
    }
}

pub(crate) fn platform_scoped_alias_key(args: &ArgStack, alias_name: &str) -> String {
    if alias_name.contains(':') {
        return alias_name.to_owned();
    }
    match principal_text(args, ActorSlot::Platform) {
        Some(platform) => format!("{platform}:{alias_name}"),
        None => alias_name.to_owned(),
    }
}

fn principal_text(args: &ArgStack, slot: ActorSlot) -> Option<&str> {
    match args.get(CanonicalVariable::actor(ActorRole::Principal, slot).name()) {
        Some(Variant::String(value)) if !value.trim().is_empty() => Some(value.trim()),
        _ => None,
    }
}

fn principal_viewer(args: &ArgStack) -> Option<SpeakingViewer> {
    let login = principal_text(args, ActorSlot::Login);
    let id = principal_text(args, ActorSlot::Id).or(login)?;
    let name = principal_text(args, ActorSlot::Name)
        .or(login)
        .unwrap_or(id);
    Some(SpeakingViewer {
        platform: principal_text(args, ActorSlot::Platform)
            .unwrap_or_default()
            .to_owned(),
        id: id.to_owned(),
        name: name.to_owned(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowSpeech {
    pub text: String,
    pub voice: Option<String>,
    pub overlay: String,
    pub show: String,
    pub origin: SpeechOrigin,
}

#[derive(Debug, Clone)]
pub struct SpeechStartSignal(Arc<watch::Sender<bool>>);

impl SpeechStartSignal {
    pub fn new() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }

    pub fn mark_started(&self) {
        self.0.send_replace(true);
    }

    pub(crate) async fn started(&self) {
        let mut seen = self.0.subscribe();
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a dropped start signal also releases the waiter"
        )]
        let _ = seen.wait_for(|started| *started).await;
    }
}

impl Default for SpeechStartSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechVoice {
    Chosen(Option<String>),
    Engine(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechRequest {
    pub text: String,
    pub voice: SpeechVoice,
    pub is_reward: bool,
    pub origin: SpeechOrigin,
}

impl SpeechRequest {
    pub fn chosen_voice(text: String, voice: Option<String>, origin: SpeechOrigin) -> Self {
        Self {
            text,
            voice: SpeechVoice::Chosen(voice),
            is_reward: false,
            origin,
        }
    }

    pub fn reward_sourced(text: String, voice: Option<String>, origin: SpeechOrigin) -> Self {
        Self {
            is_reward: true,
            ..Self::chosen_voice(text, voice, origin)
        }
    }

    pub fn with_engine(text: String, engine_id: String, origin: SpeechOrigin) -> Self {
        Self {
            text,
            voice: SpeechVoice::Engine(engine_id),
            is_reward: false,
            origin,
        }
    }
}

#[async_trait]
pub trait SpeakDispatcher: Send + Sync {
    async fn speak(&self, request: SpeechRequest) -> Result<(), MessageRef>;

    async fn speak_queued_for_run(
        &self,
        request: SpeechRequest,
        run: CancelSignal,
    ) -> Result<(), MessageRef>;

    async fn speak_and_wait(
        &self,
        request: SpeechRequest,
        cancel: CancelSignal,
    ) -> Result<(), MessageRef>;

    async fn available_voices(&self) -> Vec<VoiceDescriptor>;
}

#[async_trait]
pub trait ShowSpeaker: Send + Sync {
    async fn speak_for_show(
        &self,
        speech: ShowSpeech,
        cancel: CancelSignal,
        started: SpeechStartSignal,
    ) -> Result<(), MessageRef>;

    fn check_voice(&self, encoded: &str) -> Result<(), MessageRef>;
}

#[async_trait]
pub trait SpeakQueueControl: Send + Sync {
    async fn stop_current(&self) -> Result<(), MessageRef>;

    async fn pause(&self) -> Result<(), MessageRef>;

    async fn resume(&self) -> Result<(), MessageRef>;

    async fn skip_current(&self) -> Result<(), MessageRef>;

    async fn clear_keep_current(&self) -> Result<(), MessageRef>;
}

#[async_trait]
pub trait VoiceAliasWriter: Send + Sync {
    async fn alias_set(
        &self,
        viewer_id: String,
        viewer_name: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), MessageRef>;

    async fn alias_switch(
        &self,
        viewer_id: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), MessageRef>;
}

#[derive(Clone)]
pub struct SpeechPorts {
    pub speech: Arc<dyn SpeakDispatcher>,
    pub queue: Arc<dyn SpeakQueueControl>,
    pub aliases: Arc<dyn VoiceAliasWriter>,
}

impl SpeechPorts {
    pub fn from_shared<T>(speaker: Arc<T>) -> Self
    where
        T: SpeakDispatcher + SpeakQueueControl + VoiceAliasWriter + 'static,
    {
        Self {
            speech: Arc::clone(&speaker) as Arc<dyn SpeakDispatcher>,
            queue: Arc::clone(&speaker) as Arc<dyn SpeakQueueControl>,
            aliases: speaker as Arc<dyn VoiceAliasWriter>,
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_types::MessageCode;

    use super::*;

    fn stack(pairs: &[(&str, &str)]) -> ArgStack {
        pairs.iter().fold(ArgStack::new(), |stack, (key, value)| {
            stack.set((*key).to_owned(), Variant::String((*value).to_owned()))
        })
    }

    fn viewer(platform: &str, id: &str, name: &str) -> Option<SpeakingViewer> {
        Some(SpeakingViewer {
            platform: platform.to_owned(),
            id: id.to_owned(),
            name: name.to_owned(),
        })
    }

    #[test]
    fn principal_viewer_fills_missing_slots_from_the_login() {
        for (label, pairs, expected) in [
            (
                "all slots",
                vec![
                    ("user_id", "77"),
                    ("user_name", "Aurora"),
                    ("user_login", "aurora"),
                    ("user_platform", "kick"),
                ],
                viewer("kick", "77", "Aurora"),
            ),
            (
                "login only",
                vec![("user_login", "aurora")],
                viewer("", "aurora", "aurora"),
            ),
            (
                "id and login, no name",
                vec![("user_id", "77"), ("user_login", "aurora")],
                viewer("", "77", "aurora"),
            ),
            ("id only", vec![("user_id", "77")], viewer("", "77", "77")),
            (
                "blank id and name fall back to the login",
                vec![
                    ("user_id", "  "),
                    ("user_name", " "),
                    ("user_login", "aurora"),
                ],
                viewer("", "aurora", "aurora"),
            ),
            (
                "padded values are trimmed",
                vec![("user_id", " 77 "), ("user_name", " Aurora ")],
                viewer("", "77", "Aurora"),
            ),
            (
                "Unicode display name",
                vec![("user_id", "9"), ("user_name", "Зірка")],
                viewer("", "9", "Зірка"),
            ),
        ] {
            let origin = SpeechOrigin::from_args(&stack(&pairs), None);

            assert_eq!(origin.viewer, expected, "{label}");
        }
    }

    #[test]
    fn no_viewer_without_an_id_or_login() {
        for pairs in [
            vec![],
            vec![("user_name", "Aurora"), ("user_platform", "kick")],
            vec![("user_id", ""), ("user_login", "   ")],
            vec![("user", "Aurora")],
        ] {
            let origin = SpeechOrigin::from_args(&stack(&pairs), None);

            assert_eq!(origin.viewer, None, "{pairs:?}");
        }
    }

    #[test]
    fn alias_key_is_scoped_to_the_triggering_platform_unless_already_scoped() {
        for (pairs, name, expected) in [
            (vec![("user_platform", "twitch")], "Alice", "twitch:Alice"),
            (vec![("user_platform", " kick ")], "Alice", "kick:Alice"),
            (vec![], "Alice", "Alice"),
            (vec![("user_platform", "   ")], "Alice", "Alice"),
            (
                vec![("user_platform", "twitch")],
                "youtube:Alice",
                "youtube:Alice",
            ),
            (vec![], "kick:Alice", "kick:Alice"),
        ] {
            let key = platform_scoped_alias_key(&stack(&pairs), name);

            assert_eq!(key, expected, "{pairs:?} / {name:?}");
        }
    }

    fn queue_stopped() -> MessageRef {
        MessageRef::new(MessageCode::from_static("speak", "queue_stopped"))
    }

    #[test]
    fn dispatch_outcome_wraps_the_error_message_as_localizable_text_and_passes_success_through() {
        assert_eq!(
            (
                dispatch_outcome(&Ok(())),
                dispatch_outcome::<()>(&Err(queue_stopped())),
            ),
            (
                SubActionOutcome::Success,
                SubActionOutcome::Failed(UserText::Message(queue_stopped())),
            )
        );
    }
}
