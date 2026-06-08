use forge_platform_core::{QuickAction, QuickActions};

use crate::client::HotkeyClient;

impl QuickActions for HotkeyClient {
    fn actions(&self) -> Vec<QuickAction> {
        vec![]
    }
}
