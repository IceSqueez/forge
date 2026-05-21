use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum SpeakDispatchError {
    #[error("{0}")]
    Dispatch(String),
}

/// Narrow speak contract used by the action engine.
///
/// Implemented by the forge-app `SpeakBridge` wrapper around `SpeakQueueHandle`.
/// Keeping the trait here prevents a dependency cycle: `forge-runtime` never imports
/// `forge-speak-queue`.
#[async_trait]
pub trait SpeakDispatcher: Send + Sync {
    /// Enqueue a speak request with an optional raw voice-ID override string.
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError>;
}
