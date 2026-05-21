use std::sync::Arc;

use async_trait::async_trait;
use forge_runtime::{SpeakDispatchError, SpeakDispatcher};
use forge_script::SpeakRequester;
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_voice::AliasId;

pub struct SpeakBridge {
    handle: Arc<SpeakQueueHandle>,
}

impl SpeakBridge {
    pub fn new(handle: Arc<SpeakQueueHandle>) -> Self {
        Self { handle }
    }

    async fn enqueue(&self, text: String, voice_id_override: Option<String>) -> Result<(), String> {
        let request = SpeakRequest {
            request_id: RequestId::new(),
            viewer_id: "system".to_owned(),
            viewer_name: "Forge".to_owned(),
            text,
            priority: Priority::Normal,
            alias_override: voice_id_override.map(AliasId),
            source_event_id: forge_types::EventId::new(),
        };
        self.handle
            .send(SpeakCommand::Enqueue(request))
            .await
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl SpeakDispatcher for SpeakBridge {
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, voice_id_override)
            .await
            .map_err(SpeakDispatchError::Dispatch)
    }
}

#[async_trait]
impl SpeakRequester for SpeakBridge {
    async fn speak(&self, text: String, voice_id_override: Option<String>) {
        if let Err(e) = self.enqueue(text, voice_id_override).await {
            tracing::warn!(error = %e, "forge::tts::speak failed");
        }
    }

    async fn skip(&self) {
        if let Err(e) = self.handle.send(SpeakCommand::Skip).await {
            tracing::warn!(error = %e, "forge::tts::skip failed");
        }
    }

    async fn clear(&self) {
        if let Err(e) = self.handle.send(SpeakCommand::Clear).await {
            tracing::warn!(error = %e, "forge::tts::clear failed");
        }
    }
}
