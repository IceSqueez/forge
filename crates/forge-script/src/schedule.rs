use forge_types::Variant;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq)]
pub enum ScriptScheduleDue {
    AfterSeconds(i64),
    At(Variant),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptScheduleRequest {
    pub action_id_or_name: String,
    pub due: ScriptScheduleDue,
    pub key: Option<String>,
    pub inherit_args: bool,
    pub skip_if_late_minutes: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptSchedulePlacement {
    pub id: i64,
    pub due_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptScheduleError {
    #[error("no action has the id or name '{0}'")]
    UnknownAction(String),
    #[error("{count} actions are named '{name}', schedule it by id instead")]
    AmbiguousAction { name: String, count: usize },
    #[error("actions cannot be listed: {0}")]
    ActionsUnavailable(String),
    #[error("{0}")]
    Rejected(String),
}

#[async_trait::async_trait]
pub trait ActionScheduler: Send + Sync {
    async fn schedule(
        &self,
        request: ScriptScheduleRequest,
    ) -> Result<ScriptSchedulePlacement, ScriptScheduleError>;

    async fn cancel_by_key(&self, key: &str) -> Result<bool, ScriptScheduleError>;
}
