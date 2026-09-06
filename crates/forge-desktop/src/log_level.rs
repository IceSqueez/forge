use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use forge_storage::DEFAULT_DIAGNOSTIC_LOG_LEVEL;
use forge_types::LogLevel;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::reload;

static HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();
static ENV_OVERRIDDEN: AtomicBool = AtomicBool::new(false);

/// Decoder chatter, not level policy: demoted at every selectable level.
const SYMPHONIA_TARGETS: &[&str] = &[
    "symphonia_core",
    "symphonia_common",
    "symphonia_metadata",
    "symphonia_bundle_flac",
    "symphonia_bundle_mp3",
    "symphonia_codec_aac",
    "symphonia_codec_adpcm",
    "symphonia_codec_alac",
    "symphonia_codec_pcm",
    "symphonia_codec_vorbis",
    "symphonia_format_caf",
    "symphonia_format_isomp4",
    "symphonia_format_mkv",
    "symphonia_format_ogg",
    "symphonia_format_riff",
];

pub const SELECTABLE: [LogLevel; 5] = [
    LogLevel::Trace,
    LogLevel::Debug,
    LogLevel::Info,
    LogLevel::Warn,
    LogLevel::Error,
];

fn directive_word(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn filter_for(level: &LogLevel) -> EnvFilter {
    let mut directives = String::from(directive_word(level));
    for target in SYMPHONIA_TARGETS {
        directives.push(',');
        directives.push_str(target);
        directives.push_str("=warn");
    }
    EnvFilter::new(directives)
}

pub fn default_filter() -> EnvFilter {
    filter_for(&DEFAULT_DIAGNOSTIC_LOG_LEVEL)
}

/// Wraps the filter rather than a layer, so the layers below stay downcastable.
pub fn reloadable(filter: EnvFilter, env_overridden: bool) -> reload::Layer<EnvFilter, Registry> {
    ENV_OVERRIDDEN.store(env_overridden, Ordering::Relaxed);
    let (layer, handle) = reload::Layer::new(filter);
    let _ = HANDLE.set(handle);
    layer
}

pub fn env_overridden() -> bool {
    ENV_OVERRIDDEN.load(Ordering::Relaxed)
}

/// An environment filter accepted at boot owns the level for the whole run; every other
/// source is refused while it holds. Returns whether the swap actually happened.
pub fn apply(level: &LogLevel) -> bool {
    if env_overridden() {
        return false;
    }
    HANDLE
        .get()
        .is_some_and(|handle| handle.reload(filter_for(level)).is_ok())
}
