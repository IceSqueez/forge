#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
use forge_events::{Event, EventSource};
use forge_platform_twitch::register_twitch_triggers;
use forge_registry::{TriggerKindDescriptor, TriggerRegistry};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const FOLLOW: &str = "twitch.channel.follow";
const RAID: &str = "twitch.channel.raid_received";

fn follow_moment_of(kind_id: &str, payload: Value) -> Option<OffsetDateTime> {
    let mut reg = TriggerRegistry::new();
    register_twitch_triggers(&mut reg).expect("the twitch triggers register");
    let descriptor: &dyn TriggerKindDescriptor = reg.get(kind_id).expect("kind is registered");
    let kind = descriptor.event_filter().kind_prefix.unwrap_or_default();
    descriptor.follow_moment(&Event::new(EventSource::Twitch, kind, payload))
}

#[test]
fn follow_kind_reports_the_exact_instant_of_a_valid_followed_at() {
    let expected = OffsetDateTime::parse("2026-06-13T18:00:00Z", &Rfc3339).expect("fixture parses");
    let moment = follow_moment_of(FOLLOW, json!({ "followed_at": "2026-06-13T18:00:00Z" }));
    assert_eq!(moment, Some(expected));
}

#[test]
fn follow_kind_reports_no_moment_for_an_absent_or_unreadable_followed_at() {
    let cases = [
        (json!({}), "missing"),
        (json!({ "followed_at": null }), "null"),
        (json!({ "followed_at": "" }), "empty"),
        (json!({ "followed_at": 1_781_373_600 }), "number"),
        (json!({ "followed_at": true }), "bool"),
        (json!({ "followed_at": ["2026-06-13T18:00:00Z"] }), "array"),
        (json!({ "followed_at": "yesterday" }), "malformed"),
    ];
    for (payload, what) in cases {
        assert_eq!(follow_moment_of(FOLLOW, payload), None, "{what}");
    }
}

#[test]
fn a_kind_that_is_not_a_follow_reports_no_moment_even_with_followed_at_present() {
    let moment = follow_moment_of(RAID, json!({ "followed_at": "2026-06-13T18:00:00Z" }));
    assert_eq!(moment, None);
}
