use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use forge_storage::{DEFAULT_DIAGNOSTIC_LOG_LEVEL, log_level_as_str};
use forge_types::LogLevel;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::reload;

static HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();
static ENV_OVERRIDDEN: AtomicBool = AtomicBool::new(false);

/// Decoder chatter, not level policy: capped at the quieter of `warn` and the selected level.
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

fn filter_for(level: &LogLevel) -> EnvFilter {
    let word = log_level_as_str(level);
    let demotion = match level {
        LogLevel::Warn | LogLevel::Error => word,
        _ => "warn",
    };
    let mut directives = String::from(word);
    for target in SYMPHONIA_TARGETS {
        directives.push(',');
        directives.push_str(target);
        directives.push('=');
        directives.push_str(demotion);
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forge_types::LogLevel;

    use super::{SYMPHONIA_TARGETS, filter_for};

    // Why: two failure modes in one comparison. The directives are joined by hand and handed to
    // `EnvFilter::new`, which drops any directive it cannot parse without reporting it - a lost
    // separator, a stray character or a word the parser rejects would leave the demotions, or
    // the level itself, quietly absent and the process running at a level nobody chose. And the
    // demotion word is capped at the quieter of `warn` and the selected level, so at the two
    // quiet tiers it must follow the level down: an uncapped `=warn` would let decoder chatter
    // outrank forge's own suppressed records, which is the noise the demotion exists to remove.
    // Comparing the whole parsed set catches a dropped directive and an unintended extra alike.
    #[test]
    fn every_selectable_level_composes_into_that_level_plus_capped_decoder_demotions() {
        for (level, word, demotion) in [
            (LogLevel::Trace, "trace", "warn"),
            (LogLevel::Debug, "debug", "warn"),
            (LogLevel::Info, "info", "warn"),
            (LogLevel::Warn, "warn", "warn"),
            (LogLevel::Error, "error", "error"),
        ] {
            let mut expected: BTreeSet<String> = SYMPHONIA_TARGETS
                .iter()
                .map(|target| format!("{target}={demotion}"))
                .collect();
            expected.insert(word.to_owned());

            let rendered = filter_for(&level).to_string();
            let parsed: BTreeSet<String> = rendered.split(',').map(str::to_owned).collect();

            assert_eq!(parsed, expected, "unexpected filter for {level:?}");
        }
    }
}
