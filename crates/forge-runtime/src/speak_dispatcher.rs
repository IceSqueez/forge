use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::CancelSignal;
use forge_types::{ActorRole, ActorSlot, ArgStack, CanonicalVariable, EventId, Variant};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum SpeakDispatchError {
    #[error("{0}")]
    Dispatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDescriptor {
    pub id: String,
    pub name: String,
    pub locale: String,
    pub engine_id: String,
}

const SHOW_SPEECH_UNAVAILABLE: &str = "speech for an overlay show is not available";

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
}

impl SpeechOrigin {
    pub fn from_args(args: &ArgStack, caused_by: Option<EventId>) -> Self {
        Self {
            viewer: principal_viewer(args),
            caused_by,
        }
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
    pub voice_alias: Option<String>,
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
        let _ = seen.wait_for(|started| *started).await;
    }
}

impl Default for SpeechStartSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
pub trait SpeakDispatcher: Send + Sync {
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
        origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError>;

    async fn speak_with_engine(
        &self,
        text: String,
        engine_id: String,
        origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (text, engine_id, origin);
        Ok(())
    }

    async fn speak_reward_sourced(
        &self,
        text: String,
        voice_id_override: Option<String>,
        origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        self.speak(text, voice_id_override, origin).await
    }

    async fn speak_and_wait(
        &self,
        text: String,
        voice_id_override: Option<String>,
        is_reward: bool,
        origin: SpeechOrigin,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let _ = cancel;
        if is_reward {
            self.speak_reward_sourced(text, voice_id_override, origin)
                .await
        } else {
            self.speak(text, voice_id_override, origin).await
        }
    }

    async fn speak_with_engine_and_wait(
        &self,
        text: String,
        engine_id: String,
        origin: SpeechOrigin,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let _ = cancel;
        self.speak_with_engine(text, engine_id, origin).await
    }

    async fn speak_for_show(
        &self,
        speech: ShowSpeech,
        cancel: CancelSignal,
        started: SpeechStartSignal,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (speech, cancel, started);
        Err(SpeakDispatchError::Dispatch(
            SHOW_SPEECH_UNAVAILABLE.to_owned(),
        ))
    }

    async fn stop_current(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn pause(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn resume(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn skip_current(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn clear_keep_current(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn get_queue_depth(&self) -> usize {
        0
    }

    async fn get_available_voices(&self) -> Vec<VoiceDescriptor> {
        Vec::new()
    }

    async fn get_engines(&self) -> Vec<String> {
        Vec::new()
    }

    async fn alias_set(
        &self,
        viewer_id: String,
        viewer_name: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (viewer_id, viewer_name, engine_id, voice_id);
        Ok(())
    }

    async fn alias_switch(
        &self,
        viewer_id: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (viewer_id, engine_id, voice_id);
        Ok(())
    }
}
