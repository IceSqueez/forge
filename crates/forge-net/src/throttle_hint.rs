use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header::RETRY_AFTER;

use crate::cooldown::MAX_REMOTE_COOLDOWN;
use crate::header_lookup::HeaderLookup;

const RATELIMIT_RESET: &str = "ratelimit-reset";
const X_RATELIMIT_RESET_AFTER: &str = "x-ratelimit-reset-after";
const X_RATELIMIT_SCOPE: &str = "x-ratelimit-scope";
const X_RATELIMIT_GLOBAL: &str = "x-ratelimit-global";
const GLOBAL_SCOPE_VALUE: &str = "global";
const GLOBAL_FLAG_VALUE: &str = "true";
const DECIMAL_POINT: char = '.';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThrottleHintFamily {
    RetryAfter,
    ResetEpochSeconds,
    ResetAfterSeconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThrottleScope {
    #[default]
    Bucket,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThrottleHint {
    pub wait: Option<Duration>,
    pub scope: ThrottleScope,
}

impl ThrottleHint {
    pub fn wait_or(&self, no_hint_default: Duration) -> Duration {
        self.wait
            .unwrap_or(no_hint_default)
            .min(MAX_REMOTE_COOLDOWN)
    }
}

pub fn parse_throttle_hint<H: HeaderLookup + ?Sized>(
    headers: &H,
    family: ThrottleHintFamily,
    now: SystemTime,
) -> ThrottleHint {
    let retry_after = || {
        headers
            .header_value(RETRY_AFTER.as_str())
            .and_then(|value| parse_retry_after(value, now))
    };
    match family {
        ThrottleHintFamily::RetryAfter => ThrottleHint {
            wait: retry_after(),
            scope: ThrottleScope::Bucket,
        },
        ThrottleHintFamily::ResetEpochSeconds => ThrottleHint {
            wait: headers
                .header_value(RATELIMIT_RESET)
                .and_then(|value| parse_reset_epoch(value, now))
                .or_else(retry_after),
            scope: ThrottleScope::Bucket,
        },
        ThrottleHintFamily::ResetAfterSeconds => {
            let reset_after = || {
                headers
                    .header_value(X_RATELIMIT_RESET_AFTER)
                    .and_then(parse_delay_seconds)
            };
            let scope = reset_after_scope(headers);
            let wait = match scope {
                ThrottleScope::Bucket => reset_after().or_else(retry_after),
                ThrottleScope::Global => retry_after().or_else(reset_after),
            };
            ThrottleHint { wait, scope }
        }
    }
}

pub fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    parse_delay_seconds(value).or_else(|| {
        httpdate::parse_http_date(value)
            .ok()
            .map(|at| remaining_until(at, now))
    })
}

fn parse_reset_epoch(value: &str, now: SystemTime) -> Option<Duration> {
    let since_epoch = parse_decimal_seconds(value)?;
    Some(
        UNIX_EPOCH
            .checked_add(since_epoch)
            .map_or(MAX_REMOTE_COOLDOWN, |at| remaining_until(at, now)),
    )
}

fn parse_delay_seconds(value: &str) -> Option<Duration> {
    parse_decimal_seconds(value).map(|delay| delay.min(MAX_REMOTE_COOLDOWN))
}

fn parse_decimal_seconds(value: &str) -> Option<Duration> {
    let value = value.trim();
    if !is_unsigned_decimal(value) {
        return None;
    }
    if !value.contains(DECIMAL_POINT) {
        return Some(
            value
                .parse::<u64>()
                .map_or(Duration::MAX, Duration::from_secs),
        );
    }
    let seconds = value.parse::<f64>().ok()?;
    Some(Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX))
}

fn is_unsigned_decimal(value: &str) -> bool {
    let mut digits = 0_usize;
    let mut points = 0_usize;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
        } else if ch == DECIMAL_POINT {
            points += 1;
        } else {
            return false;
        }
    }
    digits > 0 && points <= 1
}

fn remaining_until(at: SystemTime, now: SystemTime) -> Duration {
    at.duration_since(now)
        .unwrap_or(Duration::ZERO)
        .min(MAX_REMOTE_COOLDOWN)
}

fn reset_after_scope<H: HeaderLookup + ?Sized>(headers: &H) -> ThrottleScope {
    let scope_is_global = headers
        .header_value(X_RATELIMIT_SCOPE)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(GLOBAL_SCOPE_VALUE));
    let flag_is_global = headers
        .header_value(X_RATELIMIT_GLOBAL)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(GLOBAL_FLAG_VALUE));
    if scope_is_global || flag_is_global {
        ThrottleScope::Global
    } else {
        ThrottleScope::Bucket
    }
}
