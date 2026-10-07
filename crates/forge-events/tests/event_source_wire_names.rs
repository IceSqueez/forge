#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_events::EventSource;

#[test]
fn every_source_round_trips_with_flat_lowercase_wire_names() {
    let wire = [
        (EventSource::Twitch, "twitch"),
        (EventSource::YouTube, "youtube"),
        (EventSource::Kick, "kick"),
        (EventSource::Core, "core"),
        (EventSource::Rhai, "rhai"),
        (EventSource::Http, "http"),
        (EventSource::Obs, "obs"),
        (EventSource::VTube, "vtube"),
        (EventSource::Discord, "discord"),
        (EventSource::Midi, "midi"),
        (EventSource::Hotkey, "hotkey"),
        (EventSource::Timer, "timer"),
        (EventSource::Server, "server"),
        (EventSource::Audio, "audio"),
        (EventSource::Donation, "donation"),
    ];
    for (source, name) in wire {
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, format!("\"{name}\""));
        assert_eq!(serde_json::from_str::<EventSource>(&json).unwrap(), source);
    }
}

#[test]
fn legacy_snake_case_spellings_are_rejected() {
    for legacy in ["\"you_tube\"", "\"v_tube\""] {
        assert!(serde_json::from_str::<EventSource>(legacy).is_err());
    }
}
