use async_trait::async_trait;
use forge_types::Variant;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;
use crate::globals::type_mismatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGlobalEntry {
    pub broadcaster_id: String,
    pub user_id: String,
    pub name: String,
    pub value: Variant,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait UserGlobalsRepo: Send + Sync {
    async fn get(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Variant>, StorageError>;

    async fn set(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        value: Variant,
    ) -> Result<(), StorageError>;

    async fn delete(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<bool, StorageError>;

    async fn list_for_user(
        &self,
        broadcaster_id: &str,
        user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError>;

    async fn list_for_broadcaster(
        &self,
        broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError>;

    async fn incr(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        amount: i64,
    ) -> Result<Variant, StorageError> {
        let current = self.get(broadcaster_id, user_id, name).await?;
        let next = incremented(name, current, amount)?;
        self.set(broadcaster_id, user_id, name, next.clone())
            .await?;
        Ok(next)
    }
}

pub fn incremented(
    name: &str,
    current: Option<Variant>,
    amount: i64,
) -> Result<Variant, StorageError> {
    match current {
        None => Ok(Variant::Int(amount)),
        Some(Variant::Int(value)) => Ok(Variant::Int(value.saturating_add(amount))),
        Some(Variant::Float(value)) => Variant::float(value + amount as f64)
            .map_err(|_| type_mismatch(name, &Variant::Float(value))),
        Some(other) => Err(type_mismatch(name, &other)),
    }
}
