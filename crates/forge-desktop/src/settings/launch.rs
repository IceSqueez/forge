use std::sync::Arc;

use forge_audio::DeviceSink;
use forge_host::audio_router::AudioRouter;
use forge_host::boot_warnings::BootWarnings;
use forge_host::integrations::BuiltinRegistry;
use forge_host::voice_gate::VoiceGateOwner;
use forge_hotkey::HotkeyClient;
use forge_runtime::{
    ChatHistoryRetentionHandle, DateNamesHandle, NotificationPolicyHandle, OverlaySampleSource,
    OverlayServiceHandle,
};
use forge_server::ServerHandle;
use forge_storage::DataProvider;
use forge_transfer::{ScheduledBackupHandle, TransferExclusivity};
use forge_types::Shared;

use crate::log_tail::LogTail;
use crate::runtime_handles::RuntimeHandles;
use crate::stay_awake::StayAwakeSetting;

pub(super) struct SettingsServices {
    pub(super) backend: Arc<dyn DataProvider>,
    pub(super) rt_handle: tokio::runtime::Handle,
    pub(super) stay_awake: StayAwakeSetting,
    pub(super) anonymous_donor: Shared<String>,
    pub(super) date_names: DateNamesHandle,
}

pub(crate) struct SettingsAudioLaunch {
    pub(crate) backend: Arc<dyn DataProvider>,
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) speech_output: Arc<DeviceSink>,
    pub(crate) overlays: OverlayServiceHandle,
    pub(crate) audio_router: Arc<AudioRouter>,
    pub(crate) server_available: bool,
    pub(crate) voice_gate: Arc<VoiceGateOwner>,
}

pub(crate) struct SettingsLaunch {
    pub(super) services: SettingsServices,
    pub(super) audio: SettingsAudioLaunch,
    pub(super) log_tail: LogTail,
    pub(super) server: Option<ServerHandle>,
    pub(super) overlays: OverlayServiceHandle,
    pub(super) overlay_samples: Arc<dyn OverlaySampleSource>,
    pub(super) hotkey_client: Option<Arc<HotkeyClient>>,
    pub(super) notification_policy: NotificationPolicyHandle,
    pub(super) scheduled_backups: ScheduledBackupHandle,
    pub(super) transfer_exclusivity: TransferExclusivity,
    pub(super) chat_history_retention: ChatHistoryRetentionHandle,
    pub(super) builtins: BuiltinRegistry,
    pub(super) boot_warnings: BootWarnings,
}

impl SettingsLaunch {
    pub(crate) fn of(handles: &RuntimeHandles) -> Self {
        Self {
            services: SettingsServices {
                backend: Arc::clone(&handles.backend),
                rt_handle: handles.rt_handle.clone(),
                stay_awake: handles.stay_awake.clone(),
                anonymous_donor: handles.anonymous_donor.clone(),
                date_names: handles.date_names.clone(),
            },
            audio: SettingsAudioLaunch {
                backend: Arc::clone(&handles.backend),
                rt_handle: handles.rt_handle.clone(),
                speech_output: Arc::clone(&handles.speech.speech_output),
                overlays: handles.overlays.service.clone(),
                audio_router: Arc::clone(&handles.audio.audio_router),
                server_available: handles.server.is_some(),
                voice_gate: Arc::clone(&handles.speech.voice_gate),
            },
            log_tail: handles.log_tail.clone(),
            server: handles.server.clone(),
            overlays: handles.overlays.service.clone(),
            overlay_samples: Arc::clone(&handles.overlays.samples),
            hotkey_client: handles.integrations.hotkey_client.clone(),
            notification_policy: handles.core.notification_policy.clone(),
            scheduled_backups: handles.core.scheduled_backups.clone(),
            transfer_exclusivity: handles.core.transfer_exclusivity.clone(),
            chat_history_retention: handles.core.chat_history_retention.clone(),
            builtins: handles.integrations.builtins.clone(),
            boot_warnings: handles.boot_warnings.clone(),
        }
    }
}
