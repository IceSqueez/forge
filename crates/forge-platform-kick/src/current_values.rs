use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{BuiltinCurrentValues, QuickActionFieldValue};

use crate::api::channel::{ChannelSnapshot, KickChannel};
use crate::auth::credentials_manager::KickCredentialsManager;
use crate::vocabulary::config_keys;

pub struct KickCurrentValues {
    channel: Arc<KickChannel>,
    credentials_manager: Arc<KickCredentialsManager>,
}

impl KickCurrentValues {
    pub fn new(
        channel: Arc<KickChannel>,
        credentials_manager: Arc<KickCredentialsManager>,
    ) -> Self {
        Self {
            channel,
            credentials_manager,
        }
    }

    async fn snapshot(&self) -> Option<ChannelSnapshot> {
        let token = match self.credentials_manager.get_valid_access_token().await {
            Ok(token) => token,
            Err(error) => {
                tracing::warn!(%error, "kick token unavailable for current values");
                return None;
            }
        };
        match self.channel.get_channel(&token).await {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::warn!(%error, "kick channel read failed for current values");
                None
            }
        }
    }
}

fn value_of(key: &str, snapshot: &ChannelSnapshot) -> Option<QuickActionFieldValue> {
    match key {
        config_keys::TITLE if !snapshot.stream_title.is_empty() => {
            Some(QuickActionFieldValue::Text(snapshot.stream_title.clone()))
        }
        config_keys::CATEGORY_ID => snapshot
            .category_id
            .map(|id| QuickActionFieldValue::Text(id.to_string())),
        _ => None,
    }
}

#[async_trait]
impl BuiltinCurrentValues for KickCurrentValues {
    async fn read_current(&self, field_keys: &[String]) -> BTreeMap<String, QuickActionFieldValue> {
        let Some(snapshot) = self.snapshot().await else {
            return BTreeMap::new();
        };
        field_keys
            .iter()
            .filter_map(|key| Some((key.clone(), value_of(key, &snapshot)?)))
            .collect()
    }
}
