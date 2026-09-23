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

        let clip_id = Ulid::generate().to_string();
        let content = announcement_content(&AudioAnnouncement {
            clip_id: &clip_id,
            clip_path: ticket.clip_path(),
            report_path: ticket.report_path(),
            media_type: media_type.as_str(),
            duration_ms,
        });
        self.register(clip_id.clone(), ticket.capability().clone(), outcome);

        match self
            .overlays
            .deliver_content(&identity, content, None)
            .await
        {
            Ok(delivery) => Ok(RemoteDelivery {
                clip_id: RemoteClipId::new(clip_id),
                live_players: live_players(delivery),
            }),
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

        let identity = OverlayId::new(destination.as_str());
        let content = command_content(overlay_command(command), Some(clip_id.expose()));
        let pushed = self
            .overlays
            .deliver_content(&identity, content, None)
            .await;

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
        let _release = Release {
            destination: self,
            clip_id: clip_id.expose(),
        };
        Ok(verdict_of(outcome.recv().await))
    }
}

struct Release<'a> {
    destination: &'a OverlayAudioDestination,
    clip_id: &'a str,
}

impl Drop for Release<'_> {
    fn drop(&mut self) {
        self.destination.forget(self.clip_id);
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
