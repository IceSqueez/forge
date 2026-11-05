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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_platform_core::RevisionWait;
    use forge_registry::{FormField, SubActionRegistry};

    use super::*;

    const HANG_GUARD: Duration = Duration::from_secs(5);

    fn hook_url(id: u64) -> String {
        format!("https://discord.com/api/webhooks/{id}/tok")
    }

    async fn offered(client: &DiscordClient) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = client
            .resolve(&OptionListId::new(WEBHOOK_NAMES), &BTreeMap::new())
            .await
            .unwrap()
            .into_iter()
            .map(|entry| (entry.value, entry.label))
            .collect();
        entries.sort();
        entries
    }

    fn named(names: &[&str]) -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| ((*name).to_owned(), (*name).to_owned()))
            .collect()
    }

    async fn moved(revisions: &mut CollectionRevisions) -> bool {
        matches!(
            tokio::time::timeout(HANG_GUARD, revisions.changed()).await,
            Ok(RevisionWait::Changed)
        )
    }

    #[tokio::test]
    async fn the_webhook_list_offers_each_saved_webhook_under_its_own_name() {
        let client = DiscordClient::new_for_test();
        client.save_webhook("alerts", &hook_url(1)).await.unwrap();
        client
            .save_webhook("Stream Ünits", &hook_url(2))
            .await
            .unwrap();

        assert_eq!(offered(&client).await, named(&["Stream Ünits", "alerts"]));
    }

    #[tokio::test]
    async fn a_deleted_webhook_leaves_the_list() {
        let client = DiscordClient::new_for_test();
        client.save_webhook("alerts", &hook_url(1)).await.unwrap();
        client.save_webhook("clips", &hook_url(2)).await.unwrap();

        client.delete_webhook("alerts").await.unwrap();

        assert_eq!(offered(&client).await, named(&["clips"]));
    }

    #[tokio::test]
    async fn saving_and_deleting_a_webhook_each_move_the_option_revision() {
        let client = DiscordClient::new_for_test();
        let mut revisions = BuiltinOptionSource::revisions(client.as_ref());

        client.save_webhook("alerts", &hook_url(1)).await.unwrap();
        assert!(moved(&mut revisions).await, "save");

        client.delete_webhook("alerts").await.unwrap();
        assert!(moved(&mut revisions).await, "delete");
    }

    #[tokio::test]
    async fn a_list_discord_does_not_declare_is_not_eligible() {
        let client = DiscordClient::new_for_test();

        let outcome = client
            .resolve(&OptionListId::new("discord.channels"), &BTreeMap::new())
            .await;

        assert!(matches!(outcome, Err(CollectionFailure::NotEligible)));
    }

    fn select_keys(field: &FormField, out: &mut Vec<&'static str>) {
        match field {
            FormField::DynamicSelect { options_key, .. } => out.push(options_key),
            FormField::Optional { inner, .. } => select_keys(inner, out),
            _ => {}
        }
    }

    #[test]
    fn every_picker_in_a_discord_step_is_a_list_discord_serves() {
        let client = DiscordClient::new_for_test();
        let mut sub_actions = SubActionRegistry::new();
        crate::register_discord_sub_actions(&mut sub_actions, Arc::clone(&client)).unwrap();
        let mut declared = Vec::new();
        for runner in sub_actions.all() {
            for field in runner.config_fields() {
                select_keys(&field, &mut declared);
            }
        }
        let served = client.option_lists();

        assert!(!declared.is_empty());
        for key in declared {
            assert!(served.iter().any(|list| list.id.as_str() == key), "{key}");
        }
    }
}
