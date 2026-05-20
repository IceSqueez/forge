use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Twitch,
    YouTube,
    Kick,
    Trovo,
    Core,
    Rhai,
    Http,
    Obs,
    VTube,
    Discord,
    Midi,
    Hotkey,
    Timer,
    Server,
    Audio,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_serialize_and_deserialize() {
        let variants = [
            EventSource::Twitch,
            EventSource::YouTube,
            EventSource::Kick,
            EventSource::Trovo,
            EventSource::Core,
            EventSource::Rhai,
            EventSource::Http,
            EventSource::Obs,
            EventSource::VTube,
            EventSource::Discord,
            EventSource::Midi,
            EventSource::Hotkey,
            EventSource::Timer,
            EventSource::Server,
            EventSource::Audio,
        ];
        for src in variants {
            let json = serde_json::to_string(&src).unwrap();
            let back: EventSource = serde_json::from_str(&json).unwrap();
            assert_eq!(src, back, "serde roundtrip failed for {src:?}");
        }
    }

    #[test]
    fn source_count_is_fifteen() {
        let variants = [
            EventSource::Twitch,
            EventSource::YouTube,
            EventSource::Kick,
            EventSource::Trovo,
            EventSource::Core,
            EventSource::Rhai,
            EventSource::Http,
            EventSource::Obs,
            EventSource::VTube,
            EventSource::Discord,
            EventSource::Midi,
            EventSource::Hotkey,
            EventSource::Timer,
            EventSource::Server,
            EventSource::Audio,
        ];
        assert_eq!(
            variants.len(),
            15,
            "EventSource must have exactly 15 variants per CLAUDE.md §12b"
        );
    }
}
