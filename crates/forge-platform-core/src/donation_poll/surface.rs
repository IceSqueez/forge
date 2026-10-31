use std::time::Duration;

use async_trait::async_trait;
use forge_types::{IntegrationId, UserText, VerbatimText};

use super::control::DonationPoll;
use crate::builtin::{
    BuiltinControl, BuiltinHealth, BuiltinStatus, CapabilityFlags, ControlFailure, ControlOutcome,
    HeaderAction, HealthMetric, HealthStream,
};
use crate::chat::ConnectionState;

pub trait PolledDonationProvider: Send + Sync {
    type Subject: Clone + PartialEq;
    type Failure: Copy + PartialEq;

    fn poll(&self) -> &DonationPoll<Self::Subject, Self::Failure>;
    fn brand_name(&self) -> &'static str;
    fn api_base_url(&self) -> &str;
}

#[async_trait]
impl<T: PolledDonationProvider> BuiltinControl for T {
    async fn reconnect(&self) -> ControlOutcome {
        self.poll().restart();
        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        self.poll().pause();
        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}

impl<T: PolledDonationProvider> BuiltinStatus for T {
    fn id(&self) -> &IntegrationId {
        self.poll().id()
    }

    fn display_name(&self) -> &str {
        self.brand_name()
    }

    fn version(&self) -> Option<String> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.poll().status().phase.connection()
    }

    fn uptime(&self) -> Option<Duration> {
        self.poll().status().uptime()
    }

    fn endpoint(&self) -> Option<UserText> {
        Some(VerbatimText::new(self.api_base_url()).into())
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        if self.poll().is_enabled() {
            vec![HeaderAction::Disconnect, HeaderAction::Settings]
        } else {
            vec![HeaderAction::Reconnect, HeaderAction::Settings]
        }
    }
}

impl<T: PolledDonationProvider> BuiltinHealth for T {
    fn metrics(&self) -> [HealthMetric; 4] {
        self.poll().board().current_metrics()
    }

    fn stream(&self) -> HealthStream {
        self.poll().board().health_stream()
    }
}
