use std::fmt;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthColor {
    Ok,
    Warn,
    Crit,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetric {
    pub label: String,
    pub value: String,
    pub sublabel: Option<String>,
    pub color: HealthColor,
}

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
    /// Exactly 4 metrics shown as the status grid on the integration detail page.
    fn metrics(&self) -> [HealthMetric; 4];
}

#[async_trait]
pub trait IntegrationCatalog: Send + Sync {
    async fn primary_list(&self) -> Result<Vec<CatalogEntry>, PlatformError>;
    async fn secondary_list(&self) -> Result<Option<Vec<CatalogEntry>>, PlatformError>;
}

pub trait QuickActions: Send + Sync {
    /// Up to 4 quick-action buttons pre-filled with SubAction config.
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
            value: "Connected".to_owned(),
            sublabel: Some("IRC".to_owned()),
            color: HealthColor::Ok,
        };
        let json = serde_json::to_string(&metric).unwrap();
        let back: HealthMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label, metric.label);
        assert_eq!(back.value, metric.value);
        assert_eq!(back.sublabel, metric.sublabel);
    }

    #[test]
    fn health_color_serde_snake_case() {
        assert_eq!(serde_json::to_string(&HealthColor::Ok).unwrap(), r#""ok""#);
        assert_eq!(
            serde_json::to_string(&HealthColor::Warn).unwrap(),
            r#""warn""#
        );
        assert_eq!(
            serde_json::to_string(&HealthColor::Crit).unwrap(),
            r#""crit""#
        );
        assert_eq!(
            serde_json::to_string(&HealthColor::Neutral).unwrap(),
            r#""neutral""#
        );
        let back: HealthColor = serde_json::from_str(r#""warn""#).unwrap();
        assert_eq!(back, HealthColor::Warn);
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
