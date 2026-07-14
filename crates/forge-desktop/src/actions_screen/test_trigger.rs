//! "Test run" plumbing for the actions editor: synthesizes a test event for the
//! selected action's first linked trigger and injects it through the re-entrant
//! bus path (store-then-replay, a single evaluation pass), so the button drives
//! the exact trigger → action pipeline a live event would — never a direct engine
//! call.

use forge_events::{Event, EventSource};
use forge_registry::{TriggerRegistry, effective_config};
use forge_runtime::EventBus;
use forge_runtime::actions::ActionsService;
use forge_types::{ActionId, TriggerConfig, TriggerInstance, Variant};
use serde_json::json;

/// Loads the action's detail, synthesizes a test event for its first linked
/// trigger (or a `test.trigger` fallback when none is linked), records it, then
/// replays it through the bus for a single evaluation pass. Returns whether the
/// synthesized event satisfied the trigger's own source / kind-prefix / predicate
/// checks, so the caller can pick a matched vs. non-matched toast.
pub(super) async fn run_test_trigger(
    service: &ActionsService,
    registry: &TriggerRegistry,
    bus: &EventBus,
    id: ActionId,
) -> Result<bool, String> {
    let detail = service.load_detail(id).await.map_err(|e| e.to_string())?;
    let resolved = detail
        .trigger_instances
        .first()
        .and_then(|inst| registry.get(&inst.kind_id).map(|desc| (inst, desc)));
    let (event, matched) = match resolved {
        Some((instance, descriptor)) => {
            let config = effective_config(&descriptor.default_config(), &instance.overrides);
            let event = synthesize_test_event(instance, &config);
            let filter = descriptor.event_filter();
            let matched = filter.source.is_none_or(|s| s == event.source)
                && filter
                    .kind_prefix
                    .as_deref()
                    .is_none_or(|p| event.kind.starts_with(p))
                && descriptor.matches_trigger(&config, &event);
            (event, matched)
        }
        None => (
            Event::new(
                EventSource::Core,
                "test.trigger",
                json!({ "action_id": id.to_string() }),
            ),
            false,
        ),
    };
    let event_id = event.id;
    // Store-only: the synthesized event is retained for replay/observability but
    // never broadcast, so replay_and_publish is the single evaluation pass.
    bus.record(event);
    bus.replay_and_publish(event_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(matched)
}

/// Builds a synthetic event that the trigger evaluator judges with the same
/// `matches_trigger` predicate a live event faces. `config` is the instance's
/// effective config (descriptor defaults merged with overrides), so the synthetic
/// chat message carries the trigger's configured phrase instead of a fixed literal.
pub(super) fn synthesize_test_event(instance: &TriggerInstance, config: &TriggerConfig) -> Event {
    match instance.kind_id.as_str() {
        "twitch.chat.command" => {
            let phrase = match config.get("phrase") {
                Some(Variant::String(s)) if !s.is_empty() => s.as_str(),
                _ => "!command",
            };
            Event::new(
                EventSource::Twitch,
                "chat.message",
                json!({
                    "message": format!("{phrase} test"),
                    "user_login": "test_user",
                    "channel": "test_channel"
                }),
            )
        }
        "twitch.chat.message" => Event::new(
            EventSource::Twitch,
            "chat.message",
            json!({
                "message": "test message",
                "user_login": "test_user",
                "channel": "test_channel"
            }),
        ),
        "twitch.support.subscriber" => Event::new(
            EventSource::Twitch,
            "sub.received",
            json!({
                "user_login": "test_user",
                "tier": "1000"
            }),
        ),
        "twitch.support.resubscriber" => Event::new(
            EventSource::Twitch,
            "resub.received",
            json!({
                "user_login": "test_user",
                "tier": "1000",
                "months": 3
            }),
        ),
        "twitch.support.gift_sub" => Event::new(
            EventSource::Twitch,
            "giftsub.received",
            json!({
                "gifter_login": "test_gifter",
                "recipient_login": "test_recipient",
                "tier": "1000"
            }),
        ),
        "twitch.support.cheer" => Event::new(
            EventSource::Twitch,
            "cheer.received",
            json!({
                "user_login": "test_user",
                "bits": 100
            }),
        ),
        "twitch.channel.raid_received" => Event::new(
            EventSource::Twitch,
            "raid.received",
            json!({
                "from_broadcaster_login": "test_raider",
                "viewers": 10
            }),
        ),
        "obs.scenes.current_changed" => {
            let scene_name = match config.get("scene") {
                Some(Variant::String(s)) => s.clone(),
                _ => "TestScene".to_owned(),
            };
            Event::new(
                EventSource::Obs,
                "scene.changed",
                json!({ "scene": scene_name }),
            )
        }
        "script.event.custom" => {
            let event_name = match config.get("name") {
                Some(Variant::String(s)) if !s.is_empty() => s.as_str(),
                _ => "test",
            };
            Event::new(
                EventSource::Server,
                format!("custom.{event_name}"),
                json!({}),
            )
        }
        _ => Event::new(EventSource::Core, "test.trigger", json!({})),
    }
}
