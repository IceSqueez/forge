#![allow(clippy::expect_used)]

use std::time::{Duration, Instant};

use forge_net::{Cooldown, MAX_REMOTE_COOLDOWN};

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn cooled(now: Instant, retry_after: Duration) -> Cooldown {
    let mut cooldown = Cooldown::default();
    cooldown.extend(now, retry_after);
    cooldown
}

fn latest_reachable_instant() -> Instant {
    let mut instant = Instant::now();
    let mut step = Duration::from_secs(u64::MAX >> 1);
    while step >= Duration::from_secs(1) {
        match instant.checked_add(step) {
            Some(later) => instant = later,
            None => step /= 2,
        }
    }
    instant
}

#[test]
fn untouched_cooldown_has_nothing_remaining() {
    assert_eq!(
        Cooldown::default().remaining(Instant::now()),
        Duration::ZERO
    );
}

#[test]
fn later_hints_extend_but_never_shorten_the_deadline() {
    let t0 = Instant::now();
    let cases = [
        (
            "shorter hint at same instant",
            secs(30),
            secs(0),
            secs(5),
            secs(30),
        ),
        ("shorter hint later", secs(30), secs(10), secs(5), secs(20)),
        (
            "equal deadline later",
            secs(30),
            secs(10),
            secs(20),
            secs(20),
        ),
        (
            "longer hint at same instant",
            secs(5),
            secs(0),
            secs(30),
            secs(30),
        ),
        (
            "longer deadline later",
            secs(30),
            secs(10),
            secs(25),
            secs(25),
        ),
        (
            "zero hint after a cooldown",
            secs(30),
            secs(1),
            secs(0),
            secs(29),
        ),
    ];
    for (label, first, offset, second, expected_left) in cases {
        let mut cooldown = cooled(t0, first);
        let later = t0 + offset;
        cooldown.extend(later, second);
        assert_eq!(cooldown.remaining(later), expected_left, "{label}");
    }
}

#[test]
fn hint_is_clamped_to_the_remote_cooldown_ceiling() {
    let t0 = Instant::now();
    let cases = [
        MAX_REMOTE_COOLDOWN - secs(1),
        MAX_REMOTE_COOLDOWN,
        MAX_REMOTE_COOLDOWN + secs(1),
        Duration::MAX,
    ];
    for hint in cases {
        assert_eq!(
            cooled(t0, hint).remaining(t0),
            hint.min(MAX_REMOTE_COOLDOWN),
            "hint {hint:?}"
        );
    }
}

#[test]
fn clamped_hint_does_not_shorten_a_longer_existing_deadline() {
    let t0 = Instant::now();
    let mut cooldown = cooled(t0, MAX_REMOTE_COOLDOWN);
    let later = t0 + secs(600);
    cooldown.extend(later, Duration::MAX);
    assert_eq!(cooldown.remaining(later), MAX_REMOTE_COOLDOWN);
}

#[test]
fn overflowing_deadline_is_ignored_on_a_fresh_cooldown() {
    let edge = latest_reachable_instant();
    assert_eq!(
        cooled(edge, MAX_REMOTE_COOLDOWN).remaining(edge),
        Duration::ZERO
    );
}

#[test]
fn overflowing_deadline_keeps_the_existing_cooldown() {
    let edge = latest_reachable_instant();
    let before = edge
        .checked_sub(secs(60))
        .expect("an instant past now has room one minute back");
    let mut cooldown = cooled(before, secs(30));
    cooldown.extend(edge, MAX_REMOTE_COOLDOWN);
    assert_eq!(cooldown.remaining(before), secs(30));
}

#[test]
fn remaining_counts_down_and_saturates_at_zero() {
    let t0 = Instant::now();
    let cooldown = cooled(t0, secs(5));
    let cases = [
        (Duration::ZERO, secs(5)),
        (secs(4), secs(1)),
        (secs(5) - Duration::from_millis(1), Duration::from_millis(1)),
        (secs(5), Duration::ZERO),
        (secs(6), Duration::ZERO),
        (secs(86_400), Duration::ZERO),
    ];
    for (elapsed, expected) in cases {
        assert_eq!(
            cooldown.remaining(t0 + elapsed),
            expected,
            "after {elapsed:?}"
        );
    }
}
