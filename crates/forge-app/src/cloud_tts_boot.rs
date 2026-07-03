use std::sync::{Arc, RwLock};

use forge_storage::{CredentialId, CredentialsRepo};
use forge_tts_cloud::azure::AzureEngineFactory;
use forge_tts_cloud::credentials::{
    AZURE_CREDENTIAL_ID, AzureCredentials, ELEVENLABS_CREDENTIAL_ID, ElevenLabsCredentials,
    OPENAI_CREDENTIAL_ID, OpenAiCredentials, POLLY_CREDENTIAL_ID, PollyCredentials,
};
use forge_tts_cloud::elevenlabs::ElevenLabsEngineFactory;
use forge_tts_cloud::openai::OpenAiEngineFactory;
use forge_tts_cloud::polly::PollyEngineFactory;
use forge_tts_core::{EngineId, TtsRegistry};

use crate::message::CloudEngineKind;

pub async fn register_cloud_engines(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
) -> Vec<EngineId> {
    let mut registered = Vec::new();
    if let Some(id) = try_register_azure(registry, creds).await {
        registered.push(id);
    }
    if let Some(id) = try_register_elevenlabs(registry, creds).await {
        registered.push(id);
    }
    if let Some(id) = try_register_openai(registry, creds).await {
        registered.push(id);
    }
    if let Some(id) = try_register_polly(registry, creds).await {
        registered.push(id);
    }
    registered
}

/// Registers Azure into `registry` under the shared `azure` `EngineId`. Callable
/// both from boot load and from the Cloud Engines save flow (hot-register, no
/// restart) so both paths agree on the id and factory construction.
pub fn register_azure(registry: &RwLock<TtsRegistry>, creds: AzureCredentials) -> EngineId {
    let id = CloudEngineKind::Azure.engine_id();
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(AzureEngineFactory::new(creds)));
    id
}

/// Registers ElevenLabs into `registry` under the shared `elevenlabs` `EngineId`.
pub fn register_elevenlabs(
    registry: &RwLock<TtsRegistry>,
    creds: ElevenLabsCredentials,
) -> EngineId {
    let id = CloudEngineKind::ElevenLabs.engine_id();
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(ElevenLabsEngineFactory::new(creds)));
    id
}

/// Registers OpenAI into `registry` under the shared `openai` `EngineId`.
pub fn register_openai(registry: &RwLock<TtsRegistry>, creds: OpenAiCredentials) -> EngineId {
    let id = CloudEngineKind::OpenAI.engine_id();
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(OpenAiEngineFactory::new(creds)));
    id
}

/// Registers Polly into `registry` under the shared `polly` `EngineId`.
pub fn register_polly(registry: &RwLock<TtsRegistry>, creds: PollyCredentials) -> EngineId {
    let id = CloudEngineKind::Polly.engine_id();
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(PollyEngineFactory::new(creds)));
    id
}

async fn try_register_azure(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
) -> Option<EngineId> {
    let json = match creds.load(&CredentialId::new(AZURE_CREDENTIAL_ID)).await {
        Ok(Some(j)) => j,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load Azure TTS credentials");
            return None;
        }
    };
    let azure_creds: AzureCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed Azure TTS credentials");
            return None;
        }
    };
    let id = register_azure(registry, azure_creds);
    tracing::info!("registered Azure TTS engine");
    Some(id)
}

async fn try_register_elevenlabs(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
) -> Option<EngineId> {
    let json = match creds
        .load(&CredentialId::new(ELEVENLABS_CREDENTIAL_ID))
        .await
    {
        Ok(Some(j)) => j,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load ElevenLabs TTS credentials");
            return None;
        }
    };
    let el_creds: ElevenLabsCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed ElevenLabs TTS credentials");
            return None;
        }
    };
    let id = register_elevenlabs(registry, el_creds);
    tracing::info!("registered ElevenLabs TTS engine");
    Some(id)
}

async fn try_register_openai(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
) -> Option<EngineId> {
    let json = match creds.load(&CredentialId::new(OPENAI_CREDENTIAL_ID)).await {
        Ok(Some(j)) => j,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load OpenAI TTS credentials");
            return None;
        }
    };
    let oa_creds: OpenAiCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed OpenAI TTS credentials");
            return None;
        }
    };
    let id = register_openai(registry, oa_creds);
    tracing::info!("registered OpenAI TTS engine");
    Some(id)
}

async fn try_register_polly(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
) -> Option<EngineId> {
    let json = match creds.load(&CredentialId::new(POLLY_CREDENTIAL_ID)).await {
        Ok(Some(j)) => j,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load Polly TTS credentials");
            return None;
        }
    };
    let polly_creds: PollyCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed Polly TTS credentials");
            return None;
        }
    };
    let id = register_polly(registry, polly_creds);
    tracing::info!("registered Polly TTS engine");
    Some(id)
}
