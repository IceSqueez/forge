use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::CancelSignal;
use forge_runtime::{SpeakDispatchError, SpeakDispatcher, VoiceDescriptor};
use forge_script::SpeakRequester;
use forge_speak_queue::{
    Priority, RequestId, SpeakCommand, SpeakEvent, SpeakQueueHandle, SpeakRequest,
};
use forge_tts_core::{EngineId, VoiceId};
use forge_voice::{AliasId, AliasState, VoiceAlias};

const SPEAK_WAIT_HARD_CAP: Duration = Duration::from_secs(600);
const SPEAK_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct SpeakBridge {
    handle: Arc<SpeakQueueHandle>,
}

impl SpeakBridge {
    pub fn new(handle: Arc<SpeakQueueHandle>) -> Self {
        Self { handle }
    }

    #[allow(clippy::too_many_arguments)]
    async fn enqueue(
        &self,
        text: String,
        alias_override: Option<AliasId>,
        engine_override: Option<EngineId>,
        voice_override: Option<VoiceId>,
        is_reward: bool,
    ) -> Result<RequestId, String> {
        let request_id = RequestId::new();
        let request = SpeakRequest {
            request_id: request_id.clone(),
            viewer_id: "system".to_owned(),
            viewer_name: "Forge".to_owned(),
            text,
            priority: Priority::Normal,
            alias_override,
            engine_override,
            voice_override,
            source_event_id: forge_types::EventId::new(),
            is_reward,
        };
        self.handle
            .send(SpeakCommand::Enqueue(request))
            .await
            .map_err(|e| e.to_string())?;
        Ok(request_id)
    }

    async fn dispatch(&self, cmd: SpeakCommand) -> Result<(), SpeakDispatchError> {
        self.handle
            .send(cmd)
            .await
            .map_err(|e| SpeakDispatchError::Dispatch(e.to_string()))
    }
}

async fn wait_for_terminal(
    events: &mut tokio::sync::broadcast::Receiver<SpeakEvent>,
    request_id: &RequestId,
    cancel: CancelSignal,
) -> Result<(), SpeakDispatchError> {
    let wait = async {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(SPEAK_WAIT_POLL_INTERVAL) => {
                    if cancel.is_cancelled() {
                        return Err(SpeakDispatchError::Dispatch(
                            "speak wait cancelled".to_owned(),
                        ));
                    }
                }
                event = events.recv() => {
                    match event {
                        Ok(SpeakEvent::Finished { request_id: rid }) if &rid == request_id => {
                            return Ok(());
                        }
                        Ok(SpeakEvent::Failed { request_id: rid, error }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(error));
                        }
                        Ok(SpeakEvent::Skipped { request_id: rid, reason }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(format!(
                                "speak skipped: {reason}"
                            )));
                        }
                        Ok(SpeakEvent::Rejected { request_id: rid, reason }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(format!(
                                "speak rejected: {reason}"
                            )));
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            return Err(SpeakDispatchError::Dispatch(
                                "speak event stream closed".to_owned(),
                            ));
                        }
                    }
                }
            }
        }
    };
    match tokio::time::timeout(SPEAK_WAIT_HARD_CAP, wait).await {
        Ok(result) => result,
        Err(_) => Err(SpeakDispatchError::Dispatch(
            "speak wait timed out".to_owned(),
        )),
    }
}

#[async_trait]
impl SpeakDispatcher for SpeakBridge {
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, voice_id_override.map(AliasId), None, None, false)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_reward_sourced(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, voice_id_override.map(AliasId), None, None, true)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_alias(
        &self,
        text: String,
        alias_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, Some(AliasId(alias_id)), None, None, false)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_engine(
        &self,
        text: String,
        engine_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, None, Some(EngineId(engine_id)), None, false)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_voice(
        &self,
        text: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, None, None, Some(VoiceId(voice_id)), false)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_and_wait(
        &self,
        text: String,
        voice_id_override: Option<String>,
        is_reward: bool,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(text, voice_id_override.map(AliasId), None, None, is_reward)
            .await
            .map_err(SpeakDispatchError::Dispatch)?;
        wait_for_terminal(&mut events, &request_id, cancel).await
    }

    async fn speak_with_engine_and_wait(
        &self,
        text: String,
        engine_id: String,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(text, None, Some(EngineId(engine_id)), None, false)
            .await
            .map_err(SpeakDispatchError::Dispatch)?;
        wait_for_terminal(&mut events, &request_id, cancel).await
    }

    async fn stop_current(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Skip).await
    }

    async fn pause(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Pause).await
    }

    async fn resume(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Resume).await
    }

    async fn skip_current(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Skip).await
    }

    async fn clear_keep_current(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::ClearPending).await
    }

    async fn get_queue_depth(&self) -> usize {
        self.handle.queue_depth()
    }

    async fn get_available_voices(&self) -> Vec<VoiceDescriptor> {
        self.handle
            .available_voices()
            .iter()
            .map(|v| VoiceDescriptor {
                id: v.id.0.clone(),
                name: v.name.clone(),
                locale: v.locale.clone(),
                engine_id: v.engine_id.0.clone(),
            })
            .collect()
    }

    async fn get_engines(&self) -> Vec<String> {
        self.handle.engines().into_iter().map(|e| e.0).collect()
    }

    async fn alias_set(
        &self,
        viewer_id: String,
        viewer_name: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let alias = VoiceAlias {
            id: AliasId::new(),
            viewer_id,
            viewer_name,
            engine_id: EngineId(engine_id),
            voice_id: VoiceId(voice_id),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        };
        self.dispatch(SpeakCommand::SetAlias(alias)).await
    }

    async fn alias_switch(
        &self,
        viewer_id: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::SwitchAlias {
            viewer_id,
            engine_id: EngineId(engine_id),
            voice_id: VoiceId(voice_id),
        })
        .await
    }
}

#[async_trait]
impl SpeakRequester for SpeakBridge {
    async fn speak(&self, text: String, voice_id_override: Option<String>) {
        if let Err(e) = self
            .enqueue(text, voice_id_override.map(AliasId), None, None, false)
            .await
        {
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
