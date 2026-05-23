use forge_events::{Event, EventSource};
use forge_storage::{ActionTelemetry, DataProvider, StorageError};
use forge_types::{
    Action, ActionId, ClipId, Command, CommandPermission, LogLevel, QueueId, SubActionSpec,
    Trigger, TriggerId, TriggerKind,
};
use iced::{Color, Element, Length, Task};
use std::sync::Arc;
use time::OffsetDateTime;

use crate::Message;
use crate::message::{ActionEditorMsg, ActionsMsg, ToastMsg};
use crate::runtime_view::RuntimeView;
use crate::test_trigger::synthesize_test_event;

#[derive(Debug, Clone)]
pub struct ActionSummary {
    pub id: ActionId,
    pub name: String,
    pub enabled: bool,
    pub sub_action_count: u16,
    pub trigger_category: TriggerCategory,
    pub trigger_label: String,
    pub queue_name: String,
    pub last_ran: Option<OffsetDateTime>,
    pub runs_24h: u32,
    pub extra_subtitle: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActionsGroup {
    pub category: TriggerCategory,
    pub fired_24h: u32,
    pub actions: Vec<ActionSummary>,
}

#[derive(Debug, Clone)]
pub struct ActionDetail {
    pub action: Action,
    pub triggers: Vec<Trigger>,
    pub commands: Vec<Command>,
    /// Rolling average duration (ms) per sub-action index across recent executions.
    /// `None` at an index means no telemetry yet recorded for that step.
    pub sub_action_avg_ms: Vec<Option<u64>>,
}

pub struct AddActionForm {
    pub name: String,
    pub group: String,
    pub queue_id: Option<QueueId>,
    pub description: String,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub random_pick: bool,
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
            random_pick: false,
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
    RandomPickToggled(bool),
    Cancel,
    Submit,
    Saved(Result<ActionId, String>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TriggerCategory {
    Chat,
    Subscriptions,
    Bits,
    Raids,
    Obs,
    Server,
    Timer,
    Ungrouped,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionsFilter {
    #[default]
    All,
    Chat,
    Timers,
    Points,
}

impl TriggerCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            TriggerCategory::Chat => "CHAT COMMANDS",
            TriggerCategory::Subscriptions => "SUBS & BITS",
            TriggerCategory::Bits => "BITS",
            TriggerCategory::Raids => "RAIDS",
            TriggerCategory::Obs => "OBS EVENTS",
            TriggerCategory::Server => "SERVER EVENTS",
            TriggerCategory::Timer => "TIMERS",
            TriggerCategory::Ungrouped => "UNGROUPED",
            TriggerCategory::All => "ALL",
        }
    }
}

pub fn category_of(kind: &TriggerKind) -> TriggerCategory {
    match kind {
        TriggerKind::TwitchChatCommand | TriggerKind::TwitchChatAnyMessage => TriggerCategory::Chat,
        TriggerKind::TwitchSubscribe
        | TriggerKind::TwitchResubscribe
        | TriggerKind::TwitchGiftSub => TriggerCategory::Subscriptions,
        TriggerKind::TwitchCheer => TriggerCategory::Bits,
        TriggerKind::TwitchRaid => TriggerCategory::Raids,
        TriggerKind::ObsSceneChanged { .. } => TriggerCategory::Obs,
        TriggerKind::CodeEvent { .. } => TriggerCategory::Server,
    }
}

pub fn trigger_label_of(kind: &TriggerKind) -> String {
    match kind {
        TriggerKind::TwitchChatCommand => "Twitch \u{00b7} chat command".to_string(),
        TriggerKind::TwitchChatAnyMessage => "Twitch \u{00b7} any chat message".to_string(),
        TriggerKind::TwitchSubscribe => "Twitch \u{00b7} new subscriber".to_string(),
        TriggerKind::TwitchResubscribe => "Twitch \u{00b7} re-subscribe".to_string(),
        TriggerKind::TwitchGiftSub => "Twitch \u{00b7} gift subs".to_string(),
        TriggerKind::TwitchCheer => "Twitch \u{00b7} bits cheered".to_string(),
        TriggerKind::TwitchRaid => "Twitch \u{00b7} raid received".to_string(),
        TriggerKind::ObsSceneChanged { .. } => "OBS \u{00b7} scene changed".to_string(),
        TriggerKind::CodeEvent { .. } => "Server \u{00b7} custom event".to_string(),
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
        TriggerKind::ObsSceneChanged { .. } => "OBS \u{00b7} Scene changed",
        TriggerKind::CodeEvent { .. } => "Server \u{00b7} Custom event",
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
        TriggerKind::ObsSceneChanged { .. } => "Fires when OBS switches the active scene",
        TriggerKind::CodeEvent { .. } => {
            "Fires when triggerCodeEvent is called via the WebSocket API"
        }
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
        TriggerKind::ObsSceneChanged { .. } => "obs scene changed obsscenechanged",
        TriggerKind::CodeEvent { .. } => "server code event custom overlay api trigger",
    }
}

pub fn all_trigger_kinds() -> [TriggerKind; 9] {
    [
        TriggerKind::TwitchChatCommand,
        TriggerKind::TwitchChatAnyMessage,
        TriggerKind::TwitchSubscribe,
        TriggerKind::TwitchResubscribe,
        TriggerKind::TwitchGiftSub,
        TriggerKind::TwitchCheer,
        TriggerKind::TwitchRaid,
        TriggerKind::ObsSceneChanged { scene: None },
        TriggerKind::CodeEvent {
            name: String::new(),
        },
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SubActionKindChoice {
    #[default]
    SendChat,
    SetGlobal,
    Delay,
    Log,
    PlaySound,
    Speak,
    ReadFile,
    RandomInt,
}

#[derive(Debug, Clone)]
pub struct SubActionConfigForm {
    pub send_chat_message: String,
    pub send_chat_target: String,
    pub set_global_name: String,
    pub set_global_value: String,
    pub delay_ms: String,
    pub log_level: LogLevel,
    pub log_message: String,
    pub play_sound_clip_id: Option<ClipId>,
    pub speak_text: String,
    pub speak_voice_override: String,
    pub read_file_path: String,
    pub read_file_target_var: String,
    pub random_int_min: String,
    pub random_int_max: String,
    pub random_int_target_var: String,
}

impl Default for SubActionConfigForm {
    fn default() -> Self {
        Self {
            send_chat_message: String::new(),
            send_chat_target: "twitch".to_string(),
            set_global_name: String::new(),
            set_global_value: String::new(),
            delay_ms: "500".to_string(),
            log_level: LogLevel::Info,
            log_message: String::new(),
            play_sound_clip_id: None,
            speak_text: String::new(),
            speak_voice_override: String::new(),
            read_file_path: String::new(),
            read_file_target_var: String::new(),
            random_int_min: "1".to_string(),
            random_int_max: "100".to_string(),
            random_int_target_var: String::new(),
        }
    }
}

#[derive(Debug)]
pub struct AddSubActionForm {
    pub for_action_id: ActionId,
    pub kind: SubActionKindChoice,
    pub config: SubActionConfigForm,
    pub available_clips: Vec<(ClipId, String)>,
    pub error: Option<String>,
    pub saving: bool,
    pub editing_index: Option<usize>,
}

impl AddSubActionForm {
    pub fn new(for_action_id: ActionId) -> Self {
        Self {
            for_action_id,
            kind: SubActionKindChoice::SendChat,
            config: SubActionConfigForm::default(),
            available_clips: vec![],
            error: None,
            saving: false,
            editing_index: None,
        }
    }

    pub fn populate_from_spec(&mut self, spec: &SubActionSpec) {
        match spec {
            SubActionSpec::SendChat { target, message } => {
                self.kind = SubActionKindChoice::SendChat;
                self.config.send_chat_target = target.clone();
                self.config.send_chat_message = message.clone();
            }
            SubActionSpec::SetGlobal { name, value } => {
                self.kind = SubActionKindChoice::SetGlobal;
                self.config.set_global_name = name.clone();
                self.config.set_global_value = value.clone();
            }
            SubActionSpec::Delay { ms } => {
                self.kind = SubActionKindChoice::Delay;
                self.config.delay_ms = ms.to_string();
            }
            SubActionSpec::Log { level, message } => {
                self.kind = SubActionKindChoice::Log;
                self.config.log_level = level.clone();
                self.config.log_message = message.clone();
            }
            SubActionSpec::PlaySound { clip_id, .. } => {
                self.kind = SubActionKindChoice::PlaySound;
                self.config.play_sound_clip_id = Some(*clip_id);
            }
            SubActionSpec::Speak {
                text,
                voice_id_override,
            } => {
                self.kind = SubActionKindChoice::Speak;
                self.config.speak_text = text.clone();
                self.config.speak_voice_override = voice_id_override.clone().unwrap_or_default();
            }
            SubActionSpec::ReadFile { path, target_var } => {
                self.kind = SubActionKindChoice::ReadFile;
                self.config.read_file_path = path.clone();
                self.config.read_file_target_var = target_var.clone();
            }
            SubActionSpec::RandomInt {
                min,
                max,
                target_var,
            } => {
                self.kind = SubActionKindChoice::RandomInt;
                self.config.random_int_min = min.to_string();
                self.config.random_int_max = max.to_string();
                self.config.random_int_target_var = target_var.clone();
            }
            _ => {}
        }
    }

    pub fn is_valid(&self) -> bool {
        match self.kind {
            SubActionKindChoice::SendChat => !self.config.send_chat_message.trim().is_empty(),
            SubActionKindChoice::SetGlobal => !self.config.set_global_name.trim().is_empty(),
            SubActionKindChoice::Delay => self.config.delay_ms.trim().parse::<u64>().is_ok(),
            SubActionKindChoice::Log => !self.config.log_message.trim().is_empty(),
            SubActionKindChoice::PlaySound => self.config.play_sound_clip_id.is_some(),
            SubActionKindChoice::Speak => !self.config.speak_text.trim().is_empty(),
            SubActionKindChoice::ReadFile => {
                !self.config.read_file_path.trim().is_empty()
                    && !self.config.read_file_target_var.trim().is_empty()
            }
            SubActionKindChoice::RandomInt => {
                let min = self.config.random_int_min.trim().parse::<i64>().ok();
                let max = self.config.random_int_max.trim().parse::<i64>().ok();
                let target_ok = !self.config.random_int_target_var.trim().is_empty();
                matches!((min, max), (Some(lo), Some(hi)) if lo <= hi) && target_ok
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AddSubActionMsg {
    OpenRequested(ActionId),
    KindSelected(SubActionKindChoice),
    SendChatMessageChanged(String),
    SendChatTargetChanged(String),
    SetGlobalNameChanged(String),
    SetGlobalValueChanged(String),
    DelayMsChanged(String),
    LogLevelSelected(LogLevel),
    LogMessageChanged(String),
    PlaySoundClipSelected(ClipId),
    SpeakTextChanged(String),
    SpeakVoiceOverrideChanged(String),
    ReadFilePathChanged(String),
    ReadFileTargetVarChanged(String),
    RandomIntMinChanged(String),
    RandomIntMaxChanged(String),
    RandomIntTargetVarChanged(String),
    ClipsLoaded(Vec<(ClipId, String)>),
    Cancel,
    Submit,
    Saved(Result<(), String>),
    DuplicateRequested(ActionId, usize),
    Duplicated(Result<ActionId, String>),
    EditRequested(ActionId, usize),
}

#[derive(Debug, Clone)]
pub enum RemoveSubActionMsg {
    Requested(ActionId, usize),
    Removed(Result<(), String>),
}

#[derive(Default)]
pub struct ActionsState {
    pub tree: Vec<ActionsGroup>,
    pub selected: Option<ActionId>,
    pub detail: Option<ActionDetail>,
    pub loading: bool,
    pub search: String,
    pub filter: ActionsFilter,
    pub collapsed_groups: std::collections::HashSet<TriggerCategory>,
    pub add_action_modal: Option<AddActionForm>,
    pub add_trigger_modal: Option<AddTriggerForm>,
    pub add_sub_action_modal: Option<AddSubActionForm>,
    pub telemetry: Option<ActionTelemetry>,
    pub telemetry_loading: bool,
    pub step_menu_open: Option<usize>,
    pub action_menu_open: Option<forge_types::ActionId>,
    pub renaming_action: Option<(forge_types::ActionId, String)>,
    pub last_selected_action: Option<(forge_types::ActionId, std::time::Instant)>,
}

impl ActionsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_actions(&self) -> usize {
        self.tree.iter().map(|g| g.actions.len()).sum()
    }

    pub fn visible_actions(&self) -> usize {
        self.tree
            .iter()
            .flat_map(|g| g.actions.iter())
            .filter(|a| self.action_passes_filter(a))
            .count()
    }

    pub fn action_passes_filter(&self, summary: &ActionSummary) -> bool {
        let filter_ok = match self.filter {
            ActionsFilter::All => true,
            ActionsFilter::Chat => summary.trigger_category == TriggerCategory::Chat,
            ActionsFilter::Timers => summary.trigger_category == TriggerCategory::Timer,
            ActionsFilter::Points => false,
        };
        let search_ok = if self.search.is_empty() {
            true
        } else {
            let q = self.search.to_lowercase();
            summary.name.to_lowercase().contains(&q)
                || summary.trigger_label.to_lowercase().contains(&q)
                || summary.queue_name.to_lowercase().contains(&q)
        };
        filter_ok && search_ok
    }
}

pub async fn load_actions_tree(
    dp: Arc<dyn DataProvider>,
) -> Result<Vec<ActionsGroup>, StorageError> {
    let actions = dp.action_repo().list().await?;
    let all_queues = dp.queue_repo().list().await?;
    let since = OffsetDateTime::now_utc() - time::Duration::hours(24);
    let stats = dp.history_repo().stats_summary(since).await?;

    let mut by_category: std::collections::BTreeMap<TriggerCategory, Vec<ActionSummary>> =
        std::collections::BTreeMap::new();

    for action in actions {
        let action_triggers = dp.trigger_repo().list_for_action(action.id).await?;

        let (trigger_category, trigger_label) = action_triggers
            .first()
            .map(|t| (category_of(&t.kind), trigger_label_of(&t.kind)))
            .unwrap_or((TriggerCategory::Ungrouped, "\u{2014}".to_string()));

        let queue_name = all_queues
            .iter()
            .find(|q| q.id == action.queue_id)
            .map(|q| q.name.clone())
            .unwrap_or_else(|| "Default".to_string());

        let (last_ran, runs_24h) = stats
            .get(&action.id)
            .map(|s| (Some(s.last_ran_at), s.runs_24h))
            .unwrap_or((None, 0));

        let summary = ActionSummary {
            id: action.id,
            name: action.name,
            enabled: action.enabled,
            sub_action_count: action.sub_actions.len() as u16,
            trigger_category: trigger_category.clone(),
            trigger_label,
            queue_name,
            last_ran,
            runs_24h,
            extra_subtitle: None,
        };

        by_category
            .entry(trigger_category)
            .or_default()
            .push(summary);
    }

    let result = by_category
        .into_iter()
        .map(|(category, mut actions)| {
            actions.sort_by_key(|a| a.name.to_lowercase());
            let fired_24h = actions.iter().map(|a| a.runs_24h).sum();
            ActionsGroup {
                category,
                fired_24h,
                actions,
            }
        })
        .collect();

    Ok(result)
}

pub async fn load_action_detail(
    dp: Arc<dyn DataProvider>,
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
    let commands: Vec<_> = all_commands
        .into_iter()
        .filter(|c| c.action_id == id)
        .collect();

    let recent = dp.history_repo().recent_for_action(id, 20).await?;
    let sub_action_avg_ms = compute_sub_action_averages(&recent, action.sub_actions.len());

    Ok(ActionDetail {
        action,
        triggers,
        commands,
        sub_action_avg_ms,
    })
}

fn compute_sub_action_averages(
    history: &[forge_types::ExecutionContext],
    sub_action_count: usize,
) -> Vec<Option<u64>> {
    let mut sums: Vec<u64> = vec![0; sub_action_count];
    let mut counts: Vec<u64> = vec![0; sub_action_count];
    for ctx in history {
        for t in &ctx.telemetry {
            if t.index < sub_action_count {
                sums[t.index] += t.duration_ms;
                counts[t.index] += 1;
            }
        }
    }
    sums.iter()
        .zip(counts.iter())
        .map(|(s, c)| if *c > 0 { Some(s / c) } else { None })
        .collect()
}

pub async fn save_sub_action(
    dp: Arc<dyn DataProvider>,
    action_id: ActionId,
    spec: SubActionSpec,
    editing_index: Option<usize>,
) -> Result<(), StorageError> {
    let Some(mut action) = dp.action_repo().get(action_id).await? else {
        return Err(StorageError::NotFound {
            key: action_id.to_string(),
        });
    };
    if let Some(idx) = editing_index {
        if idx < action.sub_actions.len() {
            action.sub_actions[idx] = spec;
        } else {
            action.sub_actions.push(spec);
        }
    } else {
        action.sub_actions.push(spec);
    }
    dp.action_repo().save(&action).await
}

pub async fn load_telemetry(
    dp: Arc<dyn DataProvider>,
    id: ActionId,
) -> Result<ActionTelemetry, String> {
    dp.action_repo()
        .telemetry(id)
        .await
        .map_err(|e| e.to_string())
}

pub fn format_relative_time(opt: Option<OffsetDateTime>) -> String {
    let Some(dt) = opt else {
        return "never".to_string();
    };
    let delta = OffsetDateTime::now_utc() - dt;
    let secs = delta.whole_seconds().max(0) as u64;
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub fn action_stat<'a, Msg: 'a>(
    label: &str,
    value: &str,
    value_color: Color,
    hint: Option<&str>,
    palette: &forge_widgets::ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::FontRole;
    use forge_widgets::tokens::{FONT_SM, FONT_XS};
    use iced::widget::{column, text};

    let p = *palette;
    let mono = forge_widgets::font(FontRole::Monospace);

    let label_el = text(label.to_owned())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let value_el = text(value.to_owned())
        .size(FONT_SM)
        .color(value_color)
        .font(mono);

    if let Some(hint_str) = hint {
        let hint_el = text(hint_str.to_owned())
            .size(FONT_XS)
            .color(p.text_muted)
            .font(mono);
        column![label_el, value_el, hint_el].spacing(2).into()
    } else {
        column![label_el, value_el].spacing(2).into()
    }
}

pub fn telemetry_grid<'a, Msg: 'a>(
    t: &ActionTelemetry,
    palette: &forge_widgets::ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::radius;
    use forge_widgets::tokens::Radius;
    use iced::widget::{container, row};

    let p = *palette;

    let last_fired_val = format_relative_time(t.last_fired_at);
    let runs_val = t.runs_today.to_string();
    let avg_val = t
        .avg_duration_ms
        .map(|ms| format!("{ms} ms"))
        .unwrap_or_else(|| "\u{2014}".to_string());
    let errors_val = t.errors_7d.to_string();
    let errors_color = if t.errors_7d > 0 { p.random } else { p.success };

    let cells = row![
        container(action_stat(
            "LAST FIRED",
            &last_fired_val,
            p.text_primary,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
        container(action_stat(
            "RUNS \u{00b7} TODAY",
            &runs_val,
            p.brand,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
        container(action_stat("AVG TIME", &avg_val, p.success, None, palette))
            .width(Length::FillPortion(1)),
        container(action_stat(
            "ERRORS \u{00b7} 7D",
            &errors_val,
            errors_color,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
    ]
    .spacing(8);

    container(cells)
        .width(Length::Fill)
        .padding([18, 12])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_input,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

pub async fn load_clip_options(dp: Arc<dyn DataProvider>) -> Vec<(ClipId, String)> {
    dp.soundboard_clips_repo()
        .list()
        .await
        .map(|clips| clips.into_iter().map(|c| (c.id, c.name)).collect())
        .unwrap_or_default()
}

pub async fn move_sub_action(
    dp: Arc<dyn DataProvider>,
    action_id: ActionId,
    from: usize,
    to: usize,
) -> Result<ActionId, StorageError> {
    let Some(mut action) = dp.action_repo().get(action_id).await? else {
        return Err(StorageError::NotFound {
            key: action_id.to_string(),
        });
    };
    let len = action.sub_actions.len();
    if from < len && to < len && from != to {
        let item = action.sub_actions.remove(from);
        action.sub_actions.insert(to, item);
        dp.action_repo().save(&action).await?;
    }
    Ok(action_id)
}

pub async fn duplicate_sub_action(
    dp: Arc<dyn DataProvider>,
    action_id: ActionId,
    index: usize,
) -> Result<ActionId, StorageError> {
    let Some(mut action) = dp.action_repo().get(action_id).await? else {
        return Err(StorageError::NotFound {
            key: action_id.to_string(),
        });
    };
    if index < action.sub_actions.len() {
        let copy = action.sub_actions[index].clone();
        action.sub_actions.insert(index + 1, copy);
        dp.action_repo().save(&action).await?;
    }
    Ok(action_id)
}

pub async fn remove_sub_action(
    dp: Arc<dyn DataProvider>,
    action_id: ActionId,
    index: usize,
) -> Result<(), StorageError> {
    let Some(mut action) = dp.action_repo().get(action_id).await? else {
        return Err(StorageError::NotFound {
            key: action_id.to_string(),
        });
    };
    if index < action.sub_actions.len() {
        action.sub_actions.remove(index);
    }
    dp.action_repo().save(&action).await
}

pub fn action_rename_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:action_rename")
}

pub fn update(state: &mut ActionsState, rt: &RuntimeView, msg: ActionsMsg) -> Task<Message> {
    match msg {
        ActionsMsg::LoadRequested => {
            state.loading = true;
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move { load_actions_tree(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::TreeLoaded(r)),
            )
        }
        ActionsMsg::TreeLoaded(Ok(tree)) => {
            state.tree = tree;
            state.loading = false;
            Task::none()
        }
        ActionsMsg::TreeLoaded(Err(e)) => {
            state.loading = false;
            tracing::warn!(error = %e, "actions tree load failed");
            Task::none()
        }
        ActionsMsg::ActionSelected(id) => {
            let already_loaded = state.selected == Some(id)
                && state
                    .detail
                    .as_ref()
                    .map(|d| d.action.id == id)
                    .unwrap_or(false);
            if already_loaded {
                return Task::none();
            }
            state.selected = Some(id);
            state.detail = None;
            state.telemetry = None;
            state.telemetry_loading = true;
            let dp1 = Arc::clone(&rt.backend);
            let dp2 = Arc::clone(&rt.backend);
            let detail_task = Task::perform(
                async move { load_action_detail(dp1, id).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::DetailLoaded(r)),
            );
            let telemetry_task = Task::perform(async move { load_telemetry(dp2, id).await }, |r| {
                Message::Actions(ActionsMsg::TelemetryLoaded(r))
            });
            Task::batch([detail_task, telemetry_task])
        }
        ActionsMsg::DetailLoaded(Ok(detail)) => {
            state.detail = Some(detail);
            Task::none()
        }
        ActionsMsg::DetailLoaded(Err(e)) => {
            state.detail = None;
            tracing::warn!(error = %e, "action detail load failed");
            Task::none()
        }
        ActionsMsg::ToggleEnabled(id, enabled) => {
            if let Some(detail) = state.detail.as_mut()
                && detail.action.id == id
            {
                detail.action.enabled = enabled;
            }
            for group in &mut state.tree {
                for summary in &mut group.actions {
                    if summary.id == id {
                        summary.enabled = enabled;
                    }
                }
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let Some(mut action) =
                        dp.action_repo().get(id).await.map_err(|e| e.to_string())?
                    else {
                        return Err("action not found".to_string());
                    };
                    action.enabled = enabled;
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::EnabledToggled(r)),
            )
        }
        ActionsMsg::EnabledToggled(Ok(())) => Task::none(),
        ActionsMsg::EnabledToggled(Err(e)) => {
            tracing::warn!(error = %e, "toggle enabled persist failed");
            Task::none()
        }
        ActionsMsg::TestTrigger(id) => {
            let bus = Arc::clone(&rt.bus);
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let detail = load_action_detail(Arc::clone(&dp), id)
                        .await
                        .map_err(|e| e.to_string())?;
                    let event = match detail.triggers.first() {
                        Some(trigger) => synthesize_test_event(trigger, &detail.commands),
                        None => Event::new(
                            EventSource::Core,
                            "test.trigger",
                            serde_json::json!({ "action_id": id.to_string() }),
                        ),
                    };
                    let event_id = event.id;
                    bus.publish(event);
                    bus.replay_and_publish(event_id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    if let Err(e) = r {
                        tracing::warn!(error = %e, "test trigger failed");
                    }
                    Message::Noop
                },
            )
        }
        ActionsMsg::DeleteAction(id) => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move { dp.action_repo().delete(id).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::ActionDeleted(r.map(|_| ()))),
            )
        }
        ActionsMsg::ActionDeleted(Ok(())) => {
            state.selected = None;
            state.detail = None;
            Task::done(Message::Actions(ActionsMsg::LoadRequested))
        }
        ActionsMsg::ActionDeleted(Err(e)) => {
            tracing::warn!(error = %e, "delete action failed");
            Task::none()
        }
        ActionsMsg::DuplicateAction(id) => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let original = dp
                        .action_repo()
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "source action not found".to_string())?;
                    let mut copy = original.clone();
                    copy.id = forge_types::ActionId::new();
                    copy.name = format!("{} (copy)", original.name);
                    dp.action_repo()
                        .save(&copy)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(copy.id)
                },
                |r| Message::Actions(ActionsMsg::ActionDuplicated(r)),
            )
        }
        ActionsMsg::ActionDuplicated(Ok(new_id)) => {
            tracing::info!(action_id = %new_id, "action duplicated");
            Task::done(Message::Actions(ActionsMsg::LoadRequested))
        }
        ActionsMsg::ActionDuplicated(Err(e)) => {
            tracing::warn!(error = %e, "duplicate action failed");
            Task::none()
        }
        ActionsMsg::DeleteTrigger(trigger_id, action_id) => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_repo()
                        .delete(trigger_id)
                        .await
                        .map(|_| action_id)
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::TriggerDeleted(r)),
            )
        }
        ActionsMsg::TriggerDeleted(Ok(action_id)) => {
            Task::done(Message::Actions(ActionsMsg::ActionSelected(action_id)))
        }
        ActionsMsg::TriggerDeleted(Err(e)) => {
            tracing::warn!(error = %e, "delete trigger failed");
            Task::none()
        }
        ActionsMsg::OpenAddActionModal => Task::done(Message::Actions(ActionsMsg::Editor(
            ActionEditorMsg::AddAction(AddActionMsg::OpenRequested),
        ))),
        ActionsMsg::OpenAddTriggerModal(action_id) => {
            Task::done(Message::Actions(ActionsMsg::Editor(
                ActionEditorMsg::AddTrigger(AddTriggerMsg::OpenRequested(action_id)),
            )))
        }
        ActionsMsg::SearchChanged(q) => {
            state.search = q;
            Task::none()
        }
        ActionsMsg::FilterChanged(f) => {
            state.filter = f;
            Task::none()
        }
        ActionsMsg::ToggleGroupCollapsed(cat) => {
            if state.collapsed_groups.contains(&cat) {
                state.collapsed_groups.remove(&cat);
            } else {
                state.collapsed_groups.insert(cat);
            }
            Task::none()
        }
        ActionsMsg::TelemetryLoaded(Ok(t)) => {
            state.telemetry = Some(t);
            state.telemetry_loading = false;
            Task::none()
        }
        ActionsMsg::TelemetryLoaded(Err(e)) => {
            state.telemetry = None;
            state.telemetry_loading = false;
            tracing::warn!(error = %e, "action telemetry load failed");
            Task::none()
        }
        ActionsMsg::ToggleStepMenu(i) => {
            state.step_menu_open = if state.step_menu_open == Some(i) {
                None
            } else {
                Some(i)
            };
            Task::none()
        }
        ActionsMsg::DismissStepMenu => {
            state.step_menu_open = None;
            Task::none()
        }
        ActionsMsg::ToggleActionMenu(id) => {
            state.action_menu_open = if state.action_menu_open == Some(id) {
                None
            } else {
                Some(id)
            };
            Task::none()
        }
        ActionsMsg::DismissActionMenu => {
            state.action_menu_open = None;
            Task::none()
        }
        ActionsMsg::RenameStarted(id) => {
            let current_name = state
                .tree
                .iter()
                .flat_map(|g| g.actions.iter())
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            state.renaming_action = Some((id, current_name));
            state.action_menu_open = None;
            iced::widget::operation::focus(action_rename_input_id())
        }
        ActionsMsg::RenameBufferChanged(buf) => {
            if let Some((_, name)) = state.renaming_action.as_mut() {
                *name = buf;
            }
            Task::none()
        }
        ActionsMsg::RenameCancel => {
            state.renaming_action = None;
            Task::none()
        }
        ActionsMsg::RenameSubmit => {
            let Some((id, name)) = state.renaming_action.clone() else {
                return Task::none();
            };
            let trimmed = name.trim().to_owned();
            if trimmed.is_empty() {
                state.renaming_action = None;
                return Task::none();
            }
            let already_taken = state
                .tree
                .iter()
                .flat_map(|g| g.actions.iter())
                .any(|a| a.id != id && a.name.eq_ignore_ascii_case(&trimmed));
            if already_taken {
                let toast_msg = format!("Name \u{201c}{trimmed}\u{201d} is already taken");
                return Task::done(Message::Toast(ToastMsg::Fired {
                    kind: forge_widgets::ToastKind::Error,
                    message: toast_msg,
                    duration_ms: 3000,
                }));
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let mut action = dp
                        .action_repo()
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "action not found".to_owned())?;
                    action.name = trimmed.clone();
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>((id, trimmed))
                },
                |r| Message::Actions(ActionsMsg::RenameSaved(r)),
            )
        }
        ActionsMsg::RenameSaved(Ok((id, new_name))) => {
            state.renaming_action = None;
            for group in &mut state.tree {
                let touched = group.actions.iter().any(|s| s.id == id);
                for summary in &mut group.actions {
                    if summary.id == id {
                        summary.name = new_name.clone();
                    }
                }
                if touched {
                    group.actions.sort_by_key(|a| a.name.to_lowercase());
                }
            }
            if let Some(detail) = state.detail.as_mut()
                && detail.action.id == id
            {
                detail.action.name = new_name;
            }
            Task::none()
        }
        ActionsMsg::RenameSaved(Err(e)) => {
            state.renaming_action = None;
            tracing::warn!(error = %e, "action rename failed");
            Task::none()
        }
        ActionsMsg::Editor(sub) => match sub {
            ActionEditorMsg::AddAction(m) => {
                crate::action_editor::add_action_update(&mut state.add_action_modal, rt, m)
            }
            ActionEditorMsg::AddTrigger(m) => {
                crate::action_editor::add_trigger_update(&mut state.add_trigger_modal, rt, m)
            }
            ActionEditorMsg::AddSubAction(m) => crate::action_editor::add_sub_action_update(
                &mut state.add_sub_action_modal,
                rt,
                state.detail.as_ref(),
                m,
            ),
            ActionEditorMsg::RemoveSubAction(m) => {
                crate::action_editor::remove_sub_action_update(state.selected, rt, m)
            }
            ActionEditorMsg::MoveSubAction(m) => crate::action_editor::move_sub_action_update(
                rt,
                state
                    .detail
                    .as_ref()
                    .map(|d| d.action.sub_actions.len())
                    .unwrap_or(0),
                m,
            ),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
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

    async fn open_backend() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
                .await
                .unwrap(),
        )
    }

    async fn make_action(dp: &Arc<dyn DataProvider>, name: &str, group: Option<&str>) -> Action {
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
            execution_mode: forge_types::ExecutionMode::Sequential,
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
    async fn actions_without_triggers_land_in_ungrouped() {
        let dp = open_backend().await;
        let a1 = make_action(&dp, "!so", Some("Chat Commands")).await;
        let a2 = make_action(&dp, "HydrateCheck", Some("Timers")).await;
        dp.action_repo().save(&a1).await.unwrap();
        dp.action_repo().save(&a2).await.unwrap();

        let tree = load_actions_tree(dp).await.unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].category, TriggerCategory::Ungrouped);
        assert_eq!(tree[0].actions.len(), 2);
    }

    #[tokio::test]
    async fn chat_trigger_produces_chat_group() {
        use forge_types::TriggerId;
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&a).await.unwrap();
        let t = Trigger {
            id: TriggerId::new(),
            action_id: a.id,
            kind: TriggerKind::TwitchChatCommand,
            config: std::collections::BTreeMap::new(),
        };
        dp.trigger_repo().save(&t).await.unwrap();

        let tree = load_actions_tree(dp).await.unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].category, TriggerCategory::Chat);
    }

    #[tokio::test]
    async fn ungrouped_action_goes_into_ungrouped_group() {
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&a).await.unwrap();

        let tree = load_actions_tree(dp).await.unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].category, TriggerCategory::Ungrouped);
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

    #[test]
    fn add_sub_action_form_send_chat_invalid_without_message() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::SendChat;
        form.config.send_chat_message = String::new();
        assert!(!form.is_valid());
    }

    #[test]
    fn add_sub_action_form_send_chat_valid_with_message() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::SendChat;
        form.config.send_chat_message = "Hello %user%!".to_string();
        assert!(form.is_valid());
    }

    #[test]
    fn add_sub_action_form_set_global_invalid_without_name() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::SetGlobal;
        form.config.set_global_name = String::new();
        assert!(!form.is_valid());
    }

    #[test]
    fn add_sub_action_form_set_global_valid_with_name() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::SetGlobal;
        form.config.set_global_name = "counter".to_string();
        assert!(form.is_valid());
    }

    #[test]
    fn add_sub_action_form_delay_invalid_with_non_numeric() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::Delay;
        form.config.delay_ms = "abc".to_string();
        assert!(!form.is_valid());
    }

    #[test]
    fn add_sub_action_form_delay_valid_with_numeric() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::Delay;
        form.config.delay_ms = "500".to_string();
        assert!(form.is_valid());
    }

    #[test]
    fn add_sub_action_form_log_invalid_without_message() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::Log;
        form.config.log_message = String::new();
        assert!(!form.is_valid());
    }

    #[test]
    fn add_sub_action_form_log_valid_with_message() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::Log;
        form.config.log_message = "action started".to_string();
        assert!(form.is_valid());
    }

    #[test]
    fn add_sub_action_form_play_sound_invalid_without_clip() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::PlaySound;
        assert!(!form.is_valid());
    }

    #[test]
    fn add_sub_action_form_play_sound_valid_with_clip() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::PlaySound;
        form.config.play_sound_clip_id = Some(ClipId::new());
        assert!(form.is_valid());
    }

    #[tokio::test]
    async fn load_clip_options_empty_db_returns_empty() {
        let dp = open_backend().await;
        let clips = load_clip_options(dp).await;
        assert!(clips.is_empty());
    }

    #[tokio::test]
    async fn save_sub_action_appends_send_chat() {
        let dp = open_backend().await;
        let action = make_action(&dp, "say hello", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let spec = SubActionSpec::SendChat {
            message: "Hello %user%!".to_string(),
            target: "twitch".to_string(),
        };
        save_sub_action(Arc::clone(&dp), action.id, spec.clone(), None)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
        assert_eq!(loaded.sub_actions[0], spec);
    }

    #[tokio::test]
    async fn save_sub_action_appends_set_global() {
        let dp = open_backend().await;
        let action = make_action(&dp, "track", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let spec = SubActionSpec::SetGlobal {
            name: "counter".to_string(),
            value: "1".to_string(),
        };
        save_sub_action(Arc::clone(&dp), action.id, spec.clone(), None)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions[0], spec);
    }

    #[tokio::test]
    async fn save_sub_action_delay_stores_ms_correctly() {
        let dp = open_backend().await;
        let action = make_action(&dp, "pause", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let spec = SubActionSpec::Delay { ms: 500 };
        save_sub_action(Arc::clone(&dp), action.id, spec.clone(), None)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions[0], SubActionSpec::Delay { ms: 500 });
    }

    #[tokio::test]
    async fn remove_sub_action_removes_at_valid_index() {
        let dp = open_backend().await;
        let mut action = make_action(&dp, "multi", None).await;
        action.sub_actions = vec![
            SubActionSpec::Delay { ms: 100 },
            SubActionSpec::Log {
                level: LogLevel::Info,
                message: "done".to_string(),
            },
        ];
        dp.action_repo().save(&action).await.unwrap();

        remove_sub_action(Arc::clone(&dp), action.id, 0)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
        assert!(matches!(loaded.sub_actions[0], SubActionSpec::Log { .. }));
    }

    #[tokio::test]
    async fn remove_sub_action_out_of_range_leaves_action_unchanged() {
        let dp = open_backend().await;
        let mut action = make_action(&dp, "single", None).await;
        action.sub_actions = vec![SubActionSpec::Delay { ms: 250 }];
        dp.action_repo().save(&action).await.unwrap();

        remove_sub_action(Arc::clone(&dp), action.id, 99)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
    }
}
