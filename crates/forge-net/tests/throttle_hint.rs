use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_net::{
    MAX_REMOTE_COOLDOWN, ThrottleHint, ThrottleHintFamily, ThrottleScope, parse_retry_after,
    parse_throttle_hint,
};

const HTTP_DATE_NOW_EPOCH_SECS: u64 = 784_111_777;
const TWITCH_NOW_EPOCH_SECS: u64 = 1_700_000_000;

fn at_epoch(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn millis(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn hint(headers: &[(&str, &str)], family: ThrottleHintFamily, now: SystemTime) -> ThrottleHint {
    parse_throttle_hint(headers, family, now)
}

#[test]
fn retry_after_delay_seconds_follow_the_unsigned_decimal_grammar() {
    let now = at_epoch(HTTP_DATE_NOW_EPOCH_SECS);
    for (value, expected) in [
        ("0", Some(Duration::ZERO)),
        ("5", Some(secs(5))),
        ("  5  ", Some(secs(5))),
        ("\t7\r\n", Some(secs(7))),
        ("1.5", Some(millis(1500))),
        ("0.25", Some(millis(250))),
        ("2.", Some(secs(2))),
        (".5", Some(millis(500))),
        ("", None),
        ("   ", None),
        ("+5", None),
        ("-5", None),
        ("nan", None),
        ("NaN", None),
        ("inf", None),
        ("infinity", None),
        ("1e3", None),
        ("1E3", None),
        ("1..5", None),
        ("1.2.3", None),
        (".", None),
        ("5s", None),
        ("5 5", None),
        ("\u{ff15}", None),
        ("soon", None),
    ] {
        assert_eq!(parse_retry_after(value, now), expected, "value {value:?}");
    }
}

#[test]
fn retry_after_delay_seconds_clamp_to_the_remote_ceiling() {
    let now = at_epoch(HTTP_DATE_NOW_EPOCH_SECS);
    let ceiling = MAX_REMOTE_COOLDOWN.as_secs();
    for (value, expected) in [
        ((ceiling - 1).to_string(), secs(ceiling - 1)),
        (ceiling.to_string(), MAX_REMOTE_COOLDOWN),
        ((ceiling + 1).to_string(), MAX_REMOTE_COOLDOWN),
        (u64::MAX.to_string(), MAX_REMOTE_COOLDOWN),
        ("18446744073709551616".to_owned(), MAX_REMOTE_COOLDOWN),
        (
            "99999999999999999999999999999999".to_owned(),
            MAX_REMOTE_COOLDOWN,
        ),
        (
            "99999999999999999999999999999999.5".to_owned(),
            MAX_REMOTE_COOLDOWN,
        ),
    ] {
        assert_eq!(
            parse_retry_after(&value, now),
            Some(expected),
            "value {value:?}"
        );
    }
}

#[test]
fn retry_after_http_date_yields_time_remaining_in_every_rfc9110_form() {
    let now = at_epoch(HTTP_DATE_NOW_EPOCH_SECS);
    for value in [
        "Sun, 06 Nov 1994 08:49:47 GMT",
        "Sunday, 06-Nov-94 08:49:47 GMT",
        "Sun Nov  6 08:49:47 1994",
        "  Sun, 06 Nov 1994 08:49:47 GMT  ",
    ] {
        assert_eq!(
            parse_retry_after(value, now),
            Some(secs(10)),
            "value {value:?}"
        );
    }
}

#[test]
fn retry_after_http_date_at_or_before_now_means_retry_immediately() {
    let now = at_epoch(HTTP_DATE_NOW_EPOCH_SECS);
    for value in [
        "Sun, 06 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:36 GMT",
        "Thu, 01 Jan 1970 00:00:00 GMT",
    ] {
        assert_eq!(
            parse_retry_after(value, now),
            Some(Duration::ZERO),
            "value {value:?}"
        );
    }
}

#[test]
fn retry_after_http_date_beyond_the_ceiling_clamps() {
    let now = at_epoch(HTTP_DATE_NOW_EPOCH_SECS);
    assert_eq!(
        parse_retry_after("Sun, 06 Nov 1994 10:49:37 GMT", now),
        Some(MAX_REMOTE_COOLDOWN)
    );
}

#[test]
fn retry_after_rejects_malformed_http_dates() {
    let now = at_epoch(HTTP_DATE_NOW_EPOCH_SECS);
    for value in [
        "Sun, 06 Nov 1994 08:49:47",
        "Sun, 32 Nov 1994 08:49:47 GMT",
        "1994-11-06T08:49:47Z",
        "tomorrow",
    ] {
        assert_eq!(parse_retry_after(value, now), None, "value {value:?}");
    }
}

#[test]
fn retry_after_family_reads_only_retry_after_with_bucket_scope() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for (headers, expected) in [
        (vec![("Retry-After", "12")], Some(secs(12))),
        (vec![("ratelimit-reset", "1700000030")], None),
        (vec![("x-ratelimit-reset-after", "3")], None),
        (vec![("retry-after", "bogus")], None),
        (vec![], None),
    ] {
        assert_eq!(
            hint(&headers, ThrottleHintFamily::RetryAfter, now),
            ThrottleHint {
                wait: expected,
                scope: ThrottleScope::Bucket
            },
            "headers {headers:?}"
        );
    }
}

#[test]
fn retry_after_family_ignores_the_global_marker() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    let headers = [("retry-after", "4"), ("x-ratelimit-global", "true")];
    assert_eq!(
        hint(&headers, ThrottleHintFamily::RetryAfter, now).scope,
        ThrottleScope::Bucket
    );
}

#[test]
fn twitch_reset_epoch_resolves_against_injected_now() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for (reset, expected) in [
        ("1700000030", secs(30)),
        (" 1700000001 ", secs(1)),
        ("1700000030.5", millis(30_500)),
        ("1700000000", Duration::ZERO),
        ("1699999990", Duration::ZERO),
        ("0", Duration::ZERO),
        ("1700003599", secs(3599)),
        ("1700003600", MAX_REMOTE_COOLDOWN),
        ("1700003601", MAX_REMOTE_COOLDOWN),
        ("17000000000000", MAX_REMOTE_COOLDOWN),
        ("18446744073709551615", MAX_REMOTE_COOLDOWN),
        ("99999999999999999999", MAX_REMOTE_COOLDOWN),
    ] {
        let headers = [("Ratelimit-Reset", reset)];
        assert_eq!(
            hint(&headers, ThrottleHintFamily::ResetEpochSeconds, now),
            ThrottleHint {
                wait: Some(expected),
                scope: ThrottleScope::Bucket
            },
            "reset {reset:?}"
        );
    }
}

#[test]
fn twitch_reset_epoch_takes_precedence_over_retry_after() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    let headers = [("retry-after", "7"), ("ratelimit-reset", "1700000030")];
    assert_eq!(
        hint(&headers, ThrottleHintFamily::ResetEpochSeconds, now).wait,
        Some(secs(30))
    );
}

#[test]
fn twitch_falls_back_to_retry_after_when_reset_is_absent_or_malformed() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for headers in [
        vec![("retry-after", "7")],
        vec![("ratelimit-reset", ""), ("retry-after", "7")],
        vec![("ratelimit-reset", "-1700000030"), ("retry-after", "7")],
        vec![("ratelimit-reset", "1e9"), ("retry-after", "7")],
        vec![
            ("ratelimit-reset", "Sun, 06 Nov 1994 08:49:47 GMT"),
            ("retry-after", "7"),
        ],
    ] {
        assert_eq!(
            hint(&headers, ThrottleHintFamily::ResetEpochSeconds, now).wait,
            Some(secs(7)),
            "headers {headers:?}"
        );
    }
}

#[test]
fn twitch_without_any_usable_hint_reports_no_wait() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for headers in [
        vec![],
        vec![("ratelimit-reset", "soon")],
        vec![("ratelimit-reset", "soon"), ("retry-after", "later")],
    ] {
        assert_eq!(
            hint(&headers, ThrottleHintFamily::ResetEpochSeconds, now),
            ThrottleHint {
                wait: None,
                scope: ThrottleScope::Bucket
            },
            "headers {headers:?}"
        );
    }
}

#[test]
fn discord_scope_detects_global_from_either_marker() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for (headers, expected) in [
        (vec![], ThrottleScope::Bucket),
        (vec![("X-RateLimit-Scope", "global")], ThrottleScope::Global),
        (
            vec![("x-ratelimit-scope", " GLOBAL ")],
            ThrottleScope::Global,
        ),
        (vec![("x-ratelimit-scope", "Global")], ThrottleScope::Global),
        (vec![("x-ratelimit-scope", "user")], ThrottleScope::Bucket),
        (vec![("x-ratelimit-scope", "shared")], ThrottleScope::Bucket),
        (
            vec![("x-ratelimit-scope", "globally")],
            ThrottleScope::Bucket,
        ),
        (vec![("X-RateLimit-Global", "true")], ThrottleScope::Global),
        (
            vec![("x-ratelimit-global", "\tTRUE\r\n")],
            ThrottleScope::Global,
        ),
        (vec![("x-ratelimit-global", "false")], ThrottleScope::Bucket),
        (vec![("x-ratelimit-global", "1")], ThrottleScope::Bucket),
        (
            vec![
                ("x-ratelimit-scope", "user"),
                ("x-ratelimit-global", "true"),
            ],
            ThrottleScope::Global,
        ),
        (
            vec![
                ("x-ratelimit-scope", "global"),
                ("x-ratelimit-global", "false"),
            ],
            ThrottleScope::Global,
        ),
    ] {
        assert_eq!(
            hint(&headers, ThrottleHintFamily::ResetAfterSeconds, now).scope,
            expected,
            "headers {headers:?}"
        );
    }
}

#[test]
fn discord_wait_source_order_depends_on_scope() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for (headers, expected) in [
        (
            vec![("x-ratelimit-reset-after", "1.5"), ("retry-after", "9")],
            millis(1500),
        ),
        (
            vec![
                ("x-ratelimit-scope", "user"),
                ("x-ratelimit-reset-after", "2"),
                ("retry-after", "9"),
            ],
            secs(2),
        ),
        (
            vec![
                ("x-ratelimit-scope", "shared"),
                ("x-ratelimit-reset-after", "2"),
                ("retry-after", "9"),
            ],
            secs(2),
        ),
        (vec![("retry-after", "9")], secs(9)),
        (
            vec![("x-ratelimit-reset-after", "-2"), ("retry-after", "9")],
            secs(9),
        ),
        (
            vec![
                ("x-ratelimit-scope", "global"),
                ("x-ratelimit-reset-after", "2"),
                ("retry-after", "9"),
            ],
            secs(9),
        ),
        (
            vec![
                ("x-ratelimit-global", "true"),
                ("x-ratelimit-reset-after", "2"),
                ("retry-after", "9"),
            ],
            secs(9),
        ),
        (
            vec![
                ("x-ratelimit-scope", "global"),
                ("x-ratelimit-reset-after", "2"),
            ],
            secs(2),
        ),
        (
            vec![
                ("x-ratelimit-scope", "global"),
                ("x-ratelimit-reset-after", "2"),
                ("retry-after", "nan"),
            ],
            secs(2),
        ),
        (
            vec![("x-ratelimit-reset-after", "7200.5")],
            MAX_REMOTE_COOLDOWN,
        ),
    ] {
        assert_eq!(
            hint(&headers, ThrottleHintFamily::ResetAfterSeconds, now).wait,
            Some(expected),
            "headers {headers:?}"
        );
    }
}

#[test]
fn discord_scope_is_reported_even_without_a_wait() {
    let now = at_epoch(TWITCH_NOW_EPOCH_SECS);
    for (headers, scope) in [
        (vec![("x-ratelimit-scope", "global")], ThrottleScope::Global),
        (
            vec![
                ("x-ratelimit-global", "true"),
                ("x-ratelimit-reset-after", "x"),
            ],
            ThrottleScope::Global,
        ),
        (vec![("x-ratelimit-scope", "user")], ThrottleScope::Bucket),
    ] {
        assert_eq!(
            hint(&headers, ThrottleHintFamily::ResetAfterSeconds, now),
            ThrottleHint { wait: None, scope },
            "headers {headers:?}"
        );
    }
}

#[test]
fn wait_or_prefers_the_hint_and_clamps_the_default() {
    let ceiling = MAX_REMOTE_COOLDOWN.as_secs();
    for (wait, default, expected) in [
        (Some(secs(3)), secs(5), secs(3)),
        (Some(Duration::ZERO), secs(5), Duration::ZERO),
        (None, secs(5), secs(5)),
        (None, Duration::ZERO, Duration::ZERO),
        (None, secs(ceiling - 1), secs(ceiling - 1)),
        (None, secs(ceiling + 1), MAX_REMOTE_COOLDOWN),
        (None, Duration::MAX, MAX_REMOTE_COOLDOWN),
    ] {
        let hint = ThrottleHint {
            wait,
            scope: ThrottleScope::Bucket,
        };
        assert_eq!(
            hint.wait_or(default),
            expected,
            "wait {wait:?} default {default:?}"
        );
    }
}
