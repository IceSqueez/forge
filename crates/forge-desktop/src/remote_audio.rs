use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use forge_audio::{
    AudioError, RemoteAudioDestination, RemoteClip, RemoteClipId, RemoteCommand, RemoteDelivery,
    RemoteDestinationId, RemoteVerdict,
};
use forge_overlay::{AudioAnnouncement, AudioCommand, announcement_content, command_content};
use forge_runtime::{OverlayDelivery, OverlayServiceHandle};
use forge_server::{
    ClipCapability, ClipMediaType, ClipOffer, ClipOutcome, ClipOutcomeHandle, ServerHandle,
};
use forge_storage::OverlayId;
use ulid::Ulid;

const UNKNOWN_CLIP: &str = "the clip is no longer waiting on a verdict";
const NEVER_FETCHED: &str = "the page never asked for the clip";
const NO_VERDICT: &str = "the page never reported whether it played the clip";
const REVOKED: &str = "the clip was withdrawn before it settled";
const SERVER_STOPPED: &str = "the server stopped before the clip settled";

struct Pending {
    capability: ClipCapability,
    outcome: Option<ClipOutcomeHandle>,
}

pub struct OverlayAudioDestination {
    server: ServerHandle,
    overlays: OverlayServiceHandle,
    pending: Mutex<HashMap<String, Pending>>,
}

impl OverlayAudioDestination {
    pub fn new(server: ServerHandle, overlays: OverlayServiceHandle) -> Self {
        Self {
            server,
            overlays,
            pending: Mutex::new(HashMap::new()),
        }
    }

    fn pending(&self) -> MutexGuard<'_, HashMap<String, Pending>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn register(&self, clip_id: String, capability: ClipCapability, outcome: ClipOutcomeHandle) {
        self.pending().insert(
            clip_id,
            Pending {
                capability,
                outcome: Some(outcome),
            },
        );
    }

    fn capability_of(&self, clip_id: &str) -> Option<ClipCapability> {
        self.pending()
            .get(clip_id)
            .map(|pending| pending.capability.clone())
    }

    fn take_outcome(&self, clip_id: &str) -> Option<ClipOutcomeHandle> {
        self.pending()
            .get_mut(clip_id)
            .and_then(|pending| pending.outcome.take())
    }

    fn forget(&self, clip_id: &str) -> Option<ClipCapability> {
        self.pending()
            .remove(clip_id)
            .map(|pending| pending.capability)
    }
}

#[async_trait]
impl RemoteAudioDestination for OverlayAudioDestination {
    async fn deliver(
        &self,
        destination: &RemoteDestinationId,
        clip: RemoteClip,
    ) -> Result<RemoteDelivery, AudioError> {
        let identity = OverlayId::new(destination.as_str());
        let RemoteClip {
            bytes,
            media_type,
            duration_ms,
        } = clip;
        let expected_players = self.overlays.receivers(&identity).await.sources;

        let (ticket, outcome) = self
            .server
            .offer_audio_clip(
                &identity,
                ClipOffer {
                    bytes,
                    media_type: server_media_type(media_type),
                    duration_ms,
                },
            )
            .await
            .map_err(|e| AudioError::RemoteDestination(e.to_string()))?;
        self.server
            .admit_audio_players(ticket.capability(), expected_players)
            .await;

        let clip_id = Ulid::generate().to_string();
        let content = announcement_content(&AudioAnnouncement {
            clip_id: &clip_id,
            clip_path: ticket.clip_path(),
            report_path: ticket.report_path(),
            media_type: media_type.as_str(),
            duration_ms,
        });
        self.register(clip_id.clone(), ticket.capability().clone(), outcome);

        match self.overlays.deliver_audio(&identity, content).await {
            Ok(delivery) => {
                let live_players = live_players(delivery);
                self.server
                    .admit_audio_players(ticket.capability(), live_players)
                    .await;
                Ok(RemoteDelivery {
                    clip_id: RemoteClipId::new(clip_id),
                    live_players,
                })
            }
            Err(e) => {
                if let Some(capability) = self.forget(&clip_id) {
                    self.server.revoke_audio_clip(&capability).await;
                }
                Err(AudioError::RemoteDestination(e.to_string()))
            }
        }
    }

    async fn control(
        &self,
        destination: &RemoteDestinationId,
        clip_id: &RemoteClipId,
        command: RemoteCommand,
    ) -> Result<(), AudioError> {
        let Some(capability) = self.capability_of(clip_id.expose()) else {
            return Ok(());
        };

        match command {
            RemoteCommand::Pause => {
                self.server.hold_audio_clip(&capability).await;
            }
            RemoteCommand::Resume => {
                self.server.release_audio_clip(&capability).await;
            }
            RemoteCommand::Stop => {}
        }

        let identity = OverlayId::new(destination.as_str());
        let content = command_content(overlay_command(command), Some(clip_id.expose()));
        let pushed = self.overlays.deliver_audio(&identity, content).await;

        if matches!(command, RemoteCommand::Stop) {
            self.forget(clip_id.expose());
            self.server.revoke_audio_clip(&capability).await;
        }

        pushed
            .map(|_| ())
            .map_err(|e| AudioError::RemoteDestination(e.to_string()))
    }

    async fn verdict(&self, clip_id: &RemoteClipId) -> Result<RemoteVerdict, AudioError> {
        let Some(outcome) = self.take_outcome(clip_id.expose()) else {
            return Err(AudioError::RemoteDestination(UNKNOWN_CLIP.to_owned()));
        };
        let settled = outcome.recv().await;
        self.forget(clip_id.expose());
        Ok(verdict_of(settled))
    }
}

fn server_media_type(media_type: forge_audio::ClipMediaType) -> ClipMediaType {
    match media_type {
        forge_audio::ClipMediaType::Wave => ClipMediaType::Wave,
    }
}

fn overlay_command(command: RemoteCommand) -> AudioCommand {
    match command {
        RemoteCommand::Stop => AudioCommand::Stop,
        RemoteCommand::Pause => AudioCommand::Pause,
        RemoteCommand::Resume => AudioCommand::Resume,
    }
}

fn live_players(delivery: OverlayDelivery) -> usize {
    match delivery {
        OverlayDelivery::Delivered { sources } => sources,
        OverlayDelivery::OnlyPreview { .. } | OverlayDelivery::NoPage => 0,
    }
}

fn verdict_of(outcome: ClipOutcome) -> RemoteVerdict {
    match outcome {
        ClipOutcome::Played => RemoteVerdict::Played,
        ClipOutcome::Refused { reason } => RemoteVerdict::Refused { reason },
        ClipOutcome::NeverFetched => RemoteVerdict::Refused {
            reason: NEVER_FETCHED.to_owned(),
        },
        ClipOutcome::NoVerdict => RemoteVerdict::Unknown {
            reason: NO_VERDICT.to_owned(),
        },
        ClipOutcome::Revoked => RemoteVerdict::Unknown {
            reason: REVOKED.to_owned(),
        },
        ClipOutcome::ServerStopped => RemoteVerdict::Unknown {
            reason: SERVER_STOPPED.to_owned(),
        },
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_audio::RemoteClip;
    use forge_overlay::kinds::alert::KIND_ID as RECEIVER_KIND;
    use forge_overlay::{OverlayKindRegistry, config, register_builtin_kinds};
    use forge_runtime::{EventBus, OverlayFrameSink, OverlayReceivers};
    use forge_storage::{MockOverlayRepo, OverlayRepo, SettingsRepo};

    use super::*;
    use crate::test_support::{StubEventLog, overlay_named, stopped_server_handle, test_backend};

    const DESTINATION: &str = "stage-audio";

    /// Why: a clip that is never revoked or reported leaves `verdict` awaiting forever, so a
    /// regression has to fail the run instead of stalling it.
    const HANG_GUARD: Duration = Duration::from_secs(5);

    struct FakePages {
        receivers: OverlayReceivers,
        pushed: Mutex<Vec<serde_json::Value>>,
    }

    impl FakePages {
        fn with(receivers: OverlayReceivers) -> Arc<Self> {
            Arc::new(Self {
                receivers,
                pushed: Mutex::new(Vec::new()),
            })
        }

        fn pushed(&self) -> Vec<serde_json::Value> {
            self.pushed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn forget_what_was_pushed(&self) {
            self.pushed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
        }
    }

    #[async_trait]
    impl OverlayFrameSink for FakePages {
        async fn deliver_content(
            &self,
            _: &OverlayId,
            content: serde_json::Value,
            _: Option<u64>,
        ) -> OverlayReceivers {
            self.pushed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(content);
            self.receivers
        }

        async fn deliver_reload(&self, _: &OverlayId) {}

        async fn revoke(&self, _: &OverlayId) {}
    }

    fn field(content: &serde_json::Value, key: &str) -> String {
        content
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("the delivered content carries no {key}"))
            .to_owned()
    }

    fn clip() -> RemoteClip {
        RemoteClip {
            bytes: vec![0_u8; 64],
            media_type: forge_audio::ClipMediaType::Wave,
            duration_ms: 250,
        }
    }

    fn destination_id() -> RemoteDestinationId {
        RemoteDestinationId::new(DESTINATION)
    }

    async fn audio_overlay(
        receivers: OverlayReceivers,
        known: bool,
    ) -> (OverlayAudioDestination, Arc<FakePages>) {
        let (backend, _writes) = test_backend();
        let server = stopped_server_handle(&backend).await;
        let pages = FakePages::with(receivers);

        let mut repo = MockOverlayRepo::new();
        repo.expect_get()
            .returning(move |id| Ok(known.then(|| overlay_named(id.as_str(), RECEIVER_KIND))));

        let mut kinds = OverlayKindRegistry::new();
        register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");

        let overlays = OverlayServiceHandle::new(
            Arc::new(repo) as Arc<dyn OverlayRepo>,
            Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            Arc::new(kinds),
            EventBus::new(Arc::new(StubEventLog)),
            Some(Arc::clone(&pages) as Arc<dyn OverlayFrameSink>),
        );

        (OverlayAudioDestination::new(server, overlays), pages)
    }

    async fn settled(
        destination: &OverlayAudioDestination,
        clip_id: &RemoteClipId,
    ) -> Result<RemoteVerdict, AudioError> {
        tokio::time::timeout(HANG_GUARD, destination.verdict(clip_id))
            .await
            .expect("the clip settles rather than hanging")
    }

    async fn listening() -> (OverlayAudioDestination, Arc<FakePages>) {
        audio_overlay(
            OverlayReceivers {
                sources: 1,
                preview_tabs: 0,
            },
            true,
        )
        .await
    }

    #[tokio::test]
    async fn each_delivery_announces_a_clip_id_of_its_own() {
        let (destination, pages) = listening().await;

        let first = destination
            .deliver(&destination_id(), clip())
            .await
            .expect("the first clip is announced");
        let second = destination
            .deliver(&destination_id(), clip())
            .await
            .expect("the second clip is announced");

        assert_ne!(first.clip_id.expose(), second.clip_id.expose());
        assert_eq!(
            pages
                .pushed()
                .iter()
                .map(|content| field(content, config::CLIP_ID))
                .collect::<Vec<_>>(),
            vec![
                first.clip_id.expose().to_owned(),
                second.clip_id.expose().to_owned()
            ]
        );
    }

    #[tokio::test]
    async fn the_announced_clip_id_never_reveals_the_addresses_that_authorize_the_fetch() {
        let (destination, pages) = listening().await;

        let delivered = destination
            .deliver(&destination_id(), clip())
            .await
            .expect("the clip is announced");

        let content = pages.pushed().pop().expect("one announcement was pushed");
        let clip_id = delivered.clip_id.expose();
        for key in [config::CLIP_PATH, config::REPORT_PATH] {
            let address = field(&content, key);
            assert!(
                !address.contains(clip_id),
                "{key} embeds the announced clip id"
            );
        }
    }

    #[tokio::test]
    async fn live_players_counts_browser_sources_and_ignores_preview_tabs() {
        for (receivers, expected) in [
            (
                OverlayReceivers {
                    sources: 2,
                    preview_tabs: 0,
                },
                2,
            ),
            (
                OverlayReceivers {
                    sources: 2,
                    preview_tabs: 3,
                },
                2,
            ),
            (
                OverlayReceivers {
                    sources: 0,
                    preview_tabs: 3,
                },
                0,
            ),
            (
                OverlayReceivers {
                    sources: 0,
                    preview_tabs: 0,
                },
                0,
            ),
        ] {
            let (destination, _pages) = audio_overlay(receivers, true).await;

            let delivered = destination
                .deliver(&destination_id(), clip())
                .await
                .expect("the clip is announced");

            assert_eq!(delivered.live_players, expected, "for {receivers:?}");
        }
    }

    #[tokio::test]
    async fn a_command_for_a_clip_that_was_never_announced_reaches_no_page() {
        let (destination, pages) = listening().await;

        destination
            .control(
                &destination_id(),
                &RemoteClipId::new("01JQZZZZZZZZZZZZZZZZZZZZZZ"),
                RemoteCommand::Pause,
            )
            .await
            .expect("an unknown clip is not an error");

        assert!(pages.pushed().is_empty());
    }

    #[tokio::test]
    async fn a_command_addresses_its_clip_with_the_verb_the_caller_named() {
        for (command, expected) in [
            (RemoteCommand::Stop, AudioCommand::Stop),
            (RemoteCommand::Pause, AudioCommand::Pause),
            (RemoteCommand::Resume, AudioCommand::Resume),
        ] {
            let (destination, pages) = listening().await;
            let delivered = destination
                .deliver(&destination_id(), clip())
                .await
                .expect("the clip is announced");
            pages.forget_what_was_pushed();

            destination
                .control(&destination_id(), &delivered.clip_id, command)
                .await
                .expect("the command is pushed");

            let content = pages.pushed().pop().expect("one command was pushed");
            assert_eq!(field(&content, config::COMMAND), expected.as_str());
            assert_eq!(
                field(&content, config::CLIP_ID),
                delivered.clip_id.expose(),
                "{command:?} was not addressed at its own clip"
            );
        }
    }

    #[tokio::test]
    async fn stopping_a_clip_revokes_it_so_it_settles_without_a_verdict() {
        let (destination, _pages) = listening().await;
        let delivered = destination
            .deliver(&destination_id(), clip())
            .await
            .expect("the clip is announced");

        let target = destination_id();
        let (outcome, stopped) = tokio::time::timeout(HANG_GUARD, async {
            tokio::join!(
                destination.verdict(&delivered.clip_id),
                destination.control(&target, &delivered.clip_id, RemoteCommand::Stop)
            )
        })
        .await
        .expect("the clip settles rather than hanging");

        stopped.expect("the stop is pushed");
        assert!(matches!(
            outcome.expect("the clip settles"),
            RemoteVerdict::Unknown { .. }
        ));
    }

    #[tokio::test]
    async fn a_stopped_clip_is_forgotten_and_can_no_longer_be_addressed() {
        let (destination, pages) = listening().await;
        let delivered = destination
            .deliver(&destination_id(), clip())
            .await
            .expect("the clip is announced");

        destination
            .control(&destination_id(), &delivered.clip_id, RemoteCommand::Stop)
            .await
            .expect("the stop is pushed");
        pages.forget_what_was_pushed();

        destination
            .control(&destination_id(), &delivered.clip_id, RemoteCommand::Resume)
            .await
            .expect("a forgotten clip is not an error");

        assert!(pages.pushed().is_empty());
        assert!(settled(&destination, &delivered.clip_id).await.is_err());
    }

    /// Why: the speak queue drops a skipped clip's completion (parked in `verdict`) before its
    /// Stop is pushed; the Stop must still find the clip.
    #[tokio::test]
    async fn a_stop_after_the_verdict_wait_was_dropped_still_reaches_the_page_and_revokes() {
        let (destination, pages) = listening().await;
        let delivered = destination
            .deliver(&destination_id(), clip())
            .await
            .expect("the clip is announced");
        let capability = destination
            .capability_of(delivered.clip_id.expose())
            .expect("the announced clip is pending");
        let abandoned =
            tokio::time::timeout(Duration::ZERO, destination.verdict(&delivered.clip_id)).await;
        assert!(
            abandoned.is_err(),
            "the clip settled before anyone reported"
        );
        pages.forget_what_was_pushed();

        destination
            .control(&destination_id(), &delivered.clip_id, RemoteCommand::Stop)
            .await
            .expect("the stop is pushed");

        let content = pages.pushed().pop().expect("the stop reached the page");
        assert_eq!(
            field(&content, config::COMMAND),
            AudioCommand::Stop.as_str()
        );
        assert!(
            !destination.server.hold_audio_clip(&capability).await,
            "the stop must withdraw the clip from the server"
        );
    }

    #[tokio::test]
    async fn a_verdict_for_a_clip_that_is_not_waiting_is_an_error_rather_than_a_hang() {
        let (destination, _pages) = listening().await;

        let refused = settled(
            &destination,
            &RemoteClipId::new("01JQZZZZZZZZZZZZZZZZZZZZZZ"),
        )
        .await;

        assert!(matches!(refused, Err(AudioError::RemoteDestination(_))));
    }

    #[tokio::test]
    async fn an_announcement_that_reaches_no_overlay_reports_a_remote_destination_error() {
        let (destination, pages) = audio_overlay(
            OverlayReceivers {
                sources: 1,
                preview_tabs: 0,
            },
            false,
        )
        .await;

        let refused = destination.deliver(&destination_id(), clip()).await;

        assert!(matches!(refused, Err(AudioError::RemoteDestination(_))));
        assert!(pages.pushed().is_empty());
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Class {
        Played,
        Refused,
        Unknown,
    }

    fn class(verdict: &RemoteVerdict) -> Class {
        match verdict {
            RemoteVerdict::Played => Class::Played,
            RemoteVerdict::Refused { .. } => Class::Refused,
            RemoteVerdict::Unknown { .. } => Class::Unknown,
        }
    }

    /// Why: only `Refused` is a settled "it did not play"; `Unknown` means forge cannot say,
    /// and the speak queue acts differently on each.
    #[test]
    fn every_clip_outcome_settles_into_the_verdict_class_the_queue_acts_on() {
        for (outcome, expected) in [
            (ClipOutcome::Played, Class::Played),
            (
                ClipOutcome::Refused {
                    reason: "the page is muted".to_owned(),
                },
                Class::Refused,
            ),
            (ClipOutcome::NeverFetched, Class::Refused),
            (ClipOutcome::NoVerdict, Class::Unknown),
            (ClipOutcome::Revoked, Class::Unknown),
            (ClipOutcome::ServerStopped, Class::Unknown),
        ] {
            let label = format!("{outcome:?}");

            assert_eq!(class(&verdict_of(outcome)), expected, "{label}");
        }
    }
}
