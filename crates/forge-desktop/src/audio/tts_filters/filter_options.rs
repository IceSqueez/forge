use forge_components::tr;
use gpui::SharedString;

pub(super) const MAX_DURATION_MIN: u32 = 1;
pub(super) const MAX_DURATION_MAX: u32 = 600;
const MAX_DURATION_DEFAULT: u32 = 30;

#[derive(Clone, Copy)]
pub(super) enum SkipRule {
    ContainsUrl,
    BotAccounts,
    LongerThan,
    Repeat,
    EmoteOnly,
    MostlyNonLatin,
}

impl SkipRule {
    pub(super) fn key(self) -> &'static str {
        match self {
            SkipRule::ContainsUrl => "url",
            SkipRule::BotAccounts => "bots",
            SkipRule::LongerThan => "length",
            SkipRule::Repeat => "repeat",
            SkipRule::EmoteOnly => "emote-only",
            SkipRule::MostlyNonLatin => "non-latin",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum OutputOpt {
    ReadName,
    EmoteWord,
    Sanitize,
    StripTwitchEmotes,
    StripRewardEmotes,
    StripEmoji,
}

impl OutputOpt {
    pub(super) fn key(self) -> &'static str {
        match self {
            OutputOpt::ReadName => "name",
            OutputOpt::EmoteWord => "emote",
            OutputOpt::Sanitize => "sanitize",
            OutputOpt::StripTwitchEmotes => "strip-twitch-emotes",
            OutputOpt::StripRewardEmotes => "strip-reward-emotes",
            OutputOpt::StripEmoji => "strip-emoji",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModalStage {
    Skip,
    Blocklist,
    Replace,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SkipPreset {
    Url,
    Prefix,
    Bots,
    Length,
    Repeat,
    EmoteOnly,
    NonLatin,
    Regex,
}

pub(super) const SKIP_PRESETS: [SkipPreset; 8] = [
    SkipPreset::Url,
    SkipPreset::Prefix,
    SkipPreset::Bots,
    SkipPreset::Length,
    SkipPreset::Repeat,
    SkipPreset::EmoteOnly,
    SkipPreset::NonLatin,
    SkipPreset::Regex,
];

impl SkipPreset {
    pub(super) fn key(self) -> &'static str {
        match self {
            SkipPreset::Url => "url",
            SkipPreset::Prefix => "prefix",
            SkipPreset::Bots => "bots",
            SkipPreset::Length => "length",
            SkipPreset::Repeat => "repeat",
            SkipPreset::EmoteOnly => "emote-only",
            SkipPreset::NonLatin => "non-latin",
            SkipPreset::Regex => "regex",
        }
    }

    pub(super) fn label(self) -> SharedString {
        match self {
            SkipPreset::Url => tr!("tts_filters_preset_skip_url"),
            SkipPreset::Prefix => tr!("tts_filters_preset_skip_prefix"),
            SkipPreset::Bots => tr!("tts_filters_preset_skip_bots"),
            SkipPreset::Length => tr!("tts_filters_preset_skip_length"),
            SkipPreset::Repeat => tr!("tts_filters_preset_skip_repeat"),
            SkipPreset::EmoteOnly => tr!("tts_filters_preset_skip_emote_only"),
            SkipPreset::NonLatin => tr!("tts_filters_preset_skip_non_latin"),
            SkipPreset::Regex => tr!("tts_filters_preset_skip_regex"),
        }
        .into()
    }

    pub(super) fn param_label(self) -> Option<SharedString> {
        match self {
            SkipPreset::Prefix => Some(tr!("tts_filters_preset_skip_prefix_label").into()),
            SkipPreset::Length => Some(tr!("tts_filters_preset_skip_length_label").into()),
            SkipPreset::Regex => Some(tr!("tts_filters_preset_skip_regex_label").into()),
            _ => None,
        }
    }

    pub(super) fn placeholder(self) -> SharedString {
        match self {
            SkipPreset::Prefix => tr!("tts_filters_preset_skip_prefix_placeholder").into(),
            SkipPreset::Length => tr!("tts_filters_preset_skip_length_placeholder").into(),
            SkipPreset::Regex => tr!("tts_filters_preset_skip_regex_placeholder").into(),
            _ => SharedString::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputPreset {
    Name,
    Emote,
    Lang,
    MaxDur,
    Sanitize,
}

pub(super) const OUTPUT_PRESETS: [OutputPreset; 5] = [
    OutputPreset::Name,
    OutputPreset::Emote,
    OutputPreset::Lang,
    OutputPreset::MaxDur,
    OutputPreset::Sanitize,
];

impl OutputPreset {
    pub(super) fn key(self) -> &'static str {
        match self {
            OutputPreset::Name => "name",
            OutputPreset::Emote => "emote",
            OutputPreset::Lang => "lang",
            OutputPreset::MaxDur => "maxdur",
            OutputPreset::Sanitize => "sanitize",
        }
    }

    pub(super) fn label(self) -> SharedString {
        match self {
            OutputPreset::Name => tr!("tts_filters_output_read_name"),
            OutputPreset::Emote => tr!("tts_filters_output_emote"),
            OutputPreset::Lang => tr!("tts_filters_preset_output_lang"),
            OutputPreset::MaxDur => tr!("tts_filters_preset_output_maxdur"),
            OutputPreset::Sanitize => tr!("tts_filters_output_sanitize"),
        }
        .into()
    }

    pub(super) fn hint(self) -> SharedString {
        match self {
            OutputPreset::Name => tr!("tts_filters_preset_output_name_hint"),
            OutputPreset::Emote => tr!("tts_filters_preset_output_emote_hint"),
            OutputPreset::Lang => tr!("tts_filters_preset_output_lang_hint"),
            OutputPreset::MaxDur => tr!("tts_filters_preset_output_maxdur_hint"),
            OutputPreset::Sanitize => tr!("tts_filters_preset_output_sanitize_hint"),
        }
        .into()
    }

    pub(super) fn param_label(self) -> Option<SharedString> {
        match self {
            OutputPreset::MaxDur => Some(tr!("tts_filters_preset_output_maxdur_label").into()),
            _ => None,
        }
    }

    pub(super) fn placeholder(self) -> SharedString {
        match self {
            OutputPreset::MaxDur => tr!("tts_filters_preset_output_maxdur_placeholder").into(),
            _ => SharedString::default(),
        }
    }

    pub(super) fn default_param(self) -> String {
        match self {
            OutputPreset::MaxDur => MAX_DURATION_DEFAULT.to_string(),
            _ => String::new(),
        }
    }
}

pub(super) fn parse_max_duration(raw: &str) -> Option<u32> {
    raw.trim()
        .parse::<u32>()
        .ok()
        .filter(|secs| (MAX_DURATION_MIN..=MAX_DURATION_MAX).contains(secs))
}
