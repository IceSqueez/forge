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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use forge_audio::{AudioRoute, NullAudioEventSink};
    use forge_overlay::kinds::alert::KIND_ID as ALERT_KIND;
    use forge_overlay::kinds::blank::KIND_ID as BLANK_KIND;
    use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
    use forge_runtime::{EventBus, OverlayFrameSink, OverlayReceivers};
    use forge_soundboard::{ClipLibrary, CpalSinkFactory, SoundboardSettingsHandle};
    use forge_storage::soundboard::MockSoundboardClipsRepo;
    use forge_storage::{
        MockMediaRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
        StorageError,
    };
    use tokio::sync::Notify;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

    use super::*;
    use crate::audio_routes::{AudioDomain, RouteFallback, set_destination, set_route};
    use crate::test_support::{
        RecordingSink, StubEventLog, TestBackend, overlay_named, stopped_server_handle,
        test_backend_with_overlays, tone,
    };

    const BLANK_RECEIVER: &str = "blank-receiver";
    const ALERT_RECEIVER: &str = "alert-receiver";

    /// Why: a delivery that never reaches a page must fail the run instead of stalling it.
    const HANG_GUARD: Duration = Duration::from_secs(5);

    struct Gate {
        id: OverlayId,
        entered: Arc<Notify>,
        release: Arc<Notify>,
    }

    struct StoredOverlays {
        stored: Mutex<Vec<OverlayDefinition>>,
        gate: Mutex<Option<Gate>>,
    }

    impl StoredOverlays {
        fn remove(&self, id: &str) {
            self.stored.lock().unwrap().retain(|d| d.id.as_str() != id);
        }

        fn hold_next_lookup_of(&self, id: &str) -> (Arc<Notify>, Arc<Notify>) {
            let entered = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            *self.gate.lock().unwrap() = Some(Gate {
                id: OverlayId::new(id),
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
            });
            (entered, release)
        }
    }

    #[async_trait]
    impl OverlayRepo for StoredOverlays {
        async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError> {
            Ok(self.stored.lock().unwrap().clone())
        }

        async fn get(&self, id: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError> {
            let gate = {
                let mut slot = self.gate.lock().unwrap();
                match slot.as_ref() {
                    Some(gate) if &gate.id == id => slot.take(),
                    _ => None,
                }
            };
            if let Some(gate) = gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            Ok(self
                .stored
                .lock()
                .unwrap()
                .iter()
                .find(|d| &d.id == id)
                .cloned())
        }

        async fn get_by_credential(
            &self,
            _: &OverlayCredential,
        ) -> Result<Option<OverlayDefinition>, StorageError> {
            Ok(None)
        }

        async fn create(
            &self,
            _: &str,
            _: &str,
            _: u32,
        ) -> Result<OverlayDefinition, StorageError> {
            unreachable!("routing never creates an overlay")
        }

        async fn save(&self, _: &OverlayDefinition) -> Result<(), StorageError> {
            unreachable!("routing never saves an overlay")
        }

        async fn set_enabled(&self, _: &OverlayId, _: bool) -> Result<bool, StorageError> {
            unreachable!("routing never toggles an overlay")
        }

        async fn delete(&self, _: &OverlayId) -> Result<bool, StorageError> {
            unreachable!("routing never deletes an overlay")
        }

        async fn get_retained_content(
            &self,
            _: &OverlayId,
        ) -> Result<Option<OverlayConfig>, StorageError> {
            Ok(None)
        }

        async fn set_retained_content(
            &self,
            _: &OverlayId,
            _: &OverlayConfig,
        ) -> Result<(), StorageError> {
            Ok(())
        }
    }

    struct Pages {
        announced: UnboundedSender<OverlayId>,
    }

    #[async_trait]
    impl OverlayFrameSink for Pages {
        async fn deliver_content(
            &self,
            identity: &OverlayId,
            _: serde_json::Value,
            _: Option<u64>,
        ) -> OverlayReceivers {
            let _ = self.announced.send(identity.clone());
            OverlayReceivers {
                sources: 1,
                preview_tabs: 0,
            }
        }

        async fn deliver_reload(&self, _: &OverlayId) {}

        async fn revoke(&self, _: &OverlayId) {}
    }

    struct Rig {
        backend: Arc<TestBackend>,
        overlays: Arc<StoredOverlays>,
        local: Arc<RecordingSink>,
        announced: UnboundedReceiver<OverlayId>,
        router: Arc<AudioRouter>,
    }

    impl Rig {
        async fn new() -> Self {
            let overlays = Arc::new(StoredOverlays {
                stored: Mutex::new(vec![
                    overlay_named(BLANK_RECEIVER, BLANK_KIND),
                    overlay_named(ALERT_RECEIVER, ALERT_KIND),
                ]),
                gate: Mutex::new(None),
            });
            let (backend, _writes) =
                test_backend_with_overlays(Arc::clone(&overlays) as Arc<dyn OverlayRepo>);
            let server = stopped_server_handle(&backend).await;

            let mut kinds = OverlayKindRegistry::new();
            register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");
            let (tx, announced) = unbounded_channel();
            let service = OverlayServiceHandle::new(
                Arc::clone(&overlays) as Arc<dyn OverlayRepo>,
                Arc::clone(&backend) as Arc<dyn SettingsRepo>,
                Arc::new(kinds),
                EventBus::new(Arc::new(StubEventLog)),
                Some(Arc::new(Pages { announced: tx }) as Arc<dyn OverlayFrameSink>),
            );

            let local = RecordingSink::new();
            let player = SoundboardPlayer::with_settings(
                Arc::new(CpalSinkFactory),
                Arc::new(NullAudioEventSink),
                Arc::new(ClipLibrary::new(
                    Arc::new(MockSoundboardClipsRepo::new()),
                    Arc::new(MockMediaRepo::new()),
                )),
                SoundboardSettingsHandle::new(Default::default()),
            );
            let router = Arc::new(AudioRouter::new(AudioRouterParts {
                backend: Arc::clone(&backend) as Arc<dyn DataProvider>,
                server: Some(server),
                overlays: service,
                speech_sink: Arc::new(RoutedSink::new(Arc::clone(&local) as Arc<dyn AudioSink>)),
                speech_output: Arc::clone(&local) as Arc<dyn AudioSink>,
                soundboard_player: Arc::new(player),
                speak: None,
            }));

            let rig = Self {
                backend,
                overlays,
                local,
                announced,
                router,
            };
            rig.route_speech(AudioRoute::Overlay).await;
            rig
        }

        async fn route_speech(&self, route: AudioRoute) {
            set_route(self.backend.as_ref(), AudioDomain::Speech, route)
                .await
                .expect("the speech route is stored");
        }

        async fn choose(&self, id: &str) {
            set_destination(self.backend.as_ref(), Some(&OverlayId::new(id)))
                .await
                .expect("the receiver is stored");
        }

        async fn next_line_reaches(&mut self) -> Option<String> {
            let before = self.local.calls();
            self.router
                .speech_sink()
                .play_stoppable(tone())
                .await
                .expect("the installed speech route accepts a line");
            if self.local.calls() > before {
                return None;
            }
            let announced = tokio::time::timeout(HANG_GUARD, self.announced.recv())
                .await
                .expect("the line reached neither the local device nor a page")
                .expect("the page recorder is alive");
            Some(announced.as_str().to_owned())
        }
    }

    #[tokio::test]
    async fn a_new_receiver_takes_the_next_speech_line_without_a_restart() {
        let mut rig = Rig::new().await;
        rig.choose(BLANK_RECEIVER).await;
        rig.router.apply().await;
        assert_eq!(
            rig.next_line_reaches().await.as_deref(),
            Some(BLANK_RECEIVER)
        );

        rig.choose(ALERT_RECEIVER).await;
        rig.router.apply().await;

        assert_eq!(
            rig.next_line_reaches().await.as_deref(),
            Some(ALERT_RECEIVER),
            "the speech line still went to the receiver chosen before the change"
        );
    }

    #[tokio::test]
    async fn an_apply_overtaken_while_it_reads_installs_nothing_and_the_newer_routing_stands() {
        let mut rig = Rig::new().await;
        rig.choose(BLANK_RECEIVER).await;
        let (entered, release) = rig.overlays.hold_next_lookup_of(BLANK_RECEIVER);
        let older = tokio::spawn({
            let router = Arc::clone(&rig.router);
            async move { router.apply().await }
        });
        entered.notified().await;

        rig.choose(ALERT_RECEIVER).await;
        let newer = rig.router.apply().await;
        release.notify_one();
        let older = older.await.expect("the older apply finishes");

        assert_eq!(
            (older.is_none(), newer.is_some()),
            (true, true),
            "only the newest apply may report an installed routing"
        );
        assert_eq!(
            rig.next_line_reaches().await.as_deref(),
            Some(ALERT_RECEIVER),
            "an apply that read the routing before a newer one overwrote what the newer installed"
        );
    }

    #[tokio::test]
    async fn a_deleted_receiver_sends_the_next_speech_line_to_the_local_device() {
        let mut rig = Rig::new().await;
        rig.choose(ALERT_RECEIVER).await;
        rig.router.apply().await;

        rig.overlays.remove(ALERT_RECEIVER);
        let applied = rig.router.apply().await.expect("a lone apply installs");

        assert_eq!(
            applied.speech.fallback,
            Some(RouteFallback::DestinationNotFound)
        );
        assert_eq!(
            rig.next_line_reaches().await,
            None,
            "speech kept flowing to an overlay that no longer exists"
        );
    }

    #[tokio::test]
    async fn apply_reports_each_domain_with_the_plan_installed_for_it() {
        let rig = Rig::new().await;
        rig.choose(ALERT_RECEIVER).await;
        set_route(rig.backend.as_ref(), AudioDomain::Clips, AudioRoute::Both)
            .await
            .expect("the clip route is stored");

        let applied = rig.router.apply().await.expect("a lone apply installs");

        assert_eq!(
            (applied.speech.route, applied.clips.route),
            (AudioRoute::Overlay, AudioRoute::Both),
            "the reported routing swapped or dropped a domain, so Settings shows the wrong one in effect"
        );
    }
}
