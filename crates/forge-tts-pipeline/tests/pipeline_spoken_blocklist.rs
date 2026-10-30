#![allow(clippy::unwrap_used)]
mod common;

use common::neutral_config;
use forge_tts_pipeline::{
    AppliedReplacement, BlocklistMode, EmoteTokenSet, EmoteTreatment, PipelineConfig,
    PipelineContext, PipelineResult, ReplacementOrigin, ReplacementRule, SkipReason, StageAction,
    StageName, StageOutcome, preview, process, process_for_language,
};

const BOTH_MODES: [BlocklistMode; 2] = [BlocklistMode::Censor, BlocklistMode::SkipMessage];
const BOTH_TOGGLES: [bool; 2] = [false, true];

const BLOCKED: PipelineResult = PipelineResult::Skip {
    reason: SkipReason::BlockedByWordFilter,
};

fn ctx() -> PipelineContext<'static> {
    PipelineContext {
        viewer_name: "viewer",
        recent_messages: &[],
        speech_language: None,
    }
}

fn blocklist(entries: &[&str], mode: BlocklistMode, strip_emoji: bool) -> PipelineConfig {
    let mut config = PipelineConfig {
        word_blocklist: entries.iter().map(|entry| (*entry).to_owned()).collect(),
        blocklist_mode: mode,
        ..neutral_config()
    };
    config.output.strip_emoji = strip_emoji;
    config
}

fn speak(text: &str) -> PipelineResult {
    PipelineResult::Speak(text.to_owned())
}

fn stripping_twitch_emotes(entries: &[&str]) -> PipelineConfig {
    let mut config = blocklist(entries, BlocklistMode::Censor, false);
    config.output.emote_treatment = EmoteTreatment::StripTwitch;
    config.emote_tokens = EmoteTokenSet {
        tokens: ["Kappa".to_owned()].into_iter().collect(),
    };
    config
}

fn colon_emotes_to_words(entries: &[&str]) -> PipelineConfig {
    let mut config = blocklist(entries, BlocklistMode::Censor, false);
    config.output.emote_treatment = EmoteTreatment::ToWord;
    config
}

fn sanitizing(entries: &[&str]) -> PipelineConfig {
    let mut config = blocklist(entries, BlocklistMode::Censor, false);
    config.output.sanitize_punctuation = true;
    config
}

fn stripping_emoji_and_sanitizing(entries: &[&str]) -> PipelineConfig {
    let mut config = sanitizing(entries);
    config.output.strip_emoji = true;
    config
}

fn stage(outcomes: &[StageOutcome], name: StageName) -> &StageOutcome {
    outcomes
        .iter()
        .find(|outcome| outcome.stage == name)
        .unwrap()
}

fn blocked_mark(
    output_range: std::ops::Range<usize>,
    original: &str,
    entry_index: usize,
) -> AppliedReplacement {
    AppliedReplacement {
        output_range,
        original: original.to_owned(),
        replacement: "[beep]".to_owned(),
        origin: ReplacementOrigin::BlockedWordIndex(entry_index),
    }
}

#[test]
fn an_invisible_char_inside_a_word_cannot_hide_it_in_either_mode_or_toggle_state() {
    for invisible in [
        '\u{200D}',
        '\u{200B}',
        '\u{00AD}',
        '\u{E0041}',
        '\u{FEFF}',
        '\u{FE0F}',
        '\u{3164}',
        '\u{2800}',
    ] {
        let input = format!("say ba{invisible}d now");
        for strip_emoji in BOTH_TOGGLES {
            let censoring = blocklist(&["bad"], BlocklistMode::Censor, strip_emoji);
            assert_eq!(
                process(&input, &censoring, &ctx()),
                speak("say [beep] now"),
                "input {input:?}, strip emoji {strip_emoji}"
            );
            let skipping = blocklist(&["bad"], BlocklistMode::SkipMessage, strip_emoji);
            assert_eq!(
                process(&input, &skipping, &ctx()),
                BLOCKED,
                "input {input:?}, strip emoji {strip_emoji}"
            );
        }
    }
}

#[test]
fn word_boundaries_are_judged_across_invisible_chars() {
    for (input, spoken_with_emoji, spoken_without_emoji, blocked) in [
        ("x\u{200D}bad", "x\u{200D}bad", "xbad", false),
        ("bad\u{200D}x", "bad\u{200D}x", "badx", false),
        (
            "\u{200D}bad\u{200D}!",
            "\u{200D}[beep]\u{200D}!",
            "[beep]!",
            true,
        ),
        ("x\u{3164}bad", "x\u{3164}bad", "x\u{3164}bad", false),
        ("bad\u{3164}x", "bad\u{3164}x", "bad\u{3164}x", false),
        ("\u{3164}bad", "\u{3164}[beep]", "\u{3164}[beep]", true),
    ] {
        for (strip_emoji, expected) in [(false, spoken_with_emoji), (true, spoken_without_emoji)] {
            let censoring = blocklist(&["bad"], BlocklistMode::Censor, strip_emoji);
            assert_eq!(
                process(input, &censoring, &ctx()),
                speak(expected),
                "input {input:?}, strip emoji {strip_emoji}"
            );
            let skipping = blocklist(&["bad"], BlocklistMode::SkipMessage, strip_emoji);
            assert_eq!(
                process(input, &skipping, &ctx()) == BLOCKED,
                blocked,
                "input {input:?}, strip emoji {strip_emoji}"
            );
        }
    }
}

#[test]
fn whitespace_in_an_entry_matches_any_unicode_whitespace_run() {
    for separator in [
        " ",
        "  ",
        "\u{A0}",
        "\u{2003}",
        "\t",
        "\n",
        "\u{3000}\u{A0}",
        " \u{200B} ",
        "\u{200B} ",
        " \u{200B}",
    ] {
        let input = format!("a bad{separator}word b");
        assert_eq!(
            process(
                &input,
                &blocklist(&["bad word"], BlocklistMode::Censor, false),
                &ctx()
            ),
            speak("a [beep] b"),
            "separator {separator:?}"
        );
        assert_eq!(
            process(
                &input,
                &blocklist(&["bad word"], BlocklistMode::SkipMessage, false),
                &ctx()
            ),
            BLOCKED,
            "separator {separator:?}"
        );
    }
}

#[test]
fn a_phrase_entry_still_needs_its_words_whole_and_separated() {
    for input in ["bad words", "abad word", "badword", "bad\u{200B}word"] {
        for mode in BOTH_MODES {
            assert_eq!(
                process(input, &blocklist(&["bad word"], mode, false), &ctx()),
                speak(input),
                "input {input:?}"
            );
        }
    }
}

#[test]
fn entries_are_normalised_to_the_form_they_are_spoken_in() {
    for (entry, input) in [
        ("ba\u{200D}d", "bad"),
        ("\u{FEFF}bad", "bad"),
        (" bad ", "bad"),
        ("bad  word", "bad word"),
        ("bad\u{A0}word", "bad\tword"),
    ] {
        let censoring = blocklist(&[entry], BlocklistMode::Censor, false);
        assert_eq!(
            process(input, &censoring, &ctx()),
            speak("[beep]"),
            "entry {entry:?}"
        );
        let skipping = blocklist(&[entry], BlocklistMode::SkipMessage, false);
        assert_eq!(
            process(input, &skipping, &ctx()),
            BLOCKED,
            "entry {entry:?}"
        );
    }
}

#[test]
fn an_entry_empty_after_normalisation_matches_nothing() {
    let entries = ["\u{200B}", "   ", "\u{FE0F}\u{200D}"];
    for input in ["hello", "\u{200B}hello", "a  b"] {
        for mode in BOTH_MODES {
            assert_eq!(
                process(input, &blocklist(&entries, mode, false), &ctx()),
                speak(input),
                "input {input:?}"
            );
        }
    }
}

#[test]
fn an_empty_entry_keeps_its_slot_in_the_index_a_mark_reports() {
    let config = blocklist(&["\u{200B}", "bad"], BlocklistMode::Censor, false);
    let (_, outcomes) = preview("bad", &config, &ctx());
    assert_eq!(
        stage(&outcomes, StageName::WordBlocklist).replacements,
        vec![blocked_mark(0..6, "bad", 1)]
    );
}

#[test]
fn emoji_entries_match_with_or_without_their_variation_selector() {
    for (entry, input, strip_emoji, expected) in [
        ("🎉", "a 🎉 b", true, "a [beep] b"),
        ("🎉", "a 🎉 b", false, "a [beep] b"),
        ("❤\u{FE0F}", "i ❤\u{FE0F} you", true, "i [beep] you"),
        (
            "❤\u{FE0F}",
            "i ❤\u{FE0F} you",
            false,
            "i [beep]\u{FE0F} you",
        ),
        ("❤\u{FE0F}", "i ❤ you", false, "i [beep] you"),
        ("❤", "i ❤\u{FE0F} you", true, "i [beep] you"),
        ("❤", "i ❤\u{FE0F} you", false, "i [beep]\u{FE0F} you"),
    ] {
        let censoring = blocklist(&[entry], BlocklistMode::Censor, strip_emoji);
        assert_eq!(
            process(input, &censoring, &ctx()),
            speak(expected),
            "entry {entry:?}, input {input:?}, strip emoji {strip_emoji}"
        );
        let skipping = blocklist(&[entry], BlocklistMode::SkipMessage, strip_emoji);
        assert_eq!(
            process(input, &skipping, &ctx()),
            BLOCKED,
            "entry {entry:?}, input {input:?}, strip emoji {strip_emoji}"
        );
    }
}

#[test]
fn an_emoji_hides_a_match_only_while_it_is_spoken() {
    for (entry, input, strip_emoji, spoken, blocked) in [
        ("bad", "ba🎉d", true, "[beep]", true),
        ("bad", "ba🎉d", false, "ba🎉d", false),
        ("bad word", "bad 🎉 word", true, "[beep]", true),
        ("bad word", "bad 🎉 word", false, "bad 🎉 word", false),
        ("bad", "bad🎉x", true, "badx", false),
        ("bad", "bad🎉x", false, "[beep]🎉x", true),
        ("bad", "x🎉bad", true, "xbad", false),
        ("bad", "x🎉bad", false, "x🎉[beep]", true),
    ] {
        let censoring = blocklist(&[entry], BlocklistMode::Censor, strip_emoji);
        assert_eq!(
            process(input, &censoring, &ctx()),
            speak(spoken),
            "input {input:?}, strip emoji {strip_emoji}"
        );
        let skipping = blocklist(&[entry], BlocklistMode::SkipMessage, strip_emoji);
        assert_eq!(
            process(input, &skipping, &ctx()) == BLOCKED,
            blocked,
            "input {input:?}, strip emoji {strip_emoji}"
        );
    }
}

fn late_match_cases() -> Vec<(&'static str, PipelineConfig, &'static str, &'static str)> {
    vec![
        (
            "emote stripping joins a phrase",
            stripping_twitch_emotes(&["bad word", "gtfo"]),
            "a bad Kappa word b",
            "a [beep] b",
        ),
        (
            "emote stripping beside an early match",
            stripping_twitch_emotes(&["bad word", "gtfo"]),
            "gtfo a bad Kappa word",
            "[beep] a [beep]",
        ),
        (
            "colon emotes join a word",
            colon_emotes_to_words(&["badword", "gtfo"]),
            "say :bad::word: now",
            "say [beep] now",
        ),
        (
            "colon emotes beside an early match",
            colon_emotes_to_words(&["badword", "gtfo"]),
            ":bad::word: gtfo",
            "[beep] [beep]",
        ),
        (
            "doubled punctuation collapses into an entry",
            sanitizing(&["a!b", "gtfo"]),
            "x a!!b y",
            "x [beep] y",
        ),
        (
            "collapsed punctuation beside an early match",
            sanitizing(&["a!b", "gtfo"]),
            "a!!!b gtfo",
            "[beep] [beep]",
        ),
        (
            "stripped emoji leave punctuation to collapse",
            stripping_emoji_and_sanitizing(&["a!b", "gtfo"]),
            "a🎉!🎉!b",
            "[beep]",
        ),
        (
            "stripped emoji beside an early match",
            stripping_emoji_and_sanitizing(&["a!b", "gtfo"]),
            "gtfo a🎉!🎉!b",
            "[beep] [beep]",
        ),
    ]
}

#[test]
fn a_phrase_appearing_only_after_output_transforms_is_censored() {
    for (case, config, input, expected) in late_match_cases() {
        assert_eq!(process(input, &config, &ctx()), speak(expected), "{case}");
    }
}

#[test]
fn a_phrase_appearing_only_after_output_transforms_skips_the_message() {
    for (case, mut config, input, _) in late_match_cases() {
        config.blocklist_mode = BlocklistMode::SkipMessage;
        assert_eq!(process(input, &config, &ctx()), BLOCKED, "{case}");
    }
}

#[test]
fn a_late_skip_is_reported_at_the_blocklist_stage_of_the_preview() {
    let mut config = colon_emotes_to_words(&["badword"]);
    config.blocklist_mode = BlocklistMode::SkipMessage;
    let (result, outcomes) = preview(":bad::word:", &config, &ctx());
    assert_eq!(result, BLOCKED);
    assert_eq!(
        stage(&outcomes, StageName::WordBlocklist).action,
        StageAction::Skipped {
            reason: SkipReason::BlockedByWordFilter
        }
    );
}

#[test]
fn a_late_mark_names_the_transformed_fragment_and_ranges_over_the_stage_output() {
    for (case, config, input, stage_output, expected) in [
        (
            "colon emotes",
            colon_emotes_to_words(&["gtfo", "badword"]),
            "say :bad::word: now",
            "say [beep] now",
            vec![blocked_mark(4..10, "badword", 1)],
        ),
        (
            "emote stripping",
            stripping_twitch_emotes(&["bad word"]),
            "a bad Kappa word b",
            "a [beep] b",
            vec![blocked_mark(2..8, "bad word", 0)],
        ),
        (
            "doubled punctuation",
            sanitizing(&["a!b"]),
            "x a!!b y",
            "x [beep] y",
            vec![blocked_mark(2..8, "a!b", 0)],
        ),
        (
            "cyrillic before the mark counts in bytes",
            stripping_twitch_emotes(&["дурень слово"]),
            "дякую дурень Kappa слово",
            "дякую [beep]",
            vec![blocked_mark(11..17, "дурень слово", 0)],
        ),
        (
            "early mark compressed by a stripped emote",
            stripping_twitch_emotes(&["гидота", "дурень слово"]),
            "Kappa гидота дурень Kappa слово",
            "[beep] [beep]",
            vec![
                blocked_mark(0..6, "гидота", 0),
                blocked_mark(7..13, "дурень слово", 1),
            ],
        ),
        (
            "early mark compressed by collapsed punctuation",
            sanitizing(&["gtfo", "a!b"]),
            "wow!!! gtfo a!!b",
            "wow! [beep] [beep]",
            vec![
                blocked_mark(5..11, "gtfo", 0),
                blocked_mark(12..18, "a!b", 1),
            ],
        ),
        (
            "early mark compressed by colon delimiters",
            colon_emotes_to_words(&["gtfo", "badword"]),
            ":hi: gtfo :bad::word:",
            "hi [beep] [beep]",
            vec![
                blocked_mark(3..9, "gtfo", 0),
                blocked_mark(10..16, "badword", 1),
            ],
        ),
    ] {
        let (_, outcomes) = preview(input, &config, &ctx());
        let blocklist_stage = stage(&outcomes, StageName::WordBlocklist);
        assert_eq!(blocklist_stage.output, stage_output, "{case}");
        assert_eq!(blocklist_stage.replacements, expected, "{case}");
    }
}

#[test]
fn a_late_match_transforms_the_blocklist_stage_and_leaves_output_nothing_to_do() {
    for (case, config, input, _) in late_match_cases() {
        let (_, outcomes) = preview(input, &config, &ctx());
        assert_eq!(
            stage(&outcomes, StageName::WordBlocklist).action,
            StageAction::Transformed,
            "{case}"
        );
        assert_eq!(
            stage(&outcomes, StageName::Output).action,
            StageAction::PassedThrough,
            "{case}"
        );
    }
}

#[test]
fn without_a_late_match_the_stage_keeps_the_text_for_output_to_transform() {
    for (case, config, input) in [
        (
            "emote token",
            stripping_twitch_emotes(&["bad word"]),
            "fine Kappa text",
        ),
        (
            "colon emote",
            colon_emotes_to_words(&["bad word"]),
            "fine :wave: text",
        ),
        ("doubled punctuation", sanitizing(&["a!b"]), "wow!! text"),
    ] {
        let (_, outcomes) = preview(input, &config, &ctx());
        let blocklist_stage = stage(&outcomes, StageName::WordBlocklist);
        assert_eq!(blocklist_stage.output, input, "{case}");
        assert_eq!(blocklist_stage.action, StageAction::PassedThrough, "{case}");
        assert_eq!(
            stage(&outcomes, StageName::Output).action,
            StageAction::Transformed,
            "{case}"
        );
    }
}

#[test]
fn an_early_match_alone_leaves_output_transforms_to_the_output_stage() {
    let config = colon_emotes_to_words(&["gtfo"]);
    let (result, outcomes) = preview("gtfo :wave:", &config, &ctx());
    assert_eq!(
        stage(&outcomes, StageName::WordBlocklist).output,
        "[beep] :wave:"
    );
    assert_eq!(result, speak("[beep] wave"));
}

#[test]
fn a_blocked_word_equal_to_the_censor_word_does_not_censor_the_censor_token() {
    let config = colon_emotes_to_words(&["bad", "beep"]);
    for (input, expected) in [
        ("bad", "[beep]"),
        ("beep", "[beep]"),
        (":ba::d: bad", "[beep] [beep]"),
    ] {
        assert_eq!(
            process(input, &config, &ctx()),
            speak(expected),
            "input {input:?}"
        );
    }
}

#[test]
fn the_name_prefix_is_not_judged() {
    let context = PipelineContext {
        viewer_name: "bad",
        ..ctx()
    };
    for mode in BOTH_MODES {
        let mut config = colon_emotes_to_words(&["bad"]);
        config.blocklist_mode = mode;
        config.output.read_display_name_first = true;
        assert_eq!(
            process("hello", &config, &context),
            speak("bad says: hello")
        );
    }
}

#[test]
fn replacement_output_is_not_censored() {
    for mode in BOTH_MODES {
        let mut config = colon_emotes_to_words(&["bad"]);
        config.blocklist_mode = mode;
        config.replacement_rules = vec![ReplacementRule::Text {
            pattern: "hello".into(),
            replacement: "bad".into(),
        }];
        assert_eq!(process("hello there", &config, &ctx()), speak("bad there"));
    }
}

#[test]
fn process_preview_and_language_processing_agree() {
    let entries = ["bad", "bad word", "a!b", "🎉", "badword"];
    let inputs = [
        "plain text",
        "ba\u{200B}d",
        "x\u{200D}bad",
        "bad\u{A0}word",
        "bad Kappa word",
        ":bad::word:",
        "a!!b",
        "a🎉!🎉!b",
        "ba🎉d",
        "🎉",
        "\u{200B}",
        "gtfo :ba::d: дякую",
        "\u{7}",
        "ba\u{7F}d",
        "[:ba::d:]",
        "[bad a!!b",
        "wow🎉",
    ];
    let treatments = [
        EmoteTreatment::Keep,
        EmoteTreatment::StripTwitch,
        EmoteTreatment::ToWord,
    ];
    for mode in BOTH_MODES {
        for treatment in treatments {
            for strip_emoji in BOTH_TOGGLES {
                for sanitize_punctuation in BOTH_TOGGLES {
                    let mut config = stripping_twitch_emotes(&entries);
                    config.blocklist_mode = mode.clone();
                    config.output.emote_treatment = treatment;
                    config.output.strip_emoji = strip_emoji;
                    config.output.sanitize_punctuation = sanitize_punctuation;
                    for input in inputs {
                        let processed = process(input, &config, &ctx());
                        let (previewed, _) = preview(input, &config, &ctx());
                        assert_eq!(previewed, processed, "input {input:?}, {config:?}");
                        let spoken = match processed {
                            PipelineResult::Speak(spoken) => Some(spoken),
                            PipelineResult::Skip { .. } => None,
                        };
                        assert_eq!(
                            process_for_language(input, &config, &ctx()),
                            spoken,
                            "input {input:?}, {config:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn control_chars_inside_a_word_cannot_hide_it() {
    for control in [
        '\u{0}', '\u{7}', '\u{1B}', '\u{1F}', '\u{7F}', '\u{80}', '\u{9F}',
    ] {
        let input = format!("say ba{control}d now");
        assert_eq!(
            process(
                &input,
                &blocklist(&["bad"], BlocklistMode::Censor, false),
                &ctx()
            ),
            speak("say [beep] now"),
            "control {control:?}"
        );
        assert_eq!(
            process(
                &input,
                &blocklist(&["bad"], BlocklistMode::SkipMessage, false),
                &ctx()
            ),
            BLOCKED,
            "control {control:?}"
        );
    }
}

#[test]
fn word_boundaries_are_judged_across_control_chars() {
    for (input, expected) in [
        ("x\u{7}bad", "x\u{7}bad"),
        ("bad\u{7F}x", "bad\u{7F}x"),
        ("\u{7}bad\u{80}!", "\u{7}[beep]\u{80}!"),
    ] {
        assert_eq!(
            process(
                input,
                &blocklist(&["bad"], BlocklistMode::Censor, false),
                &ctx()
            ),
            speak(expected),
            "input {input:?}"
        );
    }
}

#[test]
fn whitespace_controls_stay_whitespace() {
    for (input, entry, expected) in [
        ("ba\td", "bad", "ba\td"),
        ("ba\nd", "bad", "ba\nd"),
        ("ba\u{85}d", "bad", "ba\u{85}d"),
        ("x\tbad", "bad", "x\t[beep]"),
        ("bad\nx", "bad", "[beep]\nx"),
        ("bad\u{7}\tword", "bad word", "[beep]"),
    ] {
        assert_eq!(
            process(
                input,
                &blocklist(&[entry], BlocklistMode::Censor, false),
                &ctx()
            ),
            speak(expected),
            "input {input:?}"
        );
    }
}

#[test]
fn an_entry_of_control_chars_matches_nothing() {
    for mode in BOTH_MODES {
        assert_eq!(
            process("hello", &blocklist(&["\u{7}\u{7F}"], mode, false), &ctx()),
            speak("hello")
        );
    }
}

#[test]
fn a_message_of_only_unspoken_chars_ends_empty() {
    let empty = PipelineResult::Skip {
        reason: SkipReason::EmptyAfterProcessing,
    };
    for read_name in BOTH_TOGGLES {
        for input in [
            "\u{200B}",
            "\u{200D}\u{FE0F}",
            " \u{3164} \u{2800} ",
            "\u{7}",
            "\u{7F}\t\u{80}",
        ] {
            let mut config = neutral_config();
            config.output.read_display_name_first = read_name;
            assert_eq!(
                process(input, &config, &ctx()),
                empty,
                "input {input:?}, read name {read_name}"
            );
            assert_eq!(
                preview(input, &config, &ctx()).0,
                empty,
                "input {input:?}, read name {read_name}"
            );
        }
        let mut stripping = blocklist(&[], BlocklistMode::Censor, true);
        stripping.output.read_display_name_first = read_name;
        assert_eq!(process("🎉\u{200D}", &stripping, &ctx()), empty);
        assert_eq!(preview("🎉\u{200D}", &stripping, &ctx()).0, empty);
    }
}

#[test]
fn a_visible_char_among_unspoken_chars_keeps_the_message_and_its_text() {
    let config = neutral_config();
    for input in ["\u{200B}a", "!\u{7}", "\u{200D}🎉"] {
        assert_eq!(
            process(input, &config, &ctx()),
            speak(input),
            "input {input:?}"
        );
        assert_eq!(
            preview(input, &config, &ctx()).0,
            speak(input),
            "input {input:?}"
        );
    }
}

#[test]
fn a_late_match_assembling_a_literal_censor_token_is_still_judged() {
    let censoring = colon_emotes_to_words(&["beep"]);
    assert_eq!(process("[:be::ep:]", &censoring, &ctx()), speak("[[beep]]"));
    let (previewed, outcomes) = preview("[:be::ep:]", &censoring, &ctx());
    assert_eq!(previewed, speak("[[beep]]"));
    assert_eq!(
        stage(&outcomes, StageName::WordBlocklist).replacements,
        vec![blocked_mark(1..7, "beep", 0)]
    );

    let mut skipping = colon_emotes_to_words(&["beep"]);
    skipping.blocklist_mode = BlocklistMode::SkipMessage;
    assert_eq!(process("[:be::ep:]", &skipping, &ctx()), BLOCKED);
    assert_eq!(preview("[:be::ep:]", &skipping, &ctx()).0, BLOCKED);
}

#[test]
fn a_late_match_on_the_literal_token_entry_keeps_its_mark() {
    let censoring = colon_emotes_to_words(&["[beep]"]);
    let (previewed, outcomes) = preview("[:be::ep:]", &censoring, &ctx());
    assert_eq!(previewed, speak("[beep]"));
    assert_eq!(
        stage(&outcomes, StageName::WordBlocklist).replacements,
        vec![blocked_mark(0..6, "[beep]", 0)]
    );
}

#[test]
fn forge_own_token_is_never_rechecked_even_beside_a_late_match() {
    for (config, input, expected) in [
        (colon_emotes_to_words(&["bad", "beep"]), "bad", "[beep]"),
        (
            colon_emotes_to_words(&["bad", "beep"]),
            "[bad] :be::ep:",
            "[[beep]] [beep]",
        ),
        (sanitizing(&["bad", "beep"]), "[bad", "[beep]"),
        (
            sanitizing(&["bad", "beep", "a!b"]),
            "[bad a!!b",
            "[beep] [beep]",
        ),
    ] {
        assert_eq!(
            process(input, &config, &ctx()),
            speak(expected),
            "input {input:?}"
        );
    }
}

#[test]
fn an_edge_without_word_chars_needs_no_word_boundary() {
    for (entry, input, spoken, blocked) in [
        ("🎉", "wow🎉", "wow[beep]", true),
        ("🎉", "🎉wow", "[beep]wow", true),
        ("🎉", "wo🎉w", "wo[beep]w", true),
        ("!!", "a!!b", "a[beep]b", true),
        ("#tag", "x#tag", "x[beep]", true),
        ("#tag", "x#tagged", "x#tagged", false),
        ("tag#", "tag#x", "[beep]x", true),
        ("tag#", "xtag#", "xtag#", false),
        ("bad", "wowbad", "wowbad", false),
        ("bad", "badwow", "badwow", false),
    ] {
        assert_eq!(
            process(
                input,
                &blocklist(&[entry], BlocklistMode::Censor, false),
                &ctx()
            ),
            speak(spoken),
            "entry {entry:?}, input {input:?}"
        );
        assert_eq!(
            process(
                input,
                &blocklist(&[entry], BlocklistMode::SkipMessage, false),
                &ctx()
            ) == BLOCKED,
            blocked,
            "entry {entry:?}, input {input:?}"
        );
    }
}

#[test]
fn the_longest_entry_wins_with_each_entry_judging_its_own_boundaries() {
    let config = blocklist(&["🎉", "🎉 party"], BlocklistMode::Censor, false);
    assert_eq!(
        process("wow🎉 party time", &config, &ctx()),
        speak("wow[beep] time")
    );
    assert_eq!(
        process("wow🎉 partying", &config, &ctx()),
        speak("wow[beep] partying")
    );
}

#[test]
fn a_mark_slices_exactly_the_token_when_sanitizing_collapses_its_brackets() {
    for (case, config, input, expected_output, expected_marks) in [
        (
            "bracket before the token",
            sanitizing(&["gtfo"]),
            "[gtfo",
            "[beep]",
            vec![(0, 6)],
        ),
        (
            "bracket after the token",
            sanitizing(&["gtfo"]),
            "gtfo]",
            "[beep]",
            vec![(0, 6)],
        ),
        (
            "brackets on both sides",
            sanitizing(&["gtfo"]),
            "[gtfo]",
            "[beep]",
            vec![(0, 6)],
        ),
        (
            "a longer bracket run",
            sanitizing(&["gtfo"]),
            "x [[[gtfo]]] y",
            "x [beep] y",
            vec![(2, 8)],
        ),
        (
            "bracket before the token beside a late match",
            sanitizing(&["gtfo", "a!b"]),
            "[gtfo a!!b",
            "[beep] [beep]",
            vec![(0, 6), (7, 13)],
        ),
        (
            "late token inside brackets",
            sanitizing(&["a!b"]),
            "[a!!b]",
            "[beep]",
            vec![(0, 6)],
        ),
    ] {
        let (result, outcomes) = preview(input, &config, &ctx());
        assert_eq!(result, speak(expected_output), "{case}");
        let output_stage = stage(&outcomes, StageName::Output);
        let marks: Vec<_> = output_stage
            .replacements
            .iter()
            .map(|mark| (mark.output_range.start, mark.output_range.end))
            .collect();
        assert_eq!(marks, expected_marks, "{case}");
        for (start, end) in marks {
            assert_eq!(&output_stage.output[start..end], "[beep]", "{case}");
        }
    }
}

#[test]
fn collapsing_punctuation_keeps_the_spoken_text_unchanged_around_a_run() {
    let config = sanitizing(&[]);
    for (input, expected) in [
        ("wow!!!", "wow!"),
        ("a!?!?b", "a!?!?b"),
        ("...", "."),
        ("дякую!!", "дякую!"),
        ("aa  bb", "aa  bb"),
        ("__x__", "_x_"),
    ] {
        assert_eq!(
            process(input, &config, &ctx()),
            speak(expected),
            "input {input:?}"
        );
    }
}
