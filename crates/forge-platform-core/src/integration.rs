use std::fmt;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{ConnectionState, PlatformError};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntegrationId(pub String);

impl IntegrationId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IntegrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HealthValue {
    Status {
        label: String,
        active: bool,
    },
    Text {
        primary: String,
        secondary: Option<String>,
    },
    Pair {
        left: String,
        right: String,
    },
    Ratio {
        used: u64,
        total: u64,
        reset_hint: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthMetric {
    pub label: String,
    pub value: HealthValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDelta {
    pub index: u8,
    pub new_value: HealthValue,
}

pub type HealthStream = Pin<Box<dyn futures_core::Stream<Item = HealthDelta> + Send + 'static>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub label: String,
    pub sublabel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub limited: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderAction {
    Reconnect,
    RefreshToken,
    Disconnect,
    Settings,
}

pub trait IntegrationStatus: Send + Sync {
    fn id(&self) -> &IntegrationId;
    fn display_name(&self) -> &str;
    fn version(&self) -> Option<&str>;
    fn connection(&self) -> ConnectionState;
    fn uptime(&self) -> Option<Duration>;
    fn endpoint(&self) -> Option<&str>;
    fn capability_flags(&self) -> CapabilityFlags;
    fn header_actions(&self) -> Vec<HeaderAction>;
}

pub trait IntegrationHealth: Send + Sync {
    fn metrics(&self) -> [HealthMetric; 4];
    fn stream(&self) -> HealthStream;
}

#[async_trait]
pub trait IntegrationCatalog: Send + Sync {
    async fn primary_list(&self) -> Result<Vec<CatalogEntry>, PlatformError>;
    async fn secondary_list(&self) -> Result<Option<Vec<CatalogEntry>>, PlatformError>;
}

pub trait QuickActions: Send + Sync {
    fn actions(&self) -> Vec<QuickAction>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn integration_id_new_and_as_str() {
        let id = IntegrationId::new("twitch");
        assert_eq!(id.as_str(), "twitch");
    }

    #[test]
    fn integration_id_display() {
        let id = IntegrationId::new("youtube");
        assert_eq!(id.to_string(), "youtube");
    }

    #[test]
    fn integration_id_serde_transparent() {
        let id = IntegrationId::new("kick");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""kick""#);
        let back: IntegrationId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn health_metric_serde_roundtrip() {
        let metric = HealthMetric {
            label: "Chat".to_owned(),
            value: HealthValue::Status {
                label: "Connected".to_owned(),
                active: true,
            },
        };
        let json = serde_json::to_string(&metric).unwrap();
        let back: HealthMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(back, metric);
    }

    #[test]
    fn health_value_status_serde() {
        let v = HealthValue::Status {
            label: "Connected".to_owned(),
            active: true,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: HealthValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn health_value_text_serde() {
        let v = HealthValue::Text {
            primary: "42 msg/s".to_owned(),
            secondary: Some("peak: 150".to_owned()),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: HealthValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn health_value_text_no_secondary_serde() {
        let v = HealthValue::Text {
            primary: "idle".to_owned(),
            secondary: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: HealthValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn health_value_pair_serde() {
        let v = HealthValue::Pair {
            left: "60 fps".to_owned(),
            right: "2.4%".to_owned(),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: HealthValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn health_value_ratio_serde() {
        let v = HealthValue::Ratio {
            used: 800,
            total: 1000,
            reset_hint: Some("resets hourly".to_owned()),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: HealthValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn health_value_ratio_no_hint_serde() {
        let v = HealthValue::Ratio {
            used: 0,
            total: 100,
            reset_hint: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: HealthValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn health_delta_serde_roundtrip() {
        let delta = HealthDelta {
            index: 2,
            new_value: HealthValue::Ratio {
                used: 450,
                total: 800,
                reset_hint: None,
            },
        };
        let json = serde_json::to_string(&delta).unwrap();
        let back: HealthDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.index, delta.index);
        assert_eq!(back.new_value, delta.new_value);
    }

    #[test]
    fn catalog_entry_serde_with_sublabel() {
        let entry = CatalogEntry {
            id: "scene-1".to_owned(),
            label: "Main Scene".to_owned(),
            sublabel: Some("720p".to_owned()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: CatalogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, entry.id);
        assert_eq!(back.sublabel, Some("720p".to_owned()));
    }

    #[test]
    fn catalog_entry_serde_without_sublabel() {
        let entry = CatalogEntry {
            id: "scene-2".to_owned(),
            label: "Offline Scene".to_owned(),
            sublabel: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: CatalogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sublabel, None);
    }

    #[test]
    fn quick_action_serde_roundtrip() {
        let action = QuickAction {
            id: "switch-scene".to_owned(),
            label: "Switch to Main".to_owned(),
            icon: Some("play".to_owned()),
            payload: serde_json::json!({ "scene": "Main Scene" }),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: QuickAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, action.id);
        assert_eq!(back.icon, action.icon);
        assert_eq!(back.payload["scene"], "Main Scene");
    }

    #[test]
    fn capability_flags_serde_with_label() {
        let flags = CapabilityFlags {
            limited: true,
            label: Some("read-only".to_owned()),
        };
        let json = serde_json::to_string(&flags).unwrap();
        let back: CapabilityFlags = serde_json::from_str(&json).unwrap();
        assert_eq!(back, flags);
    }

    #[test]
    fn capability_flags_serde_without_label() {
        let flags = CapabilityFlags {
            limited: false,
            label: None,
        };
        let json = serde_json::to_string(&flags).unwrap();
        let back: CapabilityFlags = serde_json::from_str(&json).unwrap();
        assert_eq!(back, flags);
    }

    #[test]
    fn header_action_serde_variants() {
        let cases = [
            (HeaderAction::Reconnect, r#""reconnect""#),
            (HeaderAction::RefreshToken, r#""refresh_token""#),
            (HeaderAction::Disconnect, r#""disconnect""#),
            (HeaderAction::Settings, r#""settings""#),
        ];
        for (action, expected_json) in &cases {
            let json = serde_json::to_string(action).unwrap();
            assert_eq!(json, *expected_json);
            let back: HeaderAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *action);
        }
    }

    #[allow(dead_code)]
    fn dyn_all(
        _: &dyn IntegrationStatus,
        _: &dyn IntegrationHealth,
        _: &dyn IntegrationCatalog,
        _: &dyn QuickActions,
    ) {
    }
}
