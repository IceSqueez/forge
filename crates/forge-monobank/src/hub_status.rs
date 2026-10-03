use std::time::Duration;

use forge_platform_core::{BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction};
use forge_types::IntegrationId;

use crate::integration::MONOBANK_INTEGRATION;
use crate::provider::MonobankProvider;

impl BuiltinStatus for MonobankProvider {
    fn id(&self) -> &IntegrationId {
        &self.id
    }

    fn display_name(&self) -> &str {
        MONOBANK_INTEGRATION.brand_name
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.poll_status().phase.connection()
    }

    fn uptime(&self) -> Option<Duration> {
        self.poll_status().uptime()
    }

    fn endpoint(&self) -> Option<&str> {
        Some(&self.config.base_url)
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        if self.is_polling_enabled() {
            vec![HeaderAction::Disconnect, HeaderAction::Settings]
        } else {
            vec![HeaderAction::Reconnect, HeaderAction::Settings]
        }
    }
}
