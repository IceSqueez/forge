use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::remote::{
    ClipMediaType, RemoteAudioDestination, RemoteClip, RemoteClipId, RemoteCommand, RemoteDelivery,
    RemoteDestinationId, RemoteVerdict,
};
use crate::sink::AudioSink;
use crate::wave::encode_wave;

const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WATCHDOG_SLACK: Duration = Duration::from_secs(45);
const WATCHDOG_CEILING: Duration = Duration::from_secs(360);
const MAX_REASON_CHARS: usize = 200;
const SINGLE_PLAYER: usize = 1;
const WATCHDOG_REASON: &str = "the page never reported whether it played the clip";

pub struct RemoteSink {
    destination: Arc<dyn RemoteAudioDestination>,
    destination_id: RemoteDestinationId,
}

impl RemoteSink {
    pub fn new(
        destination: Arc<dyn RemoteAudioDestination>,
        destination_id: RemoteDestinationId,
    ) -> Self {
        Self {
            destination,
            destination_id,
        }
    }
}

#[async_trait]
impl AudioSink for RemoteSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        self.play_stoppable(buffer).await.map(|_| ())
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let playback = self.play_controlled(buffer).await?;
        let handle = playback.handle();
        let destination_id = self.destination_id.clone();
        tokio::spawn(async move {
            if let Err(e) = playback.await {
                tracing::warn!(
                    destination = %destination_id,
                    error = %e,
                    "remote audio clip did not play"
                );
            }
        });
        Ok(handle)
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let duration_ms = buffer.duration_ms();
        let clip = RemoteClip {
            bytes: encode_wave(&buffer),
            media_type: ClipMediaType::Wave,
            duration_ms,
        };

        let RemoteDelivery {
            clip_id,
            live_players,
        } = self.destination.deliver(&self.destination_id, clip).await?;

        if let Err(refusal) = admit(&self.destination_id, live_players) {
            push(
                self.destination.as_ref(),
                &self.destination_id,
                &clip_id,
                RemoteCommand::Stop,
            )
            .await;
            return Err(refusal);
        }

        if live_players > SINGLE_PLAYER {
            tracing::warn!(
                destination = %self.destination_id,
                live_players,
                "more than one audio page is live, so every one of them plays this clip"
            );
        }

        let controls = Controls::new();
        let handle =
            PlaybackHandle::from_flags(Arc::clone(&controls.stop), Arc::clone(&controls.pause));
        let budget = watchdog_budget(duration_ms);

        tokio::spawn(observe_controls(
            Arc::clone(&self.destination),
            self.destination_id.clone(),
            clip_id.clone(),
            controls.clone(),
            budget,
        ));

        let completion = settle_clip(
            Arc::clone(&self.destination),
            self.destination_id.clone(),
            clip_id,
            controls,
            budget,
            live_players,
        );
        Ok(ControlledPlayback::merged(handle, Box::pin(completion)))
    }
}

#[derive(Clone)]
struct Controls {
    stop: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

impl Controls {
    fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
        }
    }

    fn observed(&self) -> ControlState {
        ControlState {
            stopped: self.stop.load(Ordering::Relaxed),
            held: self.pause.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ControlState {
    stopped: bool,
    held: bool,
}

enum RemoteSettlement {
    Finished,
    Degraded(String),
    Failed(AudioError),
}

async fn observe_controls(
    destination: Arc<dyn RemoteAudioDestination>,
    destination_id: RemoteDestinationId,
    clip_id: RemoteClipId,
    controls: Controls,
    budget: Duration,
) {
    let give_up = tokio::time::sleep(budget);
    tokio::pin!(give_up);
    let mut sent = ControlState::default();

    loop {
        tokio::select! {
            () = &mut give_up => return,
            () = tokio::time::sleep(CONTROL_POLL_INTERVAL) => {}
        }

        if let Some(command) = next_command(sent, controls.observed()) {
            sent = applied(sent, command);
            push(destination.as_ref(), &destination_id, &clip_id, command).await;
        }

        if sent.stopped || controls.finished.load(Ordering::Relaxed) {
            return;
        }
    }
}

async fn settle_clip(
    destination: Arc<dyn RemoteAudioDestination>,
    destination_id: RemoteDestinationId,
    clip_id: RemoteClipId,
    controls: Controls,
    budget: Duration,
    live_players: usize,
) -> Result<(), AudioError> {
    let settlement = tokio::select! {
        verdict = destination.verdict(&clip_id) => {
            settle(controls.stop.load(Ordering::Relaxed), verdict)
        }
        () = tokio::time::sleep(budget) => {
            RemoteSettlement::Degraded(WATCHDOG_REASON.to_owned())
        }
    };
    controls.finished.store(true, Ordering::Relaxed);

    match settlement {
        RemoteSettlement::Finished => Ok(()),
        RemoteSettlement::Degraded(reason) => {
            tracing::warn!(
                destination = %destination_id,
                live_players,
                reason = %reason,
                "remote audio route is degraded; the clip counts as played"
            );
            Ok(())
        }
        RemoteSettlement::Failed(e) => Err(e),
    }
}

async fn push(
    destination: &dyn RemoteAudioDestination,
    destination_id: &RemoteDestinationId,
    clip_id: &RemoteClipId,
    command: RemoteCommand,
) {
    if let Err(e) = destination.control(destination_id, clip_id, command).await {
        tracing::warn!(
            destination = %destination_id,
            command = ?command,
            error = %e,
            "remote audio command did not reach the destination"
        );
    }
}

fn admit(destination_id: &RemoteDestinationId, live_players: usize) -> Result<(), AudioError> {
    if live_players == 0 {
        return Err(AudioError::NoRemoteListener {
            destination: destination_id.to_string(),
            live_players,
        });
    }
    Ok(())
}

fn next_command(sent: ControlState, observed: ControlState) -> Option<RemoteCommand> {
    if sent.stopped {
        return None;
    }
    if observed.stopped {
        return Some(RemoteCommand::Stop);
    }
    match (observed.held, sent.held) {
        (true, false) => Some(RemoteCommand::Pause),
        (false, true) => Some(RemoteCommand::Resume),
        _ => None,
    }
}

fn applied(sent: ControlState, command: RemoteCommand) -> ControlState {
    match command {
        RemoteCommand::Stop => ControlState {
            stopped: true,
            ..sent
        },
        RemoteCommand::Pause => ControlState { held: true, ..sent },
        RemoteCommand::Resume => ControlState {
            held: false,
            ..sent
        },
    }
}

fn watchdog_budget(duration_ms: u64) -> Duration {
    Duration::from_millis(duration_ms)
        .saturating_add(WATCHDOG_SLACK)
        .min(WATCHDOG_CEILING)
}

fn settle(stopped: bool, verdict: Result<RemoteVerdict, AudioError>) -> RemoteSettlement {
    if stopped {
        return RemoteSettlement::Finished;
    }
    match verdict {
        Ok(RemoteVerdict::Played) => RemoteSettlement::Finished,
        Ok(RemoteVerdict::Unknown { reason }) => {
            RemoteSettlement::Degraded(bounded_reason(&reason))
        }
        Ok(RemoteVerdict::Refused { reason }) => {
            RemoteSettlement::Failed(AudioError::RemoteDestination(bounded_reason(&reason)))
        }
        Err(e) => RemoteSettlement::Failed(e),
    }
}

fn bounded_reason(reason: &str) -> String {
    reason
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_REASON_CHARS)
        .collect()
}
