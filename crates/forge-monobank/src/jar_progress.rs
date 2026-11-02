use forge_events::TypedEvent;
use forge_types::{CurrencyCode, MoneyAmount};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::currency;
use crate::jar::MonobankJar;
use crate::wire::{TRANSACTION_BALANCE_FIELD, TRANSACTION_CURRENCY_FIELD, TRANSACTION_TIME_FIELD};

pub const JAR_PROGRESS_KIND: &str = "monobank.jar.progress";

const PERCENT_SCALE: f64 = 100.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JarProgressed {
    pub jar_id: String,
    pub jar_title: Option<String>,
    pub balance_micros: u64,
    pub goal_micros: Option<u64>,
    pub currency: CurrencyCode,
    pub is_synced: bool,
}

impl JarProgressed {
    pub fn balance(&self) -> MoneyAmount {
        MoneyAmount::from_micros(self.balance_micros, self.currency.clone())
    }

    pub fn goal(&self) -> Option<MoneyAmount> {
        self.goal_micros
            .map(|goal| MoneyAmount::from_micros(goal, self.currency.clone()))
    }

    pub fn percent(&self) -> f64 {
        match self.goal() {
            Some(goal) if goal.micros() > 0 => {
                self.balance().major_units() / goal.major_units() * PERCENT_SCALE
            }
            _ => 0.0,
        }
    }
}

impl TypedEvent for JarProgressed {
    const KIND: &'static str = JAR_PROGRESS_KIND;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JarProgressOrigin {
    Live,
    ConnectSync,
}

#[derive(Default)]
pub(crate) struct JarProgressTracker {
    jar: Option<MonobankJar>,
    published: Option<MoneyAmount>,
}

impl JarProgressTracker {
    pub(crate) fn connected(&mut self, jar: MonobankJar) {
        self.jar = Some(jar);
        self.published = None;
    }

    pub(crate) fn observe(&mut self, newest_balance: Option<MoneyAmount>) -> Option<JarProgressed> {
        let jar = self.jar.as_ref()?;
        let (balance, origin) = match &self.published {
            None => (
                newest_balance.or_else(|| jar.balance.clone())?,
                JarProgressOrigin::ConnectSync,
            ),
            Some(published) => {
                let balance = newest_balance.filter(|balance| balance != published)?;
                (balance, JarProgressOrigin::Live)
            }
        };
        self.published = Some(balance.clone());
        Some(progress_of(jar, &balance, origin))
    }
}

fn progress_of(
    jar: &MonobankJar,
    balance: &MoneyAmount,
    origin: JarProgressOrigin,
) -> JarProgressed {
    JarProgressed {
        jar_id: jar.id.as_str().to_owned(),
        jar_title: jar.title.clone(),
        balance_micros: balance.micros(),
        goal_micros: jar
            .goal
            .as_ref()
            .filter(|goal| goal.currency() == balance.currency())
            .map(MoneyAmount::micros),
        currency: balance.currency().clone(),
        is_synced: origin == JarProgressOrigin::ConnectSync,
    }
}

pub(crate) fn newest_balance(items: &[Value]) -> Option<MoneyAmount> {
    let (_, newest) = items
        .iter()
        .rev()
        .filter_map(|item| {
            item.get(TRANSACTION_TIME_FIELD)
                .and_then(Value::as_i64)
                .map(|time| (time, item))
        })
        .max_by_key(|(time, _)| *time)?;
    let minor_units = newest
        .get(TRANSACTION_BALANCE_FIELD)
        .and_then(Value::as_i64)
        .and_then(|balance| u64::try_from(balance).ok())?;
    newest
        .get(TRANSACTION_CURRENCY_FIELD)
        .and_then(Value::as_i64)
        .and_then(currency::from_iso_numeric)?
        .amount(minor_units)
        .ok()
}
