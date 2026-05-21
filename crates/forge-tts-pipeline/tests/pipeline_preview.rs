//! Regression: preview() returns one StageOutcome per stage with correct before/after.
//!
//! The preview API is used by the UI live-preview panel. If it returns fewer than
//! five stages, or incorrect input/output values, the UI will display wrong diffs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_pipeline::{
    BlocklistMode, EmoteTokenSet, PipelineConfig, PipelineResult, ReplacementRule, SkipReason,
    StageAction, StageName, UrlMode, preview,
};

#[test]
fn preview_always_returns_five_stages() {
    let configs = vec![
        PipelineConfig::default(),
        PipelineConfig {
            url_mode: UrlMode::SkipMessage,
            ..PipelineConfig::default()
        },
        PipelineConfig {
            word_blocklist: vec!["bad".into()],
            blocklist_mode: BlocklistMode::SkipMessage,
            ..PipelineConfig::default()
        },
        PipelineConfig {
            max_chars: 2,
            ..PipelineConfig::default()
        },
    ];

    for config in configs {
        let (_result, outcomes) = preview("test message with content", &config);
        assert_eq!(outcomes.len(), 5, "preview must return exactly 5 stages");
        assert_eq!(outcomes[0].stage, StageName::EmoteStripper);
        assert_eq!(outcomes[1].stage, StageName::UrlSanitizer);
        assert_eq!(outcomes[2].stage, StageName::TextReplacements);
        assert_eq!(outcomes[3].stage, StageName::WordBlocklist);
        assert_eq!(outcomes[4].stage, StageName::LengthCapper);
    }
}

#[test]
fn preview_stage_output_feeds_next_stage_input() {
    let config = PipelineConfig {
        emote_tokens: EmoteTokenSet {
            tokens: ["LUL".to_string()].into_iter().collect(),
        },
        replacement_rules: vec![ReplacementRule::Text {
            pattern: "world".into(),
            replacement: "forge".into(),
        }],
        ..PipelineConfig::default()
    };

    let (_result, outcomes) = preview("hello LUL world", &config);

    let emote_out = &outcomes[0].output;
    let url_in = &outcomes[1].input;
    assert_eq!(
        emote_out, url_in,
        "EmoteStripper output must equal UrlSanitizer input"
    );

    let url_out = &outcomes[1].output;
    let replace_in = &outcomes[2].input;
    assert_eq!(
        url_out, replace_in,
        "UrlSanitizer output must equal TextReplacements input"
    );
}

#[test]
fn preview_emote_stage_shows_transform_when_token_stripped() {
    let config = PipelineConfig {
        emote_tokens: EmoteTokenSet {
            tokens: ["Pog".to_string()].into_iter().collect(),
        },
        ..PipelineConfig::default()
    };

    let (_result, outcomes) = preview("hello Pog world", &config);
    assert_eq!(outcomes[0].action, StageAction::Transformed);
    assert_eq!(outcomes[0].input, "hello Pog world");
    assert_eq!(outcomes[0].output, "hello world");
}

#[test]
fn preview_url_skip_marks_remaining_stages_as_skipped() {
    let config = PipelineConfig {
        url_mode: UrlMode::SkipMessage,
        ..PipelineConfig::default()
    };

    let (result, outcomes) = preview("visit https://evil.com now", &config);

    assert!(matches!(result, PipelineResult::Skip { .. }));
    assert_eq!(
        outcomes[1].stage,
        StageName::UrlSanitizer,
        "stage 1 must be UrlSanitizer"
    );
    assert!(
        matches!(outcomes[1].action, StageAction::Skipped { .. }),
        "UrlSanitizer must be Skipped"
    );
    for (idx, outcome) in outcomes[2..].iter().enumerate() {
        assert!(
            matches!(outcome.action, StageAction::Skipped { .. }),
            "stage {} must be Skipped after URL skip",
            idx + 2
        );
    }
}

#[test]
fn preview_passthrough_stage_shows_passed_through() {
    let config = PipelineConfig::default();
    let (result, outcomes) = preview("clean message", &config);

    assert!(matches!(result, PipelineResult::Speak(_)));
    for outcome in &outcomes[..4] {
        assert_eq!(
            outcome.action,
            StageAction::PassedThrough,
            "stage {:?} should be PassedThrough for clean input",
            outcome.stage
        );
    }
}

#[test]
fn preview_word_blocklist_censor_still_speaks() {
    let config = PipelineConfig {
        word_blocklist: vec!["bad".into()],
        blocklist_mode: BlocklistMode::Censor,
        ..PipelineConfig::default()
    };

    let (result, outcomes) = preview("this is bad content", &config);

    assert!(
        matches!(result, PipelineResult::Speak(_)),
        "Censor mode should Speak not Skip"
    );
    assert_eq!(outcomes[3].stage, StageName::WordBlocklist);
    assert_eq!(
        outcomes[3].action,
        StageAction::Transformed,
        "WordBlocklist in Censor mode must be Transformed"
    );
    assert!(
        outcomes[3].output.contains("[beep]"),
        "censored output must contain [beep]: {}",
        outcomes[3].output
    );
}

#[test]
fn preview_length_capper_records_truncation_as_transformed() {
    let config = PipelineConfig {
        max_chars: 5,
        ..PipelineConfig::default()
    };

    let (result, outcomes) = preview("hello world extra", &config);

    assert!(matches!(result, PipelineResult::Speak(_)));
    assert_eq!(outcomes[4].stage, StageName::LengthCapper);
    assert_eq!(
        outcomes[4].action,
        StageAction::Transformed,
        "LengthCapper must be Transformed when truncation occurs"
    );
    assert!(
        outcomes[4].output.contains('\u{2026}'),
        "truncated output must contain ellipsis: {}",
        outcomes[4].output
    );
}

#[test]
fn preview_skip_produces_final_result_skip_not_speak() {
    let config = PipelineConfig {
        word_blocklist: vec!["forbidden".into()],
        blocklist_mode: BlocklistMode::SkipMessage,
        ..PipelineConfig::default()
    };

    let (result, _) = preview("contains forbidden word", &config);
    assert!(
        matches!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::BlockedByWordFilter
            }
        ),
        "final result must be Skip(BlockedByWordFilter)"
    );
}
