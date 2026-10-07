use std::ops::RangeInclusive;

use crate::form::AmountUnit;

pub const DURATION_UNIT_MILLISECONDS: &str = "ms";
pub const DURATION_UNIT_SECONDS: &str = "seconds";
pub const DURATION_UNIT_MINUTES: &str = "minutes";
pub const DURATION_UNIT_HOURS: &str = "hours";

const MILLIS_PER_MILLISECOND: u64 = 1;
const MILLIS_PER_SECOND: u64 = 1_000;
const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;
const MILLIS_PER_MINUTE: u64 = SECONDS_PER_MINUTE * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: u64 = MINUTES_PER_HOUR * MILLIS_PER_MINUTE;

const DURATION_UNITS: &[AmountUnit] = &[
    AmountUnit {
        value: DURATION_UNIT_MILLISECONDS,
        base_units: MILLIS_PER_MILLISECOND,
    },
    AmountUnit {
        value: DURATION_UNIT_SECONDS,
        base_units: MILLIS_PER_SECOND,
    },
    AmountUnit {
        value: DURATION_UNIT_MINUTES,
        base_units: MILLIS_PER_MINUTE,
    },
    AmountUnit {
        value: DURATION_UNIT_HOURS,
        base_units: MILLIS_PER_HOUR,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationBounds {
    pub min_ms: i64,
    pub max_ms: i64,
}

impl DurationBounds {
    pub fn fitting_units(&self) -> impl DoubleEndedIterator<Item = &'static AmountUnit> + '_ {
        DURATION_UNITS.iter().filter(|unit| {
            let (start, end) = self.whole_amounts(unit);
            start <= end
        })
    }

    pub fn unit_or_first(&self, value: &str) -> Option<&'static AmountUnit> {
        self.fitting_units()
            .find(|unit| unit.value == value)
            .or_else(|| self.fitting_units().next())
    }

    pub fn range_for(&self, unit: &str) -> RangeInclusive<i64> {
        match self.unit_or_first(unit) {
            Some(unit) => {
                let (start, end) = self.whole_amounts(unit);
                start..=end
            }
            None => self.min_ms..=self.max_ms.max(self.min_ms),
        }
    }

    pub fn in_largest_exact_unit(&self, millis: i64) -> Option<(&'static AmountUnit, i64)> {
        let unit = if millis == 0 {
            self.fitting_units().next()
        } else {
            self.fitting_units()
                .rev()
                .find(|unit| millis % unit_millis(unit) == 0)
        }?;
        Some((unit, millis / unit_millis(unit)))
    }

    pub fn millis(&self, unit: &str, amount: i64) -> i64 {
        let base = self.unit_or_first(unit).map_or(1, unit_millis);
        amount.saturating_mul(base)
    }

    pub fn nearest_amount_in(&self, amount: i64, from: &str, to: &str) -> i64 {
        let millis = self.millis(from, amount);
        let base = self.unit_or_first(to).map_or(1, unit_millis);
        let half = base / 2;
        let nudged = if millis < 0 {
            millis.saturating_sub(half)
        } else {
            millis.saturating_add(half)
        };
        nudged / base
    }

    fn whole_amounts(&self, unit: &AmountUnit) -> (i64, i64) {
        let base = unit.base_units.max(1);
        let min = u64::try_from(self.min_ms).unwrap_or(0);
        let max = u64::try_from(self.max_ms).unwrap_or(0);
        (
            i64::try_from(min.div_ceil(base)).unwrap_or(i64::MAX),
            i64::try_from(max / base).unwrap_or(i64::MAX),
        )
    }
}

fn unit_millis(unit: &AmountUnit) -> i64 {
    i64::try_from(unit.base_units.max(1)).unwrap_or(i64::MAX)
}
