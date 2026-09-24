mod hotkey_pressed;
mod hotkey_released;

pub use hotkey_pressed::HotkeyPressedDescriptor;
pub use hotkey_released::HotkeyReleasedDescriptor;

use forge_events::Event;
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::{TriggerConfig, Variant};

use crate::combo::HotkeyCombo;
use crate::payload_fields as fields;

pub fn register_hotkey_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(HotkeyPressedDescriptor))?;
    reg.register(Box::new(HotkeyReleasedDescriptor))?;
    Ok(())
}

/// Stored combos may be hand-typed in any case or modifier order; events always carry the
/// canonical form, so the configured side is canonicalised before comparing.
fn config_accepts_combo(config: &TriggerConfig, event: &Event) -> bool {
    let Some(Variant::String(configured)) = config.get(fields::COMBO) else {
        return true;
    };
    let event_combo = event
        .payload
        .get(fields::COMBO)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    HotkeyCombo::parse(configured).is_ok_and(|combo| combo.as_str() == event_combo)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn both_hotkey_edge_trigger_ids_are_present() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        for id in ["hotkey.global.pressed", "hotkey.global.released"] {
            assert!(reg.get(id).is_some(), "missing trigger: {id}");
        }
    }

    fn pressed(combo: &str) -> Event {
        Event::new(
            forge_events::EventSource::Hotkey,
            "hotkey.global.pressed",
            serde_json::json!({ "combo": combo }),
        )
    }

    fn configured(combo: &str) -> TriggerConfig {
        TriggerConfig::from([(fields::COMBO.to_owned(), Variant::String(combo.to_owned()))])
    }

    #[test]
    fn a_hand_typed_combo_matches_the_canonical_combo_the_event_carries() {
        for (config, event_combo, expected) in [
            ("F5", "F5", true),
            ("f5", "F5", true),
            ("shift+ctrl+1", "Ctrl+Shift+1", true),
            (" Ctrl + F5 ", "Ctrl+F5", true),
            ("control+option+a", "Ctrl+Alt+A", true),
            ("f5", "F6", false),
            ("Ctrl+F5", "F5", false),
            ("Ctrl+XYZ", "Ctrl+XYZ", false),
            ("", "", false),
        ] {
            assert_eq!(
                config_accepts_combo(&configured(config), &pressed(event_combo)),
                expected,
                "config {config:?} vs event {event_combo:?}"
            );
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        assert!(register_hotkey_triggers(&mut reg).is_err());
    }
}
