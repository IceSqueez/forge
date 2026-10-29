use std::sync::Arc;

use forge_platform_core::PlatformError;
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use time::{OffsetDateTime, UtcOffset};
use tokio::sync::{Mutex, MutexGuard, TryLockError};

const QUOTA_HIGH_WATER: u32 = 9_000;
pub(crate) const QUOTA_DAILY_LIMIT: u32 = 10_000;
const QUOTA_RESET_ZONE: &str = "America/Los_Angeles";
const PACIFIC_STANDARD_HOURS: i8 = -8;
pub(crate) const BROADCAST_COST: u32 = 1;
pub(crate) const CHAT_POLL_COST: u32 = 5;

pub(crate) struct QuotaState {
    pub used_today: u32,
    pub last_reset_date: time::Date,
}

impl Default for QuotaState {
    fn default() -> Self {
        Self {
            used_today: 0,
            last_reset_date: time::Date::MIN,
        }
    }
}

impl QuotaState {
    pub fn charge(&mut self, cost: u32, today: time::Date) -> Result<(), PlatformError> {
        self.roll_over(today);
        let used_after = self.used_today.saturating_add(cost);
        if used_after > QUOTA_DAILY_LIMIT {
            return Err(PlatformError::QuotaExhausted);
        }
        self.used_today = used_after;
        Ok(())
    }

    pub fn refund(&mut self, cost: u32, charged_on: time::Date) {
        if self.last_reset_date == charged_on {
            self.used_today = self.used_today.saturating_sub(cost);
        }
    }

    pub fn used_on(&self, today: time::Date) -> u32 {
        if self.last_reset_date == today {
            self.used_today
        } else {
            0
        }
    }

    pub fn long_interval_mode(&self, today: time::Date) -> bool {
        self.used_on(today) >= QUOTA_HIGH_WATER
    }

    fn roll_over(&mut self, today: time::Date) {
        if self.last_reset_date != today {
            self.used_today = 0;
            self.last_reset_date = today;
        }
    }
}

#[derive(Clone)]
pub struct SharedQuota(Arc<Mutex<QuotaState>>);

impl Default for SharedQuota {
    fn default() -> Self {
        Self::new(QuotaState::default())
    }
}

impl SharedQuota {
    pub(crate) fn new(state: QuotaState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }

    pub(crate) async fn lock(&self) -> MutexGuard<'_, QuotaState> {
        self.0.lock().await
    }

    pub(crate) fn try_lock(&self) -> Result<MutexGuard<'_, QuotaState>, TryLockError> {
        self.0.try_lock()
    }
}

pub(crate) fn today_pacific() -> time::Date {
    pacific_date(OffsetDateTime::now_utc())
}

pub(crate) fn pacific_date(at: OffsetDateTime) -> time::Date {
    let zone = TimeZone::get(QUOTA_RESET_ZONE)
        .unwrap_or_else(|_| TimeZone::fixed(Offset::constant(PACIFIC_STANDARD_HOURS)));
    let offset_seconds = Timestamp::from_second(at.unix_timestamp()).map_or(
        Offset::constant(PACIFIC_STANDARD_HOURS).seconds(),
        |instant| zone.to_offset(instant).seconds(),
    );
    UtcOffset::from_whole_seconds(offset_seconds)
        .ok()
        .and_then(|offset| at.checked_to_offset(offset))
        .map_or(at.date(), |local| local.date())
}

#[cfg(test)]
mod tests {
    use forge_platform_core::PlatformError;
    use time::{Month, PrimitiveDateTime, Time};

    use super::*;

    const DAY: time::Date = day(2026, Month::March, 7);
    const NEXT_DAY: time::Date = day(2026, Month::March, 8);

    const fn day(year: i32, month: Month, day: u8) -> time::Date {
        match time::Date::from_calendar_date(year, month, day) {
            Ok(date) => date,
            Err(_) => panic!("invalid fixture date"),
        }
    }

    fn utc(date: time::Date, hour: u8, minute: u8, second: u8) -> OffsetDateTime {
        PrimitiveDateTime::new(date, Time::from_hms(hour, minute, second).unwrap()).assume_utc()
    }

    fn used_on_day(used_today: u32) -> QuotaState {
        QuotaState {
            used_today,
            last_reset_date: DAY,
        }
    }

    #[test]
    fn same_day_charge_accepts_up_to_the_daily_limit_and_rejects_past_it() {
        let cases = [
            (0, CHAT_POLL_COST, true, CHAT_POLL_COST),
            (
                QUOTA_DAILY_LIMIT - CHAT_POLL_COST,
                CHAT_POLL_COST,
                true,
                QUOTA_DAILY_LIMIT,
            ),
            (
                QUOTA_DAILY_LIMIT - CHAT_POLL_COST + 1,
                CHAT_POLL_COST,
                false,
                QUOTA_DAILY_LIMIT - CHAT_POLL_COST + 1,
            ),
            (QUOTA_DAILY_LIMIT, BROADCAST_COST, false, QUOTA_DAILY_LIMIT),
        ];
        for (used, cost, accepted, used_after) in cases {
            let mut qt = used_on_day(used);
            let result = qt.charge(cost, DAY);
            assert_eq!(
                (result.is_ok(), qt.used_on(DAY)),
                (accepted, used_after),
                "used {used} + cost {cost}"
            );
        }
    }

    #[test]
    fn rejected_charge_reports_quota_exhausted() {
        let mut qt = used_on_day(QUOTA_DAILY_LIMIT);
        let result = qt.charge(BROADCAST_COST, DAY);
        assert!(
            matches!(result, Err(PlatformError::QuotaExhausted)),
            "got {result:?}"
        );
    }

    #[test]
    fn long_interval_mode_engages_at_the_high_water_mark() {
        for (used, expected) in [
            (QUOTA_HIGH_WATER - 1, false),
            (QUOTA_HIGH_WATER, true),
            (QUOTA_HIGH_WATER + 1, true),
        ] {
            assert_eq!(
                used_on_day(used).long_interval_mode(DAY),
                expected,
                "used {used}"
            );
        }
    }

    #[test]
    fn charge_that_crosses_the_high_water_mark_engages_long_interval_mode_at_once() {
        let mut qt = used_on_day(QUOTA_HIGH_WATER - 1);
        qt.charge(CHAT_POLL_COST, DAY).unwrap();
        assert!(qt.long_interval_mode(DAY));
    }

    #[test]
    fn a_new_pacific_day_reads_as_untouched_before_any_charge() {
        let qt = used_on_day(QUOTA_HIGH_WATER + 500);
        assert_eq!(
            (
                qt.long_interval_mode(DAY),
                qt.long_interval_mode(NEXT_DAY),
                qt.used_on(NEXT_DAY)
            ),
            (true, false, 0)
        );
    }

    #[test]
    fn first_charge_on_a_new_day_counts_only_that_charge() {
        let mut qt = used_on_day(QUOTA_DAILY_LIMIT - 1);
        qt.charge(CHAT_POLL_COST, NEXT_DAY).unwrap();
        assert_eq!(
            (qt.used_on(NEXT_DAY), qt.long_interval_mode(NEXT_DAY)),
            (CHAT_POLL_COST, false)
        );
    }

    #[test]
    fn charge_near_u32_max_saturates_into_quota_exhausted_instead_of_overflowing() {
        for (used, cost) in [(u32::MAX, CHAT_POLL_COST), (CHAT_POLL_COST, u32::MAX)] {
            let mut qt = used_on_day(used);
            let result = qt.charge(cost, DAY);
            assert!(
                matches!(result, Err(PlatformError::QuotaExhausted)) && qt.used_on(DAY) == used,
                "used {used} + cost {cost}: got {result:?}, used now {}",
                qt.used_on(DAY)
            );
        }
    }

    #[test]
    fn a_same_day_refund_gives_the_charge_back() {
        let mut qt = used_on_day(QUOTA_HIGH_WATER - 1);
        qt.charge(CHAT_POLL_COST, DAY).unwrap();

        qt.refund(CHAT_POLL_COST, DAY);

        assert_eq!(
            (qt.used_on(DAY), qt.long_interval_mode(DAY)),
            (QUOTA_HIGH_WATER - 1, false)
        );
    }

    #[test]
    fn a_refund_for_a_day_already_rolled_over_leaves_the_new_day_untouched() {
        let mut qt = used_on_day(QUOTA_HIGH_WATER);
        qt.charge(CHAT_POLL_COST, NEXT_DAY).unwrap();

        qt.refund(CHAT_POLL_COST, DAY);

        assert_eq!(qt.used_on(NEXT_DAY), CHAT_POLL_COST);
    }

    #[test]
    fn a_refund_larger_than_the_usage_floors_at_zero() {
        let mut qt = used_on_day(BROADCAST_COST);

        qt.refund(CHAT_POLL_COST, DAY);

        assert_eq!(qt.used_on(DAY), 0);
    }

    #[test]
    fn pacific_date_follows_daylight_saving_time() {
        let cases = [
            ((Month::July, 15, 6, 59, 59), (Month::July, 14)),
            ((Month::July, 15, 7, 0, 0), (Month::July, 15)),
            ((Month::July, 15, 7, 30, 0), (Month::July, 15)),
            ((Month::January, 15, 7, 30, 0), (Month::January, 14)),
            ((Month::January, 15, 7, 59, 59), (Month::January, 14)),
            ((Month::January, 15, 8, 0, 0), (Month::January, 15)),
            ((Month::March, 8, 7, 59, 59), (Month::March, 7)),
            ((Month::March, 8, 8, 0, 0), (Month::March, 8)),
            ((Month::March, 8, 9, 59, 59), (Month::March, 8)),
            ((Month::March, 8, 10, 0, 0), (Month::March, 8)),
            ((Month::March, 9, 6, 59, 59), (Month::March, 8)),
            ((Month::March, 9, 7, 0, 0), (Month::March, 9)),
            ((Month::November, 1, 6, 59, 59), (Month::October, 31)),
            ((Month::November, 1, 7, 0, 0), (Month::November, 1)),
            ((Month::November, 1, 8, 30, 0), (Month::November, 1)),
            ((Month::November, 1, 9, 30, 0), (Month::November, 1)),
            ((Month::November, 2, 7, 59, 59), (Month::November, 1)),
            ((Month::November, 2, 8, 0, 0), (Month::November, 2)),
        ];
        for ((month, date, hour, minute, second), (local_month, local_date)) in cases {
            let at = utc(day(2026, month, date), hour, minute, second);
            assert_eq!(
                pacific_date(at),
                day(2026, local_month, local_date),
                "at {at}"
            );
        }
    }

    #[test]
    fn pacific_date_ignores_the_offset_the_instant_is_expressed_in() {
        assert_eq!(
            pacific_date(
                utc(day(2026, Month::July, 15), 22, 0, 0)
                    .to_offset(time::UtcOffset::from_hms(3, 0, 0).unwrap())
            ),
            day(2026, Month::July, 15)
        );
    }

    #[tokio::test]
    async fn a_cloned_shared_quota_draws_on_the_same_daily_budget() {
        let poller = SharedQuota::new(used_on_day(QUOTA_DAILY_LIMIT - CHAT_POLL_COST));
        let sender = poller.clone();

        sender.lock().await.charge(CHAT_POLL_COST, DAY).unwrap();
        let result = poller.lock().await.charge(BROADCAST_COST, DAY);

        assert!(
            matches!(result, Err(PlatformError::QuotaExhausted)),
            "a spend through one clone must exhaust the other; got {result:?}"
        );
    }
}
