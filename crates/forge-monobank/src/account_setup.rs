use async_trait::async_trait;
use forge_platform_core::{
    AccountTarget, DonationAccountSetup, SavedAccount, TokenCheck, TokenProblem,
};

use crate::error::MonobankError;
use crate::integration::MONOBANK_INTEGRATION;
use crate::jar::MonobankJar;
use crate::provider::MonobankProvider;

const TOKEN_PAGE: &str = "https://api.monobank.ua/";

impl From<&MonobankError> for TokenProblem {
    fn from(error: &MonobankError) -> Self {
        match error {
            MonobankError::MissingToken => Self::Missing,
            MonobankError::MalformedToken => Self::Malformed,
            MonobankError::Unauthorized => Self::Rejected,
            MonobankError::MissingJar | MonobankError::InvalidJarId => Self::NoJarPicked,
            MonobankError::JarNotFound => Self::JarNotFound,
            MonobankError::CoolingDown { retry_after_secs }
            | MonobankError::RateLimited { retry_after_secs } => Self::CoolingDown {
                retry_after_secs: *retry_after_secs,
            },
            MonobankError::Http { status } => Self::ServiceError { status: *status },
            MonobankError::Network { .. } => Self::Unreachable,
            MonobankError::MalformedResponse { .. } | MonobankError::InvalidDonation { .. } => {
                Self::UnreadableAnswer
            }
            MonobankError::Credential { .. } => Self::StorageFailed,
            MonobankError::ClientInit { .. } | MonobankError::NoRuntime => Self::Local,
        }
    }
}

fn problem_of(error: MonobankError) -> TokenProblem {
    TokenProblem::from(&error)
}

fn target_of(jar: MonobankJar) -> AccountTarget {
    AccountTarget {
        id: jar.id,
        title: jar.title,
        currency: jar.currency.as_ref().map(ToString::to_string),
        goal: jar.goal.as_ref().map(|goal| goal.formatted()),
    }
}

fn targets_of(jars: Vec<MonobankJar>) -> Vec<AccountTarget> {
    jars.into_iter().map(target_of).collect()
}

#[async_trait]
impl DonationAccountSetup for MonobankProvider {
    fn brand_name(&self) -> &'static str {
        MONOBANK_INTEGRATION.brand_name
    }

    fn token_page(&self) -> &'static str {
        TOKEN_PAGE
    }

    fn picks_a_target(&self) -> bool {
        true
    }

    async fn check_token(&self, raw: &str) -> Result<TokenCheck, TokenProblem> {
        let jars = self.verify_token(raw).await.map_err(problem_of)?;
        Ok(TokenCheck::Targets(targets_of(jars)))
    }

    async fn save_token(
        &self,
        raw: &str,
        target: Option<&str>,
    ) -> Result<SavedAccount, TokenProblem> {
        let target = target.ok_or(TokenProblem::NoJarPicked)?;
        let jar = MonobankProvider::save_token(self, raw, target)
            .await
            .map_err(problem_of)?;
        Ok(SavedAccount::Target(target_of(jar)))
    }

    async fn remove_token(&self) -> Result<(), TokenProblem> {
        MonobankProvider::remove_token(self)
            .await
            .map(|_| ())
            .map_err(problem_of)
    }

    async fn stored_target(&self) -> Option<String> {
        match self.selected_jar_id().await {
            Ok(jar) => jar,
            Err(error) => {
                tracing::warn!(%error, "could not read the selected Monobank jar");
                None
            }
        }
    }

    async fn stored_targets(&self) -> Result<Vec<AccountTarget>, TokenProblem> {
        self.stored_jars().await.map(targets_of).map_err(problem_of)
    }

    async fn select_target(&self, target: &str) -> Result<SavedAccount, TokenProblem> {
        let jar = self.select_jar(target).await.map_err(problem_of)?;
        Ok(SavedAccount::Target(target_of(jar)))
    }
}
