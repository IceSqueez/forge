use forge_storage::{DataProvider, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, Command, Trigger};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ActionSummary {
    pub id: ActionId,
    pub name: String,
    pub enabled: bool,
    pub sub_action_count: u16,
}

#[derive(Debug, Clone)]
pub struct ActionsGroup {
    pub name: String,
    pub actions: Vec<ActionSummary>,
}

#[derive(Debug, Clone)]
pub struct ActionDetail {
    pub action: Action,
    pub triggers: Vec<Trigger>,
    pub commands: Vec<Command>,
}

#[derive(Default)]
pub struct ActionsState {
    pub tree: Vec<ActionsGroup>,
    pub selected: Option<ActionId>,
    pub detail: Option<ActionDetail>,
    pub loading: bool,
}

impl ActionsState {
    pub fn new() -> Self {
        Self::default()
    }
}

pub async fn load_actions_tree(dp: Arc<SqliteBackend>) -> Result<Vec<ActionsGroup>, StorageError> {
    let actions = dp.action_repo().list().await?;

    let mut ungrouped: Vec<ActionSummary> = Vec::new();
    let mut grouped: std::collections::BTreeMap<String, Vec<ActionSummary>> =
        std::collections::BTreeMap::new();

    for action in actions {
        let summary = ActionSummary {
            id: action.id,
            name: action.name,
            enabled: action.enabled,
            sub_action_count: action.sub_actions.len() as u16,
        };
        match action.group {
            None => ungrouped.push(summary),
            Some(g) => grouped.entry(g).or_default().push(summary),
        }
    }

    let mut result: Vec<ActionsGroup> = Vec::new();

    if !ungrouped.is_empty() {
        result.push(ActionsGroup {
            name: "Ungrouped".to_string(),
            actions: ungrouped,
        });
    }

    for (name, actions) in grouped {
        result.push(ActionsGroup { name, actions });
    }

    Ok(result)
}

pub async fn load_action_detail(
    dp: Arc<SqliteBackend>,
    id: ActionId,
) -> Result<ActionDetail, StorageError> {
    let action = dp
        .action_repo()
        .get(id)
        .await?
        .ok_or_else(|| StorageError::NotFound {
            key: id.to_string(),
        })?;
    let triggers = dp.trigger_repo().list_for_action(id).await?;
    let all_commands = dp.command_repo().list().await?;
    let commands = all_commands
        .into_iter()
        .filter(|c| c.action_id == id)
        .collect();
    Ok(ActionDetail {
        action,
        triggers,
        commands,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_storage::DataProvider;
    use forge_types::{Action, ActionId, Queue, QueueId};

    const TEST_KEY: [u8; 32] = [0xab; 32];

    async fn open_backend() -> Arc<SqliteBackend> {
        Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
                .await
                .unwrap(),
        )
    }

    async fn make_action(dp: &Arc<SqliteBackend>, name: &str, group: Option<&str>) -> Action {
        let queue = Queue {
            id: QueueId::new(),
            name: "Default".to_string(),
            blocking: false,
        };
        dp.queue_repo().save(&queue).await.unwrap();
        Action {
            id: ActionId::new(),
            name: name.to_string(),
            group: group.map(str::to_string),
            queue_id: queue.id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            description: None,
            sub_actions: vec![],
        }
    }

    #[tokio::test]
    async fn empty_db_yields_empty_tree() {
        let dp = open_backend().await;
        let tree = load_actions_tree(dp).await.unwrap();
        assert!(tree.is_empty());
    }

    #[tokio::test]
    async fn two_actions_different_groups_produce_two_groups() {
        let dp = open_backend().await;
        let a1 = make_action(&dp, "!so", Some("Chat Commands")).await;
        let a2 = make_action(&dp, "HydrateCheck", Some("Timers")).await;
        dp.action_repo().save(&a1).await.unwrap();
        dp.action_repo().save(&a2).await.unwrap();

        let tree = load_actions_tree(dp).await.unwrap();
        assert_eq!(tree.len(), 2);
        let names: Vec<&str> = tree.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"Chat Commands"));
        assert!(names.contains(&"Timers"));
    }

    #[tokio::test]
    async fn ungrouped_action_goes_into_ungrouped_group() {
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&a).await.unwrap();

        let tree = load_actions_tree(dp).await.unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Ungrouped");
    }

    #[tokio::test]
    async fn load_action_detail_not_found_returns_error() {
        let dp = open_backend().await;
        let missing_id = ActionId::new();
        let result = load_action_detail(dp, missing_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_action_detail_found_returns_detail() {
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", Some("Chat Commands")).await;
        dp.action_repo().save(&a).await.unwrap();

        let detail = load_action_detail(dp, a.id).await.unwrap();
        assert_eq!(detail.action.name, "!quote");
        assert!(detail.triggers.is_empty());
        assert!(detail.commands.is_empty());
    }
}
