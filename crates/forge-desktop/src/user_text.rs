use forge_components::{ArgsBuilder, DateShape, fmt_display, fmt_uptime, tr, tr_find_warned};
use forge_types::{MessageArg, MessageRef, UPSTREAM_FAILURE, UserText, VariantKind};

use crate::chat::platform_display_name;
use crate::config_field_label::step_field_label;
use crate::integration_switch::integration_name;

const LIST_JOINER: &str = ", ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedText {
    pub primary: String,
    pub secondary: Option<String>,
}

impl ResolvedText {
    pub fn inline(self) -> String {
        match self.secondary {
            Some(secondary) => format!("{} ({secondary})", self.primary),
            None => self.primary,
        }
    }
}

pub fn resolve_user_text(text: &UserText) -> ResolvedText {
    match text {
        UserText::Message(message) => resolve_message(message),
        UserText::Verbatim(verbatim) => ResolvedText {
            primary: verbatim.as_str().to_owned(),
            secondary: None,
        },
    }
}

pub fn resolve_message(message: &MessageRef) -> ResolvedText {
    let detail = message.detail().map(|detail| detail.as_str().to_owned());
    let args = message
        .args()
        .fold(ArgsBuilder::new(), |args, (name, value)| {
            with_fluent_arg(args, name, value)
        })
        .build();
    if let Some(primary) = tr_find_warned(&message.code().ftl_key(), Some(&args)) {
        return ResolvedText {
            primary,
            secondary: detail,
        };
    }
    let diagnostic = message.code().to_string();
    match tr_find_warned(&UPSTREAM_FAILURE.ftl_key(), None) {
        Some(primary) => ResolvedText {
            primary,
            secondary: detail.or(Some(diagnostic)),
        },
        None => ResolvedText {
            primary: diagnostic,
            secondary: detail,
        },
    }
}

fn with_fluent_arg<'a>(
    args: ArgsBuilder<'a>,
    name: &'a str,
    value: &MessageArg,
) -> ArgsBuilder<'a> {
    match value {
        MessageArg::Int(number) => args.set(name, *number),
        MessageArg::Float(number) => args.set(name, number.get()),
        MessageArg::Datetime(at) => args.set(name, fmt_display(DateShape::DateTime, *at)),
        MessageArg::Duration(span) => args.set(name, fmt_uptime(span.as_secs())),
        MessageArg::Verbatim(text) => args.set(name, text.as_str().to_owned()),
        MessageArg::Integration(id) => args.set(name, integration_name(id).to_string()),
        MessageArg::Platform(platform) => args.set(name, platform_display_name(*platform)),
        MessageArg::ValueKind(kind) => args.set(name, value_kind_name(*kind)),
        MessageArg::StepFields { kind, keys } => {
            let labels: Vec<String> = keys.iter().map(|key| step_field_label(kind, key)).collect();
            args.set(name, labels.join(LIST_JOINER))
        }
        MessageArg::Message(nested) => args.set(name, resolve_message(nested).primary),
    }
}

fn value_kind_name(kind: VariantKind) -> String {
    match kind {
        VariantKind::Int => tr!("value_kind_int"),
        VariantKind::Float => tr!("value_kind_float"),
        VariantKind::Bool => tr!("value_kind_bool"),
        VariantKind::String => tr!("value_kind_string"),
        VariantKind::Datetime => tr!("value_kind_datetime"),
        VariantKind::Array => tr!("value_kind_array"),
        VariantKind::Object => tr!("value_kind_object"),
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_components::{
        CatalogLanguage, ClockStyle, DateDisplay, DateFormatChoice, DisplayFormats, FirstWeekday,
        install_display_formats, install_thread_language,
    };
    use forge_storage::Language;
    use forge_types::{
        IntegrationId, MESSAGE_KEY_PREFIX, MessageCode, NamesLanguage, PatternZone, PlatformId,
        UpstreamDetail, VerbatimText,
    };
    use time::{Date, Month, OffsetDateTime, Time};

    use super::*;
    use crate::i18n::{app_catalogs, catalog_language};

    const PROBE_EN: &str = "msg-probe-count = { $count ->\n    [one] { $count } item\n   *[other] { $count } items\n}\nmsg-probe-when = At { $when }\nmsg-probe-took = Took { $span }\nmsg-probe-service = Via { $service }\nmsg-probe-site = On { $site }\nmsg-probe-raw = Said { $text }\nmsg-probe-ratio = Ratio { $ratio }\nmsg-probe-plain = Plain\n";
    const PROBE_UK: &str = "msg-probe-count = { $count ->\n    [one] { $count } елемент\n    [few] { $count } елементи\n   *[many] { $count } елементів\n}\nmsg-probe-when = О { $when }\nmsg-probe-took = Тривало { $span }\nmsg-probe-service = Через { $service }\nmsg-probe-site = На { $site }\nmsg-probe-raw = Сказано { $text }\nmsg-probe-ratio = Частка { $ratio }\nmsg-probe-plain = Просто\n";

    const PROBE_PLAIN: MessageCode = MessageCode::from_static("probe", "plain");
    const PROBE_COUNT: MessageCode = MessageCode::from_static("probe", "count");
    const PROBE_ABSENT: MessageCode = MessageCode::from_static("probe", "absent");

    fn formats(date_format: DateFormatChoice, clock: ClockStyle) -> DisplayFormats {
        DisplayFormats {
            date_format,
            clock,
            first_day: FirstWeekday::Monday,
        }
    }

    fn install(language: Language) {
        let mut sources = app_catalogs(language);
        sources.push(match language {
            Language::En => PROBE_EN,
            Language::Uk => PROBE_UK,
        });
        install_thread_language(catalog_language(language), &sources);
        install_display_formats(formats(
            DateFormatChoice::YearMonthDay,
            ClockStyle::TwentyFourHour,
        ));
    }

    fn plain(text: &str) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    fn primary(message: &MessageRef) -> String {
        plain(&resolve_message(message).primary)
    }

    fn noon_utc() -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, Month::March, 5).unwrap(),
            Time::from_hms(12, 0, 0).unwrap(),
        )
    }

    fn detail(text: &str) -> UpstreamDetail {
        UpstreamDetail::from_redacted(text)
    }

    #[test]
    fn an_int_arg_selects_the_english_plural_branch() {
        install(Language::En);

        let rendered: Vec<String> = [1_i64, 2]
            .into_iter()
            .map(|n| primary(&MessageRef::new(PROBE_COUNT).with_arg("count", n)))
            .collect();

        assert_eq!(rendered, vec!["1 item", "2 items"]);
    }

    #[test]
    fn an_int_arg_selects_the_ukrainian_one_few_and_many_plural_branches() {
        install(Language::Uk);

        let rendered: Vec<String> = [1_i64, 2, 5, 21]
            .into_iter()
            .map(|n| primary(&MessageRef::new(PROBE_COUNT).with_arg("count", n)))
            .collect();

        assert_eq!(
            rendered,
            vec!["1 елемент", "2 елементи", "5 елементів", "21 елемент"]
        );
    }

    #[test]
    fn a_float_arg_is_passed_as_a_number() {
        install(Language::En);
        let message =
            MessageRef::new(MessageCode::from_static("probe", "ratio")).with_arg("ratio", 2.5_f64);

        assert_eq!(primary(&message), "Ratio 2.5");
    }

    #[test]
    fn a_datetime_arg_follows_the_installed_display_formats() {
        install(Language::En);
        let message =
            MessageRef::new(MessageCode::from_static("probe", "when")).with_arg("when", noon_utc());
        let choices = [
            (DateFormatChoice::YearMonthDay, ClockStyle::TwentyFourHour),
            (DateFormatChoice::MonthDayYearSlash, ClockStyle::TwelveHour),
        ];

        let rendered: Vec<String> = choices
            .into_iter()
            .map(|(date_format, clock)| {
                install_display_formats(formats(date_format, clock));
                primary(&message)
            })
            .collect();
        let expected: Vec<String> = choices
            .into_iter()
            .map(|(date_format, clock)| {
                let shown = DateDisplay::new(
                    formats(date_format, clock),
                    NamesLanguage::En,
                    PatternZone::Local,
                )
                .format(DateShape::DateTime, noon_utc());
                format!("At {shown}")
            })
            .collect();

        assert_eq!(
            (rendered.clone(), rendered[0] != rendered[1]),
            (expected, true)
        );
    }

    #[test]
    fn a_duration_arg_renders_as_an_uptime_span() {
        install(Language::En);
        let message = MessageRef::new(MessageCode::from_static("probe", "took"))
            .with_arg("span", Duration::from_secs(3_725));

        assert_eq!(primary(&message), "Took 1h 2m");
    }

    #[test]
    fn integration_and_platform_args_render_brand_names() {
        install(Language::En);
        let integration = MessageRef::new(MessageCode::from_static("probe", "service"))
            .with_arg("service", IntegrationId::from_static("discord"));
        let platform = MessageRef::new(MessageCode::from_static("probe", "site"))
            .with_arg("site", PlatformId::YouTube);

        assert_eq!(
            (primary(&integration), primary(&platform)),
            ("Via Discord".to_owned(), "On YouTube".to_owned())
        );
    }

    #[test]
    fn a_verbatim_arg_is_inserted_unchanged() {
        install(Language::Uk);
        let message = MessageRef::new(MessageCode::from_static("probe", "raw"))
            .with_arg("text", VerbatimText::new("{ $x } <b> ключ"));

        assert_eq!(primary(&message), "Сказано { $x } <b> ключ");
    }

    fn raw(arg: impl Into<MessageArg>) -> String {
        primary(&MessageRef::new(MessageCode::from_static("probe", "raw")).with_arg("text", arg))
    }

    #[test]
    fn a_value_kind_arg_renders_the_translated_kind_name_in_both_languages() {
        for (language, kinds) in [
            (
                Language::En,
                [
                    (VariantKind::Int, "Said a whole number"),
                    (VariantKind::Float, "Said a decimal number"),
                    (VariantKind::Bool, "Said a true/false value"),
                    (VariantKind::String, "Said text"),
                    (VariantKind::Datetime, "Said a date and time"),
                    (VariantKind::Array, "Said an array"),
                    (VariantKind::Object, "Said an object"),
                ],
            ),
            (
                Language::Uk,
                [
                    (VariantKind::Int, "Сказано ціле число"),
                    (VariantKind::Float, "Сказано дробове число"),
                    (VariantKind::Bool, "Сказано логічне значення"),
                    (VariantKind::String, "Сказано текст"),
                    (VariantKind::Datetime, "Сказано дату й час"),
                    (VariantKind::Array, "Сказано масив"),
                    (VariantKind::Object, "Сказано об'єкт"),
                ],
            ),
        ] {
            install(language);
            for (kind, expected) in kinds {
                assert_eq!(raw(kind), expected, "{kind:?}");
            }
        }
    }

    #[test]
    fn step_fields_render_each_key_by_its_short_label_or_by_the_key_when_it_has_none() {
        install(Language::En);

        let rendered = raw(MessageArg::StepFields {
            kind: "core.time.format".to_owned(),
            keys: vec![
                "format_string".to_owned(),
                "skip_if_late".to_owned(),
                "no_such_field".to_owned(),
            ],
        });

        assert_eq!(
            rendered,
            "Said Date pattern, Skip instead of running late if forge was closed, no_such_field"
        );
    }

    #[test]
    fn a_nested_message_renders_as_its_translated_primary_without_its_detail() {
        install(Language::Uk);
        let nested = MessageRef::new(PROBE_COUNT)
            .with_arg("count", 2_i64)
            .with_detail(detail("upstream cause"));

        assert_eq!(raw(nested), "Сказано 2 елементи");
    }

    #[test]
    fn a_resolved_message_follows_the_installed_language() {
        let rendered: Vec<String> = [Language::En, Language::Uk]
            .into_iter()
            .map(|language| {
                install(language);
                primary(&MessageRef::new(PROBE_PLAIN))
            })
            .collect();

        assert_eq!(rendered, vec!["Plain", "Просто"]);
    }

    #[test]
    fn detail_lands_in_secondary_only_and_never_in_primary() {
        install(Language::En);
        let message = MessageRef::new(PROBE_PLAIN).with_detail(detail("upstream said no"));

        assert_eq!(
            resolve_message(&message),
            ResolvedText {
                primary: "Plain".to_owned(),
                secondary: Some("upstream said no".to_owned()),
            }
        );
    }

    #[test]
    fn a_message_without_detail_has_no_secondary() {
        install(Language::En);

        assert_eq!(
            resolve_message(&MessageRef::new(PROBE_PLAIN)).secondary,
            None
        );
    }

    #[test]
    fn a_missing_key_with_detail_falls_back_to_the_generic_failure_plus_detail() {
        install(Language::En);
        let message = MessageRef::new(PROBE_ABSENT).with_detail(detail("boom"));

        assert_eq!(
            resolve_message(&message),
            ResolvedText {
                primary: "The service reported an error".to_owned(),
                secondary: Some("boom".to_owned()),
            }
        );
    }

    #[test]
    fn a_missing_key_without_detail_shows_the_code_as_secondary_under_the_generic_failure() {
        install(Language::Uk);

        assert_eq!(
            resolve_message(&MessageRef::new(PROBE_ABSENT)),
            ResolvedText {
                primary: "Сервіс повідомив про помилку".to_owned(),
                secondary: Some("probe.absent".to_owned()),
            }
        );
    }

    #[test]
    fn with_the_generic_message_also_missing_the_code_string_is_primary() {
        install_thread_language(CatalogLanguage::En, &[]);
        let message = MessageRef::new(PROBE_ABSENT).with_detail(detail("boom"));

        assert_eq!(
            resolve_message(&message),
            ResolvedText {
                primary: "probe.absent".to_owned(),
                secondary: Some("boom".to_owned()),
            }
        );
    }

    #[test]
    fn a_resolved_text_never_exposes_a_raw_message_key() {
        let absent = MessageRef::new(PROBE_ABSENT);
        install_thread_language(CatalogLanguage::En, &[]);
        let without_generic = resolve_message(&absent);
        install(Language::En);
        let with_generic = resolve_message(&absent);

        let shown: Vec<String> = [without_generic, with_generic]
            .into_iter()
            .flat_map(|text| [Some(text.primary), text.secondary])
            .flatten()
            .collect();
        assert_eq!(
            shown
                .iter()
                .filter(|text| text.contains(MESSAGE_KEY_PREFIX))
                .count(),
            0
        );
    }

    #[test]
    fn verbatim_user_text_is_returned_as_is_without_a_catalog_lookup() {
        install_thread_language(CatalogLanguage::En, &[]);
        let text = UserText::Verbatim(VerbatimText::new("msg-probe-plain { $x }"));

        assert_eq!(
            resolve_user_text(&text),
            ResolvedText {
                primary: "msg-probe-plain { $x }".to_owned(),
                secondary: None,
            }
        );
    }

    #[test]
    fn message_user_text_resolves_through_the_catalog() {
        install(Language::En);
        let text = UserText::Message(MessageRef::new(PROBE_PLAIN));

        assert_eq!(resolve_user_text(&text).primary, "Plain");
    }
}
