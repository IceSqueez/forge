use std::sync::{Arc, Mutex, PoisonError};

use forge_audio::{AudioSink, RemoteAudioDestination, RemoteDestinationId, RemoteSink};
use forge_runtime::OverlayServiceHandle;
use forge_server::ServerHandle;
use forge_soundboard::SoundboardPlayer;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{DataProvider, SettingsRepo};

use crate::audio_routes::{
    AudioDomain, RoutePlan, compose_routes, load_audio_routes, plan_route, report_plan,
    resolve_destination, show_speech_legs,
};
use crate::remote_audio::OverlayAudioDestination;
use crate::routed_sink::RoutedSink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedRoutes {
    pub speech: RoutePlan,
    pub clips: RoutePlan,
}

pub struct AudioRouterParts {
    pub backend: Arc<dyn DataProvider>,
    pub server: Option<ServerHandle>,
    pub overlays: OverlayServiceHandle,
    pub speech_sink: Arc<RoutedSink>,
    pub speech_output: Arc<dyn AudioSink>,
    pub soundboard_player: Arc<SoundboardPlayer>,
    pub speak: Option<SpeakQueueHandle>,
}

/// Every install swaps what the next speech line, clip or show speech plays through; audio already
/// playing keeps the route it started on.
pub struct AudioRouter {
    parts: AudioRouterParts,
    remote: Option<Arc<dyn RemoteAudioDestination>>,
    latest: Mutex<u64>,
}

impl AudioRouter {
    pub fn new(parts: AudioRouterParts) -> Self {
        let remote = parts.server.clone().map(|handle| {
            Arc::new(OverlayAudioDestination::new(handle, parts.overlays.clone()))
                as Arc<dyn RemoteAudioDestination>
        });
        Self {
            parts,
            remote,
            latest: Mutex::new(0),
        }
    }

    pub fn speech_sink(&self) -> Arc<dyn AudioSink> {
        Arc::clone(&self.parts.speech_sink) as Arc<dyn AudioSink>
    }

    /// Reads the stored routing and installs it. `None` when a later call started before this
    /// one finished reading, so only the newest stored routing is ever installed.
    pub async fn apply(&self) -> Option<AppliedRoutes> {
        let ticket = {
            let mut latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
            *latest += 1;
            *latest
        };

        let settings: &dyn SettingsRepo = self.parts.backend.as_ref();
        let routes = load_audio_routes(settings).await;
        let destination = resolve_destination(
            self.parts.backend.overlay_repo().as_ref(),
            self.parts.server.is_some(),
            routes.destination.as_ref(),
        )
        .await;

        let latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        if *latest != ticket {
            return None;
        }

        let speech_plan = plan_route(routes.speech, &destination);
        let clips_plan = plan_route(routes.clips, &destination);
        report_plan(AudioDomain::Speech, &speech_plan);
        report_plan(AudioDomain::Clips, &clips_plan);

        let install = compose_routes(
            &speech_plan,
            &clips_plan,
            &destination,
            Arc::clone(&self.parts.speech_output),
            |id| {
                self.remote.clone().map(|destination| {
                    Arc::new(RemoteSink::new(
                        destination,
                        RemoteDestinationId::new(id.as_str()),
                    )) as Arc<dyn AudioSink>
                })
            },
        );

        self.parts.speech_sink.install(install.speech_sink);
        self.parts
            .soundboard_player
            .install_route(install.clip_route);
        if let (Some(remote), Some(speak)) = (&self.remote, &self.parts.speak) {
            speak.install_targeted_legs(show_speech_legs(
                &speech_plan,
                Arc::clone(remote),
                Arc::clone(&self.parts.speech_output),
            ));
        }

        drop(latest);

        Some(AppliedRoutes {
            speech: speech_plan,
            clips: clips_plan,
        })
    }
}
