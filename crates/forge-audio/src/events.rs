use forge_types::ClipId;

#[derive(Debug, Clone)]
pub enum AudioEvent {
    PlaybackStarted {
        clip_id: Option<ClipId>,
        clip_label: Option<String>,
        device: String,
        duration_secs: Option<f64>,
        looped: bool,
    },
    PlaybackFinished {
        clip_id: Option<ClipId>,
        clip_label: Option<String>,
    },
    PlaybackFailed {
        clip_id: Option<ClipId>,
        clip_label: Option<String>,
        error: String,
    },
}

pub trait AudioEventSink: Send + Sync {
    fn emit(&self, event: AudioEvent);
}

pub struct NullAudioEventSink;

impl AudioEventSink for NullAudioEventSink {
    fn emit(&self, _event: AudioEvent) {}
}
