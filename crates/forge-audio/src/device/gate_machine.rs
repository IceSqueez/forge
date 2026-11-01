#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GateSettings {
    pub(crate) threshold: f32,
    pub(crate) hold_nanos: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateEdge {
    Opened,
    Closed,
}

#[derive(Debug, Default)]
pub(crate) struct GateMachine {
    open: bool,
    last_loud_nanos: u64,
}

impl GateMachine {
    pub(crate) fn tick(
        &mut self,
        peak: f32,
        settings: GateSettings,
        now_nanos: u64,
    ) -> Option<GateEdge> {
        if peak >= settings.threshold {
            self.last_loud_nanos = now_nanos;
            let opened = !self.open;
            self.open = true;
            return opened.then_some(GateEdge::Opened);
        }

        let quiet_nanos = now_nanos.saturating_sub(self.last_loud_nanos);
        if self.open && quiet_nanos >= settings.hold_nanos {
            self.open = false;
            return Some(GateEdge::Closed);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: f32 = 0.5;
    const JUST_UNDER: f32 = f32::from_bits(THRESHOLD.to_bits() - 1);
    const SILENCE: f32 = 0.0;
    const HOLD: u64 = 100;
    const LAST_NANO: u64 = u64::MAX;

    const HOLDS: GateSettings = GateSettings {
        threshold: THRESHOLD,
        hold_nanos: HOLD,
    };
    const NO_HOLD: GateSettings = GateSettings {
        threshold: THRESHOLD,
        hold_nanos: 0,
    };
    const LONGEST_HOLD: GateSettings = GateSettings {
        threshold: THRESHOLD,
        hold_nanos: LAST_NANO,
    };
    const NAN_THRESHOLD: GateSettings = GateSettings {
        threshold: f32::NAN,
        hold_nanos: HOLD,
    };

    const OPENED: Option<GateEdge> = Some(GateEdge::Opened);
    const CLOSED: Option<GateEdge> = Some(GateEdge::Closed);

    type Tick = (f32, u64, Option<GateEdge>);

    fn assert_ticks(case: &str, settings: GateSettings, ticks: &[Tick]) {
        let mut machine = GateMachine::default();
        for (step, (peak, now_nanos, edge)) in ticks.iter().enumerate() {
            assert_eq!(
                machine.tick(*peak, settings, *now_nanos),
                *edge,
                "{case}: step {step}, peak {peak} at {now_nanos}"
            );
        }
    }

    #[test]
    fn the_threshold_decides_the_opening_edge_and_it_fires_once() {
        for (case, ticks) in [
            (
                "a peak at the threshold opens",
                &[(THRESHOLD, 10, OPENED)][..],
            ),
            ("a peak just under stays closed", &[(JUST_UNDER, 10, None)]),
            (
                "staying loud announces nothing more",
                &[
                    (THRESHOLD, 10, OPENED),
                    (1.0, 11, None),
                    (THRESHOLD, 12, None),
                ],
            ),
            (
                "silence on a closed gate is no edge",
                &[(SILENCE, LAST_NANO, None)],
            ),
        ] {
            assert_ticks(case, HOLDS, ticks);
        }
    }

    #[test]
    fn the_gate_closes_on_the_exact_nanosecond_the_hold_expires_and_only_once() {
        for (case, ticks) in [
            (
                "one nanosecond short holds, the expiry closes",
                &[
                    (THRESHOLD, 100, OPENED),
                    (SILENCE, 199, None),
                    (SILENCE, 200, CLOSED),
                ][..],
            ),
            (
                "a closed gate stays quiet",
                &[
                    (THRESHOLD, 100, OPENED),
                    (SILENCE, 200, CLOSED),
                    (SILENCE, 201, None),
                    (SILENCE, 100_000, None),
                ],
            ),
            (
                "the first loud tick after closing opens again",
                &[
                    (THRESHOLD, 100, OPENED),
                    (SILENCE, 200, CLOSED),
                    (THRESHOLD, 201, OPENED),
                ],
            ),
            (
                "a loud tick restarts the hold",
                &[
                    (THRESHOLD, 100, OPENED),
                    (THRESHOLD, 150, None),
                    (SILENCE, 249, None),
                    (SILENCE, 250, CLOSED),
                ],
            ),
            (
                "a peak just under does not restart the hold",
                &[
                    (THRESHOLD, 100, OPENED),
                    (JUST_UNDER, 150, None),
                    (SILENCE, 200, CLOSED),
                ],
            ),
            (
                "a clock that stepped back is not a long silence",
                &[(THRESHOLD, 100, OPENED), (SILENCE, 50, None)],
            ),
        ] {
            assert_ticks(case, HOLDS, ticks);
        }
    }

    #[test]
    fn a_zero_hold_closes_on_the_first_quiet_tick_even_at_the_same_instant() {
        assert_ticks(
            "zero hold",
            NO_HOLD,
            &[(THRESHOLD, 10, OPENED), (SILENCE, 10, CLOSED)],
        );
    }

    #[test]
    fn the_end_of_the_u64_clock_neither_wraps_nor_releases_early() {
        for (case, settings, ticks) in [
            (
                "the longest hold outlasts every later loud tick",
                LONGEST_HOLD,
                &[(THRESHOLD, 1, OPENED), (SILENCE, LAST_NANO, None)][..],
            ),
            (
                "the longest hold expires on the last nanosecond",
                LONGEST_HOLD,
                &[
                    (THRESHOLD, 0, OPENED),
                    (SILENCE, LAST_NANO - 1, None),
                    (SILENCE, LAST_NANO, CLOSED),
                ],
            ),
            (
                "a loud tick at the last nanosecond holds",
                HOLDS,
                &[(THRESHOLD, LAST_NANO, OPENED), (SILENCE, LAST_NANO, None)],
            ),
        ] {
            assert_ticks(case, settings, ticks);
        }
    }

    #[test]
    fn nan_counts_as_silence_and_never_opens_the_gate() {
        for (case, settings, ticks) in [
            (
                "a nan peak does not open",
                HOLDS,
                &[(f32::NAN, 10, None)][..],
            ),
            (
                "a nan peak inside the hold keeps it open",
                HOLDS,
                &[(THRESHOLD, 100, OPENED), (f32::NAN, 150, None)],
            ),
            (
                "a nan peak does not restart the hold",
                HOLDS,
                &[
                    (THRESHOLD, 100, OPENED),
                    (f32::NAN, 150, None),
                    (f32::NAN, 200, CLOSED),
                ],
            ),
            (
                "a nan threshold lets nothing through",
                NAN_THRESHOLD,
                &[(1.0, 10, None), (f32::INFINITY, 11, None)],
            ),
        ] {
            assert_ticks(case, settings, ticks);
        }
    }
}
