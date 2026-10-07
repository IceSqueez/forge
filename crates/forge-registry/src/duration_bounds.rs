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
        DURATION_UNITS
            .iter()
            .enumerate()
            .filter_map(|(index, unit)| {
                let (start, end) = self.whole_amounts(unit);
                let fits = start <= end && (index == 0 || end >= 1);
                fits.then_some(unit)
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

#[cfg(test)]
mod tests {
    use super::*;

    const POLL: DurationBounds = DurationBounds {
        min_ms: 100,
        max_ms: 30_000,
    };
    const TIMEOUT: DurationBounds = DurationBounds {
        min_ms: 100,
        max_ms: 600_000,
    };

    fn unit_values(bounds: DurationBounds) -> Vec<&'static str> {
        bounds.fitting_units().map(|unit| unit.value).collect()
    }

    #[test]
    fn fitting_units_offer_only_units_holding_a_whole_amount_inside_the_bounds() {
        for (max_ms, expected) in [
            (
                59_999,
                vec![DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS],
            ),
            (
                60_000,
                vec![
                    DURATION_UNIT_MILLISECONDS,
                    DURATION_UNIT_SECONDS,
                    DURATION_UNIT_MINUTES,
                ],
            ),
            (
                3_600_000,
                vec![
                    DURATION_UNIT_MILLISECONDS,
                    DURATION_UNIT_SECONDS,
                    DURATION_UNIT_MINUTES,
                    DURATION_UNIT_HOURS,
                ],
            ),
        ] {
            let bounds = DurationBounds {
                min_ms: 100,
                max_ms,
            };
            assert_eq!(unit_values(bounds), expected, "max {max_ms} ms");
        }
    }

    #[test]
    fn fitting_units_drop_a_unit_that_could_only_hold_zero() {
        let bounds = DurationBounds {
            min_ms: 0,
            max_ms: 60_000,
        };

        assert_eq!(
            unit_values(bounds),
            [
                DURATION_UNIT_MILLISECONDS,
                DURATION_UNIT_SECONDS,
                DURATION_UNIT_MINUTES,
            ]
        );
    }

    #[test]
    fn fitting_units_keep_the_smallest_unit_even_when_its_range_is_only_zero() {
        let bounds = DurationBounds {
            min_ms: 0,
            max_ms: 0,
        };

        assert_eq!(unit_values(bounds), [DURATION_UNIT_MILLISECONDS]);
    }

    #[test]
    fn fitting_units_offer_a_unit_whose_range_top_is_exactly_one() {
        for (max_ms, expected) in [
            (999, vec![DURATION_UNIT_MILLISECONDS]),
            (
                1_000,
                vec![DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS],
            ),
        ] {
            let bounds = DurationBounds { min_ms: 0, max_ms };
            assert_eq!(unit_values(bounds), expected, "max {max_ms} ms");
        }
    }

    #[test]
    fn fitting_units_drop_a_unit_whose_rounded_up_minimum_passes_the_maximum() {
        let bounds = DurationBounds {
            min_ms: 1_500,
            max_ms: 1_999,
        };

        assert_eq!(unit_values(bounds), [DURATION_UNIT_MILLISECONDS]);
    }

    #[test]
    fn range_for_rounds_the_minimum_up_and_the_maximum_down_to_whole_units() {
        let bounds = DurationBounds {
            min_ms: 1_500,
            max_ms: 150_000,
        };
        for (unit, expected) in [
            (DURATION_UNIT_MILLISECONDS, 1_500..=150_000),
            (DURATION_UNIT_SECONDS, 2..=150),
            (DURATION_UNIT_MINUTES, 1..=2),
        ] {
            assert_eq!(bounds.range_for(unit), expected, "unit {unit}");
        }
    }

    #[test]
    fn range_for_a_unit_that_does_not_fit_or_is_unknown_uses_the_first_fitting_unit() {
        for unit in [DURATION_UNIT_MINUTES, DURATION_UNIT_HOURS, "weeks", ""] {
            assert_eq!(POLL.range_for(unit), 100..=30_000, "unit {unit:?}");
        }
    }

    #[test]
    fn in_largest_exact_unit_shows_millis_in_the_largest_fitting_unit_that_divides_them() {
        for (bounds, millis, unit, amount) in [
            (TIMEOUT, 1_000, DURATION_UNIT_SECONDS, 1),
            (TIMEOUT, 60_000, DURATION_UNIT_MINUTES, 1),
            (TIMEOUT, 90_000, DURATION_UNIT_SECONDS, 90),
            (TIMEOUT, 1_500, DURATION_UNIT_MILLISECONDS, 1_500),
            (TIMEOUT, 0, DURATION_UNIT_MILLISECONDS, 0),
            (TIMEOUT, 3_600_000, DURATION_UNIT_MINUTES, 60),
            (POLL, 60_000, DURATION_UNIT_SECONDS, 60),
            (POLL, 500, DURATION_UNIT_MILLISECONDS, 500),
            (TIMEOUT, -2_000, DURATION_UNIT_SECONDS, -2),
        ] {
            let shown = bounds
                .in_largest_exact_unit(millis)
                .map(|(unit, amount)| (unit.value, amount));
            assert_eq!(shown, Some((unit, amount)), "{millis} ms in {bounds:?}");
        }
    }

    #[test]
    fn in_largest_exact_unit_finds_nothing_when_no_unit_fits_the_bounds() {
        let inverted = DurationBounds {
            min_ms: 5_000,
            max_ms: 100,
        };

        assert!(inverted.in_largest_exact_unit(1_000).is_none());
    }

    #[test]
    fn millis_scales_an_amount_by_its_unit_and_saturates_instead_of_overflowing() {
        for (unit, amount, expected) in [
            (DURATION_UNIT_MILLISECONDS, 7, 7),
            (DURATION_UNIT_SECONDS, 2, 2_000),
            (DURATION_UNIT_MINUTES, 2, 120_000),
            ("weeks", 7, 7),
            (DURATION_UNIT_MINUTES, i64::MAX, i64::MAX),
            (DURATION_UNIT_MINUTES, i64::MIN, i64::MIN),
        ] {
            assert_eq!(TIMEOUT.millis(unit, amount), expected, "{amount} {unit:?}");
        }
    }

    #[test]
    fn nearest_amount_in_converts_exactly_or_rounds_half_away_from_zero() {
        for (amount, from, to, expected) in [
            (2, DURATION_UNIT_MINUTES, DURATION_UNIT_SECONDS, 120),
            (2, DURATION_UNIT_SECONDS, DURATION_UNIT_MILLISECONDS, 2_000),
            (
                120_000,
                DURATION_UNIT_MILLISECONDS,
                DURATION_UNIT_MINUTES,
                2,
            ),
            (1_500, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS, 2),
            (1_499, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS, 1),
            (500, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS, 1),
            (400, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS, 0),
            (90, DURATION_UNIT_SECONDS, DURATION_UNIT_MINUTES, 2),
            (
                -1_500,
                DURATION_UNIT_MILLISECONDS,
                DURATION_UNIT_SECONDS,
                -2,
            ),
            (-400, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_SECONDS, 0),
        ] {
            assert_eq!(
                TIMEOUT.nearest_amount_in(amount, from, to),
                expected,
                "{amount} {from} -> {to}"
            );
        }
    }

    #[test]
    fn nearest_amount_in_saturates_at_the_ends_of_i64() {
        for (amount, from, expected) in [
            (i64::MAX, DURATION_UNIT_MINUTES, i64::MAX / 1_000),
            (i64::MAX, DURATION_UNIT_MILLISECONDS, i64::MAX / 1_000),
            (i64::MIN, DURATION_UNIT_MILLISECONDS, i64::MIN / 1_000),
        ] {
            assert_eq!(
                TIMEOUT.nearest_amount_in(amount, from, DURATION_UNIT_SECONDS),
                expected,
                "{amount} {from}"
            );
        }
    }
}
