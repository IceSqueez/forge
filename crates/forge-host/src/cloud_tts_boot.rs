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
use serde::de::DeserializeOwned;

pub async fn register_cloud_engines(
    registry: &mut TtsRegistry,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    if let Some(stored) = load_engine_credentials(creds, AZURE_CREDENTIAL_ID, "Azure").await {
        install(registry, azure_registration(stored, gateways), "Azure");
    }
    if let Some(stored) =
        load_engine_credentials(creds, ELEVENLABS_CREDENTIAL_ID, "ElevenLabs").await
    {
        install(
            registry,
            elevenlabs_registration(stored, gateways),
            "ElevenLabs",
        );
    }
    if let Some(stored) = load_engine_credentials(creds, OPENAI_CREDENTIAL_ID, "OpenAI").await {
        install(registry, openai_registration(stored, gateways), "OpenAI");
    }
    if let Some(stored) = load_engine_credentials(creds, POLLY_CREDENTIAL_ID, "Polly").await {
        install(registry, polly_registration(stored, gateways), "Polly");
    }
}

pub fn azure_registration(creds: AzureCredentials, gateways: &CloudGateways) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_AZURE.into()),
        factory: Arc::new(AzureEngineFactory::new(creds, gateways)),
    }
}

pub fn elevenlabs_registration(
    creds: ElevenLabsCredentials,
    gateways: &CloudGateways,
) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_ELEVENLABS.into()),
        factory: Arc::new(ElevenLabsEngineFactory::new(creds, gateways)),
    }
}

pub fn openai_registration(
    creds: OpenAiCredentials,
    gateways: &CloudGateways,
) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_OPENAI.into()),
        factory: Arc::new(OpenAiEngineFactory::new(creds, gateways)),
    }
}

pub fn polly_registration(creds: PollyCredentials, gateways: &CloudGateways) -> EngineRegistration {
    EngineRegistration {
        id: EngineId(forge_tts_core::ENGINE_ID_POLLY.into()),
        factory: Arc::new(PollyEngineFactory::new(creds, gateways)),
    }
}

async fn load_engine_credentials<T: DeserializeOwned>(
    creds: &dyn CredentialsRepo,
    id: &str,
    engine: &str,
) -> Option<T> {
    let json = match creds.load(&CredentialId::new(id)).await {
        Ok(json) => json?,
        Err(error) => {
            tracing::warn!(engine, %error, "failed to load cloud TTS credentials");
            return None;
        }
    };
    serde_json::from_str(&json)
        .inspect_err(|error| tracing::warn!(engine, %error, "malformed cloud TTS credentials"))
        .ok()
}

fn install(registry: &mut TtsRegistry, registration: EngineRegistration, engine: &str) {
    registry.register(registration.id, registration.factory);
    tracing::info!(engine, "registered cloud TTS engine");
}
