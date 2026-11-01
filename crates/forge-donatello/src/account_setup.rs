use std::time::Duration;

use async_trait::async_trait;
use forge_platform_core::{
    AdjustablePoll, DonationAccountSetup, SavedAccount, TokenCheck, TokenProblem,
};

use crate::config::DEFAULT_POLL_INTERVAL;
use crate::error::DonatelloError;
use crate::integration::DONATELLO_INTEGRATION;
use crate::provider::DonatelloProvider;

const TOKEN_PAGE: &str = "https://donatello.to/panel/doc-api";

impl From<&DonatelloError> for TokenProblem {
    fn from(error: &DonatelloError) -> Self {
        match error {
            DonatelloError::MissingToken => Self::Missing,
            DonatelloError::MalformedToken => Self::Malformed,
            DonatelloError::Unauthorized => Self::Rejected,
            DonatelloError::ProfileIncomplete => Self::ProfileIncomplete,
            DonatelloError::RateLimited { retry_after_secs } => Self::CoolingDown {
                retry_after_secs: *retry_after_secs,
            },
            DonatelloError::RateLimitExhausted => Self::CoolingDown {
                retry_after_secs: 0,
            },
            DonatelloError::Http { status } => Self::ServiceError { status: *status },
            DonatelloError::Network { .. } => Self::Unreachable,
            DonatelloError::MalformedResponse { .. } | DonatelloError::InvalidDonation { .. } => {
                Self::UnreadableAnswer
            }
            DonatelloError::Credential(_) | DonatelloError::CredentialBundle { .. } => {
                Self::StorageFailed
            }
            DonatelloError::ClientInit { .. } | DonatelloError::NoRuntime => Self::Local,
        }
    }
}

fn problem_of(error: DonatelloError) -> TokenProblem {
    TokenProblem::from(&error)
}

#[async_trait]
impl DonationAccountSetup for DonatelloProvider {
    fn brand_name(&self) -> &'static str {
        DONATELLO_INTEGRATION.brand_name
    }

    fn token_page(&self) -> &'static str {
        TOKEN_PAGE
    }

    async fn check_token(&self, raw: &str) -> Result<TokenCheck, TokenProblem> {
        let account = self.verify_token(raw).await.map_err(problem_of)?;
        Ok(TokenCheck::Account {
            nickname: account.nickname,
        })
    }

    async fn save_token(
        &self,
        raw: &str,
        _target: Option<&str>,
    ) -> Result<SavedAccount, TokenProblem> {
        let account = DonatelloProvider::save_token(self, raw)
            .await
            .map_err(problem_of)?;
        Ok(SavedAccount::Account {
            nickname: account.nickname,
        })
    }

    async fn remove_token(&self) -> Result<(), TokenProblem> {
        DonatelloProvider::remove_token(self)
            .await
            .map(|_| ())
            .map_err(problem_of)
    }

    async fn select_target(&self, _target: &str) -> Result<SavedAccount, TokenProblem> {
        Err(TokenProblem::NoJarPicked)
    }
}

impl AdjustablePoll for DonatelloProvider {
    fn default_interval(&self) -> Duration {
        DEFAULT_POLL_INTERVAL
    }

    fn bounded_interval(&self, requested: Duration) -> Duration {
        self.bounded_poll_interval(requested)
    }

    fn apply_interval(&self, requested: Duration) -> Duration {
        self.set_poll_interval(requested)
    }
}
