const SIGMA: char = '\u{3c3}';
const FINAL_SIGMA: char = '\u{3c2}';

pub fn command_phrase_matches(message: &str, phrase: &str, case_sensitive: bool) -> bool {
    let phrase = phrase.trim_end();
    if phrase.is_empty() {
        return false;
    }
    strip_phrase(message, phrase, case_sensitive)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
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
}
