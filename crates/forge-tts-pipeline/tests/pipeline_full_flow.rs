//! Regression: pipeline stage ordering and combined transforms.
//!
//! The canonical order is:
//!   EmoteStripper → UrlSanitizer → TextReplacements → WordBlocklist → LengthCapper
//!
//! Changing stage order silently breaks user-configured pipelines (e.g. a URL
//! replacement rule that fires before UrlSanitizer is already gone would fail).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_pipeline::{
    BlocklistMode, EmoteTokenSet, PipelineConfig, PipelineResult, ReplacementRule, SkipReason,
    UrlMode, process,
};

fn url_replace_config() -> PipelineConfig {
    PipelineConfig {
        url_mode: UrlMode::Replace {
            substitute: "link".into(),
        },
        ..PipelineConfig::default()
    }
}

#[test]
fn url_replaced_before_regex_rules_can_see_original_url() {
    // UrlSanitizer runs before TextReplacements.
    // A regex rule matching "https" never sees the raw URL - it's already "link".
    let config = PipelineConfig {
        url_mode: UrlMode::Replace {
            substitute: "link".into(),
        },
        replacement_rules: vec![ReplacementRule::Regex {
            compiled: regex::Regex::new(r"https?://\S+").unwrap(),
            replacement: "SHOULD_NOT_APPEAR".into(),
        }],
        ..PipelineConfig::default()
    };

    match process("visit https://example.com today", &config) {
        PipelineResult::Speak(text) => {
            assert!(
                !text.contains("SHOULD_NOT_APPEAR"),
                "regex ran before URL sanitizer - stage order broken"
            );
            assert!(text.contains("link"), "URL should have been substituted");
        }
        PipelineResult::Skip { .. } => panic!("expected Speak"),
    }
}

#[test]
fn blocklist_runs_after_replacement_rules() {
    // TextReplacements runs before WordBlocklist.
    // If a replacement converts a non-blocked word into a blocked word,
    // the blocklist stage catches it.
    let config = PipelineConfig {
        replacement_rules: vec![ReplacementRule::Text {
            pattern: "sneaky".into(),
            replacement: "badword".into(),
        }],
        word_blocklist: vec!["badword".into()],
        blocklist_mode: BlocklistMode::Censor,
        ..PipelineConfig::default()
    };

    match process("that was sneaky right", &config) {
        PipelineResult::Speak(text) => {
            assert!(
                text.contains("[beep]"),
                "replacement → blocklist chain must work: {text}"
            );
        }
        PipelineResult::Skip { .. } => panic!("expected Speak after censor"),
    }
}

#[test]
fn emote_stripper_runs_before_url_sanitizer() {
    // EmoteStripper runs first. Emote token "LUL" removed before URL check.
    let mut config = url_replace_config();
    config.emote_sources.twitch = true;
    config.emote_tokens = EmoteTokenSet {
        tokens: ["LUL".to_string()].into_iter().collect(),
    };

    match process("check LUL https://example.com out", &config) {
        PipelineResult::Speak(text) => {
            assert!(!text.contains("LUL"), "emote not stripped");
            assert!(!text.contains("https://"), "URL not replaced");
            assert!(text.contains("link"), "URL should be replaced with 'link'");
        }
        PipelineResult::Skip { .. } => panic!("expected Speak"),
    }
}

#[test]
fn length_capper_truncates_result_of_earlier_stages() {
    // LengthCapper is the final stage; it truncates the output of all prior stages.
    let config = PipelineConfig {
        replacement_rules: vec![ReplacementRule::Text {
            pattern: "short".into(),
            replacement: "averylongword".into(),
        }],
        max_chars: 10,
        ..PipelineConfig::default()
    };

    match process("short text", &config) {
        PipelineResult::Speak(text) => {
            // "averylongword text" (18 chars) truncated to 10 + ellipsis
            assert!(
                text.chars().count() <= 11, // 10 + ellipsis char
                "length capper must truncate post-replacement output: '{text}'"
            );
        }
        PipelineResult::Skip { .. } => panic!("expected Speak after truncation"),
    }
}

#[test]
fn url_skip_rule_prevents_downstream_processing() {
    // When UrlSanitizer skips the message, no further stages run.
    // The text must be returned as-is in SkipReason (not mutated by later stages).
    let config = PipelineConfig {
        url_mode: UrlMode::SkipMessage,
        word_blocklist: vec!["safe".into()],
        blocklist_mode: BlocklistMode::Censor,
        ..PipelineConfig::default()
    };

    match process("visit https://example.com safe word", &config) {
        PipelineResult::Skip { reason } => {
            assert_eq!(
                reason,
                SkipReason::MatchedSkipRule("message contains url"),
                "wrong skip reason"
            );
        }
        PipelineResult::Speak(_) => panic!("expected Skip due to URL"),
    }
}

#[test]
fn blocklist_skip_mode_short_circuits_length_capper() {
    // WordBlocklist in SkipMessage mode skips before LengthCapper.
    let config = PipelineConfig {
        word_blocklist: vec!["forbidden".into()],
        blocklist_mode: BlocklistMode::SkipMessage,
        max_chars: 3,
        ..PipelineConfig::default()
    };

    match process("forbidden", &config) {
        PipelineResult::Skip { reason } => {
            assert_eq!(reason, SkipReason::BlockedByWordFilter);
        }
        PipelineResult::Speak(_) => panic!("expected Skip from blocklist"),
    }
}

#[test]
fn empty_text_after_emote_strip_skips_at_length_capper() {
    let mut config = PipelineConfig {
        emote_tokens: EmoteTokenSet {
            tokens: ["LUL".to_string(), "Pog".to_string()].into_iter().collect(),
        },
        ..PipelineConfig::default()
    };
    config.emote_sources.twitch = true;

    match process("LUL Pog LUL", &config) {
        PipelineResult::Skip { reason } => {
            assert_eq!(reason, SkipReason::EmptyAfterProcessing);
        }
        PipelineResult::Speak(_) => panic!("expected Skip for empty-after-strip message"),
    }
}
