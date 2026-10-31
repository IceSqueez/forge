use super::{ActiveConnect, DonationSettingsLink};
use crate::collection_manager::CollectionManager;
use crate::in_flight_steps::InFlightSteps;
use crate::integration_quick_action_modal::QuickActionModal;
use crate::obs_settings_modal::ObsSettingsModal;
use crate::run_history_modal::RunHistoryModal;
use crate::twitch_bot_section::TwitchBotSection;
use forge_components::Confirm;
use forge_events::EventPublisher;
use forge_host::integration_sign_out::IntegrationSignOut;
use forge_host::integrations::BuiltinObject;
use forge_host::integrations::ObsInstallSeed;
use forge_platform_core::{
    BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinOptionSource,
    BuiltinStatus, CapabilityFlags, ConnectionState, CredentialState, DetailSection, HeaderAction,
    HealthMetric, QuickActions,
};
use forge_registry::TriggerRegistry;
use forge_runtime::{ActionEngineHandle, IntegrationSlot};
use forge_storage::{CredentialsRepo, HistoryRepo, SettingsRepo};
use gpui::{Entity, Subscription, Task};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

pub(crate) struct DetailServices {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) action_engine: ActionEngineHandle,
    pub(crate) credentials: Arc<dyn CredentialsRepo>,
    pub(crate) settings: Arc<dyn SettingsRepo>,
    pub(crate) history: Arc<dyn HistoryRepo>,
    pub(crate) trigger_registry: Arc<TriggerRegistry>,
    pub(crate) bus: Arc<dyn EventPublisher>,
    pub(crate) twitch_slot: Option<IntegrationSlot<BuiltinObject>>,
    pub(crate) kick_slot: Option<IntegrationSlot<BuiltinObject>>,
    pub(crate) youtube_slot: Option<IntegrationSlot<BuiltinObject>>,
    pub(crate) obs_install_seed: ObsInstallSeed,
    pub(crate) sign_out: IntegrationSignOut,
}

pub(super) struct BuiltinPorts {
    pub(super) status: Arc<dyn BuiltinStatus>,
    pub(super) health: Arc<dyn BuiltinHealth>,
    pub(super) content: Arc<dyn BuiltinContent>,
    pub(super) quick: Arc<dyn QuickActions>,
    pub(super) control: Option<Arc<dyn BuiltinControl>>,
    pub(super) collections: Option<Arc<dyn BuiltinCollections>>,
    pub(super) option_source: Option<Arc<dyn BuiltinOptionSource>>,
}

impl BuiltinPorts {
    pub(super) fn from_object(object: BuiltinObject) -> Self {
        let BuiltinObject {
            status,
            health,
            content,
            quick,
            control,
            collections,
            option_source,
            follow: _,
            ban_list: _,
        } = object;
        Self {
            status,
            health,
            content,
            quick,
            control,
            collections,
            option_source,
        }
    }
}

#[derive(Default)]
pub(super) struct SignInState {
    pub(super) connect: Option<ActiveConnect>,
    pub(super) donation_settings: Option<DonationSettingsLink>,
    pub(super) twitch_bot: Option<Entity<TwitchBotSection>>,
    pub(super) twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    pub(super) youtube_sign_in: Option<forge_platform_youtube::YoutubeSignIn>,
    pub(super) kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    pub(super) _twitch_bot_subs: Vec<Subscription>,
}

pub(super) struct ShownState {
    pub(super) credential: CredentialState,
    pub(super) display_name: String,
    pub(super) version: Option<String>,
    pub(super) connection: ConnectionState,
    pub(super) capability_flags: CapabilityFlags,
    pub(super) header_actions: Vec<HeaderAction>,
    pub(super) health_metrics: [HealthMetric; 4],
    pub(super) sections: Vec<DetailSection>,
}

impl ShownState {
    pub(super) fn read(ports: &BuiltinPorts) -> Self {
        Self {
            credential: ports.status.credential_state(),
            display_name: ports.status.display_name().to_owned(),
            version: ports.status.version(),
            connection: ports.status.connection(),
            capability_flags: ports.status.capability_flags(),
            header_actions: ports.status.header_actions(),
            health_metrics: ports.health.metrics(),
            sections: ports.content.sections(),
        }
    }
}

#[derive(Default)]
pub(super) struct LiveTallies {
    pub(super) row_steps_in_flight: InFlightSteps,
    pub(super) viewer_samples: VecDeque<(Instant, u64)>,
    pub(super) signing_out: bool,
}

#[derive(Default)]
pub(super) struct DetailModals {
    pub(super) pending_disconnect: Confirm<()>,
    pub(super) quick_action_modal: Option<Entity<QuickActionModal>>,
    pub(super) collection_manager: Option<Entity<CollectionManager>>,
    pub(super) obs_settings_modal: Option<Entity<ObsSettingsModal>>,
    pub(super) history_modal: Option<Entity<RunHistoryModal>>,
    pub(super) _qa_modal_sub: Option<Subscription>,
    pub(super) _collection_manager_sub: Option<Subscription>,
    pub(super) _obs_modal_sub: Option<Subscription>,
    pub(super) _history_modal_subs: Vec<Subscription>,
}

pub(super) struct DetailBackground {
    pub(super) _qa_search_sub: Subscription,
    pub(super) _health_bridge: Task<()>,
    pub(super) _change_bridge: Task<()>,
    pub(super) _token_countdown_repaint: Option<Task<()>>,
}
