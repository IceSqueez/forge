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

#[cfg(test)]
mod tests {
    use forge_types::MoneyAmount;
    use serde_json::{Value, json};

    use super::{JarProgressTracker, JarProgressed, newest_balance};
    use crate::currency;
    use crate::jar::{JarId, MonobankJar};

    const UAH: i64 = 980;
    const USD: i64 = 840;

    fn money(numeric: i64, minor_units: u64) -> MoneyAmount {
        currency::from_iso_numeric(numeric)
            .unwrap()
            .amount(minor_units)
            .unwrap()
    }

    fn uah(minor_units: u64) -> MoneyAmount {
        money(UAH, minor_units)
    }

    fn item(time: i64, balance: i64) -> Value {
        json!({ "time": time, "balance": balance, "currencyCode": UAH })
    }

    fn jar(balance: Option<MoneyAmount>) -> MonobankJar {
        MonobankJar {
            id: JarId::parse("jarAAA111").unwrap(),
            send_id: None,
            title: Some("Drone".to_owned()),
            currency: Some(uah(0).currency().clone()),
            goal: Some(uah(100_000)),
            balance,
        }
    }

    fn connected(balance: Option<MoneyAmount>) -> JarProgressTracker {
        let mut tracker = JarProgressTracker::default();
        tracker.connected(jar(balance));
        tracker
    }

    fn progress(balance_micros: u64, goal_micros: Option<u64>) -> JarProgressed {
        JarProgressed {
            jar_id: "jarAAA111".to_owned(),
            jar_title: None,
            balance_micros,
            goal_micros,
            currency: uah(0).currency().clone(),
            is_synced: false,
        }
    }

    #[test]
    fn newest_balance_reads_the_item_with_the_latest_time_whatever_the_listing_order() {
        let items = [
            item(100, 1_000),
            item(300, 3_000),
            json!({ "balance": 9_000, "currencyCode": UAH }),
            item(200, 2_000),
        ];

        assert_eq!(newest_balance(&items), Some(uah(3_000)));
    }

    #[test]
    fn newest_balance_on_a_time_tie_takes_the_first_listed_item() {
        let items = [item(300, 1_000), item(300, 2_000), item(100, 5_000)];

        assert_eq!(newest_balance(&items), Some(uah(1_000)));
    }

    #[test]
    fn newest_balance_is_read_in_minor_units_of_the_item_currency() {
        let items = [json!({ "time": 1, "balance": 1_234, "currencyCode": USD })];

        assert_eq!(newest_balance(&items), Some(money(USD, 1_234)));
    }

    #[test]
    fn newest_balance_is_absent_when_the_newest_item_carries_no_usable_balance() {
        let older = item(100, 5_000);
        for newest in [
            json!({ "time": 200, "currencyCode": UAH }),
            json!({ "time": 200, "balance": -1, "currencyCode": UAH }),
            json!({ "time": 200, "balance": "5000", "currencyCode": UAH }),
            json!({ "time": 200, "balance": 5_000, "currencyCode": 643 }),
            json!({ "time": 200, "balance": 5_000 }),
        ] {
            assert_eq!(
                newest_balance(&[newest.clone(), older.clone()]),
                None,
                "{newest}"
            );
        }
        assert_eq!(newest_balance(&[]), None);
    }

    #[test]
    fn tracker_publishes_nothing_before_a_jar_is_connected() {
        let mut tracker = JarProgressTracker::default();

        assert_eq!(tracker.observe(Some(uah(5_000))), None);
    }

    #[test]
    fn first_observation_after_connect_is_a_sync_with_the_newest_balance() {
        let mut tracker = connected(Some(uah(1)));

        let first = tracker.observe(Some(uah(5_000))).unwrap();

        assert_eq!(
            (first.is_synced, first.balance(), first.goal()),
            (true, uah(5_000), Some(uah(100_000)))
        );
    }

    #[test]
    fn first_observation_without_window_items_syncs_the_listed_jar_balance() {
        let mut tracker = connected(Some(uah(4_200)));

        let first = tracker.observe(None).unwrap();

        assert_eq!((first.is_synced, first.balance()), (true, uah(4_200)));
    }

    #[test]
    fn first_observation_with_no_balance_anywhere_keeps_the_sync_pending() {
        let mut tracker = connected(None);

        assert_eq!(tracker.observe(None), None);
        assert!(tracker.observe(Some(uah(700))).unwrap().is_synced);
    }

    #[test]
    fn an_unchanged_or_missing_balance_after_the_sync_never_publishes() {
        let mut tracker = connected(None);
        tracker.observe(Some(uah(5_000))).unwrap();

        for later in [Some(uah(5_000)), None, Some(uah(5_000))] {
            assert_eq!(tracker.observe(later.clone()), None, "{later:?}");
        }
    }

    #[test]
    fn a_changed_balance_after_the_sync_publishes_live_progress() {
        let mut tracker = connected(None);
        tracker.observe(Some(uah(5_000))).unwrap();

        let changed = tracker.observe(Some(uah(4_000))).unwrap();

        assert_eq!((changed.is_synced, changed.balance()), (false, uah(4_000)));
    }

    #[test]
    fn a_new_session_forces_a_fresh_sync_of_an_unchanged_balance() {
        let mut tracker = connected(None);
        tracker.observe(Some(uah(5_000))).unwrap();

        tracker.connected(jar(None));
        let resynced = tracker.observe(Some(uah(5_000))).unwrap();

        assert_eq!((resynced.is_synced, resynced.balance()), (true, uah(5_000)));
    }

    #[test]
    fn a_goal_in_another_currency_than_the_balance_is_left_out() {
        let mut tracker = connected(None);

        let first = tracker.observe(Some(money(USD, 5_000))).unwrap();

        assert_eq!((first.goal(), first.percent()), (None, 0.0));
    }

    #[test]
    fn percent_is_the_balance_share_of_the_goal_and_zero_without_a_goal() {
        for (balance, goal, expected) in [
            (25_000_000, Some(100_000_000), 25.0),
            (100_000_000, Some(100_000_000), 100.0),
            (150_000_000, Some(100_000_000), 150.0),
            (0, Some(100_000_000), 0.0),
            (25_000_000, None, 0.0),
            (25_000_000, Some(0), 0.0),
        ] {
            let percent = progress(balance, goal).percent();
            assert!(
                (percent - expected).abs() < 1e-9,
                "{balance} of {goal:?}: {percent}"
            );
        }
    }
}
