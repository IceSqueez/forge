use forge_types::{
    COMMAND_ARGS_VARIABLE, COMMAND_NAME_VARIABLE, COMMAND_TAIL_VARIABLE, DeclaredVariable,
    SynthesisHint, TriggerConfig, Variant, VariantKind,
};

const PHRASE_KEY: &str = "phrase";
const CASE_SENSITIVE_KEY: &str = "case_sensitive";

const SIGMA: char = '\u{3c3}';
const FINAL_SIGMA: char = '\u{3c2}';

pub(crate) fn command_name_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: COMMAND_NAME_VARIABLE.to_owned(),
        kind: VariantKind::String,
        label: "Command name".to_owned(),
        synthesis: None,
    }
}

pub(crate) fn command_args_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: COMMAND_ARGS_VARIABLE.to_owned(),
        kind: VariantKind::Array,
        label: "Command arguments".to_owned(),
        synthesis: None,
    }
}

pub(crate) fn command_tail_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: COMMAND_TAIL_VARIABLE.to_owned(),
        kind: VariantKind::String,
        label: "Command arguments as one text".to_owned(),
        synthesis: Some(SynthesisHint::Message),
    }
}

pub(crate) struct MatchedCommand<'m> {
    name: String,
    tail: &'m str,
}

impl<'m> MatchedCommand<'m> {
    pub(crate) fn read(message: &'m str, config: &TriggerConfig) -> Self {
        let phrase = configured_phrase(config);
        let case_sensitive = matches!(config.get(CASE_SENSITIVE_KEY), Some(Variant::Bool(true)));
        match rest_after_phrase(message, phrase, case_sensitive) {
            Some(rest) => Self {
                name: command_name_of(phrase),
                tail: rest.trim(),
            },
            None => {
                let line = message.trim();
                let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
                Self {
                    name: command_name_of(word),
                    tail: rest.trim(),
                }
            }
        }
    }

    pub(crate) fn name(&self) -> Variant {
        Variant::String(self.name.clone())
    }

    pub(crate) fn tail(&self) -> Variant {
        Variant::String(self.tail.to_owned())
    }

    pub(crate) fn args(&self) -> Variant {
        Variant::Array(
            self.tail
                .split_whitespace()
                .map(|argument| Variant::String(argument.to_owned()))
                .collect(),
        )
    }
}

fn configured_phrase(config: &TriggerConfig) -> &str {
    match config.get(PHRASE_KEY) {
        Some(Variant::String(phrase)) => phrase.trim_end(),
        _ => "",
    }
}

pub fn command_name_of(phrase: &str) -> String {
    let word = phrase.split_whitespace().next().unwrap_or_default();
    let bare = word.trim_start_matches(|character: char| !character.is_alphanumeric());
    if bare.is_empty() { word } else { bare }.to_owned()
}

pub fn command_phrase_matches(message: &str, phrase: &str, case_sensitive: bool) -> bool {
    rest_after_phrase(message, phrase.trim_end(), case_sensitive).is_some()
}

fn rest_after_phrase<'m>(message: &'m str, phrase: &str, case_sensitive: bool) -> Option<&'m str> {
    if phrase.is_empty() {
        return None;
    }
    strip_phrase(message, phrase, case_sensitive)
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

fn folded(character: char) -> impl Iterator<Item = char> {
    character.to_lowercase().map(|lowered| {
        if lowered == FINAL_SIGMA {
            SIGMA
        } else {
            lowered
        }
    })
}

fn strip_phrase<'m>(message: &'m str, phrase: &str, case_sensitive: bool) -> Option<&'m str> {
    if case_sensitive {
        return message.strip_prefix(phrase);
    }
    let mut wanted = phrase.chars().flat_map(folded);
    let mut next_wanted = wanted.next();
    for (position, character) in message.char_indices() {
        if next_wanted.is_none() {
            return message.get(position..);
        }
        for lowered in folded(character) {
            if next_wanted != Some(lowered) {
                return None;
            }
            next_wanted = wanted.next();
        }
    }
    next_wanted.is_none().then_some("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSENSITIVE: bool = false;
    const SENSITIVE: bool = true;

    #[test]
    fn a_phrase_matches_a_whole_leading_word_at_any_word_end() {
        for message in [
            "!so",
            "!so viewer",
            "!so\tviewer",
            "!so\u{a0}viewer",
            "!so ",
        ] {
            assert!(
                command_phrase_matches(message, "!so", INSENSITIVE),
                "{message:?}"
            );
        }
    }

    #[test]
    fn a_phrase_never_matches_the_front_of_a_longer_word() {
        for (message, phrase) in [
            ("!sound", "!so"),
            ("!rollover", "!roll"),
            ("!so,viewer", "!so"),
            ("!so😀", "!so"),
        ] {
            for case_sensitive in [INSENSITIVE, SENSITIVE] {
                assert!(
                    !command_phrase_matches(message, phrase, case_sensitive),
                    "{message:?} vs {phrase:?}, case sensitive {case_sensitive}"
                );
            }
        }
    }

    #[test]
    fn the_longer_of_two_prefix_sharing_commands_matches_its_own_word() {
        assert!(command_phrase_matches(
            "!rollover 5",
            "!rollover",
            INSENSITIVE
        ));
        assert!(command_phrase_matches("!roll 5", "!roll", INSENSITIVE));
    }

    #[test]
    fn a_message_that_ends_inside_the_phrase_or_does_not_start_with_it_never_matches() {
        for (message, phrase) in [
            ("!s", "!so"),
            ("", "!so"),
            ("hey !so", "!so"),
            (" !so", "!so"),
        ] {
            assert!(
                !command_phrase_matches(message, phrase, INSENSITIVE),
                "{message:?} vs {phrase:?}"
            );
        }
    }

    #[test]
    fn case_only_differs_when_the_trigger_ignores_it() {
        for (message, phrase, case_sensitive, expected) in [
            ("!SO viewer", "!so", INSENSITIVE, true),
            ("!so viewer", "!SO", INSENSITIVE, true),
            ("!SO viewer", "!so", SENSITIVE, false),
            ("!so viewer", "!so", SENSITIVE, true),
            ("!ПРИВІТ друже", "!привіт", INSENSITIVE, true),
            ("!ПРИВІТ друже", "!привіт", SENSITIVE, false),
            ("!ΣΑΣ", "!σας", INSENSITIVE, true),
        ] {
            assert_eq!(
                command_phrase_matches(message, phrase, case_sensitive),
                expected,
                "{message:?} vs {phrase:?}, case sensitive {case_sensitive}"
            );
        }
    }

    #[test]
    fn case_folding_follows_unicode_lowering_without_expanding_sharp_s() {
        for (message, phrase, expected) in [
            ("!STRAẞE", "!straße", true),
            ("!straße", "!STRAẞE", true),
            ("!STRASSE", "!straße", false),
            ("!straße", "!strasse", false),
        ] {
            assert_eq!(
                command_phrase_matches(message, phrase, INSENSITIVE),
                expected,
                "{message:?} vs {phrase:?}"
            );
        }
    }

    #[test]
    fn a_character_lowering_to_two_must_match_both_of_them() {
        assert!(command_phrase_matches("!İ", "!i\u{307}", INSENSITIVE));
        assert!(command_phrase_matches("!İ now", "!i\u{307}", INSENSITIVE));
        assert!(!command_phrase_matches("!İ", "!i", INSENSITIVE));
    }

    #[test]
    fn trailing_whitespace_in_a_phrase_is_not_part_of_the_command() {
        for case_sensitive in [INSENSITIVE, SENSITIVE] {
            assert!(command_phrase_matches("!so", "!so ", case_sensitive));
            assert!(command_phrase_matches(
                "!so viewer",
                "!so  ",
                case_sensitive
            ));
            assert!(!command_phrase_matches("!sound", "!so ", case_sensitive));
        }
    }

    #[test]
    fn a_blank_phrase_matches_nothing() {
        for phrase in ["", "   ", "\t"] {
            for message in ["", "anything", "   "] {
                assert!(
                    !command_phrase_matches(message, phrase, INSENSITIVE),
                    "{message:?} vs {phrase:?}"
                );
            }
        }
    }

    fn trigger_config(phrase: Option<Variant>, case_sensitive: bool) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        if let Some(phrase) = phrase {
            config.insert(PHRASE_KEY.to_owned(), phrase);
        }
        config.insert(CASE_SENSITIVE_KEY.to_owned(), Variant::Bool(case_sensitive));
        config
    }

    fn phrase_config(phrase: &str, case_sensitive: bool) -> TriggerConfig {
        trigger_config(Some(Variant::String(phrase.to_owned())), case_sensitive)
    }

    fn assert_command(
        matched: &MatchedCommand<'_>,
        (name, tail, arguments): (&str, &str, &[&str]),
        case: &str,
    ) {
        assert_eq!(
            matched.name(),
            Variant::String(name.to_owned()),
            "name of {case}"
        );
        assert_eq!(
            matched.tail(),
            Variant::String(tail.to_owned()),
            "tail of {case}"
        );
        assert_eq!(
            matched.args(),
            Variant::Array(
                arguments
                    .iter()
                    .map(|argument| Variant::String((*argument).to_owned()))
                    .collect()
            ),
            "args of {case}"
        );
    }

    #[test]
    fn a_matched_phrase_is_removed_whole_and_only_its_first_word_names_the_command() {
        for (message, phrase, expected) in [
            (
                "!shout out alice bob",
                "!shout out",
                ("shout", "alice bob", &["alice", "bob"][..]),
            ),
            ("!shout out", "!shout out", ("shout", "", &[][..])),
            ("!so viewer", "!so", ("so", "viewer", &["viewer"][..])),
            ("!so\u{a0}viewer", "!so", ("so", "viewer", &["viewer"][..])),
            ("!so   a  b  ", "!so", ("so", "a  b", &["a", "b"][..])),
        ] {
            assert_command(
                &MatchedCommand::read(message, &phrase_config(phrase, SENSITIVE)),
                expected,
                message,
            );
        }
    }

    #[test]
    fn the_configured_spelling_names_the_command_whatever_case_the_chatter_typed() {
        for (message, phrase, expected) in [
            (
                "!SHOUT OUT Alice",
                "!shout out",
                ("shout", "Alice", &["Alice"][..]),
            ),
            ("!sO x", "!So", ("So", "x", &["x"][..])),
            (
                "!ПРИВІТ  світ   друже",
                "!привіт",
                ("привіт", "світ   друже", &["світ", "друже"][..]),
            ),
        ] {
            assert_command(
                &MatchedCommand::read(message, &phrase_config(phrase, INSENSITIVE)),
                expected,
                message,
            );
        }
    }

    #[test]
    fn every_leading_marker_is_dropped_from_the_name_and_a_markerless_phrase_keeps_its_word() {
        for (message, phrase, name) in [
            ("!lurk now", "!lurk", "lurk"),
            ("~lurk now", "~lurk", "lurk"),
            ("lurk now", "lurk", "lurk"),
            ("!!lurk now", "!!lurk", "lurk"),
        ] {
            assert_command(
                &MatchedCommand::read(message, &phrase_config(phrase, INSENSITIVE)),
                (name, "now", &["now"]),
                message,
            );
        }
    }

    #[test]
    fn trailing_space_in_the_phrase_is_ignored_when_the_tail_is_cut() {
        for phrase in ["!shout out ", "!shout out\t  "] {
            assert_command(
                &MatchedCommand::read("!shout out alice", &phrase_config(phrase, INSENSITIVE)),
                ("shout", "alice", &["alice"]),
                phrase,
            );
        }
    }

    #[test]
    fn a_line_the_phrase_does_not_cover_splits_at_its_first_word() {
        let missing = trigger_config(None, INSENSITIVE);
        let blank = phrase_config("   ", INSENSITIVE);
        let not_text = trigger_config(Some(Variant::Int(1)), INSENSITIVE);
        let other_case = phrase_config("!shout out", SENSITIVE);
        let longer_word = phrase_config("!shout", INSENSITIVE);
        for (message, config, expected) in [
            (
                "!SHOUT OUT alice",
                &missing,
                ("SHOUT", "OUT alice", &["OUT", "alice"][..]),
            ),
            (
                "!shout out alice",
                &blank,
                ("shout", "out alice", &["out", "alice"][..]),
            ),
            (
                "!shout out alice",
                &not_text,
                ("shout", "out alice", &["out", "alice"][..]),
            ),
            (
                "!SHOUT OUT alice",
                &other_case,
                ("SHOUT", "OUT alice", &["OUT", "alice"][..]),
            ),
            (
                "!shoutout alice",
                &longer_word,
                ("shoutout", "alice", &["alice"][..]),
            ),
            ("  !go now  ", &missing, ("go", "now", &["now"][..])),
            ("!", &missing, ("!", "", &[][..])),
            ("!!! now", &missing, ("!!!", "now", &["now"][..])),
            ("", &missing, ("", "", &[][..])),
            ("   ", &missing, ("", "", &[][..])),
        ] {
            assert_command(&MatchedCommand::read(message, config), expected, message);
        }
    }
}
