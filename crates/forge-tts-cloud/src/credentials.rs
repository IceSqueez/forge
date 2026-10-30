use forge_types::Secret;
use serde::{Deserialize, Serialize};

pub const AZURE_CREDENTIAL_ID: &str = "audio:azure";
pub const ELEVENLABS_CREDENTIAL_ID: &str = "audio:elevenlabs";
pub const OPENAI_CREDENTIAL_ID: &str = "audio:openai";
pub const POLLY_CREDENTIAL_ID: &str = "audio:polly";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureCredentials {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub api_key: Secret,
    pub region: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevenLabsCredentials {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub api_key: Secret,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCredentials {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub api_key: Secret,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollyCredentials {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub access_key_id: Secret,
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub secret_access_key: Secret,
    pub region: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    fn assert_saved_bytes_survive<T: Serialize + DeserializeOwned>(saved: &str) {
        let loaded: T = serde_json::from_str(saved).expect("a saved bundle loads");

        assert_eq!(serde_json::to_string(&loaded).unwrap(), saved);
    }

    #[test]
    fn saved_bundles_save_back_byte_identical_for_every_cloud_engine() {
        assert_saved_bytes_survive::<AzureCredentials>(
            r#"{"api_key":"az-key","region":"eastus","base_url":null}"#,
        );
        assert_saved_bytes_survive::<AzureCredentials>(
            r#"{"api_key":"az-key","region":"eastus","base_url":"http://192.0.2.1:8080"}"#,
        );
        assert_saved_bytes_survive::<ElevenLabsCredentials>(
            r#"{"api_key":"xi-key","base_url":null}"#,
        );
        assert_saved_bytes_survive::<ElevenLabsCredentials>(
            r#"{"api_key":"xi-key","base_url":"http://192.0.2.1:8080"}"#,
        );
        assert_saved_bytes_survive::<OpenAiCredentials>(r#"{"api_key":"sk-key","base_url":null}"#);
        assert_saved_bytes_survive::<OpenAiCredentials>(
            r#"{"api_key":"sk-key","base_url":"https://custom.example.com/v1"}"#,
        );
        assert_saved_bytes_survive::<PollyCredentials>(
            r#"{"access_key_id":"AKID","secret_access_key":"s3cret","region":"us-east-1","base_url":null}"#,
        );
        assert_saved_bytes_survive::<PollyCredentials>(
            r#"{"access_key_id":"AKID","secret_access_key":"s3cret","region":"us-east-1","base_url":"http://192.0.2.1:9000"}"#,
        );
    }

    #[test]
    fn a_saved_bundle_hands_every_secret_field_back_to_its_engine() {
        let azure: AzureCredentials =
            serde_json::from_str(r#"{"api_key":"az-key","region":"r"}"#).unwrap();
        let eleven: ElevenLabsCredentials =
            serde_json::from_str(r#"{"api_key":"xi-key"}"#).unwrap();
        let openai: OpenAiCredentials = serde_json::from_str(r#"{"api_key":"sk-key"}"#).unwrap();
        let polly: PollyCredentials = serde_json::from_str(
            r#"{"access_key_id":"AKID","secret_access_key":"s3cret","region":"us-east-1"}"#,
        )
        .unwrap();

        assert_eq!(
            [
                azure.api_key.expose(),
                eleven.api_key.expose(),
                openai.api_key.expose(),
                polly.access_key_id.expose(),
                polly.secret_access_key.expose(),
            ],
            ["az-key", "xi-key", "sk-key", "AKID", "s3cret"]
        );
    }

    #[test]
    fn a_bundle_saved_without_a_base_url_loads_as_none() {
        let azure: AzureCredentials =
            serde_json::from_str(r#"{"api_key":"k","region":"r"}"#).unwrap();
        let eleven: ElevenLabsCredentials = serde_json::from_str(r#"{"api_key":"k"}"#).unwrap();
        let openai: OpenAiCredentials = serde_json::from_str(r#"{"api_key":"k"}"#).unwrap();
        let polly: PollyCredentials = serde_json::from_str(
            r#"{"access_key_id":"A","secret_access_key":"s","region":"us-east-1"}"#,
        )
        .unwrap();

        assert_eq!(
            [
                azure.base_url,
                eleven.base_url,
                openai.base_url,
                polly.base_url
            ],
            [None, None, None, None]
        );
    }

    #[test]
    fn debug_of_every_cloud_credential_never_carries_a_key() {
        const KEY: &str = "cloud-key-sentinel";
        let rendered = [
            format!(
                "{:#?}",
                AzureCredentials {
                    api_key: Secret::new(KEY),
                    region: "eastus".into(),
                    base_url: None,
                }
            ),
            format!(
                "{:#?}",
                ElevenLabsCredentials {
                    api_key: Secret::new(KEY),
                    base_url: None,
                }
            ),
            format!(
                "{:#?}",
                OpenAiCredentials {
                    api_key: Secret::new(KEY),
                    base_url: None,
                }
            ),
            format!(
                "{:#?}",
                PollyCredentials {
                    access_key_id: Secret::new(KEY),
                    secret_access_key: Secret::new(KEY),
                    region: "us-east-1".into(),
                    base_url: None,
                }
            ),
        ];

        for debug in rendered {
            assert!(!debug.contains(KEY), "leaked: {debug}");
        }
    }
}
