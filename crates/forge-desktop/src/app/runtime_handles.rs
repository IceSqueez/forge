use std::sync::Arc;

use forge_host::host_handles::{
    AudioHandles, AutomationHandles, CoreHandles, IntegrationHandles, OverlayHandles, SpeechHandles,
};
use forge_host::stop::OrderedStop;
use forge_runtime::DateNamesHandle;
use forge_storage::{CredentialsKeyLoss, DataProvider, Language, SettingsCatalog};

use crate::app::log_tail::LogTail;

pub(crate) struct RuntimeHandles {
    pub rt_handle: tokio::runtime::Handle,
    pub log_tail: LogTail,
    pub backend: Arc<dyn DataProvider>,
    pub settings_catalog: Arc<SettingsCatalog>,
    pub startup_language: Language,
    pub startup_display_formats: forge_components::DisplayFormatChoices,
    pub anonymous_donor: forge_types::Shared<String>,
    pub date_names: DateNamesHandle,
    pub credentials_key_loss: Option<CredentialsKeyLoss>,
    pub overlay_build_failure: Option<crate::tools::overlay_build_failure::OverlayBuildFailure>,
    pub boot_warnings: forge_host::boot_warnings::BootWarnings,
    pub server_unavailable: Option<crate::shared::repo_failure::RepoFailure>,
    pub stay_awake: crate::app::stay_awake::StayAwakeSetting,
    pub first_run: crate::app::first_run::FirstRun,
    pub core: CoreHandles,
    pub automation: AutomationHandles,
    pub integrations: IntegrationHandles,
    pub speech: SpeechHandles,
    pub audio: AudioHandles,
    pub overlays: OverlayHandles,
    pub server: Option<forge_server::ServerHandle>,
    pub stop: OrderedStop,
}
