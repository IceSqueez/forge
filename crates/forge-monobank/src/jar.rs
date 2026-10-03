use std::sync::Mutex;
use std::time::{Duration, Instant};

use forge_types::{CurrencyCode, MoneyAmount};

use crate::api::MonobankApi;
use crate::currency;
use crate::error::MonobankError;
use crate::token::MonobankToken;
use crate::wire::JarWire;

const JAR_LIST_REUSE_WINDOW: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct JarId(String);

impl JarId {
    pub(crate) fn parse(raw: &str) -> Result<Self, MonobankError> {
        let trimmed = raw.trim();
        let path_safe = trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if trimmed.is_empty() || !path_safe {
            return Err(MonobankError::InvalidJarId);
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonobankJar {
    pub id: String,
    pub send_id: Option<String>,
    pub title: Option<String>,
    pub currency: Option<CurrencyCode>,
    pub goal: Option<MoneyAmount>,
}

impl MonobankJar {
    pub(crate) fn from_wire(wire: JarWire) -> Option<Self> {
        let id = JarId::parse(&wire.id).ok()?;
        let known_currency = wire.currency_code.and_then(currency::from_iso_numeric);
        let goal = known_currency.as_ref().and_then(|currency| {
            wire.goal
                .and_then(|goal| u64::try_from(goal).ok())
                .filter(|goal| *goal > 0)
                .and_then(|goal| currency.amount(goal).ok())
        });
        Some(Self {
            id: id.as_str().to_owned(),
            send_id: wire.send_id.filter(|send_id| !send_id.trim().is_empty()),
            title: wire
                .title
                .map(|title| title.trim().to_owned())
                .filter(|title| !title.is_empty()),
            currency: known_currency.map(|currency| currency.code),
            goal,
        })
    }
}

struct JarListing {
    owner: MonobankToken,
    fetched_at: Instant,
    jars: Vec<MonobankJar>,
}

#[derive(Default)]
pub(crate) struct JarDirectory {
    latest: Mutex<Option<JarListing>>,
}

impl JarDirectory {
    pub(crate) async fn jars(
        &self,
        api: &MonobankApi,
        token: &MonobankToken,
    ) -> Result<Vec<MonobankJar>, MonobankError> {
        if let Some(jars) = self.reusable(token) {
            return Ok(jars);
        }
        let jars = api.jars(token).await?;
        *self.lock() = Some(JarListing {
            owner: token.clone(),
            fetched_at: Instant::now(),
            jars: jars.clone(),
        });
        Ok(jars)
    }

    pub(crate) async fn find(
        &self,
        api: &MonobankApi,
        token: &MonobankToken,
        jar_id: &JarId,
    ) -> Result<MonobankJar, MonobankError> {
        self.jars(api, token)
            .await?
            .into_iter()
            .find(|jar| jar.id == jar_id.as_str())
            .ok_or(MonobankError::JarNotFound)
    }

    pub(crate) fn forget(&self) {
        *self.lock() = None;
    }

    fn reusable(&self, token: &MonobankToken) -> Option<Vec<MonobankJar>> {
        self.lock()
            .as_ref()
            .filter(|listing| {
                listing.owner == *token && listing.fetched_at.elapsed() < JAR_LIST_REUSE_WINDOW
            })
            .map(|listing| listing.jars.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<JarListing>> {
        self.latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
