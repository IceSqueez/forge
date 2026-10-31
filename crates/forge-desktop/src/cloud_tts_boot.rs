use std::sync::Arc;

use forge_speak_queue::EngineRegistration;
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
    registry: &mut TtsRegistry,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    try_register_azure(registry, creds, gateways).await;
    try_register_elevenlabs(registry, creds, gateways).await;
    try_register_openai(registry, creds, gateways).await;
    try_register_polly(registry, creds, gateways).await;
}

pub(crate) fn azure_registration(
    creds: AzureCredentials,
    gateways: &CloudGateways,
) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_AZURE.into()),
        factory: Arc::new(AzureEngineFactory::new(creds, gateways)),
    }
}

pub(crate) fn elevenlabs_registration(
    creds: ElevenLabsCredentials,
    gateways: &CloudGateways,
) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_ELEVENLABS.into()),
        factory: Arc::new(ElevenLabsEngineFactory::new(creds, gateways)),
    }
}

pub(crate) fn openai_registration(
    creds: OpenAiCredentials,
    gateways: &CloudGateways,
) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_OPENAI.into()),
        factory: Arc::new(OpenAiEngineFactory::new(creds, gateways)),
    }
}

pub(crate) fn polly_registration(
    creds: PollyCredentials,
    gateways: &CloudGateways,
) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_POLLY.into()),
        factory: Arc::new(PollyEngineFactory::new(creds, gateways)),
    }
}

async fn try_register_azure(
    registry: &mut TtsRegistry,
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
    let registration = azure_registration(azure_creds, gateways);
    registry.register(registration.id, registration.factory);
    tracing::info!(engine = "Azure", "registered cloud TTS engine");
}

async fn try_register_elevenlabs(
    registry: &mut TtsRegistry,
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
    let registration = elevenlabs_registration(el_creds, gateways);
    registry.register(registration.id, registration.factory);
    tracing::info!(engine = "ElevenLabs", "registered cloud TTS engine");
}

async fn try_register_openai(
    registry: &mut TtsRegistry,
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
    let registration = openai_registration(oa_creds, gateways);
    registry.register(registration.id, registration.factory);
    tracing::info!(engine = "OpenAI", "registered cloud TTS engine");
}

async fn try_register_polly(
    registry: &mut TtsRegistry,
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
    let registration = polly_registration(polly_creds, gateways);
    registry.register(registration.id, registration.factory);
    tracing::info!(engine = "Polly", "registered cloud TTS engine");
}
