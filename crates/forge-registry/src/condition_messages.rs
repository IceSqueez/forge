use forge_types::{
    DatePatternError, MessageCatalog, MessageCode, MessageRef, TriggerConfig, UserText,
    VerbatimText,
};

const CONDITION_NAMESPACE: &str = "condition";

const COMMAND_PHRASE_KEY: &str = "phrase";

const FIRST_ARG: &str = "first";
const NEXT_ARG: &str = "next";
const PHRASE_ARG: &str = "phrase";
const POSITION_ARG: &str = "position";
const LETTER_ARG: &str = "letter";
const SPELLING_ARG: &str = "spelling";
const REPLACEMENT_ARG: &str = "replacement";

const ANY: MessageCode = MessageCode::from_static(CONDITION_NAMESPACE, "any");
const FIRST_TIME_CHATTERS: MessageCode =
    MessageCode::from_static(CONDITION_NAMESPACE, "first_time_chatters");
const COMMAND_PHRASE: MessageCode = MessageCode::from_static(CONDITION_NAMESPACE, "command_phrase");
const JOINED: MessageCode = MessageCode::from_static(CONDITION_NAMESPACE, "joined");
const DATE_PATTERN_EMPTY: MessageCode =
    MessageCode::from_static(CONDITION_NAMESPACE, "date_pattern_empty");
const DATE_PATTERN_UNTERMINATED_QUOTE: MessageCode =
    MessageCode::from_static(CONDITION_NAMESPACE, "date_pattern_unterminated_quote");
const DATE_PATTERN_UNSUPPORTED_LETTER: MessageCode =
    MessageCode::from_static(CONDITION_NAMESPACE, "date_pattern_unsupported_letter");
const DATE_PATTERN_UNSUPPORTED_LENGTH: MessageCode =
    MessageCode::from_static(CONDITION_NAMESPACE, "date_pattern_unsupported_length");
const DATE_PATTERN_REPLACED_SPELLING: MessageCode =
    MessageCode::from_static(CONDITION_NAMESPACE, "date_pattern_replaced_spelling");

pub const CONDITION_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: CONDITION_NAMESPACE,
    codes: &[
        ANY,
        FIRST_TIME_CHATTERS,
        COMMAND_PHRASE,
        JOINED,
        DATE_PATTERN_EMPTY,
        DATE_PATTERN_UNTERMINATED_QUOTE,
        DATE_PATTERN_UNSUPPORTED_LETTER,
        DATE_PATTERN_UNSUPPORTED_LENGTH,
        DATE_PATTERN_REPLACED_SPELLING,
    ],
    en_ftl: include_str!("../locales/en/condition.ftl"),
    uk_ftl: include_str!("../locales/uk/condition.ftl"),
};

pub fn any_condition() -> UserText {
    ANY.into()
}

pub(crate) fn first_time_chatters_condition() -> UserText {
    FIRST_TIME_CHATTERS.into()
}

pub fn chat_command_condition(config: &TriggerConfig) -> UserText {
    config
        .get(COMMAND_PHRASE_KEY)
        .and_then(forge_types::Variant::as_str)
        .filter(|phrase| !phrase.is_empty())
        .map_or_else(any_condition, |phrase| {
            MessageRef::new(COMMAND_PHRASE)
                .with_arg(PHRASE_ARG, VerbatimText::new(phrase))
                .into()
        })
}

pub fn joined_condition(parts: impl IntoIterator<Item = MessageRef>) -> UserText {
    parts
        .into_iter()
        .reduce(|joined, next| {
            MessageRef::new(JOINED)
                .with_arg(FIRST_ARG, joined)
                .with_arg(NEXT_ARG, next)
        })
        .map_or_else(any_condition, UserText::from)
}

pub(crate) fn date_pattern_message(error: &DatePatternError) -> MessageRef {
    match error {
        DatePatternError::Empty => MessageRef::new(DATE_PATTERN_EMPTY),
        DatePatternError::UnterminatedQuote { position } => {
            MessageRef::new(DATE_PATTERN_UNTERMINATED_QUOTE).with_arg(POSITION_ARG, *position)
        }
        DatePatternError::UnsupportedLetter { letter, position } => {
            MessageRef::new(DATE_PATTERN_UNSUPPORTED_LETTER)
                .with_arg(LETTER_ARG, VerbatimText::new(letter.to_string()))
                .with_arg(POSITION_ARG, *position)
        }
        DatePatternError::UnsupportedLength {
            letter,
            spelling,
            position,
        } => MessageRef::new(DATE_PATTERN_UNSUPPORTED_LENGTH)
            .with_arg(LETTER_ARG, VerbatimText::new(letter.to_string()))
            .with_arg(SPELLING_ARG, VerbatimText::new(spelling.as_str()))
            .with_arg(POSITION_ARG, *position),
        DatePatternError::DotNetSpelling {
            spelling,
            replacement,
            position,
        } => MessageRef::new(DATE_PATTERN_REPLACED_SPELLING)
            .with_arg(SPELLING_ARG, VerbatimText::new(spelling.as_str()))
            .with_arg(REPLACEMENT_ARG, VerbatimText::new(*replacement))
            .with_arg(POSITION_ARG, *position),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use forge_types::{MessageArg, Variant};

    use super::*;

    fn ftl_variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn phrase_config(phrase: Variant) -> TriggerConfig {
        TriggerConfig::from([(COMMAND_PHRASE_KEY.to_owned(), phrase)])
    }

    fn message_of(text: UserText) -> MessageRef {
        match text {
            UserText::Message(message) => message,
            UserText::Verbatim(text) => panic!("expected a message, got {text:?}"),
        }
    }

    fn every_constructor() -> Vec<MessageRef> {
        let length = DatePatternError::UnsupportedLength {
            letter: 'H',
            spelling: "HHH".to_owned(),
            position: 2,
        };
        let spelling = DatePatternError::DotNetSpelling {
            spelling: "tt".to_owned(),
            replacement: "a",
            position: 2,
        };
        vec![
            message_of(any_condition()),
            message_of(first_time_chatters_condition()),
            message_of(chat_command_condition(&phrase_config(Variant::String(
                "!hi".to_owned(),
            )))),
            message_of(joined_condition([
                MessageRef::new(ANY),
                MessageRef::new(ANY),
            ])),
            date_pattern_message(&DatePatternError::Empty),
            date_pattern_message(&DatePatternError::UnterminatedQuote { position: 2 }),
            date_pattern_message(&DatePatternError::UnsupportedLetter {
                letter: 'q',
                position: 2,
            }),
            date_pattern_message(&length),
            date_pattern_message(&spelling),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(CONDITION_MESSAGES.en_ftl);
        let uk = ftl_variables(CONDITION_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                CONDITION_MESSAGES.codes.contains(code),
                "{code} not catalogued"
            );
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = CONDITION_MESSAGES.codes.iter().cloned().collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(CONDITION_MESSAGES.en_ftl),
            ftl_variables(CONDITION_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn a_chat_command_without_a_usable_phrase_reads_as_any() {
        for config in [
            TriggerConfig::new(),
            phrase_config(Variant::String(String::new())),
            phrase_config(Variant::Int(7)),
        ] {
            assert_eq!(
                chat_command_condition(&config),
                any_condition(),
                "{config:?}"
            );
        }
    }

    #[test]
    fn a_chat_command_phrase_stays_verbatim() {
        let phrase = "!{ $not_fluent }";
        let message = message_of(chat_command_condition(&phrase_config(Variant::String(
            phrase.to_owned(),
        ))));
        let args: Vec<(&str, &MessageArg)> = message.args().collect();
        assert!(
            matches!(args.as_slice(), [(PHRASE_ARG, MessageArg::Verbatim(text))] if text.as_str() == phrase),
            "{args:?}"
        );
    }

    #[test]
    fn joining_no_parts_reads_as_any_and_one_part_stays_unwrapped() {
        let part = MessageRef::new(FIRST_TIME_CHATTERS);
        assert_eq!(joined_condition([]), any_condition());
        assert_eq!(joined_condition([part.clone()]), UserText::from(part));
    }

    #[test]
    fn joining_three_parts_keeps_their_order() {
        let [first, second, third] =
            [ANY, FIRST_TIME_CHATTERS, DATE_PATTERN_EMPTY].map(MessageRef::new);
        let inner = MessageRef::new(JOINED)
            .with_arg(FIRST_ARG, first.clone())
            .with_arg(NEXT_ARG, second.clone());
        let expected = MessageRef::new(JOINED)
            .with_arg(FIRST_ARG, inner)
            .with_arg(NEXT_ARG, third.clone());
        assert_eq!(
            joined_condition([first, second, third]),
            UserText::from(expected)
        );
    }
}
