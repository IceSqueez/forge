use std::collections::HashSet;
use std::sync::Arc;

use forge_discord::DiscordClient;
use forge_storage::ActionRepo;
use forge_types::{Action, ActionId};

use crate::repo_failure::RepoFailure;

const DISCORD_SUB_ACTION_PREFIX: &str = "discord.";
const WEBHOOK_NAME_FIELD: &str = "webhook_name";

pub(crate) struct WebhookRow {
    pub name: String,
    pub linked_actions: Vec<String>,
    pub linked_action_ids: Vec<ActionId>,
}

pub(crate) async fn load_webhooks(
    client: Arc<DiscordClient>,
    actions: Arc<dyn ActionRepo>,
) -> Result<Vec<WebhookRow>, RepoFailure> {
    let names = client.list_webhooks().await.map_err(RepoFailure::from)?;
    let actions = actions.list().await.map_err(RepoFailure::from)?;
    Ok(names
        .into_iter()
        .map(|name| WebhookRow {
            linked_actions: linked_action_names(&actions, &name),
            linked_action_ids: linked_action_ids(&actions, &name),
            name,
        })
        .collect())
}

pub(crate) fn linked_action_names(actions: &[Action], webhook_name: &str) -> Vec<String> {
    actions
        .iter()
        .filter(|action| posts_to(action, webhook_name))
        .map(|action| action.name.clone())
        .collect()
}

fn linked_action_ids(actions: &[Action], webhook_name: &str) -> Vec<ActionId> {
    actions
        .iter()
        .filter(|action| posts_to(action, webhook_name))
        .map(|action| action.id)
        .collect()
}

pub(crate) fn distinct_linked_actions(rows: &[WebhookRow]) -> usize {
    rows.iter()
        .flat_map(|row| row.linked_action_ids.iter())
        .collect::<HashSet<&ActionId>>()
        .len()
}

pub(crate) fn name_is_taken(rows: &[WebhookRow], name: &str) -> bool {
    rows.iter().any(|row| row.name == name)
}

fn posts_to(action: &Action, webhook_name: &str) -> bool {
    action.sub_actions.iter().any(|step| {
        step.kind_id.starts_with(DISCORD_SUB_ACTION_PREFIX)
            && step
                .config
                .get(WEBHOOK_NAME_FIELD)
                .and_then(|value| value.as_str())
                == Some(webhook_name)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ActionId, QueueId, SubActionStep, Variant};

    use super::*;

    fn step(kind_id: &str, webhook: Option<Variant>) -> SubActionStep {
        let mut config = BTreeMap::new();
        if let Some(value) = webhook {
            config.insert(WEBHOOK_NAME_FIELD.to_owned(), value);
        }
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn action(name: &str, sub_actions: Vec<SubActionStep>) -> Action {
        Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: Default::default(),
            description: None,
            sub_actions,
        }
    }

    fn targeting(name: &str, webhook: &str) -> Action {
        action(
            name,
            vec![step(
                "discord.webhook.send_message",
                Some(Variant::String(webhook.to_owned())),
            )],
        )
    }

    fn id_of(name: &str) -> ActionId {
        let seed: u32 = name.bytes().map(u32::from).sum();
        format!("{seed:0>26}").parse().expect("a valid action id")
    }

    fn row(name: &str, linked: &[&str]) -> WebhookRow {
        WebhookRow {
            name: name.to_owned(),
            linked_actions: linked.iter().map(|s| (*s).to_owned()).collect(),
            linked_action_ids: linked.iter().map(|s| id_of(s)).collect(),
        }
    }

    #[test]
    fn linked_action_names_collects_actions_whose_discord_step_targets_the_webhook() {
        let actions = [
            targeting("Announce", "alerts"),
            targeting("Clip Drop", "clips"),
            targeting("Raid Ping", "alerts"),
        ];

        assert_eq!(
            linked_action_names(&actions, "alerts"),
            ["Announce", "Raid Ping"]
        );
    }

    #[test]
    fn linked_action_names_ignores_steps_from_other_integrations() {
        let actions = [action(
            "Scene Swap",
            vec![step(
                "obs.scene.set",
                Some(Variant::String("alerts".to_owned())),
            )],
        )];

        assert!(linked_action_names(&actions, "alerts").is_empty());
    }

    #[test]
    fn linked_action_names_ignores_discord_steps_without_a_string_webhook_name() {
        let actions = [
            action("No Field", vec![step("discord.webhook.send_message", None)]),
            action(
                "Wrong Type",
                vec![step("discord.webhook.send_embed", Some(Variant::Int(7)))],
            ),
            targeting("Other Hook", "clips"),
        ];

        assert!(linked_action_names(&actions, "alerts").is_empty());
    }

    #[test]
    fn distinct_linked_actions_counts_an_action_once_across_several_webhooks() {
        let rows = [
            row("alerts", &["Announce", "Raid Ping"]),
            row("clips", &["Raid Ping", "Clip Drop"]),
        ];

        assert_eq!(distinct_linked_actions(&rows), 3);
    }

    #[test]
    fn distinct_linked_actions_counts_two_actions_sharing_a_name_separately() {
        let actions = [targeting("Alert", "first"), targeting("Alert", "second")];
        let rows: Vec<WebhookRow> = ["first", "second"]
            .map(|webhook| WebhookRow {
                name: webhook.to_owned(),
                linked_actions: linked_action_names(&actions, webhook),
                linked_action_ids: linked_action_ids(&actions, webhook),
            })
            .into();

        assert_eq!(distinct_linked_actions(&rows), 2);
    }

    #[test]
    fn name_is_taken_matches_the_exact_name_and_respects_case() {
        let rows = [row("alerts", &[])];

        assert!(name_is_taken(&rows, "alerts"));
        assert!(!name_is_taken(&rows, "Alerts"));
        assert!(!name_is_taken(&rows, "alert"));
    }

    #[test]
    fn the_pure_joins_report_nothing_for_empty_inputs() {
        assert!(linked_action_names(&[], "alerts").is_empty());
        assert_eq!(distinct_linked_actions(&[]), 0);
        assert!(!name_is_taken(&[], "alerts"));
    }
}
