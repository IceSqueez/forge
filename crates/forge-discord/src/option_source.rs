use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinOptionSource, CollectionFailure, CollectionOutcome, CollectionRevisions, OptionEntry,
    OptionListId, OptionListMetadata, QuickActionFieldValue,
};

use crate::client::DiscordClient;
use crate::vocabulary::picker_keys::WEBHOOK_NAMES;

#[async_trait]
impl BuiltinOptionSource for DiscordClient {
    fn option_lists(&self) -> Vec<OptionListMetadata> {
        vec![OptionListMetadata::new(OptionListId::new(WEBHOOK_NAMES))]
    }

    fn revisions(&self) -> CollectionRevisions {
        self.status_signal().subscribe()
    }

    async fn resolve(
        &self,
        list: &OptionListId,
        _: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<Vec<OptionEntry>> {
        match list.as_str() {
            WEBHOOK_NAMES => Ok(self
                .webhook_names()
                .into_iter()
                .map(|name| OptionEntry::new(name.clone(), name))
                .collect()),
            _ => Err(CollectionFailure::NotEligible),
        }
    }
}
