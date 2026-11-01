use std::sync::Arc;

use forge_audio::AudioSinkFactory;
use forge_events::EventPublisher;
use forge_hotkey::{HotkeyReconciler, HotkeySyncedClipsRepo};
use forge_registry::SubActionRegistry;
use forge_runtime::EventBus;
use forge_soundboard::{
    BusAudioEventSink, ClipLibrary, SoundboardControl, SoundboardPlayer, SoundboardSettingsHandle,
    load_soundboard_settings, register_soundboard_sub_actions,
};
use forge_storage::{DataProvider, SettingsRepo};

use crate::boot_warnings::{BootWarning, BootWarnings};

pub(crate) struct SoundboardInputs<'a> {
    pub(crate) backend: &'a Arc<dyn DataProvider>,
    pub(crate) bus: &'a Arc<EventBus>,
    pub(crate) audio_sinks: Arc<dyn AudioSinkFactory>,
    pub(crate) hotkey_reconciler: Arc<HotkeyReconciler>,
}

pub(crate) async fn build_soundboard(
    sub_actions: &mut SubActionRegistry,
    inputs: SoundboardInputs<'_>,
    warnings: &mut BootWarnings,
) -> Arc<SoundboardPlayer> {
    let SoundboardInputs {
        backend,
        bus,
        audio_sinks,
        hotkey_reconciler,
    } = inputs;
    let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let settings =
        SoundboardSettingsHandle::new(load_soundboard_settings(settings_repo.as_ref()).await);
    let library = Arc::new(ClipLibrary::new(
        HotkeySyncedClipsRepo::wrap(backend.soundboard_clips_repo(), hotkey_reconciler),
        backend.media_repo(),
        Arc::clone(bus) as Arc<dyn EventPublisher>,
    ));
    let player = Arc::new(SoundboardPlayer::with_settings(
        audio_sinks,
        Arc::new(BusAudioEventSink::new(
            Arc::clone(bus) as Arc<dyn EventPublisher>
        )),
        library,
        settings,
        settings_repo,
    ));
    if let Err(e) = register_soundboard_sub_actions(
        sub_actions,
        Arc::clone(&player) as Arc<dyn SoundboardControl>,
    ) {
        tracing::error!(error = %e, "soundboard sub-action runner registration failed");
        warnings.record(BootWarning::SoundboardSubActions);
    }
    player
}
