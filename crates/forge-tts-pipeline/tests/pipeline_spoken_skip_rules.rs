#![allow(clippy::unwrap_used)]
mod common;

use common::neutral_config;
use forge_tts_pipeline::{
    EmoteTokenSet, EmoteTreatment, PipelineConfig, PipelineContext, PipelineResult, SkipReason,
    SkipRule, SkipRulesConfig, process,
};

fn ctx() -> PipelineContext<'static> {
    PipelineContext {
        viewer_name: "viewer",
        recent_messages: &[],
        speech_language: None,
    }
}

fn with_rules(skip_rules: SkipRulesConfig) -> PipelineConfig {
    PipelineConfig {
        skip_rules,
        ..neutral_config()
    }
}

fn stripping_kappa(skip_rules: SkipRulesConfig) -> PipelineConfig {
    let mut config = with_rules(skip_rules);
    config.output.emote_treatment = EmoteTreatment::StripTwitch;
    config.emote_tokens = EmoteTokenSet {
        tokens: ["Kappa".to_owned()].into_iter().collect(),
    };
    config
}

fn sanitizing(skip_rules: SkipRulesConfig) -> PipelineConfig {
    let mut config = with_rules(skip_rules);
    config.output.sanitize_punctuation = true;
    config
}

fn colon_emotes_to_words(skip_rules: SkipRulesConfig) -> PipelineConfig {
    let mut config = with_rules(skip_rules);
    config.output.emote_treatment = EmoteTreatment::ToWord;
    config
}

fn stripping_emoji(skip_rules: SkipRulesConfig) -> PipelineConfig {
    let mut config = with_rules(skip_rules);
    config.output.strip_emoji = true;
    config
}

fn skipped_by(rule: SkipRule) -> PipelineResult {
    PipelineResult::Skip {
        reason: SkipReason::MatchedSkipRule(rule),
    }
}

fn speak(text: &str) -> PipelineResult {
    PipelineResult::Speak(text.to_owned())
}

fn url_rule() -> SkipRulesConfig {
    SkipRulesConfig {
        contains_url: true,
        ..SkipRulesConfig::default()
    }
}

fn prefix_rule() -> SkipRulesConfig {
    SkipRulesConfig {
        skip_prefix: Some("!".into()),
        ..SkipRulesConfig::default()
    }
}

fn emote_only_rule() -> SkipRulesConfig {
    SkipRulesConfig {
        emote_only: true,
        ..SkipRulesConfig::default()
    }
}

fn non_latin_rule() -> SkipRulesConfig {
    SkipRulesConfig {
        mostly_non_latin: true,
        ..SkipRulesConfig::default()
    }
}

fn regex_rule(pattern: &str) -> SkipRulesConfig {
    SkipRulesConfig {
        custom_regexes: vec![regex::Regex::new(pattern).unwrap()],
        ..SkipRulesConfig::default()
    }
}

fn max_chars_rule(limit: usize) -> SkipRulesConfig {
    SkipRulesConfig {
        max_chars: Some(limit),
        ..SkipRulesConfig::default()
    }
}

fn repeat_rule() -> SkipRulesConfig {
    SkipRulesConfig {
        repeat_of_recent: true,
        ..SkipRulesConfig::default()
    }
}

#[test]
fn each_skip_rule_judges_the_raw_the_spoken_and_the_transformed_form() {
    let mut emote_tokens = with_rules(emote_only_rule());
    emote_tokens.emote_tokens = EmoteTokenSet {
        tokens: ["LUL".to_owned()].into_iter().collect(),
    };
    for (case, config, input, rule) in [
        (
            "raw url",
            with_rules(url_rule()),
            "see https://a.example",
            SkipRule::ContainsUrl,
        ),
        (
            "url split by a soft hyphen",
            with_rules(url_rule()),
            "see htt\u{AD}ps://a.example",
            SkipRule::ContainsUrl,
        ),
        (
            "url only the spoken form keeps once punctuation collapses",
            sanitizing(url_rule()),
            "see htt\u{AD}ps://a.example",
            SkipRule::ContainsUrl,
        ),
        (
            "url split by a zero width space",
            with_rules(url_rule()),
            "see https:/\u{200B}/a.example",
            SkipRule::ContainsUrl,
        ),
        (
            "url completed by a colon emote",
            colon_emotes_to_words(url_rule()),
            "see :https:://a.example",
            SkipRule::ContainsUrl,
        ),
        (
            "raw prefix",
            with_rules(prefix_rule()),
            "!cmd",
            SkipRule::StartsWithPrefix,
        ),
        (
            "prefix behind an invisible char",
            with_rules(prefix_rule()),
            "\u{200B}!cmd",
            SkipRule::StartsWithPrefix,
        ),
        (
            "prefix behind leading whitespace",
            with_rules(prefix_rule()),
            "\u{A0} !cmd",
            SkipRule::StartsWithPrefix,
        ),
        (
            "prefix behind a stripped emoji",
            stripping_emoji(prefix_rule()),
            "🎉!cmd",
            SkipRule::StartsWithPrefix,
        ),
        (
            "prefix behind a stripped emote",
            stripping_kappa(prefix_rule()),
            "Kappa !cmd",
            SkipRule::StartsWithPrefix,
        ),
        (
            "raw emotes",
            emote_tokens.clone(),
            "LUL :wave:",
            SkipRule::EmoteOnly,
        ),
        (
            "emote behind an invisible char",
            emote_tokens,
            "LUL\u{200B} LUL",
            SkipRule::EmoteOnly,
        ),
        (
            "emote completed by collapsed punctuation",
            sanitizing(emote_only_rule()),
            ":wave::",
            SkipRule::EmoteOnly,
        ),
        (
            "non latin once emotes are stripped",
            stripping_kappa(non_latin_rule()),
            "Kappa Kappa Привіт",
            SkipRule::MostlyNonLatin,
        ),
        (
            "double whitespace only exists in the raw form",
            with_rules(regex_rule(r"\s{2,}")),
            "a  b",
            SkipRule::CustomRegex,
        ),
        (
            "regex anchored past an invisible char",
            with_rules(regex_rule("^secret$")),
            "sec\u{200D}ret",
            SkipRule::CustomRegex,
        ),
        (
            "regex matching after collapsed punctuation",
            sanitizing(regex_rule("^a!b$")),
            "a!!b",
            SkipRule::CustomRegex,
        ),
    ] {
        assert_eq!(process(input, &config, &ctx()), skipped_by(rule), "{case}");
    }
}

#[test]
fn messages_no_form_of_which_matches_are_spoken() {
    for (case, config, input) in [
        (
            "scheme word without a url",
            with_rules(url_rule()),
            "https is a protocol",
        ),
        (
            "prefix inside the message",
            with_rules(prefix_rule()),
            "wow !cmd",
        ),
        (
            "emoji in front of the prefix is spoken",
            with_rules(prefix_rule()),
            "🎉!cmd",
        ),
        (
            "regex matching none of the forms",
            with_rules(regex_rule("^secret$")),
            "secret word",
        ),
    ] {
        assert_eq!(process(input, &config, &ctx()), speak(input), "{case}");
    }
}

#[test]
fn invisible_hangul_fillers_do_not_count_as_non_latin_letters() {
    let config = with_rules(non_latin_rule());
    for filler in ['\u{3164}', '\u{FFA0}', '\u{115F}', '\u{1160}'] {
        let input = format!("ok{filler}{filler}{filler}{filler}");
        assert_eq!(
            process(&input, &config, &ctx()),
            speak(&input),
            "filler {filler:?}"
        );
    }
}

#[test]
fn real_hangul_still_counts_as_non_latin_letters() {
    assert_eq!(
        process("안녕하세요 ok", &with_rules(non_latin_rule()), &ctx()),
        skipped_by(SkipRule::MostlyNonLatin)
    );
}

#[test]
fn max_chars_counts_the_spoken_length_at_the_boundary() {
    for (limit, strip_emoji, input, skipped) in [
        (5, false, "12345", false),
        (5, false, "123456", true),
        (4, false, "12345", true),
        (5, false, "123\u{200B}45", false),
        (5, false, "123\u{200B}456", true),
        (5, false, "\u{FEFF}12345\u{200D}", false),
        (5, false, "  12345  ", false),
        (5, false, "1   345", false),
        (5, false, "1   3456", true),
        (6, false, "привіт", false),
        (5, false, "привіт", true),
        (5, true, "🎉12345", false),
        (5, false, "🎉12345", true),
    ] {
        let mut config = with_rules(max_chars_rule(limit));
        config.output.strip_emoji = strip_emoji;
        assert_eq!(
            process(input, &config, &ctx()) == skipped_by(SkipRule::ExceedsMaxLength),
            skipped,
            "input {input:?}, limit {limit}, strip emoji {strip_emoji}"
        );
    }
}

#[test]
fn a_repeat_is_judged_on_what_was_and_would_be_spoken() {
    for (case, config, recent, input, repeated) in [
        (
            "zero width space inside the word",
            with_rules(repeat_rule()),
            "hello world",
            "hel\u{200B}lo world",
            true,
        ),
        (
            "invisible char in the remembered message",
            with_rules(repeat_rule()),
            "hel\u{200D}lo world",
            "hello world",
            true,
        ),
        (
            "other whitespace between the same words",
            with_rules(repeat_rule()),
            "hello world",
            "\u{FEFF}hello\u{A0}\u{A0}world",
            true,
        ),
        (
            "stripped emote",
            stripping_kappa(repeat_rule()),
            "hello world",
            "Kappa hello world",
            true,
        ),
        (
            "collapsed punctuation",
            sanitizing(repeat_rule()),
            "hello world!",
            "hello world!!",
            true,
        ),
        (
            "stripped emoji",
            stripping_emoji(repeat_rule()),
            "hello",
            "hel🎉lo",
            true,
        ),
        (
            "emoji still spoken",
            with_rules(repeat_rule()),
            "hello",
            "hel🎉lo",
            false,
        ),
        (
            "case differs",
            with_rules(repeat_rule()),
            "hello world",
            "HELLO WORLD",
            false,
        ),
        (
            "different text",
            with_rules(repeat_rule()),
            "hello world",
            "hello worlds",
            false,
        ),
    ] {
        let recent = vec![recent.to_owned()];
        let context = PipelineContext {
            recent_messages: &recent,
            ..ctx()
        };
        assert_eq!(
            process(input, &config, &context) == skipped_by(SkipRule::RepeatsRecentMessage),
            repeated,
            "{case}"
        );
    }
}

#[test]
fn a_message_with_nothing_to_speak_is_never_a_repeat() {
    for (case, config, recent, input) in [
        (
            "both invisible",
            with_rules(repeat_rule()),
            "\u{200D}",
            "\u{200B}",
        ),
        ("both whitespace", with_rules(repeat_rule()), "   ", "  "),
        (
            "both stripped emoji",
            stripping_emoji(repeat_rule()),
            "🎉",
            "🎊",
        ),
        (
            "invisible against a stripped emote",
            stripping_kappa(repeat_rule()),
            "\u{200B}",
            "Kappa",
        ),
    ] {
        let recent = vec![recent.to_owned()];
        let context = PipelineContext {
            recent_messages: &recent,
            ..ctx()
        };
        assert_ne!(
            process(input, &config, &context),
            skipped_by(SkipRule::RepeatsRecentMessage),
            "{case}"
        );
    }
}
