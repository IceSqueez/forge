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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::{Value, json};

    use super::{JarId, MonobankJar};
    use crate::error::MonobankError;
    use crate::wire::JarWire;

    fn jar(overrides: Value) -> Option<MonobankJar> {
        let mut raw = json!({
            "id": "jar-AbC_123",
            "sendId": "jar/abc",
            "title": "На стрім",
            "currencyCode": 980,
            "goal": 1_000_000,
            "balance": 2_500,
        });
        for (key, value) in overrides.as_object().unwrap() {
            raw[key] = value.clone();
        }
        MonobankJar::from_wire(serde_json::from_value::<JarWire>(raw).unwrap())
    }

    #[test]
    fn jar_id_accepts_path_safe_identifiers_and_trims_them() {
        for (raw, id) in [
            ("jar-AbC_123", "jar-AbC_123"),
            ("  abc123 \n", "abc123"),
            ("A", "A"),
        ] {
            assert_eq!(
                JarId::parse(raw).ok().as_ref().map(JarId::as_str),
                Some(id),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn jar_id_rejects_anything_that_could_escape_the_statement_path() {
        for raw in [
            "",
            "   ",
            "a/b",
            "/",
            "..",
            "../x",
            "a b",
            "a%2Fb",
            "a?b",
            "a#b",
            "a.b",
            "банка",
            "a\\b",
        ] {
            assert!(
                matches!(JarId::parse(raw), Err(MonobankError::InvalidJarId)),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn jar_with_an_unsafe_id_is_dropped_from_the_listing() {
        assert_eq!(jar(json!({ "id": "../personal" })), None);
    }

    #[test]
    fn jar_goal_is_read_in_minor_units_of_the_jar_currency() {
        let jar = jar(json!({})).unwrap();
        assert_eq!(
            jar.goal
                .map(|goal| (goal.micros(), goal.currency().as_str().to_owned())),
            Some((10_000_000_000, "UAH".to_owned()))
        );
    }

    #[test]
    fn jar_without_a_positive_goal_or_known_currency_has_no_goal() {
        for overrides in [
            json!({ "goal": 0 }),
            json!({ "goal": -5 }),
            json!({ "goal": null }),
            json!({ "currencyCode": 643 }),
            json!({ "currencyCode": null }),
        ] {
            assert_eq!(jar(overrides.clone()).unwrap().goal, None, "{overrides}");
        }
    }

    #[test]
    fn jar_with_an_unknown_currency_is_listed_without_a_currency() {
        let jar = jar(json!({ "currencyCode": 643 })).unwrap();
        assert_eq!((jar.id.as_str(), jar.currency), ("jar-AbC_123", None));
    }

    #[test]
    fn blank_title_and_send_id_are_absent() {
        let jar = jar(json!({ "title": "   ", "sendId": " " })).unwrap();
        assert_eq!((jar.title, jar.send_id), (None, None));
    }
}
