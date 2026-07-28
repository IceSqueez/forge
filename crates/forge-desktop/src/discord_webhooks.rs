use std::collections::BTreeSet;
use std::sync::Arc;

use forge_discord::DiscordClient;
use forge_storage::ActionRepo;
use forge_types::Action;

pub const DISCORD_EVENT_PREFIX: &str = "discord.";
const DISCORD_SUB_ACTION_PREFIX: &str = "discord.";
const WEBHOOK_NAME_FIELD: &str = "webhook_name";

pub struct WebhookRow {
    pub name: String,
    pub linked_actions: Vec<String>,
}

pub async fn load_webhooks(
    client: Arc<DiscordClient>,
    actions: Arc<dyn ActionRepo>,
) -> Result<Vec<WebhookRow>, String> {
    let names = client.list_webhooks().await.map_err(|e| e.to_string())?;
    let actions = actions.list().await.map_err(|e| e.to_string())?;
    Ok(names
        .into_iter()
        .map(|name| WebhookRow {
            linked_actions: linked_action_names(&actions, &name),
            name,
        })
        .collect())
}

pub fn linked_action_names(actions: &[Action], webhook_name: &str) -> Vec<String> {
    actions
        .iter()
        .filter(|action| posts_to(action, webhook_name))
        .map(|action| action.name.clone())
        .collect()
}

/// Counts each action once even when several of its steps target different webhooks.
pub fn distinct_linked_actions(rows: &[WebhookRow]) -> usize {
    rows.iter()
        .flat_map(|row| row.linked_actions.iter().map(String::as_str))
        .collect::<BTreeSet<&str>>()
        .len()
}

pub fn name_is_taken(rows: &[WebhookRow], name: &str) -> bool {
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
