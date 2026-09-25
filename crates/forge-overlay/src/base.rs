use std::time::Duration;

use forge_types::Variant;

use crate::config::{self, DURATION, SOUND, SPEECH, SPEECH_VOICE, effective_overlay_config};
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};

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
    fields
}

pub(crate) fn base_defaults(disposition: DeliveryDisposition, display_secs: i64) -> OverlayConfig {
    let mut defaults = OverlayConfig::from([
        (SOUND.to_owned(), config::text("")),
        (SPEECH.to_owned(), config::text("")),
        (SPEECH_VOICE.to_owned(), config::text("")),
    ]);
    if disposition == DeliveryDisposition::Transient {
        defaults.insert(DURATION.to_owned(), Variant::Int(display_secs));
    }
    defaults
}

/// `None` for a look whose content is applied on arrival rather than shown for a while; an
/// override of zero milliseconds counts as no override.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechProgram {
    pub text: String,
    pub voice_alias: Option<String>,
}

/// Always removes the speech text from `content`: the page plays the speech, it never shows it.
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
