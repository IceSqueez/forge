use async_trait::async_trait;
use forge_registry::CancelSignal;

#[derive(Debug, thiserror::Error)]
pub enum SpeakDispatchError {
    #[error("{0}")]
    Dispatch(String),
}

/// Owned strings keep forge-speak-queue / forge-tts-core types out of this crate's surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDescriptor {
    pub id: String,
    pub name: String,
    pub locale: String,
    pub engine_id: String,
}

const SHOW_SPEECH_UNAVAILABLE: &str = "speech for an overlay show is not available";

/// Plays only through `overlay`, never through the global audio receiver; `show` joins the speech
/// to the show on that overlay's page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowSpeech {
    pub text: String,
    pub voice_alias: Option<String>,
    pub overlay: String,
    pub show: String,
}

/// Lives here to avoid a dependency cycle; every method except `speak` defaults to a no-op/empty.
#[async_trait]
pub trait SpeakDispatcher: Send + Sync {
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError>;

    async fn speak_with_alias(
        &self,
        text: String,
        alias_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (text, alias_id);
        Ok(())
    }

    async fn speak_with_engine(
        &self,
        text: String,
        engine_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (text, engine_id);
        Ok(())
    }

    async fn speak_with_voice(
        &self,
        text: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (text, voice_id);
        Ok(())
    }

    /// Marks the request as reward-sourced so the pipeline's `strip_reward_emotes` gate applies.
    async fn speak_reward_sourced(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.speak(text, voice_id_override).await
    }

    async fn speak_and_wait(
        &self,
        text: String,
        voice_id_override: Option<String>,
        is_reward: bool,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let _ = cancel;
        if is_reward {
            self.speak_reward_sourced(text, voice_id_override).await
        } else {
            self.speak(text, voice_id_override).await
        }
    }

    async fn speak_with_engine_and_wait(
        &self,
        text: String,
        engine_id: String,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let _ = cancel;
        self.speak_with_engine(text, engine_id).await
    }

    /// Resolves on the request's one terminal outcome; a cancel ends the speech wherever it is.
    async fn speak_for_show(
        &self,
        speech: ShowSpeech,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let _ = (speech, cancel);
        Err(SpeakDispatchError::Dispatch(
            SHOW_SPEECH_UNAVAILABLE.to_owned(),
        ))
    }

    /// Stop the active item; the queue then advances to the next.
    async fn stop_current(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn pause(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    async fn resume(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    /// Skip the active item; the queue then advances to the next.
    async fn skip_current(&self) -> Result<(), SpeakDispatchError> {
        Ok(())
    }

    /// Drop pending items but let the in-flight item finish (unlike a full clear).
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

    /// Repoint an existing viewer's alias; no-op when the viewer has no alias.
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
