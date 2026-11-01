use forge_types::{ClipId, RecoverMutex};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use forge_audio::PlaybackHandle;

use crate::audio_event::{AudioEvent, AudioEventSink};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock_recover()
}

#[derive(Default)]
pub(super) struct PlayLegs {
    pub(super) stopped: bool,
    pub(super) started: bool,
    pub(super) finish_claimed: bool,
    pub(super) handles: Vec<PlaybackHandle>,
}

impl PlayLegs {
    pub(super) fn claim_finish(&mut self) -> bool {
        let owed = self.started && !self.finish_claimed;
        if owed {
            self.finish_claimed = true;
        }
        owed
    }
}

#[derive(Default)]
pub(super) struct PlayControl {
    pub(super) legs: Mutex<PlayLegs>,
    pub(super) stop_signal: tokio::sync::Notify,
}

pub(super) fn stop_handles(handles: &[PlaybackHandle]) {
    for handle in handles {
        handle.stop();
    }
}

impl PlayControl {
    pub(super) fn claim_finish_on_stop(&self) -> bool {
        let (handles, owed) = {
            let mut legs = lock(&self.legs);
            legs.stopped = true;
            (std::mem::take(&mut legs.handles), legs.claim_finish())
        };
        self.stop_signal.notify_one();
        stop_handles(&handles);
        owed
    }

    pub(super) fn claim_finish_after_start(&self) -> bool {
        let mut legs = lock(&self.legs);
        legs.started = true;
        legs.stopped && legs.claim_finish()
    }

    pub(super) fn claim_finish(&self) -> bool {
        lock(&self.legs).claim_finish()
    }

    pub(super) fn abandon(&self) {
        let handles = {
            let mut legs = lock(&self.legs);
            legs.stopped = true;
            legs.finish_claimed = true;
            std::mem::take(&mut legs.handles)
        };
        self.stop_signal.notify_one();
        stop_handles(&handles);
    }

    pub(super) async fn stopped(&self) {
        while !self.is_stopped() {
            self.stop_signal.notified().await;
        }
    }

    pub(super) fn is_stopped(&self) -> bool {
        lock(&self.legs).stopped
    }

    pub(super) fn install(&self, handles: Vec<PlaybackHandle>) -> bool {
        let refused = {
            let mut legs = lock(&self.legs);
            if legs.stopped {
                Some(handles)
            } else {
                legs.handles = handles;
                None
            }
        };
        match refused {
            Some(handles) => {
                stop_handles(&handles);
                false
            }
            None => true,
        }
    }
}

pub(super) struct ActivePlay {
    pub(super) play_id: u64,
    pub(super) control: Arc<PlayControl>,
    pub(super) label: String,
}

pub(super) type ActiveRegistry = Arc<Mutex<HashMap<ClipId, Vec<ActivePlay>>>>;

pub(super) fn retire(active: &ActiveRegistry, clip_id: ClipId, play_id: u64) -> bool {
    let mut guard = lock(active);
    let Some(plays) = guard.get_mut(&clip_id) else {
        return false;
    };
    let before = plays.len();
    plays.retain(|play| play.play_id != play_id);
    let removed = plays.len() < before;
    if plays.is_empty() {
        guard.remove(&clip_id);
    }
    removed
}

pub(super) struct Reservation {
    pub(super) clip_id: ClipId,
    pub(super) play_id: u64,
    pub(super) label: String,
    pub(super) control: Arc<PlayControl>,
}

#[derive(Clone)]
pub(super) struct PlayReporter {
    pub(super) active: ActiveRegistry,
    pub(super) event_sink: Arc<dyn AudioEventSink>,
}

impl PlayReporter {
    pub(super) fn fail(&self, play: &Reservation, error: String) {
        if !retire(&self.active, play.clip_id, play.play_id) {
            return;
        }
        play.control.abandon();
        self.event_sink.emit(AudioEvent::PlaybackFailed {
            play_id: play.play_id,
            clip_id: Some(play.clip_id),
            clip_label: Some(play.label.clone()),
            error,
        });
    }

    pub(super) fn finish(&self, play: &Reservation) {
        retire(&self.active, play.clip_id, play.play_id);
        if play.control.claim_finish() {
            emit_finished(
                self.event_sink.as_ref(),
                play.clip_id,
                play.play_id,
                &play.label,
            );
        }
    }
}

pub(super) fn emit_finished(sink: &dyn AudioEventSink, clip_id: ClipId, play_id: u64, label: &str) {
    sink.emit(AudioEvent::PlaybackFinished {
        play_id,
        clip_id: Some(clip_id),
        clip_label: Some(label.to_owned()),
    });
}
