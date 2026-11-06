use std::collections::BTreeMap;

use async_trait::async_trait;

use crate::builtin::QuickActionFieldValue;

#[async_trait]
pub trait BuiltinCurrentValues: Send + Sync {
    async fn read_current(&self, field_keys: &[String]) -> BTreeMap<String, QuickActionFieldValue>;
}
