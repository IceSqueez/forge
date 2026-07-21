use serde::{Deserialize, Serialize};

pub const AZURE_CREDENTIAL_ID: &str = "audio:azure";
pub const ELEVENLABS_CREDENTIAL_ID: &str = "audio:elevenlabs";
pub const OPENAI_CREDENTIAL_ID: &str = "audio:openai";
pub const POLLY_CREDENTIAL_ID: &str = "audio:polly";

#[derive(Clone, Serialize, Deserialize)]
pub struct AzureCredentials {
    pub api_key: String,
    pub region: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl std::fmt::Debug for AzureCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureCredentials")
            .field("api_key", &"***")
            .field("region", &self.region)
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ElevenLabsCredentials {
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl std::fmt::Debug for ElevenLabsCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElevenLabsCredentials")
            .field("api_key", &"***")
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OpenAiCredentials {
    pub api_key: String,
    pub base_url: Option<String>,
}

impl std::fmt::Debug for OpenAiCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCredentials")
            .field("api_key", &"***")
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PollyCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl std::fmt::Debug for PollyCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PollyCredentials")
            .field("access_key_id", &"***")
            .field("secret_access_key", &"***")
            .field("region", &self.region)
            .field("base_url", &self.base_url)
            .finish()
    }
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
            base_url: None,
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: AzureCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_key, creds.api_key);
        assert_eq!(back.region, creds.region);
        assert!(back.base_url.is_none());
    }

    #[test]
    fn azure_base_url_roundtrip() {
        let creds = AzureCredentials {
            api_key: "key456".into(),
            region: "westus".into(),
            base_url: Some("http://localhost:8080".into()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: AzureCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.base_url, Some("http://localhost:8080".into()));
    }

    #[test]
    fn azure_base_url_absent_deserializes_as_none() {
        let json = r#"{"api_key":"k","region":"r"}"#;
        let back: AzureCredentials = serde_json::from_str(json).unwrap();
        assert!(back.base_url.is_none());
    }

    #[test]
    fn elevenlabs_serde_roundtrip() {
        let creds = ElevenLabsCredentials {
            api_key: "xi-key".into(),
            base_url: None,
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
            base_url: None,
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: PollyCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_key_id, creds.access_key_id);
        assert_eq!(back.region, creds.region);
        assert!(back.base_url.is_none());
    }

    #[test]
    fn polly_base_url_roundtrip() {
        let creds = PollyCredentials {
            access_key_id: "AKID".into(),
            secret_access_key: "secret".into(),
            region: "us-east-1".into(),
            base_url: Some("http://localhost:9000".into()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: PollyCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.base_url, Some("http://localhost:9000".into()));
    }

    #[test]
    fn polly_base_url_absent_deserializes_as_none() {
        let json = r#"{"access_key_id":"A","secret_access_key":"s","region":"us-east-1"}"#;
        let back: PollyCredentials = serde_json::from_str(json).unwrap();
        assert!(back.base_url.is_none());
    }

    #[test]
    fn azure_debug_redacts_api_key() {
        let creds = AzureCredentials {
            api_key: "super-secret-key".into(),
            region: "eastus".into(),
            base_url: None,
        };
        let s = format!("{creds:?}");
        assert!(!s.contains("super-secret-key"));
        assert!(s.contains("***"));
        assert!(s.contains("eastus"));
    }

    #[test]
    fn elevenlabs_debug_redacts_api_key() {
        let creds = ElevenLabsCredentials {
            api_key: "xi-secret-key".into(),
            base_url: None,
        };
        let s = format!("{creds:?}");
        assert!(!s.contains("xi-secret-key"));
        assert!(s.contains("***"));
    }

    #[test]
    fn openai_debug_redacts_api_key() {
        let creds = OpenAiCredentials {
            api_key: "sk-secret-key".into(),
            base_url: None,
        };
        let s = format!("{creds:?}");
        assert!(!s.contains("sk-secret-key"));
        assert!(s.contains("***"));
    }

    #[test]
    fn polly_debug_redacts_both_keys() {
        let creds = PollyCredentials {
            access_key_id: "AKIASECRETID".into(),
            secret_access_key: "very-secret-access-key".into(),
            region: "us-east-1".into(),
            base_url: None,
        };
        let s = format!("{creds:?}");
        assert!(!s.contains("AKIASECRETID"));
        assert!(!s.contains("very-secret-access-key"));
        assert!(s.contains("***"));
        assert!(s.contains("us-east-1"));
    }
}
