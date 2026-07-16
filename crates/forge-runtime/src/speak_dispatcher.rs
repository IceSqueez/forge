use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum SpeakDispatchError {
    #[error("{0}")]
    Dispatch(String),
}

/// Plain voice descriptor returned by query methods. Owned strings keep
/// forge-speak-queue / forge-tts-core types out of this crate's surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDescriptor {
    pub id: String,
    pub name: String,
    pub locale: String,
    pub engine_id: String,
}

/// Narrow speak contract used by the action engine and the §10 TTS runners.
///
/// Implemented by the forge-app `SpeakBridge` wrapper around `SpeakQueueHandle`.
/// Keeping the trait here prevents a dependency cycle: `forge-runtime` never imports
/// `forge-speak-queue`.
///
/// Every method except `speak` has a default impl: controls are no-ops returning
/// `Ok`, queries return empty/zero. Only the forge-app bridge wires them to a live
/// queue - any other impl (test doubles included) is inert without overriding them.
#[async_trait]
pub trait SpeakDispatcher: Send + Sync {
    /// Enqueue a speak request with an optional raw voice-ID override string.
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

    /// Enqueue a speak request that originated from a channel-points reward
    /// redemption, so the pipeline's `strip_reward_emotes` gate sees it as
    /// reward-sourced. Defaults to plain `speak` for implementers that don't
    /// need reward-specific gating (test doubles, the rhai bridge).
    async fn speak_reward_sourced(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.speak(text, voice_id_override).await
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
