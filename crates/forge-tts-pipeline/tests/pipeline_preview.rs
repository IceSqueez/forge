//! Regression: preview() returns one StageOutcome per stage with correct before/after.
//!
//! The preview API is used by the UI live-preview panel. If it returns fewer than
//! four stages, or incorrect input/output values, the UI will display wrong diffs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_pipeline::{
    BlocklistMode, OutputConfig, PipelineConfig, PipelineContext, PipelineResult, ReplacementRule,
    SkipReason, SkipRulesConfig, StageAction, StageName, preview,
};

fn ctx() -> PipelineContext<'static> {
    PipelineContext {
        viewer_name: "koval_dev",
        recent_messages: &[],
    }
}

#[test]
fn preview_always_returns_four_stages() {
    let configs = vec![
        PipelineConfig::default(),
        PipelineConfig {
            skip_rules: SkipRulesConfig {
                contains_url: true,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        },
        PipelineConfig {
            word_blocklist: vec!["bad".into()],
            blocklist_mode: BlocklistMode::SkipMessage,
            ..PipelineConfig::default()
        },
        PipelineConfig {
            skip_rules: SkipRulesConfig {
                longer_than: true,
                max_chars: 2,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        },
    ];

    for config in configs {
        let (_result, outcomes) = preview("test message with content", &config, &ctx());
        assert_eq!(outcomes.len(), 4, "preview must return exactly 4 stages");
        assert_eq!(outcomes[0].stage, StageName::SkipRules);
        assert_eq!(outcomes[1].stage, StageName::WordBlocklist);
        assert_eq!(outcomes[2].stage, StageName::TextReplacements);
        assert_eq!(outcomes[3].stage, StageName::Output);
    }
}

#[test]
fn preview_stage_output_feeds_next_stage_input() {
    let config = PipelineConfig {
        word_blocklist: vec!["bad".into()],
        blocklist_mode: BlocklistMode::Censor,
        replacement_rules: vec![ReplacementRule::Text {
            pattern: "world".into(),
            replacement: "forge".into(),
        }],
        ..PipelineConfig::default()
    };

    let (_result, outcomes) = preview("hello bad world", &config, &ctx());

    let blocklist_out = &outcomes[1].output;
    let replace_in = &outcomes[2].input;
    assert_eq!(
        blocklist_out, replace_in,
        "WordBlocklist output must equal TextReplacements input"
    );

    let replace_out = &outcomes[2].output;
    let output_in = &outcomes[3].input;
    assert_eq!(
        replace_out, output_in,
        "TextReplacements output must equal Output stage input"
    );
}

#[test]
fn preview_output_stage_shows_transform_when_token_stripped() {
    let mut config = PipelineConfig {
        emote_tokens: forge_tts_pipeline::EmoteTokenSet {
            tokens: ["Pog".to_string()].into_iter().collect(),
        },
        ..PipelineConfig::default()
    };
    config.emote_sources.twitch = true;

    let (_result, outcomes) = preview("hello Pog world", &config, &ctx());
    assert_eq!(outcomes[3].stage, StageName::Output);
    assert_eq!(outcomes[3].action, StageAction::Transformed);
    assert_eq!(outcomes[3].input, "hello Pog world");
    assert_eq!(outcomes[3].output, "hello world");
}

#[test]
fn preview_skip_rules_skip_marks_remaining_stages_as_skipped() {
    let config = PipelineConfig {
        skip_rules: SkipRulesConfig {
            contains_url: true,
            ..SkipRulesConfig::default()
        },
        ..PipelineConfig::default()
    };

    let (result, outcomes) = preview("visit https://evil.com now", &config, &ctx());

    assert!(matches!(result, PipelineResult::Skip { .. }));
    assert_eq!(
        outcomes[0].stage,
        StageName::SkipRules,
        "stage 0 must be SkipRules"
    );
    assert!(
        matches!(outcomes[0].action, StageAction::Skipped { .. }),
        "SkipRules must be Skipped"
    );
    for (idx, outcome) in outcomes[1..].iter().enumerate() {
        assert!(
            matches!(outcome.action, StageAction::Skipped { .. }),
            "stage {} must be Skipped after SkipRules skip",
            idx + 1
        );
    }
}

#[test]
fn preview_passthrough_stage_shows_passed_through() {
    let config = PipelineConfig::default();
    let (result, outcomes) = preview("clean message", &config, &ctx());

    assert!(matches!(result, PipelineResult::Speak(_)));
    for outcome in &outcomes {
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

    let (result, outcomes) = preview("this is bad content", &config, &ctx());

    assert!(
        matches!(result, PipelineResult::Speak(_)),
        "Censor mode should Speak not Skip"
    );
    assert_eq!(outcomes[1].stage, StageName::WordBlocklist);
    assert_eq!(
        outcomes[1].action,
        StageAction::Transformed,
        "WordBlocklist in Censor mode must be Transformed"
    );
    assert!(
        outcomes[1].output.contains("[beep]"),
        "censored output must contain [beep]: {}",
        outcomes[1].output
    );
}

#[test]
fn preview_output_stage_records_display_name_prefix_as_transformed() {
    let config = PipelineConfig {
        output: OutputConfig {
            read_display_name_first: true,
            ..OutputConfig::default()
        },
        ..PipelineConfig::default()
    };

    let (result, outcomes) = preview("hello world", &config, &ctx());

    assert!(matches!(result, PipelineResult::Speak(_)));
    assert_eq!(outcomes[3].stage, StageName::Output);
    assert_eq!(
        outcomes[3].action,
        StageAction::Transformed,
        "Output stage must be Transformed when the display-name prefix is applied"
    );
    assert!(
        outcomes[3].output.starts_with("koval_dev says: "),
        "prefixed output must start with the viewer name: {}",
        outcomes[3].output
    );
}

#[test]
fn preview_skip_produces_final_result_skip_not_speak() {
    let config = PipelineConfig {
        word_blocklist: vec!["forbidden".into()],
        blocklist_mode: BlocklistMode::SkipMessage,
        ..PipelineConfig::default()
    };

    let (result, _) = preview("contains forbidden word", &config, &ctx());
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
