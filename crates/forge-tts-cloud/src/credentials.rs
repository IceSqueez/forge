use forge_tts_core::EngineId;
use serde::{Deserialize, Serialize};

pub const AZURE_CREDENTIAL_ID: &str = "audio:azure";
pub const ELEVENLABS_CREDENTIAL_ID: &str = "audio:elevenlabs";
pub const OPENAI_CREDENTIAL_ID: &str = "audio:openai";
pub const POLLY_CREDENTIAL_ID: &str = "audio:polly";

/// Returns the `EngineId` string for a given credential ID, or `None` if unknown.
pub fn engine_id_for_credential(credential_id: &str) -> Option<EngineId> {
    match credential_id {
        AZURE_CREDENTIAL_ID => Some(EngineId("azure".into())),
        ELEVENLABS_CREDENTIAL_ID => Some(EngineId("elevenlabs".into())),
        OPENAI_CREDENTIAL_ID => Some(EngineId("openai".into())),
        POLLY_CREDENTIAL_ID => Some(EngineId("polly".into())),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureCredentials {
    pub api_key: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevenLabsCredentials {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCredentials {
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollyCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn azure_serde_roundtrip() {
        let creds = AzureCredentials {
            api_key: "key123".into(),
            region: "eastus".into(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: AzureCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_key, creds.api_key);
        assert_eq!(back.region, creds.region);
    }

    #[test]
    fn elevenlabs_serde_roundtrip() {
        let creds = ElevenLabsCredentials {
            api_key: "xi-key".into(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: ElevenLabsCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_key, creds.api_key);
    }

    #[test]
    fn openai_serde_roundtrip() {
        let creds = OpenAiCredentials {
            api_key: "sk-xxx".into(),
            base_url: Some("https://custom.openai.example.com/v1".into()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: OpenAiCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_key, creds.api_key);
        assert_eq!(back.base_url, creds.base_url);
    }

    #[test]
    fn openai_base_url_optional_absent() {
        let creds = OpenAiCredentials {
            api_key: "sk-xxx".into(),
            base_url: None,
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: OpenAiCredentials = serde_json::from_str(&json).unwrap();
        assert!(back.base_url.is_none());
    }

    #[test]
    fn polly_serde_roundtrip() {
        let creds = PollyCredentials {
            access_key_id: "AKID".into(),
            secret_access_key: "secret".into(),
            region: "us-east-1".into(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: PollyCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_key_id, creds.access_key_id);
        assert_eq!(back.region, creds.region);
    }

    #[test]
    fn engine_id_for_known_credentials() {
        assert_eq!(
            engine_id_for_credential(AZURE_CREDENTIAL_ID),
            Some(EngineId("azure".into()))
        );
        assert_eq!(
            engine_id_for_credential(POLLY_CREDENTIAL_ID),
            Some(EngineId("polly".into()))
        );
        assert_eq!(engine_id_for_credential("audio:unknown"), None);
    }
}
