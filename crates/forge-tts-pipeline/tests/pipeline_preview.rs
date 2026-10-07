#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_pipeline::{
    AppliedReplacement, BlocklistMode, EmoteTokenSet, OutputConfig, PipelineConfig,
    PipelineContext, PipelineResult, ReplacementOrigin, ReplacementRule, SkipReason,
    SkipRulesConfig, StageAction, StageName, StageOutcome, preview, process,
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

fn text_rule(pattern: &str, replacement: &str) -> ReplacementRule {
    ReplacementRule::Text {
        pattern: pattern.into(),
        replacement: replacement.into(),
    }
}

fn url_rule() -> ReplacementRule {
    ReplacementRule::Regex {
        compiled: regex::Regex::new(r"https?://\S+").unwrap(),
        replacement: "link".into(),
    }
}

fn stage(outcomes: &[StageOutcome], name: StageName) -> &StageOutcome {
    outcomes
        .iter()
        .find(|outcome| outcome.stage == name)
        .unwrap()
}

fn marked_slices(outcome: &StageOutcome) -> Vec<(&str, ReplacementOrigin)> {
    outcome
        .replacements
        .iter()
        .map(|mark| (&outcome.output[mark.output_range.clone()], mark.origin))
        .collect()
}

fn filter_config() -> PipelineConfig {
    PipelineConfig {
        word_blocklist: vec!["ass".into(), "gtfo".into()],
        blocklist_mode: BlocklistMode::Censor,
        replacement_rules: vec![url_rule(), text_rule("thanks", "дякую")],
        ..PipelineConfig::default()
    }
}

#[test]
fn preview_reaches_the_same_verdict_as_process() {
    let mut stripping = filter_config();
    stripping.emote_sources.twitch = true;
    stripping.emote_sources.emoji = true;
    stripping.emote_tokens = EmoteTokenSet {
        tokens: ["Kappa".to_owned()].into_iter().collect(),
    };
    stripping.output = OutputConfig {
        read_display_name_first: true,
        sanitize_punctuation: true,
        ..OutputConfig::default()
    };
    let skipping = PipelineConfig {
        blocklist_mode: BlocklistMode::SkipMessage,
        ..filter_config()
    };
    let suppressing = PipelineConfig {
        skip_rules: SkipRulesConfig {
            contains_url: true,
            ..SkipRulesConfig::default()
        },
        ..filter_config()
    };
    for (case, config) in [
        ("filters", filter_config()),
        ("output transforms", stripping),
        ("skip on blocked word", skipping),
        ("skip on url", suppressing),
    ] {
        for input in [
            "GG GTFO!!! check https://x.y/z thanks",
            "Kappa 😀 thanks!!",
            "Kappa",
            "classic assist",
            "",
        ] {
            assert_eq!(
                preview(input, &config, &ctx()).0,
                process(input, &config, &ctx()),
                "{case}: input {input:?}"
            );
        }
    }
}

#[test]
fn every_mark_cuts_its_own_replacement_out_of_the_stage_output() {
    let (_, outcomes) = preview(
        "GG GTFO!!! check https://x.y/z thanks",
        &filter_config(),
        &ctx(),
    );
    for name in [
        StageName::WordBlocklist,
        StageName::TextReplacements,
        StageName::Output,
    ] {
        let outcome = stage(&outcomes, name);
        for mark in &outcome.replacements {
            assert_eq!(
                &outcome.output[mark.output_range.clone()],
                mark.replacement,
                "{name:?}: {mark:?} in {:?}",
                outcome.output
            );
        }
    }
    assert_eq!(
        marked_slices(stage(&outcomes, StageName::Output)),
        vec![
            ("[beep]", ReplacementOrigin::BlockedWordIndex(1)),
            ("link", ReplacementOrigin::ReplacementRuleIndex(0)),
            ("дякую", ReplacementOrigin::ReplacementRuleIndex(1)),
        ]
    );
}

#[test]
fn a_blocklist_mark_names_the_entry_and_the_text_as_written() {
    for (case, words, input, expected) in [
        (
            "case differs from the entry",
            vec!["ass", "gtfo"],
            "GTFO!!!",
            AppliedReplacement {
                output_range: 0..6,
                original: "GTFO".into(),
                replacement: "[beep]".into(),
                origin: ReplacementOrigin::BlockedWordIndex(1),
            },
        ),
        (
            "the longest entry wins",
            vec!["go", "go-away"],
            "Go-Away",
            AppliedReplacement {
                output_range: 0..6,
                original: "Go-Away".into(),
                replacement: "[beep]".into(),
                origin: ReplacementOrigin::BlockedWordIndex(1),
            },
        ),
        (
            "entry carries its own punctuation",
            vec!["a$$!", "gtfo"],
            "(a$$!)",
            AppliedReplacement {
                output_range: 1..7,
                original: "a$$!".into(),
                replacement: "[beep]".into(),
                origin: ReplacementOrigin::BlockedWordIndex(0),
            },
        ),
    ] {
        let config = PipelineConfig {
            word_blocklist: words.into_iter().map(str::to_owned).collect(),
            blocklist_mode: BlocklistMode::Censor,
            ..PipelineConfig::default()
        };
        let (_, outcomes) = preview(input, &config, &ctx());
        assert_eq!(
            stage(&outcomes, StageName::WordBlocklist).replacements,
            vec![expected],
            "{case}"
        );
    }
}

#[test]
fn a_later_rule_rewriting_part_of_an_earlier_mark_drops_that_mark() {
    let config = PipelineConfig {
        replacement_rules: vec![text_rule("a", "bcd"), text_rule("c", "X")],
        ..PipelineConfig::default()
    };
    let (_, outcomes) = preview("a", &config, &ctx());
    assert_eq!(
        marked_slices(stage(&outcomes, StageName::TextReplacements)),
        vec![("X", ReplacementOrigin::ReplacementRuleIndex(1))]
    );
}

#[test]
fn a_later_rule_rewriting_text_before_an_earlier_mark_shifts_that_mark() {
    let config = PipelineConfig {
        replacement_rules: vec![text_rule("aa", "b"), text_rule("x", "yyy")],
        ..PipelineConfig::default()
    };
    let (_, outcomes) = preview("x aa", &config, &ctx());
    assert_eq!(
        marked_slices(stage(&outcomes, StageName::TextReplacements)),
        vec![
            ("yyy", ReplacementOrigin::ReplacementRuleIndex(1)),
            ("b", ReplacementOrigin::ReplacementRuleIndex(0)),
        ]
    );
}

#[test]
fn output_stage_marks_follow_the_text_through_prefix_strip_and_sanitize() {
    for (case, output, emoji, tokens, input, expected) in [
        (
            "display name prefix",
            OutputConfig {
                read_display_name_first: true,
                ..OutputConfig::default()
            },
            false,
            vec![],
            "ok thanks",
            vec!["дякую"],
        ),
        (
            "emoji before the mark",
            OutputConfig::default(),
            true,
            vec![],
            "😀😀 thanks",
            vec!["дякую"],
        ),
        (
            "emote token before the mark",
            OutputConfig::default(),
            false,
            vec!["Kappa"],
            "hi Kappa thanks",
            vec!["дякую"],
        ),
        (
            "doubled punctuation collapses before the mark",
            OutputConfig {
                read_display_name_first: true,
                sanitize_punctuation: true,
                ..OutputConfig::default()
            },
            false,
            vec![],
            "wow!!! thanks",
            vec!["дякую"],
        ),
    ] {
        let mut config = filter_config();
        config.output = output;
        config.emote_sources.emoji = emoji;
        config.emote_sources.twitch = !tokens.is_empty();
        config.emote_tokens = EmoteTokenSet {
            tokens: tokens.into_iter().map(str::to_owned).collect(),
        };
        let (_, outcomes) = preview(input, &config, &ctx());
        let marked: Vec<&str> = marked_slices(stage(&outcomes, StageName::Output))
            .into_iter()
            .map(|(slice, _)| slice)
            .collect();
        assert_eq!(marked, expected, "{case}");
    }
}

#[test]
fn a_mark_whose_text_the_output_stage_removes_entirely_is_dropped() {
    let mut config = PipelineConfig {
        replacement_rules: vec![text_rule("smile", "😀")],
        ..PipelineConfig::default()
    };
    config.emote_sources.emoji = true;
    let (_, outcomes) = preview("smile please", &config, &ctx());
    assert!(
        stage(&outcomes, StageName::Output).replacements.is_empty(),
        "{:?}",
        stage(&outcomes, StageName::Output).replacements
    );
}

#[test]
fn an_output_mark_keeps_its_first_letter_when_a_stripped_emote_starts_with_it() {
    let mut config = filter_config();
    config.emote_sources.twitch = true;
    config.emote_tokens = EmoteTokenSet {
        tokens: ["lol".to_owned()].into_iter().collect(),
    };
    let (_, outcomes) = preview("hi lol https://x.y/z", &config, &ctx());
    assert_eq!(
        marked_slices(stage(&outcomes, StageName::Output)),
        vec![("link", ReplacementOrigin::ReplacementRuleIndex(0))]
    );
}
