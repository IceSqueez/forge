use std::sync::Arc;

use async_trait::async_trait;
use forge_runtime::{SpeakDispatchError, SpeakDispatcher};
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_voice::AliasId;

pub struct SpeakBridge {
    handle: Arc<SpeakQueueHandle>,
}

impl SpeakBridge {
    pub fn new(handle: Arc<SpeakQueueHandle>) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl SpeakDispatcher for SpeakBridge {
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
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
            .map_err(|e| SpeakDispatchError::Dispatch(e.to_string()))
    }
}
