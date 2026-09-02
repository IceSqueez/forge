mod hotkey_pressed;
mod hotkey_released;

pub use hotkey_pressed::HotkeyPressedDescriptor;
pub use hotkey_released::HotkeyReleasedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_hotkey_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(HotkeyPressedDescriptor))?;
    reg.register(Box::new(HotkeyReleasedDescriptor))?;
    Ok(())
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

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        assert!(register_hotkey_triggers(&mut reg).is_err());
    }
}
