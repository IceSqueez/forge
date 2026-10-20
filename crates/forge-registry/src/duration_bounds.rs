use std::ops::RangeInclusive;

use crate::form::AmountUnit;

pub const DURATION_UNIT_MILLISECONDS: &str = "ms";
pub const DURATION_UNIT_SECONDS: &str = "seconds";
pub const DURATION_UNIT_MINUTES: &str = "minutes";
pub const DURATION_UNIT_HOURS: &str = "hours";
pub const DURATION_UNIT_DAYS: &str = "days";

const MILLIS_PER_MILLISECOND: u64 = 1;
const MILLIS_PER_SECOND: u64 = 1_000;
const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;
const HOURS_PER_DAY: u64 = 24;
const MILLIS_PER_MINUTE: u64 = SECONDS_PER_MINUTE * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: u64 = MINUTES_PER_HOUR * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: u64 = HOURS_PER_DAY * MILLIS_PER_HOUR;

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
    AmountUnit {
        value: DURATION_UNIT_DAYS,
        base_units: MILLIS_PER_DAY,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationBase {
    Milliseconds,
    Seconds,
    Minutes,
}

impl DurationBase {
    fn millis(self) -> u64 {
        match self {
            Self::Milliseconds => MILLIS_PER_MILLISECOND,
            Self::Seconds => MILLIS_PER_SECOND,
            Self::Minutes => MILLIS_PER_MINUTE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationBounds {
    pub min: i64,
    pub max: i64,
    pub base: DurationBase,
}

impl DurationBounds {
    pub const fn in_millis(min: i64, max: i64) -> Self {
        Self {
            min,
            max,
            base: DurationBase::Milliseconds,
        }
    }

    pub const fn in_seconds(min: i64, max: i64) -> Self {
        Self {
            min,
            max,
            base: DurationBase::Seconds,
        }
    }

    pub const fn in_minutes(min: i64, max: i64) -> Self {
        Self {
            min,
            max,
            base: DurationBase::Minutes,
        }
    }

    pub fn fitting_units(&self) -> impl DoubleEndedIterator<Item = &'static AmountUnit> + '_ {
        let base = self.base.millis();
        DURATION_UNITS.iter().filter(move |unit| {
            if unit.base_units < base || unit.base_units % base != 0 {
                return false;
            }
            let (start, end) = self.whole_amounts(unit);
            start <= end && (unit.base_units == base || end >= 1)
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
            None => self.min..=self.max.max(self.min),
        }
    }

    pub fn in_largest_exact_unit(&self, stored: i64) -> Option<(&'static AmountUnit, i64)> {
        let unit = if stored == 0 {
            self.fitting_units().next()
        } else {
            self.fitting_units()
                .rev()
                .find(|unit| stored % self.factor(unit) == 0)
        }?;
        Some((unit, stored / self.factor(unit)))
    }

    pub fn stored_amount(&self, unit: &str, amount: i64) -> i64 {
        let factor = self.unit_or_first(unit).map_or(1, |unit| self.factor(unit));
        amount.saturating_mul(factor)
    }

    pub fn nearest_amount_in(&self, amount: i64, from: &str, to: &str) -> i64 {
        let stored = self.stored_amount(from, amount);
        let factor = self.unit_or_first(to).map_or(1, |unit| self.factor(unit));
        let half = factor / 2;
        let nudged = if stored < 0 {
            stored.saturating_sub(half)
        } else {
            stored.saturating_add(half)
        };
        nudged / factor
    }

    fn unit_factor(&self, unit: &AmountUnit) -> u64 {
        (unit.base_units / self.base.millis()).max(1)
    }

    fn factor(&self, unit: &AmountUnit) -> i64 {
        i64::try_from(self.unit_factor(unit)).unwrap_or(i64::MAX)
    }

    fn whole_amounts(&self, unit: &AmountUnit) -> (i64, i64) {
        let factor = self.unit_factor(unit);
        let min = u64::try_from(self.min).unwrap_or(0);
        let max = u64::try_from(self.max).unwrap_or(0);
        (
            i64::try_from(min.div_ceil(factor)).unwrap_or(i64::MAX),
            i64::try_from(max / factor).unwrap_or(i64::MAX),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLL: DurationBounds = DurationBounds::in_millis(100, 30_000);
    const TIMEOUT: DurationBounds = DurationBounds::in_millis(100, 600_000);
    const BAN: DurationBounds = DurationBounds::in_seconds(1, 1_209_600);
    const INTERVAL: DurationBounds = DurationBounds::in_minutes(1, 1_440);
    const COOLDOWN: DurationBounds = DurationBounds::in_seconds(0, i64::MAX);

    const MS: &str = DURATION_UNIT_MILLISECONDS;
    const SEC: &str = DURATION_UNIT_SECONDS;
    const MIN: &str = DURATION_UNIT_MINUTES;
    const HOUR: &str = DURATION_UNIT_HOURS;
    const DAY: &str = DURATION_UNIT_DAYS;

    fn unit_values(bounds: DurationBounds) -> Vec<&'static str> {
        bounds.fitting_units().map(|unit| unit.value).collect()
    }

    #[test]
    fn fitting_units_offer_only_units_holding_a_whole_amount_inside_the_bounds() {
        for (bounds, expected) in [
            (DurationBounds::in_millis(100, 59_999), vec![MS, SEC]),
            (DurationBounds::in_millis(100, 60_000), vec![MS, SEC, MIN]),
            (
                DurationBounds::in_millis(100, 3_600_000),
                vec![MS, SEC, MIN, HOUR],
            ),
            (
                DurationBounds::in_millis(100, 86_399_999),
                vec![MS, SEC, MIN, HOUR],
            ),
            (
                DurationBounds::in_millis(100, 86_400_000),
                vec![MS, SEC, MIN, HOUR, DAY],
            ),
        ] {
            assert_eq!(unit_values(bounds), expected, "{bounds:?}");
        }
    }

    #[test]
    fn fitting_units_never_offer_a_unit_smaller_than_the_stored_base() {
        for (bounds, expected) in [
            (DurationBounds::in_seconds(1, 600), vec![SEC, MIN]),
            (DurationBounds::in_seconds(0, 0), vec![SEC]),
            (DurationBounds::in_minutes(1, 59), vec![MIN]),
            (DurationBounds::in_minutes(0, 0), vec![MIN]),
            (COOLDOWN, vec![SEC, MIN, HOUR, DAY]),
        ] {
            assert_eq!(unit_values(bounds), expected, "{bounds:?}");
        }
    }

    #[test]
    fn fitting_units_offer_days_only_once_the_maximum_reaches_one_whole_day() {
        for (bounds, expected) in [
            (DurationBounds::in_seconds(0, 86_399), vec![SEC, MIN, HOUR]),
            (
                DurationBounds::in_seconds(0, 86_400),
                vec![SEC, MIN, HOUR, DAY],
            ),
            (DurationBounds::in_minutes(1, 1_439), vec![MIN, HOUR]),
            (INTERVAL, vec![MIN, HOUR, DAY]),
        ] {
            assert_eq!(unit_values(bounds), expected, "{bounds:?}");
        }
    }

    #[test]
    fn fitting_units_drop_a_unit_that_could_only_hold_zero() {
        let bounds = DurationBounds::in_millis(0, 60_000);

        assert_eq!(unit_values(bounds), [MS, SEC, MIN]);
    }

    #[test]
    fn fitting_units_keep_the_smallest_unit_even_when_its_range_is_only_zero() {
        let bounds = DurationBounds::in_millis(0, 0);

        assert_eq!(unit_values(bounds), [MS]);
    }

    #[test]
    fn fitting_units_offer_a_unit_whose_range_top_is_exactly_one() {
        for (max_ms, expected) in [(999, vec![MS]), (1_000, vec![MS, SEC])] {
            let bounds = DurationBounds::in_millis(0, max_ms);
            assert_eq!(unit_values(bounds), expected, "max {max_ms} ms");
        }
    }

    #[test]
    fn fitting_units_drop_a_unit_whose_rounded_up_minimum_passes_the_maximum() {
        let bounds = DurationBounds::in_millis(1_500, 1_999);

        assert_eq!(unit_values(bounds), [MS]);
    }

    #[test]
    fn range_for_rounds_the_minimum_up_and_the_maximum_down_to_whole_units() {
        let mixed = DurationBounds::in_millis(1_500, 150_000);
        for (bounds, unit, expected) in [
            (mixed, MS, 1_500..=150_000),
            (mixed, SEC, 2..=150),
            (mixed, MIN, 1..=2),
            (INTERVAL, MIN, 1..=1_440),
            (INTERVAL, HOUR, 1..=24),
            (INTERVAL, DAY, 1..=1),
            (BAN, MIN, 1..=20_160),
            (BAN, DAY, 1..=14),
            (COOLDOWN, DAY, 0..=i64::MAX / 86_400),
        ] {
            assert_eq!(bounds.range_for(unit), expected, "{unit} in {bounds:?}");
        }
    }

    #[test]
    fn range_for_a_unit_that_does_not_fit_or_is_unknown_uses_the_first_fitting_unit() {
        for (bounds, unit, expected) in [
            (POLL, MIN, 100..=30_000),
            (POLL, HOUR, 100..=30_000),
            (POLL, "weeks", 100..=30_000),
            (POLL, "", 100..=30_000),
            (INTERVAL, SEC, 1..=1_440),
            (INTERVAL, MS, 1..=1_440),
            (BAN, MS, 1..=1_209_600),
        ] {
            assert_eq!(bounds.range_for(unit), expected, "{unit:?} in {bounds:?}");
        }
    }

    #[test]
    fn in_largest_exact_unit_shows_a_stored_amount_in_the_largest_fitting_unit_that_divides_it() {
        for (bounds, stored, unit, amount) in [
            (TIMEOUT, 1_000, SEC, 1),
            (TIMEOUT, 60_000, MIN, 1),
            (TIMEOUT, 90_000, SEC, 90),
            (TIMEOUT, 1_500, MS, 1_500),
            (TIMEOUT, 0, MS, 0),
            (TIMEOUT, 3_600_000, MIN, 60),
            (POLL, 60_000, SEC, 60),
            (POLL, 500, MS, 500),
            (TIMEOUT, -2_000, SEC, -2),
            (BAN, 3_600, HOUR, 1),
            (BAN, 90, SEC, 90),
            (BAN, 120, MIN, 2),
            (BAN, 172_800, DAY, 2),
            (BAN, 0, SEC, 0),
            (INTERVAL, 1_440, DAY, 1),
            (INTERVAL, 120, HOUR, 2),
            (INTERVAL, 90, MIN, 90),
            (INTERVAL, 0, MIN, 0),
            (COOLDOWN, i64::MAX, SEC, i64::MAX),
        ] {
            let shown = bounds
                .in_largest_exact_unit(stored)
                .map(|(unit, amount)| (unit.value, amount));
            assert_eq!(shown, Some((unit, amount)), "{stored} in {bounds:?}");
        }
    }

    #[test]
    fn in_largest_exact_unit_finds_nothing_when_no_unit_fits_the_bounds() {
        let inverted = DurationBounds::in_millis(5_000, 100);

        assert!(inverted.in_largest_exact_unit(1_000).is_none());
    }

    #[test]
    fn stored_amount_scales_an_amount_by_its_unit_in_base_units() {
        for (bounds, unit, amount, expected) in [
            (TIMEOUT, MS, 7, 7),
            (TIMEOUT, SEC, 2, 2_000),
            (TIMEOUT, MIN, 2, 120_000),
            (TIMEOUT, "weeks", 7, 7),
            (BAN, MIN, 2, 120),
            (BAN, HOUR, 1, 3_600),
            (BAN, DAY, 1, 86_400),
            (BAN, MS, 5, 5),
            (INTERVAL, HOUR, 24, 1_440),
            (INTERVAL, DAY, 1, 1_440),
            (INTERVAL, SEC, 30, 30),
        ] {
            assert_eq!(
                bounds.stored_amount(unit, amount),
                expected,
                "{amount} {unit:?} in {bounds:?}"
            );
        }
    }

    #[test]
    fn stored_amount_saturates_instead_of_overflowing() {
        for (bounds, unit, amount, expected) in [
            (TIMEOUT, MIN, i64::MAX, i64::MAX),
            (TIMEOUT, MIN, i64::MIN, i64::MIN),
            (COOLDOWN, DAY, i64::MAX, i64::MAX),
            (COOLDOWN, DAY, i64::MAX / 86_400 + 1, i64::MAX),
            (COOLDOWN, DAY, i64::MAX / 86_400, i64::MAX / 86_400 * 86_400),
        ] {
            assert_eq!(
                bounds.stored_amount(unit, amount),
                expected,
                "{amount} {unit:?} in {bounds:?}"
            );
        }
    }

    #[test]
    fn nearest_amount_in_converts_exactly_or_rounds_half_away_from_zero() {
        for (bounds, amount, from, to, expected) in [
            (TIMEOUT, 2, MIN, SEC, 120),
            (TIMEOUT, 2, SEC, MS, 2_000),
            (TIMEOUT, 120_000, MS, MIN, 2),
            (TIMEOUT, 1_500, MS, SEC, 2),
            (TIMEOUT, 1_499, MS, SEC, 1),
            (TIMEOUT, 500, MS, SEC, 1),
            (TIMEOUT, 400, MS, SEC, 0),
            (TIMEOUT, 90, SEC, MIN, 2),
            (TIMEOUT, -1_500, MS, SEC, -2),
            (TIMEOUT, -400, MS, SEC, 0),
            (BAN, 90, SEC, MIN, 2),
            (BAN, 89, SEC, MIN, 1),
            (BAN, 2, DAY, HOUR, 48),
            (INTERVAL, 1_440, MIN, HOUR, 24),
            (INTERVAL, 1, DAY, MIN, 1_440),
            (INTERVAL, 36, HOUR, DAY, 2),
            (INTERVAL, 11, HOUR, DAY, 0),
        ] {
            assert_eq!(
                bounds.nearest_amount_in(amount, from, to),
                expected,
                "{amount} {from} -> {to} in {bounds:?}"
            );
        }
    }

    #[test]
    fn nearest_amount_in_saturates_at_the_ends_of_i64() {
        for (bounds, amount, from, to, expected) in [
            (TIMEOUT, i64::MAX, MIN, SEC, i64::MAX / 1_000),
            (TIMEOUT, i64::MAX, MS, SEC, i64::MAX / 1_000),
            (TIMEOUT, i64::MIN, MS, SEC, i64::MIN / 1_000),
            (COOLDOWN, i64::MAX, DAY, HOUR, i64::MAX / 3_600),
        ] {
            assert_eq!(
                bounds.nearest_amount_in(amount, from, to),
                expected,
                "{amount} {from} -> {to} in {bounds:?}"
            );
        }
    }
}
