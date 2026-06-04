use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

use crate::client::HotkeyClient;

pub fn register_hotkey_sub_actions(
    _reg: &mut SubActionRegistry,
    _client: Arc<HotkeyClient>,
) -> Result<(), RegistryError> {
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::client::HotkeyClient;

    #[test]
    fn register_hotkey_sub_actions_registers_no_runners() {
        let mut reg = SubActionRegistry::new();
        let client = HotkeyClient::new_for_test(None);
        register_hotkey_sub_actions(&mut reg, client).unwrap();
        assert_eq!(reg.all().count(), 0);
    }
}
