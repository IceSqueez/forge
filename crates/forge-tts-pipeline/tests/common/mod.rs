use forge_tts_pipeline::{
    BlocklistMode, EmoteTokenSet, EmoteTreatment, OutputConfig, PipelineConfig, SkipRulesConfig,
};

pub fn neutral_output() -> OutputConfig {
    OutputConfig {
        read_display_name_first: false,
        emote_treatment: EmoteTreatment::Keep,
        strip_emoji: false,
        sanitize_punctuation: false,
        max_duration_secs: None,
        language_aware_voice: false,
    }
}

pub fn neutral_config() -> PipelineConfig {
    PipelineConfig {
        emote_tokens: EmoteTokenSet::default(),
        skip_rules: SkipRulesConfig::default(),
        replacement_rules: Vec::new(),
        word_blocklist: Vec::new(),
        blocklist_mode: BlocklistMode::Censor,
        output: neutral_output(),
        strip_reward_emotes: false,
    }
}
