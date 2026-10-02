use std::time::Duration;

use forge_types::Variant;

use crate::config::{self, DURATION, SOUND, SPEECH, SPEECH_VOICE, effective_overlay_config};
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::motion;
use crate::source_box::{ContentMargins, DesignSize};

pub const DEFAULT_DISPLAY_SECS: i64 = 5;

const MILLIS_PER_SEC: u64 = 1_000;

pub(crate) fn base_fields(disposition: DeliveryDisposition) -> Vec<SectionedField> {
    let mut fields = Vec::new();
    if disposition == DeliveryDisposition::Transient {
        fields.push(config::duration_field());
    }
    fields.push(config::sound_field());
    fields.push(config::speech_field());
    fields.push(config::speech_voice_field());
    fields.extend(config::design_size_fields());
    fields.extend(config::margin_fields());
    fields
}

pub(crate) fn base_defaults(
    disposition: DeliveryDisposition,
    display_secs: i64,
    design_size: DesignSize,
    margins: ContentMargins,
) -> OverlayConfig {
    let mut defaults = OverlayConfig::from([
        (SOUND.to_owned(), config::text("")),
        (SPEECH.to_owned(), config::text("")),
        (SPEECH_VOICE.to_owned(), config::text("")),
    ]);
    design_size.write_into(&mut defaults);
    margins.write_into(&mut defaults);
    if disposition == DeliveryDisposition::Transient {
        defaults.insert(DURATION.to_owned(), Variant::Int(display_secs));
    }
    defaults
}

pub fn display_window(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    override_ms: Option<u64>,
) -> Option<Duration> {
    if descriptor.delivery_disposition() != DeliveryDisposition::Transient {
        return None;
    }
    if let Some(ms) = override_ms.filter(|ms| *ms > 0) {
        return Some(Duration::from_millis(ms));
    }
    let configured = match effective_overlay_config(descriptor, stored).get(DURATION) {
        Some(Variant::Int(secs)) if *secs > 0 => *secs,
        _ => descriptor.default_display_secs(),
    };
    let secs = u64::try_from(configured).unwrap_or(DEFAULT_DISPLAY_SECS.unsigned_abs());
    Some(Duration::from_millis(secs.saturating_mul(MILLIS_PER_SEC)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowTiming {
    pub window: Duration,
    pub exit_tail: Duration,
}

impl ShowTiming {
    pub fn total(self) -> Duration {
        self.window.saturating_add(self.exit_tail)
    }
}

pub fn show_timing(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    override_ms: Option<u64>,
) -> Option<ShowTiming> {
    let window = display_window(descriptor, stored, override_ms)?;
    Some(ShowTiming {
        window,
        exit_tail: exit_tail(descriptor, stored),
    })
}

pub fn exit_tail(descriptor: &dyn OverlayKindDescriptor, stored: &OverlayConfig) -> Duration {
    if descriptor.delivery_disposition() != DeliveryDisposition::Transient
        || !descriptor.motion().axes.exit
    {
        return Duration::ZERO;
    }
    motion::exit_duration(&effective_overlay_config(descriptor, stored))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechProgram {
    pub text: String,
    pub voice_alias: Option<String>,
}

pub fn take_speech(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    content: &mut OverlayConfig,
) -> Option<SpeechProgram> {
    let text = match content.remove(SPEECH) {
        Some(Variant::String(text)) => text.trim().to_owned(),
        _ => return None,
    };
    if text.is_empty() {
        return None;
    }
    let voice_alias = effective_overlay_config(descriptor, stored)
        .get(SPEECH_VOICE)
        .and_then(Variant::as_str)
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(str::to_owned);
    Some(SpeechProgram { text, voice_alias })
}
