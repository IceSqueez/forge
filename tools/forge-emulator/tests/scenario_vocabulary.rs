//! Scenario vocabulary behaviour: payload matching over real forge events and crowd expansion.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_emulator::scenario::{
    ChatViewer, Crowd, CrowdLine, EventPattern, ObservedEvent, PayloadMatchers,
};
use forge_emulator::twitch::ViewerBadge;
use forge_events::{Event, EventSource};
use serde_json::json;

fn chat_event() -> Event {
    Event::new(
        EventSource::Twitch,
        "chat.message",
        json!({
            "message": "!ping now",
            "user": { "login": "alice", "id": "200000042" },
            "reply": null,
            "badges": [{ "set_id": "moderator" }],
            "a/b": { "c~d": 7 }
        }),
    )
}

fn observed(spec: serde_json::Value) -> ObservedEvent {
    serde_json::from_value(spec).unwrap()
}

#[test]
fn event_pattern_matches_forge_event_shapes() {
    let event = chat_event();
    let cases = [
        (
            "kind alone",
            json!({ "kind": "chat.message", "within_ms": 1 }),
            true,
        ),
        (
            "other kind",
            json!({ "kind": "chat.messages", "within_ms": 1 }),
            false,
        ),
        (
            "matching source",
            json!({ "source": "twitch", "kind": "chat.message", "within_ms": 1 }),
            true,
        ),
        (
            "other source",
            json!({ "source": "kick", "kind": "chat.message", "within_ms": 1 }),
            false,
        ),
        (
            "nested equals",
            json!({ "kind": "chat.message", "payload": { "/user/login": { "equals": "alice" } }, "within_ms": 1 }),
            true,
        ),
        (
            "nested equals on another value",
            json!({ "kind": "chat.message", "payload": { "/user/login": { "equals": "bob" } }, "within_ms": 1 }),
            false,
        ),
        (
            "equals is type-strict",
            json!({ "kind": "chat.message", "payload": { "/user/id": { "equals": 200000042 } }, "within_ms": 1 }),
            false,
        ),
        (
            "equals null on a null field",
            json!({ "kind": "chat.message", "payload": { "/reply": { "equals": null } }, "within_ms": 1 }),
            true,
        ),
        (
            "equals null on a missing field",
            json!({ "kind": "chat.message", "payload": { "/cheer": { "equals": null } }, "within_ms": 1 }),
            false,
        ),
        (
            "present on a null field",
            json!({ "kind": "chat.message", "payload": { "/reply": { "present": true } }, "within_ms": 1 }),
            true,
        ),
        (
            "absent field",
            json!({ "kind": "chat.message", "payload": { "/cheer": { "present": false } }, "within_ms": 1 }),
            true,
        ),
        (
            "contains on a string",
            json!({ "kind": "chat.message", "payload": { "/message": { "contains": "ping" } }, "within_ms": 1 }),
            true,
        ),
        (
            "contains never matches a non-string",
            json!({ "kind": "chat.message", "payload": { "/user": { "contains": "alice" } }, "within_ms": 1 }),
            false,
        ),
        (
            "array index pointer",
            json!({ "kind": "chat.message", "payload": { "/badges/0/set_id": { "equals": "moderator" } }, "within_ms": 1 }),
            true,
        ),
        (
            "escaped pointer segments",
            json!({ "kind": "chat.message", "payload": { "/a~1b/c~0d": { "equals": 7 } }, "within_ms": 1 }),
            true,
        ),
        (
            "every matcher must hold",
            json!({ "kind": "chat.message", "payload": {
                "/message": { "contains": "ping" },
                "/user/login": { "equals": "bob" }
            }, "within_ms": 1 }),
            false,
        ),
    ];
    for (label, spec, expected) in cases {
        assert_eq!(
            observed(spec).pattern().matches(&event),
            expected,
            "case: {label}"
        );
    }
}

#[test]
fn empty_pointer_compares_the_whole_payload() {
    let event = Event::new(EventSource::Core, "timer.tick", json!(3));
    let payload: PayloadMatchers = serde_json::from_value(json!({ "": { "equals": 3 } })).unwrap();
    let pattern = EventPattern {
        source: None,
        kind: "timer.tick",
        payload: &payload,
    };
    assert!(pattern.matches(&event));
}

fn plan_summary(crowd: &Crowd) -> Vec<(String, String, CrowdLine)> {
    crowd
        .plan()
        .into_iter()
        .map(|message| (message.viewer.login, message.text, message.line))
        .collect()
}

#[test]
fn crowd_plan_rotates_chatter_renders_placeholders_and_spreads_commands() {
    let crowd: Crowd = serde_json::from_value(json!({
        "viewers": 4,
        "chatter": ["hi {login}", "#{n}", "gg"],
        "chatter_per_viewer": 2,
        "command_senders": 2,
        "commands": ["!ping", "!dice"]
    }))
    .unwrap();
    let s = |login: &str, text: &str, line| (login.to_owned(), text.to_owned(), line);
    assert_eq!(
        plan_summary(&crowd),
        vec![
            s("crowd_0001", "hi crowd_0001", CrowdLine::Chatter(0)),
            s("crowd_0001", "#1", CrowdLine::Chatter(1)),
            s("crowd_0002", "#2", CrowdLine::Chatter(1)),
            s("crowd_0002", "gg", CrowdLine::Chatter(2)),
            s("crowd_0002", "!ping", CrowdLine::Command(0)),
            s("crowd_0003", "gg", CrowdLine::Chatter(2)),
            s("crowd_0003", "hi crowd_0003", CrowdLine::Chatter(0)),
            s("crowd_0004", "hi crowd_0004", CrowdLine::Chatter(0)),
            s("crowd_0004", "#4", CrowdLine::Chatter(1)),
            s("crowd_0004", "!dice", CrowdLine::Command(1)),
        ]
    );
}

#[test]
fn crowd_command_senders_are_distinct_viewers_at_every_share() {
    for (viewers, senders) in [(1, 1), (7, 1), (7, 3), (40, 8), (9, 9), (1000, 999)] {
        let crowd: Crowd = serde_json::from_value(json!({
            "viewers": viewers,
            "command_senders": senders,
            "commands": ["!ping"]
        }))
        .unwrap();
        let plan = crowd.plan();
        let mut senders_seen: Vec<String> = plan
            .iter()
            .filter(|message| matches!(message.line, CrowdLine::Command(_)))
            .map(|message| message.viewer.user_id.clone())
            .collect();
        assert_eq!(senders_seen.len(), senders as usize, "{viewers} viewers");
        senders_seen.dedup();
        assert_eq!(senders_seen.len(), senders as usize, "{viewers} viewers");
        assert_eq!(
            plan.len() as u64,
            crowd.message_count(),
            "{viewers} viewers"
        );
    }
}

#[test]
fn crowd_viewer_ids_are_stable_and_unique() {
    let crowd: Crowd = serde_json::from_value(json!({ "viewers": 3, "chatter": ["x"] })).unwrap();
    let ids: Vec<String> = crowd
        .plan()
        .into_iter()
        .map(|message| message.viewer.user_id)
        .collect();
    assert_eq!(ids, ["300000001", "300000002", "300000003"]);
}

#[test]
fn chat_viewer_falls_back_to_login_for_display_name_and_keeps_badges() {
    let viewer: ChatViewer = serde_json::from_value(json!({
        "user_id": "1",
        "login": "alice",
        "badges": ["vip", { "subscriber": { "months": 12 } }]
    }))
    .unwrap();
    let viewer = viewer.to_viewer();
    assert_eq!(viewer.display_name, "alice");
    assert_eq!(
        viewer.badges,
        [ViewerBadge::Vip, ViewerBadge::Subscriber { months: 12 }]
    );

    let named: ChatViewer = serde_json::from_value(
        json!({ "user_id": "1", "login": "alice", "display_name": "Алиса" }),
    )
    .unwrap();
    assert_eq!(named.to_viewer().display_name, "Алиса");
}
