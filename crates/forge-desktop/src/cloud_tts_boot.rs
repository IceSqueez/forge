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

/// Registers every cloud engine whose credentials are present in `creds` into
/// `registry`, skipping (with a log line) any engine whose credentials are absent or
/// unparseable. Voice-catalog / roster exposure to the UI is out of scope here.
pub async fn register_cloud_engines(registry: &RwLock<TtsRegistry>, creds: &dyn CredentialsRepo) {
    try_register_azure(registry, creds).await;
    try_register_elevenlabs(registry, creds).await;
    try_register_openai(registry, creds).await;
    try_register_polly(registry, creds).await;
}

pub fn register_azure(registry: &RwLock<TtsRegistry>, creds: AzureCredentials) -> EngineId {
    let id = EngineId("azure".into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(AzureEngineFactory::new(creds)));
    id
}

pub fn register_elevenlabs(
    registry: &RwLock<TtsRegistry>,
    creds: ElevenLabsCredentials,
) -> EngineId {
    let id = EngineId("elevenlabs".into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(ElevenLabsEngineFactory::new(creds)));
    id
}

pub fn register_openai(registry: &RwLock<TtsRegistry>, creds: OpenAiCredentials) -> EngineId {
    let id = EngineId("openai".into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(OpenAiEngineFactory::new(creds)));
    id
}

pub fn register_polly(registry: &RwLock<TtsRegistry>, creds: PollyCredentials) -> EngineId {
    let id = EngineId("polly".into());
    registry
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), Arc::new(PollyEngineFactory::new(creds)));
    id
}

async fn try_register_azure(registry: &RwLock<TtsRegistry>, creds: &dyn CredentialsRepo) {
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
    register_azure(registry, azure_creds);
    eprintln!("forge-desktop: registered Azure TTS engine");
}

async fn try_register_elevenlabs(registry: &RwLock<TtsRegistry>, creds: &dyn CredentialsRepo) {
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
    register_elevenlabs(registry, el_creds);
    eprintln!("forge-desktop: registered ElevenLabs TTS engine");
}

async fn try_register_openai(registry: &RwLock<TtsRegistry>, creds: &dyn CredentialsRepo) {
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
    register_openai(registry, oa_creds);
    eprintln!("forge-desktop: registered OpenAI TTS engine");
}

async fn try_register_polly(registry: &RwLock<TtsRegistry>, creds: &dyn CredentialsRepo) {
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
    register_polly(registry, polly_creds);
    eprintln!("forge-desktop: registered Polly TTS engine");
}
