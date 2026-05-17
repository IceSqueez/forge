use forge_storage::{DataProvider, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, Command, CommandPermission, QueueId, Trigger, TriggerId, TriggerKind,
};
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

pub struct AddActionForm {
    pub name: String,
    pub group: String,
    pub queue_id: Option<QueueId>,
    pub description: String,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub queue_options: Vec<(QueueId, String)>,
    pub selected_queue_name: Option<String>,
    pub error: Option<String>,
    pub saving: bool,
}

impl AddActionForm {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            group: String::new(),
            queue_id: None,
            description: String::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            queue_options: vec![],
            selected_queue_name: None,
            error: None,
            saving: false,
        }
    }

    pub fn set_queue_options(&mut self, opts: Vec<(QueueId, String)>) {
        let default = opts
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case("default"))
            .cloned();
        self.queue_options = opts;
        if let Some((id, name)) = default {
            self.queue_id = Some(id);
            self.selected_queue_name = Some(name);
        }
    }

    pub fn select_queue_by_name(&mut self, name: String) {
        let found = self.queue_options.iter().find(|(_, n)| *n == name);
        if let Some((id, _)) = found {
            self.queue_id = Some(*id);
        }
        self.selected_queue_name = Some(name);
    }

    /// Returns false if name is blank or no queue is selected.
    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty() && self.queue_id.is_some()
    }
}

impl Default for AddActionForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum AddActionMsg {
    OpenRequested,
    QueueOptionsLoaded(Result<Vec<(QueueId, String)>, String>),
    NameChanged(String),
    GroupChanged(String),
    QueueSelected(String),
    DescriptionChanged(String),
    EnabledToggled(bool),
    ConcurrentToggled(bool),
    BypassPauseToggled(bool),
    Cancel,
    Submit,
    Saved(Result<ActionId, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerCategory {
    All,
    Chat,
    Subscriptions,
    Bits,
    Raids,
}

pub fn category_of(kind: &TriggerKind) -> TriggerCategory {
    match kind {
        TriggerKind::TwitchChatCommand | TriggerKind::TwitchChatAnyMessage => TriggerCategory::Chat,
        TriggerKind::TwitchSubscribe
        | TriggerKind::TwitchResubscribe
        | TriggerKind::TwitchGiftSub => TriggerCategory::Subscriptions,
        TriggerKind::TwitchCheer => TriggerCategory::Bits,
        TriggerKind::TwitchRaid => TriggerCategory::Raids,
    }
}

pub fn kind_label(kind: &TriggerKind) -> &'static str {
    match kind {
        TriggerKind::TwitchChatCommand => "Twitch \u{00b7} Chat command",
        TriggerKind::TwitchChatAnyMessage => "Twitch \u{00b7} Any chat message",
        TriggerKind::TwitchSubscribe => "Twitch \u{00b7} New subscriber",
        TriggerKind::TwitchResubscribe => "Twitch \u{00b7} Re-subscribe",
        TriggerKind::TwitchGiftSub => "Twitch \u{00b7} Gift subs",
        TriggerKind::TwitchCheer => "Twitch \u{00b7} Bits cheered",
        TriggerKind::TwitchRaid => "Twitch \u{00b7} Raid received",
    }
}

pub fn kind_summary(kind: &TriggerKind) -> &'static str {
    match kind {
        TriggerKind::TwitchChatCommand => "User types !command in chat",
        TriggerKind::TwitchChatAnyMessage => "Every chat message fires this",
        TriggerKind::TwitchSubscribe => "Fires when someone subscribes",
        TriggerKind::TwitchResubscribe => "Existing sub renews",
        TriggerKind::TwitchGiftSub => "Someone gifts subs to channel",
        TriggerKind::TwitchCheer => "Viewer sends bits",
        TriggerKind::TwitchRaid => "Another stream raids you",
    }
}

/// Combined text used for case-insensitive search matching.
pub fn kind_search_text(kind: &TriggerKind) -> &'static str {
    match kind {
        TriggerKind::TwitchChatCommand => "twitch chat command !command",
        TriggerKind::TwitchChatAnyMessage => "twitch chat any message all",
        TriggerKind::TwitchSubscribe => "twitch subscribe subscriber sub new",
        TriggerKind::TwitchResubscribe => "twitch resubscribe resub renew",
        TriggerKind::TwitchGiftSub => "twitch gift sub giftsub gifted",
        TriggerKind::TwitchCheer => "twitch cheer bits cheered donate",
        TriggerKind::TwitchRaid => "twitch raid incoming raided",
    }
}

pub fn all_trigger_kinds() -> [TriggerKind; 7] {
    [
        TriggerKind::TwitchChatCommand,
        TriggerKind::TwitchChatAnyMessage,
        TriggerKind::TwitchSubscribe,
        TriggerKind::TwitchResubscribe,
        TriggerKind::TwitchGiftSub,
        TriggerKind::TwitchCheer,
        TriggerKind::TwitchRaid,
    ]
}

#[derive(Debug, Clone)]
pub struct TriggerConfigForm {
    pub command_name: String,
    pub cooldown_secs: String,
    pub permission: CommandPermission,
    pub min_bits: String,
}

impl TriggerConfigForm {
    pub fn new() -> Self {
        Self {
            command_name: String::new(),
            cooldown_secs: "0".to_string(),
            permission: CommandPermission::Everyone,
            min_bits: "1".to_string(),
        }
    }

    pub fn parsed_cooldown(&self) -> u64 {
        self.cooldown_secs.trim().parse().unwrap_or(0)
    }

    pub fn parsed_min_bits(&self) -> u32 {
        self.min_bits.trim().parse().unwrap_or(1)
    }
}

impl Default for TriggerConfigForm {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AddTriggerForm {
    pub for_action_id: ActionId,
    pub search: String,
    pub category: TriggerCategory,
    pub selected_kind: Option<TriggerKind>,
    pub config: TriggerConfigForm,
    pub error: Option<String>,
    pub saving: bool,
}

impl AddTriggerForm {
    pub fn new(for_action_id: ActionId) -> Self {
        Self {
            for_action_id,
            search: String::new(),
            category: TriggerCategory::All,
            selected_kind: None,
            config: TriggerConfigForm::new(),
            error: None,
            saving: false,
        }
    }

    /// Returns false until a kind is selected; for TwitchChatCommand also requires a non-empty command name.
    pub fn is_valid(&self) -> bool {
        let Some(kind) = &self.selected_kind else {
            return false;
        };
        if matches!(kind, TriggerKind::TwitchChatCommand) {
            !self.config.command_name.trim().is_empty()
        } else {
            true
        }
    }

    pub fn visible_kinds(&self) -> Vec<TriggerKind> {
        let query = self.search.trim().to_lowercase();
        all_trigger_kinds()
            .into_iter()
            .filter(|k| {
                let cat_match =
                    self.category == TriggerCategory::All || category_of(k) == self.category;
                let search_match = query.is_empty()
                    || kind_search_text(k).contains(&query)
                    || kind_label(k).to_lowercase().contains(&query);
                cat_match && search_match
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum AddTriggerMsg {
    OpenRequested(ActionId),
    SearchChanged(String),
    CategorySelected(TriggerCategory),
    KindSelected(TriggerKind),
    CommandNameChanged(String),
    CooldownChanged(String),
    PermissionSelected(CommandPermission),
    MinBitsChanged(String),
    Cancel,
    Submit,
    Saved(Result<TriggerId, String>),
}

#[derive(Default)]
pub struct ActionsState {
    pub tree: Vec<ActionsGroup>,
    pub selected: Option<ActionId>,
    pub detail: Option<ActionDetail>,
    pub loading: bool,
    pub add_action_modal: Option<AddActionForm>,
    pub add_trigger_modal: Option<AddTriggerForm>,
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
    use forge_types::{Action, ActionId, CommandId, Queue, QueueId};

    #[test]
    fn form_invalid_when_name_is_empty() {
        let mut form = AddActionForm::new();
        form.queue_id = Some(QueueId::new());
        assert!(!form.is_valid());
    }

    #[test]
    fn form_invalid_when_no_queue_selected() {
        let mut form = AddActionForm::new();
        form.name = "My action".to_string();
        assert!(!form.is_valid());
    }

    #[test]
    fn form_valid_when_name_and_queue_present() {
        let mut form = AddActionForm::new();
        form.name = "My action".to_string();
        form.queue_id = Some(QueueId::new());
        assert!(form.is_valid());
    }

    #[test]
    fn form_invalid_when_name_is_only_whitespace() {
        let mut form = AddActionForm::new();
        form.name = "   ".to_string();
        form.queue_id = Some(QueueId::new());
        assert!(!form.is_valid());
    }

    #[test]
    fn chat_command_category_is_chat() {
        assert_eq!(
            category_of(&TriggerKind::TwitchChatCommand),
            TriggerCategory::Chat
        );
    }

    #[test]
    fn any_message_category_is_chat() {
        assert_eq!(
            category_of(&TriggerKind::TwitchChatAnyMessage),
            TriggerCategory::Chat
        );
    }

    #[test]
    fn subscribe_category_is_subscriptions() {
        assert_eq!(
            category_of(&TriggerKind::TwitchSubscribe),
            TriggerCategory::Subscriptions
        );
        assert_eq!(
            category_of(&TriggerKind::TwitchResubscribe),
            TriggerCategory::Subscriptions
        );
        assert_eq!(
            category_of(&TriggerKind::TwitchGiftSub),
            TriggerCategory::Subscriptions
        );
    }

    #[test]
    fn cheer_category_is_bits() {
        assert_eq!(
            category_of(&TriggerKind::TwitchCheer),
            TriggerCategory::Bits
        );
    }

    #[test]
    fn raid_category_is_raids() {
        assert_eq!(
            category_of(&TriggerKind::TwitchRaid),
            TriggerCategory::Raids
        );
    }

    #[test]
    fn kind_search_text_contains_chat_keyword() {
        assert!(kind_search_text(&TriggerKind::TwitchChatCommand).contains("chat"));
        assert!(kind_search_text(&TriggerKind::TwitchChatAnyMessage).contains("chat"));
    }

    #[test]
    fn kind_search_text_contains_sub_keyword() {
        assert!(kind_search_text(&TriggerKind::TwitchSubscribe).contains("sub"));
        assert!(kind_search_text(&TriggerKind::TwitchGiftSub).contains("sub"));
    }

    #[test]
    fn add_trigger_form_invalid_without_kind() {
        let form = AddTriggerForm::new(ActionId::new());
        assert!(!form.is_valid());
    }

    #[test]
    fn add_trigger_form_invalid_chat_command_without_name() {
        let mut form = AddTriggerForm::new(ActionId::new());
        form.selected_kind = Some(TriggerKind::TwitchChatCommand);
        assert!(!form.is_valid());
    }

    #[test]
    fn add_trigger_form_valid_chat_command_with_name() {
        let mut form = AddTriggerForm::new(ActionId::new());
        form.selected_kind = Some(TriggerKind::TwitchChatCommand);
        form.config.command_name = "quote".to_string();
        assert!(form.is_valid());
    }

    #[test]
    fn add_trigger_form_valid_non_command_kind_without_name() {
        let mut form = AddTriggerForm::new(ActionId::new());
        form.selected_kind = Some(TriggerKind::TwitchSubscribe);
        assert!(form.is_valid());
    }

    #[test]
    fn search_chat_shows_command_and_any_message() {
        let mut form = AddTriggerForm::new(ActionId::new());
        form.search = "chat".to_string();
        let visible = form.visible_kinds();
        assert!(visible.contains(&TriggerKind::TwitchChatCommand));
        assert!(visible.contains(&TriggerKind::TwitchChatAnyMessage));
    }

    #[test]
    fn category_chat_filter_hides_non_chat_kinds() {
        let mut form = AddTriggerForm::new(ActionId::new());
        form.category = TriggerCategory::Chat;
        let visible = form.visible_kinds();
        assert!(!visible.contains(&TriggerKind::TwitchSubscribe));
        assert!(!visible.contains(&TriggerKind::TwitchRaid));
        assert!(visible.contains(&TriggerKind::TwitchChatCommand));
    }

    #[test]
    fn search_sub_category_all_shows_subscribe_kinds() {
        let mut form = AddTriggerForm::new(ActionId::new());
        form.search = "sub".to_string();
        let visible = form.visible_kinds();
        assert!(visible.contains(&TriggerKind::TwitchSubscribe));
        assert!(visible.contains(&TriggerKind::TwitchGiftSub));
    }

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

    #[tokio::test]
    async fn chat_command_submit_persists_trigger_and_command() {
        let dp = open_backend().await;
        let action = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let trigger = Trigger {
            id: TriggerId::new(),
            action_id: action.id,
            kind: TriggerKind::TwitchChatCommand,
            config: {
                let mut m = std::collections::BTreeMap::new();
                m.insert("cooldown_secs".to_string(), forge_types::Variant::Int(30));
                m
            },
        };
        let cmd = Command {
            id: CommandId::new(),
            action_id: action.id,
            name: "!quote".to_string(),
            cooldown_secs: 30,
            permission: CommandPermission::Everyone,
        };

        dp.trigger_repo().save(&trigger).await.unwrap();
        dp.command_repo().save(&cmd).await.unwrap();

        let detail = load_action_detail(dp, action.id).await.unwrap();
        assert_eq!(detail.triggers.len(), 1);
        assert_eq!(detail.commands.len(), 1);
        assert_eq!(detail.commands[0].name, "!quote");
    }

    #[tokio::test]
    async fn non_command_trigger_persists_only_trigger_row() {
        let dp = open_backend().await;
        let action = make_action(&dp, "sub alert", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let trigger = Trigger {
            id: TriggerId::new(),
            action_id: action.id,
            kind: TriggerKind::TwitchSubscribe,
            config: std::collections::BTreeMap::new(),
        };
        dp.trigger_repo().save(&trigger).await.unwrap();

        let detail = load_action_detail(dp, action.id).await.unwrap();
        assert_eq!(detail.triggers.len(), 1);
        assert!(detail.commands.is_empty());
    }
}
