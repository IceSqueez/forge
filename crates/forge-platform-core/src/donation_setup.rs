use std::time::Duration;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenProblem {
    Missing,
    Malformed,
    Rejected,
    ProfileIncomplete,
    NoJarPicked,
    JarNotFound,
    CoolingDown { retry_after_secs: u32 },
    Unreachable,
    ServiceError { status: u16 },
    UnreadableAnswer,
    StorageFailed,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTarget {
    pub id: String,
    pub title: Option<String>,
    pub currency: Option<String>,
    pub goal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenCheck {
    Account { nickname: Option<String> },
    Targets(Vec<AccountTarget>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavedAccount {
    Account { nickname: Option<String> },
    Target(AccountTarget),
}

pub trait AdjustablePoll: Send + Sync {
    fn default_interval(&self) -> Duration;
    fn bounded_interval(&self, requested: Duration) -> Duration;
    fn apply_interval(&self, requested: Duration) -> Duration;
}

#[async_trait]
pub trait DonationAccountSetup: Send + Sync {
    fn brand_name(&self) -> &'static str;
    fn token_page(&self) -> &'static str;
    fn picks_a_target(&self) -> bool {
        false
    }
    async fn check_token(&self, raw: &str) -> Result<TokenCheck, TokenProblem>;
    async fn save_token(
        &self,
        raw: &str,
        target: Option<&str>,
    ) -> Result<SavedAccount, TokenProblem>;
    async fn remove_token(&self) -> Result<(), TokenProblem>;
    async fn stored_target(&self) -> Option<String> {
        None
    }
    async fn stored_targets(&self) -> Result<Vec<AccountTarget>, TokenProblem> {
        Ok(Vec::new())
    }
    async fn select_target(&self, target: &str) -> Result<SavedAccount, TokenProblem>;
}
