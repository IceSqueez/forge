use std::sync::{Arc, RwLock};

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
use forge_tts_core::{EngineId, TtsRegistry};

pub async fn register_cloud_engines(
    registry: &RwLock<TtsRegistry>,
    creds: &dyn CredentialsRepo,
    gateways: &CloudGateways,
) {
    try_register_azure(registry, creds, gateways).await;
    try_register_elevenlabs(registry, creds, gateways).await;
    try_register_openai(registry, creds, gateways).await;
    try_register_polly(registry, creds, gateways).await;
}

pub fn register_azure(
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

pub fn register_elevenlabs(
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

pub fn register_openai(
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

pub fn register_polly(
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
            eprintln!("forge-desktop: failed to load Azure TTS credentials: {e}");
            return;
        }
    };
    let azure_creds: AzureCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("forge-desktop: malformed Azure TTS credentials: {e}");
            return;
        }
    };
    register_azure(registry, azure_creds, gateways);
    eprintln!("forge-desktop: registered Azure TTS engine");
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
            eprintln!("forge-desktop: failed to load ElevenLabs TTS credentials: {e}");
            return;
        }
    };
    let el_creds: ElevenLabsCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("forge-desktop: malformed ElevenLabs TTS credentials: {e}");
            return;
        }
    };
    register_elevenlabs(registry, el_creds, gateways);
    eprintln!("forge-desktop: registered ElevenLabs TTS engine");
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
            eprintln!("forge-desktop: failed to load OpenAI TTS credentials: {e}");
            return;
        }
    };
    let oa_creds: OpenAiCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("forge-desktop: malformed OpenAI TTS credentials: {e}");
            return;
        }
    };
    register_openai(registry, oa_creds, gateways);
    eprintln!("forge-desktop: registered OpenAI TTS engine");
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
            eprintln!("forge-desktop: failed to load Polly TTS credentials: {e}");
            return;
        }
    };
    let polly_creds: PollyCredentials = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("forge-desktop: malformed Polly TTS credentials: {e}");
            return;
        }
    };
    register_polly(registry, polly_creds, gateways);
    eprintln!("forge-desktop: registered Polly TTS engine");
}
