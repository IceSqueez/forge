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
    fn hotkey_pressed_id_present() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        assert!(reg.get("hotkey.global.pressed").is_some());
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        assert!(register_hotkey_triggers(&mut reg).is_err());
    }
}
