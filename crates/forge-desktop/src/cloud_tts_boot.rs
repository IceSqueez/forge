use std::sync::{Arc, RwLock};

use forge_speech_types::EngineId;
use forge_storage::{CredentialId, CredentialsRepo};
use forge_tts_cloud::CloudGateways;
use forge_tts_cloud::azure::AzureEngineFactory;
use forge_tts_cloud::credentials::{
    AZURE_CREDENTIAL_ID, AzureCredentials, ELEVENLABS_CREDENTIAL_ID, ElevenLabsCredentials,
    OPENAI_CREDENTIAL_ID, OpenAiCredentials, POLLY_CREDENTIAL_ID, PollyCredentials,
};
use forge_tts_cloud::elevenlabs::ElevenLabsEngineFactory;
use forge_tts_cloud::openai::OpenAiEngineFactory;
use forge_tts_cloud::polly::PollyEngineFactory;
use forge_tts_core::TtsRegistry;

pub(crate) async fn register_cloud_engines(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    try_register_azure(registry, creds, gateways).await;
    try_register_elevenlabs(registry, creds, gateways).await;
    try_register_openai(registry, creds, gateways).await;
    try_register_polly(registry, creds, gateways).await;
}

pub(crate) fn register_azure(
    registry: &RwLock<TtsRegistry>,
    creds: AzureCredentials,
    gateways: &CloudGateways,
) -> EngineId {
    let id = EngineId(forge_tts_core::ENGINE_ID_AZURE.into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(
            id.clone(),
            Arc::new(AzureEngineFactory::new(creds, gateways)),
        );
    id
}

pub(crate) fn register_elevenlabs(
    registry: &RwLock<TtsRegistry>,
    creds: ElevenLabsCredentials,
    gateways: &CloudGateways,
) -> EngineId {
    let id = EngineId(forge_tts_core::ENGINE_ID_ELEVENLABS.into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(
            id.clone(),
            Arc::new(ElevenLabsEngineFactory::new(creds, gateways)),
        );
    id
}

pub(crate) fn register_openai(
    registry: &RwLock<TtsRegistry>,
    creds: OpenAiCredentials,
    gateways: &CloudGateways,
) -> EngineId {
    let id = EngineId(forge_tts_core::ENGINE_ID_OPENAI.into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(
            id.clone(),
            Arc::new(OpenAiEngineFactory::new(creds, gateways)),
        );
    id
}

pub(crate) fn register_polly(
    registry: &RwLock<TtsRegistry>,
    creds: PollyCredentials,
    gateways: &CloudGateways,
) -> EngineId {
    let id = EngineId(forge_tts_core::ENGINE_ID_POLLY.into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(
            id.clone(),
            Arc::new(PollyEngineFactory::new(creds, gateways)),
        );
    id
}

async fn try_register_azure(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    let json = match creds.load(&CredentialId::new(AZURE_CREDENTIAL_ID)).await {
        Ok(Some(j)) => j,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load Azure TTS credentials");
            return;
        }
    };
    let azure_creds: AzureCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed Azure TTS credentials");
            return;
        }
    };
    register_azure(registry, azure_creds, gateways);
    tracing::info!(engine = "Azure", "registered cloud TTS engine");
}

async fn try_register_elevenlabs(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    let json = match creds
        .load(&CredentialId::new(ELEVENLABS_CREDENTIAL_ID))
        .await
    {
        Ok(Some(j)) => j,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load ElevenLabs TTS credentials");
            return;
        }
    };
    let el_creds: ElevenLabsCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed ElevenLabs TTS credentials");
            return;
        }
    };
    register_elevenlabs(registry, el_creds, gateways);
    tracing::info!(engine = "ElevenLabs", "registered cloud TTS engine");
}

async fn try_register_openai(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    let json = match creds.load(&CredentialId::new(OPENAI_CREDENTIAL_ID)).await {
        Ok(Some(j)) => j,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load OpenAI TTS credentials");
            return;
        }
    };
    let oa_creds: OpenAiCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed OpenAI TTS credentials");
            return;
        }
    };
    register_openai(registry, oa_creds, gateways);
    tracing::info!(engine = "OpenAI", "registered cloud TTS engine");
}

async fn try_register_polly(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    let json = match creds.load(&CredentialId::new(POLLY_CREDENTIAL_ID)).await {
        Ok(Some(j)) => j,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load Polly TTS credentials");
            return;
        }
    };
    let polly_creds: PollyCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "malformed Polly TTS credentials");
            return;
        }
    };
    register_polly(registry, polly_creds, gateways);
    tracing::info!(engine = "Polly", "registered cloud TTS engine");
}
