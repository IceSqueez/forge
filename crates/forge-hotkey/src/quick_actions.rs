use forge_platform_core::{QuickAction, QuickActions};

use crate::client::HotkeyClient;

impl QuickActions for HotkeyClient {
    fn actions(&self) -> Vec<QuickAction> {
        vec![]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::QuickActions;

    use crate::client::HotkeyClient;

    #[test]
    fn actions_returns_empty_vec() {
        let c = HotkeyClient::new_for_test(None);
        let qa: &dyn QuickActions = &*c;
        assert!(qa.actions().is_empty());
    }
}
