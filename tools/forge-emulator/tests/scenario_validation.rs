//! Scenario semantic validation: every rejection class with its pinned location and message.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_emulator::scenario::Scenario;
use serde_json::{Value, json};

fn base() -> Value {
    json!({
        "name": "base",
        "purpose": "a valid starting point",
        "fixture": {
            "twitch": {},
            "chat_commands": [{ "phrase": "!ping", "action_name": "Ping" }]
        },
        "fakes": { "twitch": {} },
        "steps": [
            { "do": { "forge_ready": { "within_ms": 60000 } } },
            { "do": { "twitch_subscribed": { "types": ["channel.chat.message"], "within_ms": 30000 } } }
        ]
    })
}

fn with_steps(steps: impl IntoIterator<Item = Value>) -> Value {
    let mut scenario = base();
    scenario["steps"].as_array_mut().unwrap().extend(steps);
    scenario
}

fn step(action: Value) -> Value {
    json!({ "do": action })
}

fn expecting(action: Value, expect: Value) -> Value {
    json!({ "do": action, "expect": expect })
}

fn chat(text: &str) -> Value {
    json!({ "chat": { "viewer": { "user_id": "200000042", "login": "alice" }, "text": text } })
}

fn pause() -> Value {
    json!({ "pause": { "ms": 10, "reason": "settle" } })
}

fn crowd(spec: Value) -> Value {
    json!({ "crowd": spec })
}

fn problems(scenario: Value) -> Vec<(String, String)> {
    let scenario: Scenario = serde_json::from_value(scenario).unwrap();
    scenario
        .problems()
        .into_iter()
        .map(|problem| (problem.location, problem.message))
        .collect()
}

type Case<'a> = (&'a str, Value, Vec<(&'a str, &'a str)>);

fn assert_cases(cases: Vec<Case<'_>>) {
    for (label, scenario, expected) in cases {
        let expected: Vec<(String, String)> = expected
            .into_iter()
            .map(|(location, message)| (location.to_owned(), message.to_owned()))
            .collect();
        assert_eq!(problems(scenario), expected, "case: {label}");
    }
}

#[test]
fn base_scenario_has_no_problems() {
    assert_eq!(problems(base()), Vec::new());
}

#[test]
fn metadata_and_step_order_problems_are_located() {
    let mut blank_name = base();
    blank_name["name"] = json!("  ");
    let mut blank_purpose = base();
    blank_purpose["purpose"] = json!("");
    let mut no_steps = base();
    no_steps["steps"] = json!([]);
    let mut pause_first = base();
    pause_first["steps"] = json!([step(pause())]);

    assert_cases(vec![
        (
            "blank name",
            blank_name,
            vec![("name", "must not be blank")],
        ),
        (
            "blank purpose",
            blank_purpose,
            vec![("purpose", "must not be blank")],
        ),
        (
            "no steps",
            no_steps,
            vec![("steps", "needs at least one step")],
        ),
        (
            "first step is not forge_ready",
            pause_first,
            vec![(
                "steps[0].do",
                "the first step must be forge_ready: nothing is observable before forge is up",
            )],
        ),
        (
            "forge_ready repeated later",
            with_steps([step(json!({ "forge_ready": { "within_ms": 1000 } }))]),
            vec![(
                "steps[2].do.forge_ready",
                "only the first step may be forge_ready",
            )],
        ),
    ]);
}

#[test]
fn fixture_and_fake_setup_problems_are_located() {
    let mut empty_phrase = base();
    empty_phrase["fixture"]["chat_commands"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "phrase": "", "action_name": "Silent" }));
    let mut duplicate_action = base();
    duplicate_action["fixture"]["chat_commands"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "phrase": "!pong", "action_name": "Ping" }));
    let mut account_without_fake = base();
    account_without_fake["fakes"] = json!({});
    let mut fake_without_account = base();
    fake_without_account["fixture"]["twitch"] = Value::Null;
    fake_without_account["steps"] = json!([step(json!({ "forge_ready": { "within_ms": 1 } }))]);
    let mut keepalive_zero = base();
    keepalive_zero["fakes"]["twitch"]["keepalive_interval_ms"] = json!(0);
    let mut keepalive_over = base();
    keepalive_over["fakes"]["twitch"]["keepalive_interval_ms"] = json!(14_001);

    assert_cases(vec![
        (
            "fixture validation is reused",
            empty_phrase,
            vec![(
                "fixture",
                "chat command for action `Silent` has an empty phrase",
            )],
        ),
        (
            "two commands share an action name",
            duplicate_action,
            vec![(
                "fixture.chat_commands[1].action_name",
                "`Ping` is already the action of chat_commands[0], so run_action could not tell them apart",
            )],
        ),
        (
            "seeded account without a fake would reach the real Twitch",
            account_without_fake,
            vec![
                (
                    "fakes.twitch",
                    "is required because the fixture seeds a Twitch account; without it forge would reach the real Twitch",
                ),
                (
                    "steps[1].do.twitch_subscribed",
                    "needs a fake Twitch: add fixture.twitch and fakes.twitch",
                ),
            ],
        ),
        (
            "fake without an account has no credentials to accept",
            fake_without_account,
            vec![(
                "fakes.twitch",
                "needs fixture.twitch: the fake accepts only the credentials the fixture seeds",
            )],
        ),
        (
            "keepalive below range",
            keepalive_zero,
            vec![(
                "fakes.twitch.keepalive_interval_ms",
                "must be between 1 and 14000, got 0",
            )],
        ),
        (
            "keepalive at forge's silence limit",
            keepalive_over,
            vec![(
                "fakes.twitch.keepalive_interval_ms",
                "must be between 1 and 14000, got 14001",
            )],
        ),
    ]);
}

#[test]
fn step_problems_are_located() {
    let mut chat_right_after_ready = base();
    chat_right_after_ready["steps"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    chat_right_after_ready["steps"]
        .as_array_mut()
        .unwrap()
        .push(step(chat("!ping")));
    let mut other_subscription_only = base();
    other_subscription_only["steps"][1]["do"]["twitch_subscribed"]["types"] =
        json!(["channel.follow"]);
    other_subscription_only["steps"]
        .as_array_mut()
        .unwrap()
        .push(step(chat("hello")));
    let mut reconnect_before_subscribed = base();
    reconnect_before_subscribed["steps"][1] =
        step(json!({ "session_reconnect": { "within_ms": 1000 } }));
    let mut no_twitch_at_all = base();
    no_twitch_at_all["fixture"]["twitch"] = Value::Null;
    no_twitch_at_all["fakes"] = json!({});

    let chat_before = "sends chat before any twitch_subscribed step waits for channel.chat.message, so the fake may have nowhere to deliver it";
    assert_cases(vec![
        (
            "chat before any subscription wait",
            chat_right_after_ready,
            vec![("steps[1].do.chat", chat_before)],
        ),
        (
            "chat after waiting only for an unrelated subscription",
            other_subscription_only,
            vec![("steps[2].do.chat", chat_before)],
        ),
        (
            "reconnect before any subscription wait",
            reconnect_before_subscribed,
            vec![(
                "steps[1].do.session_reconnect",
                "comes before any twitch_subscribed step, so no EventSub session is known to be live",
            )],
        ),
        (
            "twitch step without a fake",
            no_twitch_at_all,
            vec![(
                "steps[1].do.twitch_subscribed",
                "needs a fake Twitch: add fixture.twitch and fakes.twitch",
            )],
        ),
        (
            "run_action names an undefined action",
            with_steps([step(json!({ "run_action": { "action": "ping" } }))]),
            vec![(
                "steps[2].do.run_action.action",
                "names action `ping`, which the fixture does not define",
            )],
        ),
        (
            "set_global with a blank name and a null value",
            with_steps([step(json!({ "set_global": { "name": "", "value": null } }))]),
            vec![
                ("steps[2].do.set_global.name", "must not be blank"),
                (
                    "steps[2].do.set_global.value",
                    "cannot be stored as a global: null is not a supported Variant value",
                ),
            ],
        ),
        (
            "pause out of range with no reason",
            with_steps([
                step(json!({ "pause": { "ms": 0, "reason": " " } })),
                step(json!({ "pause": { "ms": 5001, "reason": "x" } })),
            ]),
            vec![
                ("steps[2].do.pause.ms", "must be between 1 and 5000, got 0"),
                ("steps[2].do.pause.reason", "must not be blank"),
                (
                    "steps[3].do.pause.ms",
                    "must be between 1 and 5000, got 5001",
                ),
            ],
        ),
        (
            "subscription wait with blank and repeated types",
            with_steps([step(json!({
                "twitch_subscribed": { "types": ["", "channel.follow", "channel.follow"], "within_ms": 120001 }
            }))]),
            vec![
                (
                    "steps[2].do.twitch_subscribed.types[0]",
                    "must not be blank",
                ),
                (
                    "steps[2].do.twitch_subscribed.types[2]",
                    "repeats `channel.follow`",
                ),
                (
                    "steps[2].do.twitch_subscribed.within_ms",
                    "must be between 1 and 120000, got 120001",
                ),
            ],
        ),
        (
            "subscription wait with no types",
            with_steps([step(
                json!({ "twitch_subscribed": { "types": [], "within_ms": 1 } }),
            )]),
            vec![(
                "steps[2].do.twitch_subscribed.types",
                "must list at least one subscription type",
            )],
        ),
        (
            "chat with a blank viewer and text",
            with_steps([step(json!({
                "chat": { "viewer": { "user_id": "", "login": " " }, "text": "" }
            }))]),
            vec![
                ("steps[2].do.chat.viewer.user_id", "must not be blank"),
                ("steps[2].do.chat.viewer.login", "must not be blank"),
                ("steps[2].do.chat.text", "must not be blank"),
            ],
        ),
    ]);
}

#[test]
fn deadline_bounds_reject_zero_and_one_over_the_ceiling() {
    let mut ready_zero = base();
    ready_zero["steps"][0]["do"]["forge_ready"]["within_ms"] = json!(0);
    let mut ready_over = base();
    ready_over["steps"][0]["do"]["forge_ready"]["within_ms"] = json!(300_001);

    assert_cases(vec![
        (
            "forge_ready zero",
            ready_zero,
            vec![(
                "steps[0].do.forge_ready.within_ms",
                "must be between 1 and 300000, got 0",
            )],
        ),
        (
            "forge_ready over",
            ready_over,
            vec![(
                "steps[0].do.forge_ready.within_ms",
                "must be between 1 and 300000, got 300001",
            )],
        ),
        (
            "reconnect wait over",
            with_steps([step(
                json!({ "session_reconnect": { "within_ms": 120_001 } }),
            )]),
            vec![(
                "steps[2].do.session_reconnect.within_ms",
                "must be between 1 and 120000, got 120001",
            )],
        ),
    ]);
}

#[test]
fn crowd_problems_are_located() {
    assert_cases(vec![
        (
            "no viewers",
            with_steps([step(crowd(json!({ "viewers": 0, "chatter": ["hi"] })))]),
            vec![(
                "steps[2].do.crowd.viewers",
                "must be between 1 and 100000, got 0",
            )],
        ),
        (
            "one viewer over the ceiling",
            with_steps([step(crowd(
                json!({ "viewers": 100_001, "chatter": ["hi"] }),
            ))]),
            vec![(
                "steps[2].do.crowd.viewers",
                "must be between 1 and 100000, got 100001",
            )],
        ),
        (
            "more command senders than viewers",
            with_steps([step(crowd(
                json!({ "viewers": 2, "command_senders": 3, "commands": ["!ping"] }),
            ))]),
            vec![(
                "steps[2].do.crowd.command_senders",
                "exceeds the crowd's 2 viewers",
            )],
        ),
        (
            "command senders with no commands",
            with_steps([step(crowd(
                json!({ "viewers": 2, "chatter": ["hi"], "command_senders": 1 }),
            ))]),
            vec![(
                "steps[2].do.crowd.commands",
                "must list at least one command when command_senders is set",
            )],
        ),
        (
            "commands with no senders",
            with_steps([step(crowd(
                json!({ "viewers": 2, "chatter": ["hi"], "commands": ["!ping"] }),
            ))]),
            vec![(
                "steps[2].do.crowd.command_senders",
                "must be at least 1 when commands are listed",
            )],
        ),
        (
            "silent crowd",
            with_steps([step(crowd(json!({ "viewers": 2 })))]),
            vec![(
                "steps[2].do.crowd",
                "sends no messages: give it chatter or command_senders",
            )],
        ),
        (
            "broken chatter templates",
            with_steps([step(crowd(json!({
                "viewers": 2,
                "chatter": ["hi {name}", "hi {login", "hi }", " "]
            })))]),
            vec![
                (
                    "steps[2].do.crowd.chatter[0]",
                    "uses unknown placeholder `{name}`; only {login} and {n} exist",
                ),
                (
                    "steps[2].do.crowd.chatter[1]",
                    "has a `{` with no matching `}`",
                ),
                (
                    "steps[2].do.crowd.chatter[2]",
                    "has a `}` with no matching `{`",
                ),
                ("steps[2].do.crowd.chatter[3]", "must not be blank"),
            ],
        ),
        (
            "command that matches no fixture phrase",
            with_steps([step(crowd(
                json!({ "viewers": 2, "command_senders": 1, "commands": ["!pong"] }),
            ))]),
            vec![(
                "steps[2].do.crowd.commands[0]",
                "`!pong` runs no chat command in the fixture",
            )],
        ),
        (
            "chatter that would run a command",
            with_steps([step(crowd(
                json!({ "viewers": 3, "chatter": ["hello", "!PING number {n}"] }),
            ))]),
            vec![(
                "steps[2].do.crowd.chatter[1]",
                "renders `!PING number 2`, which starts with the fixture command `!ping`, so chatter would run it",
            )],
        ),
        (
            "one message over the crowd ceiling",
            with_steps([step(crowd(json!({
                "viewers": 100_000,
                "chatter": ["hi"],
                "chatter_per_viewer": 10,
                "command_senders": 1,
                "commands": ["!ping"]
            })))]),
            vec![(
                "steps[2].do.crowd",
                "sends 1000001 messages, more than the limit of 1000000",
            )],
        ),
        (
            "chatter per viewer and spacing over their ceilings",
            with_steps([step(crowd(json!({
                "viewers": 1,
                "chatter": ["hi"],
                "chatter_per_viewer": 21,
                "spacing_ms": 1001
            })))]),
            vec![
                (
                    "steps[2].do.crowd.chatter_per_viewer",
                    "must be between 1 and 20, got 21",
                ),
                (
                    "steps[2].do.crowd.spacing_ms",
                    "must be between 0 and 1000, got 1001",
                ),
            ],
        ),
    ]);
}

#[test]
fn event_expectation_problems_are_located() {
    let ready = json!({ "forge_ready": { "within_ms": 1000 } });
    let on_ready = |expect: Value| {
        let mut scenario = base();
        scenario["steps"][0] = expecting(ready.clone(), json!([expect]));
        scenario
    };
    assert_cases(vec![
        (
            "blank kind and a zero deadline",
            on_ready(json!({ "event": { "kind": "", "within_ms": 0 } })),
            vec![
                ("steps[0].expect[0].event.kind", "must not be blank"),
                (
                    "steps[0].expect[0].event.within_ms",
                    "must be between 1 and 120000, got 0",
                ),
            ],
        ),
        (
            "deadline one over the ceiling",
            on_ready(json!({ "event": { "kind": "global.set", "within_ms": 120_001 } })),
            vec![(
                "steps[0].expect[0].event.within_ms",
                "must be between 1 and 120000, got 120001",
            )],
        ),
        (
            "zero count",
            on_ready(
                json!({ "event": { "kind": "global.set", "count": { "exactly": 0 }, "within_ms": 1 } }),
            ),
            vec![(
                "steps[0].expect[0].event.count",
                "must be at least 1; expect absence with event_absent",
            )],
        ),
        (
            "payload keys that are not pointers and an empty contains",
            on_ready(json!({ "event": {
                "kind": "global.set",
                "payload": { "key": { "equals": "x" }, "/via": { "contains": "" } },
                "within_ms": 1
            } })),
            vec![
                (
                    "steps[0].expect[0].event.payload[/via].contains",
                    "must not be empty: it would match every string",
                ),
                (
                    "steps[0].expect[0].event.payload",
                    "key `key` is not a JSON pointer; write `/key`",
                ),
            ],
        ),
        (
            "action.start for an action the fixture lacks",
            on_ready(json!({ "event": {
                "kind": "action.start",
                "payload": { "/action_name": { "equals": "Pong" } },
                "within_ms": 1
            } })),
            vec![(
                "steps[0].expect[0].event.payload[/action_name]",
                "expects action `Pong` to start, which the fixture does not define",
            )],
        ),
        (
            "command.matched before any command is sent",
            on_ready(json!({ "event": { "kind": "command.matched", "within_ms": 1 } })),
            vec![(
                "steps[0].expect[0].event",
                "needs 1 command.matched event(s), but chat sent up to this step can produce at most 0",
            )],
        ),
        (
            "blank event name",
            on_ready(json!({ "event": { "name": "", "kind": "global.set", "within_ms": 1 } })),
            vec![("steps[0].expect[0].event.name", "must not be blank")],
        ),
        (
            "absent-event window out of range",
            on_ready(json!({ "event_absent": { "kind": "trigger.blocked", "window_ms": 0 } })),
            vec![(
                "steps[0].expect[0].event_absent.window_ms",
                "must be between 1 and 120000, got 0",
            )],
        ),
    ]);
}

#[test]
fn command_expectations_that_cannot_pass_are_rejected() {
    let mut no_commands = with_steps([expecting(
        chat("!ping"),
        json!([{ "event": { "kind": "command.matched", "within_ms": 1000 } }]),
    )]);
    no_commands["fixture"]["chat_commands"] = json!([]);

    assert_cases(vec![
        (
            "fixture defines no command",
            no_commands,
            vec![(
                "steps[2].expect[0].event.kind",
                "expects command.matched, but the fixture defines no chat command",
            )],
        ),
        (
            "pinned command phrase not in the fixture",
            with_steps([expecting(
                chat("!ping"),
                json!([{ "event": {
                    "kind": "command.matched",
                    "payload": { "/command": { "equals": "!pong" } },
                    "within_ms": 1000
                } }]),
            )]),
            vec![(
                "steps[2].expect[0].event.payload[/command]",
                "expects command `!pong`, which the fixture does not define",
            )],
        ),
        (
            "more matches than the chat so far can produce",
            with_steps([expecting(
                chat("!ping"),
                json!([{ "event": {
                    "kind": "command.matched",
                    "count": { "exactly": 2 },
                    "within_ms": 1000
                } }]),
            )]),
            vec![(
                "steps[2].expect[0].event",
                "needs 2 command.matched event(s), but chat sent up to this step can produce at most 1",
            )],
        ),
        (
            "crowd commands count toward the bound",
            with_steps([expecting(
                crowd(json!({ "viewers": 5, "command_senders": 3, "commands": ["!ping"] })),
                json!([{ "event": {
                    "kind": "command.matched",
                    "count": { "at_least": 4 },
                    "within_ms": 1000
                } }]),
            )]),
            vec![(
                "steps[2].expect[0].event",
                "needs 4 command.matched event(s), but chat sent up to this step can produce at most 3",
            )],
        ),
    ]);
}

#[test]
fn a_message_matching_two_phrases_counts_as_two_command_matches() {
    let mut scenario = with_steps([expecting(
        chat("!pingpong"),
        json!([{ "event": {
            "kind": "command.matched",
            "count": { "exactly": 2 },
            "within_ms": 1000
        } }]),
    )]);
    scenario["fixture"]["chat_commands"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "phrase": "!pingp", "action_name": "PingP" }));
    assert_eq!(problems(scenario), Vec::new());
}

#[test]
fn unicode_command_phrases_match_case_insensitively() {
    let mut scenario = with_steps([expecting(
        chat("!ПРИВІТ друже"),
        json!([{ "event": {
            "kind": "command.matched",
            "payload": { "/command": { "equals": "!привіт" } },
            "within_ms": 1000
        } }]),
    )]);
    scenario["fixture"]["chat_commands"] = json!([{ "phrase": "!привіт", "action_name": "Hello" }]);
    assert_eq!(problems(scenario), Vec::new());
}

#[test]
fn causation_problems_are_located() {
    let named = |name: &str, kind: &str| json!({ "event": { "name": name, "kind": kind, "within_ms": 1000 } });
    assert_cases(vec![
        (
            "unknown names",
            with_steps([expecting(
                pause(),
                json!([{ "caused_by": { "effect": "done", "cause": "start" } }]),
            )]),
            vec![
                (
                    "steps[2].expect[0].caused_by.effect",
                    "names `done`, which no event expectation declares at or before this point",
                ),
                (
                    "steps[2].expect[0].caused_by.cause",
                    "names `start`, which no event expectation declares at or before this point",
                ),
            ],
        ),
        (
            "cause declared only after the causation",
            with_steps([expecting(
                pause(),
                json!([
                    named("done", "action.done"),
                    { "caused_by": { "effect": "done", "cause": "start" } },
                    named("start", "action.start")
                ]),
            )]),
            vec![(
                "steps[2].expect[1].caused_by.cause",
                "names `start`, which no event expectation declares at or before this point",
            )],
        ),
        (
            "event causing itself",
            with_steps([expecting(
                pause(),
                json!([
                    named("done", "action.done"),
                    { "caused_by": { "effect": "done", "cause": "done" } }
                ]),
            )]),
            vec![(
                "steps[2].expect[1].caused_by",
                "effect and cause are both `done`; an event cannot cause itself",
            )],
        ),
        (
            "name that may match several events",
            with_steps([expecting(
                pause(),
                json!([
                    { "event": { "name": "starts", "kind": "action.start", "count": { "at_least": 2 }, "within_ms": 1000 } },
                    named("done", "action.done"),
                    { "caused_by": { "effect": "done", "cause": "starts" } }
                ]),
            )]),
            vec![(
                "steps[2].expect[2].caused_by.cause",
                "`starts` may match several events; causation needs a name whose count is 1",
            )],
        ),
        (
            "name declared twice across steps",
            with_steps([
                expecting(pause(), json!([named("done", "action.done")])),
                expecting(pause(), json!([named("done", "action.done")])),
            ]),
            vec![(
                "steps[3].expect[0].event.name",
                "`done` is already declared at steps[2].expect[0].event",
            )],
        ),
    ]);
}

#[test]
fn causation_accepts_names_declared_on_earlier_steps() {
    let scenario = with_steps([
        expecting(
            chat("!ping"),
            json!([{ "event": { "name": "chat", "kind": "chat.message", "within_ms": 1000 } }]),
        ),
        expecting(
            pause(),
            json!([
                { "event": { "name": "started", "kind": "action.start", "count": { "exactly": 1 }, "within_ms": 1000 } },
                { "caused_by": { "effect": "started", "cause": "chat" } }
            ]),
        ),
    ]);
    assert_eq!(problems(scenario), Vec::new());
}

#[test]
fn ledger_and_log_expectation_problems_are_located() {
    let mut no_fake = base();
    no_fake["fixture"]["twitch"] = Value::Null;
    no_fake["fakes"] = json!({});
    no_fake["steps"] = json!([expecting(
        json!({ "forge_ready": { "within_ms": 1000 } }),
        json!([
            { "twitch_no_unexpected_requests": {} },
            { "twitch_subscription": { "type": "channel.chat.message", "within_ms": 1000 } },
            { "event": { "source": "twitch", "kind": "chat.message", "within_ms": 1000 } }
        ]),
    )]);
    let fake_needed = "needs a fake Twitch: add fixture.twitch and fakes.twitch";

    assert_cases(vec![
        (
            "twitch expectations without a fake",
            no_fake,
            vec![
                (
                    "steps[0].expect[0].twitch_no_unexpected_requests",
                    fake_needed,
                ),
                ("steps[0].expect[1].twitch_subscription", fake_needed),
                ("steps[0].expect[2].event", fake_needed),
            ],
        ),
        (
            "request count with no bounds, a bad path and a lower-case method",
            with_steps([expecting(
                pause(),
                json!([{ "twitch_request_count": { "method": "post", "path": "helix/users" } }]),
            )]),
            vec![
                (
                    "steps[2].expect[0].twitch_request_count.path",
                    "must start with `/`",
                ),
                (
                    "steps[2].expect[0].twitch_request_count.method",
                    "`post` is not an upper-case HTTP method such as GET",
                ),
                (
                    "steps[2].expect[0].twitch_request_count",
                    "needs min, max, or both",
                ),
            ],
        ),
        (
            "request count with inverted bounds",
            with_steps([expecting(
                pause(),
                json!([{ "twitch_request_count": { "path": "/helix/users", "min": 3, "max": 2 } }]),
            )]),
            vec![(
                "steps[2].expect[0].twitch_request_count",
                "min 3 exceeds max 2",
            )],
        ),
        (
            "subscription expectation with blank type and version",
            with_steps([expecting(
                pause(),
                json!([{ "twitch_subscription": { "type": "", "version": " ", "within_ms": 1 } }]),
            )]),
            vec![
                (
                    "steps[2].expect[0].twitch_subscription.type",
                    "must not be blank",
                ),
                (
                    "steps[2].expect[0].twitch_subscription.version",
                    "must not be blank",
                ),
            ],
        ),
        (
            "log line with no fields",
            with_steps([expecting(
                pause(),
                json!([{ "log_line": { "target": "", "fields": {}, "within_ms": 1 } }]),
            )]),
            vec![
                ("steps[2].expect[0].log_line.target", "must not be blank"),
                (
                    "steps[2].expect[0].log_line.fields",
                    "needs at least one structured field; log prose is never matched",
                ),
            ],
        ),
        (
            "log line keyed on prose or a blank field",
            with_steps([expecting(
                pause(),
                json!([{ "log_line": {
                    "target": "forge::trigger",
                    "fields": { "message": "declined", "": "x" },
                    "within_ms": 1
                } }]),
            )]),
            vec![
                (
                    "steps[2].expect[0].log_line.fields",
                    "has a blank field name",
                ),
                (
                    "steps[2].expect[0].log_line.fields.message",
                    "is the prose line, which may be reworded; match structured fields instead",
                ),
            ],
        ),
    ]);
}

#[test]
fn every_bound_accepts_its_exact_limits() {
    let mut scenario = with_steps([
        step(json!({ "pause": { "ms": 1, "reason": "lower" } })),
        step(json!({ "pause": { "ms": 5000, "reason": "upper" } })),
        expecting(
            crowd(json!({
                "viewers": 1000,
                "chatter": ["hi {login} #{n}"],
                "chatter_per_viewer": 10,
                "spacing_ms": 1000
            })),
            json!([
                { "event": { "kind": "chat.message", "within_ms": 120000 } },
                { "event_absent": { "kind": "trigger.blocked", "window_ms": 1 } },
                { "twitch_request_count": { "path": "/helix/users", "min": 2, "max": 2 } }
            ]),
        ),
        step(crowd(
            json!({ "viewers": 1, "command_senders": 1, "commands": ["!ping"], "spacing_ms": 0 }),
        )),
        step(json!({ "session_reconnect": { "within_ms": 120000 } })),
        step(json!({ "set_global": { "name": "score", "value": { "a": [1, 2.5, true] } } })),
    ]);
    scenario["steps"][0]["do"]["forge_ready"]["within_ms"] = json!(300_000);
    scenario["fakes"]["twitch"]["keepalive_interval_ms"] = json!(14_000);
    assert_eq!(problems(scenario), Vec::new());
}

fn with_overlay(mut scenario: Value) -> Value {
    scenario["fixture"]["overlays"] = json!([
        { "display_name": "Alert Box", "kind_id": "overlay.alert" }
    ]);
    scenario
}

fn opens(overlay: &str) -> Value {
    json!({ "overlay_page": { "overlay": overlay, "within_ms": 30000 } })
}

fn receives(overlay: &str) -> Value {
    json!({ "overlay_content": { "overlay": overlay, "values": { "headline": "hi" }, "within_ms": 5000 } })
}

#[test]
fn overlay_step_and_expectation_problems_are_located() {
    assert_cases(vec![
        (
            "a page opened for an overlay the fixture never declares",
            with_steps([step(opens("Alert Box"))]),
            vec![(
                "steps[2].do.overlay_page.overlay",
                "names overlay `Alert Box`, which the fixture does not declare",
            )],
        ),
        (
            "a page opened for nothing at all",
            with_overlay(with_steps([step(opens("  "))])),
            vec![("steps[2].do.overlay_page.overlay", "must not be blank")],
        ),
        (
            "content expected on a page no step ever opened",
            with_overlay(with_steps([expecting(
                pause(),
                json!([receives("Alert Box")]),
            )])),
            vec![(
                "steps[2].expect[0].overlay_content",
                "expects content on `Alert Box` before any overlay_page step opened it, so nothing can be delivered there",
            )],
        ),
        (
            "content expected with no values to judge it by",
            with_overlay(with_steps([
                step(opens("Alert Box")),
                expecting(
                    pause(),
                    json!([{ "overlay_content": { "overlay": "Alert Box", "values": {}, "within_ms": 5000 } }]),
                ),
            ])),
            vec![(
                "steps[3].expect[0].overlay_content.values",
                "needs at least one content key; an empty frame proves nothing",
            )],
        ),
        (
            "unbounded deadlines on both halves of the vocabulary",
            with_overlay(with_steps([
                step(json!({ "overlay_page": { "overlay": "Alert Box", "within_ms": 120001 } })),
                expecting(
                    pause(),
                    json!([{ "overlay_content": { "overlay": "Alert Box", "values": { "headline": "hi" }, "within_ms": 0 } }]),
                ),
            ])),
            vec![
                (
                    "steps[2].do.overlay_page.within_ms",
                    "must be between 1 and 120000, got 120001",
                ),
                (
                    "steps[3].expect[0].overlay_content.within_ms",
                    "must be between 1 and 120000, got 0",
                ),
            ],
        ),
    ]);
}

#[test]
fn a_page_opened_before_the_content_it_is_judged_on_has_no_problems() {
    let scenario = with_overlay(with_steps([expecting(
        opens("Alert Box"),
        json!([receives("Alert Box")]),
    )]));

    assert_eq!(problems(scenario), Vec::new());
}

#[test]
fn a_repeated_content_key_is_a_syntax_error_rather_than_a_silent_overwrite() {
    let json = r#"{ "overlay": "Alert Box", "values": { "headline": "a", "headline": "b" }, "within_ms": 5000 }"#;

    assert!(
        serde_json::from_str::<forge_emulator::scenario::OverlayContent>(json).is_err(),
        "a duplicate key must not silently keep only the last value"
    );
}

#[test]
fn twitch_event_problems_are_located() {
    let unawaited = "injects `channel.follow` before any twitch_subscribed step waits for it, so the fake may have nowhere to deliver it";
    let not_an_object = "must be a JSON object: EventSub carries every event payload as one";
    assert_cases(vec![
        (
            "a type no twitch_subscribed step waited for",
            with_steps([step(json!({
                "twitch_event": { "subscription_type": "channel.follow", "event": {} }
            }))]),
            vec![("steps[2].do.twitch_event", unawaited)],
        ),
        (
            "a blank type is reported once, not also as unawaited",
            with_steps([step(json!({
                "twitch_event": { "subscription_type": " ", "event": {} }
            }))]),
            vec![(
                "steps[2].do.twitch_event.subscription_type",
                "must not be blank",
            )],
        ),
        (
            "an event that is not an object",
            with_steps([
                step(json!({
                    "twitch_event": { "subscription_type": "channel.chat.message", "event": [] }
                })),
                step(json!({
                    "twitch_event": { "subscription_type": "channel.chat.message", "event": "user" }
                })),
                step(json!({
                    "twitch_event": { "subscription_type": "channel.chat.message", "event": null }
                })),
            ]),
            vec![
                ("steps[2].do.twitch_event.event", not_an_object),
                ("steps[3].do.twitch_event.event", not_an_object),
                ("steps[4].do.twitch_event.event", not_an_object),
            ],
        ),
        (
            "a subscribed type carrying an object",
            with_steps([step(json!({
                "twitch_event": {
                    "subscription_type": "channel.chat.message",
                    "event": { "chatter_user_login": "alice" }
                }
            }))]),
            vec![],
        ),
    ]);
}

#[test]
fn actions_of_both_trigger_kinds_share_one_namespace() {
    let mut event_trigger_only = base();
    event_trigger_only["fixture"]["chat_commands"] = json!([]);
    event_trigger_only["fixture"]["event_triggers"] = json!([
        { "trigger_kind": "twitch.support.subscriber", "action_name": "Announce" }
    ]);
    event_trigger_only["steps"]
        .as_array_mut()
        .unwrap()
        .push(step(json!({ "run_action": { "action": "Announce" } })));
    let mut clashing = base();
    clashing["fixture"]["event_triggers"] =
        json!([{ "trigger_kind": "twitch.support.subscriber", "action_name": "Ping" }]);

    assert_cases(vec![
        (
            "run_action resolves an action an event trigger defines",
            event_trigger_only,
            vec![],
        ),
        (
            "an event trigger reusing a chat command's action name",
            clashing,
            vec![(
                "fixture.event_triggers[0].action_name",
                "`Ping` is already the action of chat_commands[0], so run_action could not tell them apart",
            )],
        ),
    ]);
}
