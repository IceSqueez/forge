#![expect(clippy::expect_used, reason = "test fixtures assert directly")]

use forge_registry::TriggerRegistry;

pub fn every_registered_trigger() -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    forge_runtime::register_core_triggers(&mut registry).expect("core triggers");
    forge_platform_twitch::register_twitch_triggers(&mut registry).expect("twitch triggers");
    forge_platform_youtube::register_youtube_triggers(&mut registry).expect("youtube triggers");
    forge_platform_kick::register_kick_triggers(&mut registry).expect("kick triggers");
    forge_obs::register_obs_triggers(&mut registry).expect("obs triggers");
    forge_vtube::register_vtube_triggers(&mut registry).expect("vtube triggers");
    forge_midi::register_midi_triggers(&mut registry).expect("midi triggers");
    forge_hotkey::register_hotkey_triggers(&mut registry).expect("hotkey triggers");
    registry
}
