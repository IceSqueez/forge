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

    /// Returns the new value. An absent variable starts from zero (`Int(amount)`); an
    /// `Int` saturates. Errors with [`StorageError::TypeMismatch`] if the stored value is
    /// not numeric or a `Float` result is not finite. Default impl is a non-atomic get/set
    /// composition; a real backend must override it with one serialized write.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _dyn(_: &dyn UserGlobalsRepo) {}

    #[test]
    fn user_global_entry_serde_roundtrip() {
        let entry = UserGlobalEntry {
            broadcaster_id: "broadcaster_123".to_owned(),
            user_id: "user_456".to_owned(),
            name: "points".to_owned(),
            value: Variant::Int(100),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: UserGlobalEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.broadcaster_id, entry.broadcaster_id);
        assert_eq!(decoded.user_id, entry.user_id);
        assert_eq!(decoded.name, entry.name);
        assert_eq!(decoded.last_modified, entry.last_modified);

        match (decoded.value, entry.value) {
            (Variant::Int(a), Variant::Int(b)) => assert_eq!(a, b),
            _ => panic!("variant mismatch after roundtrip"),
        }
    }
}
