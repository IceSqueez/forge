use forge_platform_core::PlatformError;

const QUOTA_HIGH_WATER: u32 = 9_000;
const QUOTA_DAILY_LIMIT: u32 = 10_000;
pub(crate) const BROADCAST_COST: u32 = 1;
pub(crate) const CHAT_POLL_COST: u32 = 5;

pub struct QuotaState {
    pub used_today: u32,
    pub peak_seen: u32,
    pub last_reset_date: time::Date,
    pub long_interval_mode: bool,
}

impl Default for QuotaState {
    fn default() -> Self {
        Self {
            used_today: 0,
            peak_seen: 0,
            last_reset_date: time::Date::MIN,
            long_interval_mode: false,
        }
    }
}

impl QuotaState {
    pub fn charge(&mut self, cost: u32, today: time::Date) -> Result<(), PlatformError> {
        if self.last_reset_date != today {
            self.used_today = 0;
            self.last_reset_date = today;
        }
        if self.used_today + cost > QUOTA_DAILY_LIMIT {
            return Err(PlatformError::QuotaExhausted);
        }
        self.used_today += cost;
        if self.used_today > self.peak_seen {
            self.peak_seen = self.used_today;
        }
        if self.used_today >= QUOTA_HIGH_WATER {
            self.long_interval_mode = true;
        }
        Ok(())
    }
}

pub(crate) fn today_pacific() -> time::Date {
    (time::OffsetDateTime::now_utc() - time::Duration::hours(8)).date()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_platform_core::PlatformError;

    use super::*;

    #[test]
    fn quota_charges_correctly_for_chat_list() {
        let today = today_pacific();
        let mut qt = QuotaState {
            used_today: 0,
            peak_seen: 0,
            last_reset_date: today,
            long_interval_mode: false,
        };
        qt.charge(CHAT_POLL_COST, today).unwrap();
        assert_eq!(qt.used_today, 5);
        assert_eq!(qt.peak_seen, 5);
    }

    #[test]
    fn quota_guard_switches_to_long_interval_at_9000() {
        let today = today_pacific();
        let mut qt = QuotaState {
            used_today: 8998,
            peak_seen: 8998,
            last_reset_date: today,
            long_interval_mode: false,
        };
        qt.charge(CHAT_POLL_COST, today).unwrap();
        qt.charge(CHAT_POLL_COST, today).unwrap();
        assert!(
            qt.long_interval_mode,
            "long_interval_mode must be true at >= 9000 used"
        );
        assert_eq!(qt.used_today, 9008);
    }

    #[test]
    fn quota_exhausted_at_10000_returns_quota_exhausted_error() {
        let today = today_pacific();
        let mut qt = QuotaState {
            used_today: 9999,
            peak_seen: 9999,
            last_reset_date: today,
            long_interval_mode: true,
        };
        let result = qt.charge(CHAT_POLL_COST, today);
        assert!(
            matches!(result, Err(PlatformError::QuotaExhausted)),
            "expected QuotaExhausted, got {result:?}"
        );
    }
}
