#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_pipeline::{
    BlocklistMode, EmoteTokenSet, PipelineConfig, PipelineContext, PipelineResult, ReplacementRule,
    SkipReason, SkipRulesConfig, process,
};

fn ctx() -> PipelineContext<'static> {
    PipelineContext {
        viewer_name: "viewer",
        recent_messages: &[],
    }
}

#[test]
fn replacement_rule_order_hides_original_text_from_later_rules() {
    let config = PipelineConfig {
        replacement_rules: vec![
            ReplacementRule::Regex {
                compiled: regex::Regex::new(r"https?://\S+").unwrap(),
                replacement: "link".into(),
            },
            ReplacementRule::Regex {
                compiled: regex::Regex::new(r"https").unwrap(),
                replacement: "SHOULD_NOT_APPEAR".into(),
            },
        ],
        ..PipelineConfig::default()
    };

    match process("visit https://example.com today", &config, &ctx()) {
        PipelineResult::Speak(text) => {
            assert!(
                !text.contains("SHOULD_NOT_APPEAR"),
                "second rule ran on the original url - rule order broken"
            );
            assert!(text.contains("link"), "URL should have been substituted");
        }
        PipelineResult::Skip { .. } => panic!("expected Speak"),
    }
}

#[test]
fn replacement_output_is_not_caught_by_blocklist() {
    let config = PipelineConfig {
        replacement_rules: vec![ReplacementRule::Text {
            pattern: "sneaky".into(),
            replacement: "badword".into(),
        }],
        word_blocklist: vec!["badword".into()],
        blocklist_mode: BlocklistMode::Censor,
        ..PipelineConfig::default()
    };

    match process("that was sneaky right", &config, &ctx()) {
        PipelineResult::Speak(text) => {
            assert!(
                text.contains("badword"),
                "blocklist runs before replacements now - it must not see the introduced word: {text}"
            );
        }
        PipelineResult::Skip { .. } => panic!("expected Speak - blocklist already ran"),
    }
}

#[test]
fn skip_rules_evaluate_original_message_unaffected_by_output_settings() {
    let mut config = PipelineConfig {
        skip_rules: SkipRulesConfig {
            contains_url: true,
            ..SkipRulesConfig::default()
        },
        ..PipelineConfig::default()
    };
    config.emote_sources.twitch = true;
    config.emote_tokens.tokens.insert("LUL".into());

    match process("check LUL https://example.com out", &config, &ctx()) {
        PipelineResult::Skip { reason } => {
            assert_eq!(
                reason,
                SkipReason::MatchedSkipRule("message contains a url")
            );
        }
        PipelineResult::Speak(_) => panic!("expected Skip due to URL"),
    }
}

#[test]
fn skip_rules_longer_than_checks_original_length_not_post_replacement_length() {
    let config = PipelineConfig {
        skip_rules: SkipRulesConfig {
            longer_than: true,
            max_chars: 10,
            ..SkipRulesConfig::default()
        },
        replacement_rules: vec![ReplacementRule::Text {
            pattern: "short".into(),
            replacement: "averylongword".into(),
        }],
        ..PipelineConfig::default()
    };

    match process("short text", &config, &ctx()) {
        PipelineResult::Speak(text) => {
            assert!(
                text.contains("averylongword"),
                "replacement must still apply after the original-length check passes: {text}"
            );
        }
        PipelineResult::Skip { .. } => {
            panic!("original message is exactly 10 chars - must not be skipped")
        }
    }
}

#[test]
fn url_skip_rule_prevents_downstream_processing() {
    let config = PipelineConfig {
        skip_rules: SkipRulesConfig {
            contains_url: true,
            ..SkipRulesConfig::default()
        },
        word_blocklist: vec!["safe".into()],
        blocklist_mode: BlocklistMode::Censor,
        ..PipelineConfig::default()
    };

    match process("visit https://example.com safe word", &config, &ctx()) {
        PipelineResult::Skip { reason } => {
            assert_eq!(
                reason,
                SkipReason::MatchedSkipRule("message contains a url"),
                "wrong skip reason"
            );
        }
        PipelineResult::Speak(_) => panic!("expected Skip due to URL"),
    }
}

#[test]
fn blocklist_skip_mode_short_circuits_downstream_stages() {
    let config = PipelineConfig {
        word_blocklist: vec!["forbidden".into()],
        blocklist_mode: BlocklistMode::SkipMessage,
        ..PipelineConfig::default()
    };

    match process("forbidden", &config, &ctx()) {
        PipelineResult::Skip { reason } => {
            assert_eq!(reason, SkipReason::BlockedByWordFilter);
        }
        PipelineResult::Speak(_) => panic!("expected Skip from blocklist"),
    }
}

#[test]
fn empty_text_after_emote_strip_skips_after_output_stage() {
    let mut config = PipelineConfig {
        emote_tokens: EmoteTokenSet {
            tokens: ["LUL".to_string(), "Pog".to_string()].into_iter().collect(),
        },
        ..PipelineConfig::default()
    };
    config.emote_sources.twitch = true;

    match process("LUL Pog LUL", &config, &ctx()) {
        PipelineResult::Skip { reason } => {
            assert_eq!(reason, SkipReason::EmptyAfterProcessing);
        }
        PipelineResult::Speak(_) => panic!("expected Skip for empty-after-strip message"),
    }
}
