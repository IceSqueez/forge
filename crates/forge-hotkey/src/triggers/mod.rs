mod hotkey_pressed;

pub use hotkey_pressed::HotkeyPressedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_hotkey_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(HotkeyPressedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_triggered_id_present() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        assert!(reg.get("hotkey.triggered").is_some());
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_hotkey_triggers(&mut reg).unwrap();
        assert!(register_hotkey_triggers(&mut reg).is_err());
    }
}
