use std::collections::BTreeSet;
use std::time::Duration;

use forge_components::{PushToast, ToastKind, duplicate_message_ids, tr};
use forge_registry::{CONDITION_MESSAGES, CONFIG_MESSAGES};
use forge_runtime::{CORE_MESSAGES, LIFECYCLE_MESSAGES};
use forge_storage::Language;
use forge_types::{COMMON_MESSAGES, MessageCatalog, MessageCatalogError};
use gpui::{AppContext, Context};

use crate::i18n::{app_catalogs, catalog_language};

pub(crate) const REGISTERED_CATALOGS: &[MessageCatalog] = &[
    COMMON_MESSAGES,
    CONDITION_MESSAGES,
    CONFIG_MESSAGES,
    CORE_MESSAGES,
    LIFECYCLE_MESSAGES,
    forge_platform_twitch::vocabulary::messages::TWITCH_MESSAGES,
    forge_platform_youtube::vocabulary::messages::YOUTUBE_MESSAGES,
    forge_platform_kick::vocabulary::messages::KICK_MESSAGES,
    forge_obs::vocabulary::messages::OBS_MESSAGES,
    forge_vtube::vocabulary::messages::VTUBE_MESSAGES,
    forge_midi::vocabulary::messages::MIDI_MESSAGES,
    forge_hotkey::vocabulary::messages::HOTKEY_MESSAGES,
    forge_soundboard::vocabulary::messages::SOUNDBOARD_MESSAGES,
    forge_discord::vocabulary::messages::DISCORD_MESSAGES,
    forge_platform_core::TEST_DONATION_MESSAGES,
    forge_transfer::TRANSFER_MESSAGES,
    forge_donatello::DONATELLO_MESSAGES,
    forge_monobank::MONOBANK_MESSAGES,
    forge_speak_queue::vocabulary::messages::SPEAK_MESSAGES,
    crate::repo_failure::REPO_MESSAGES,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CatalogProblem {
    Malformed(MessageCatalogError),
    DuplicateNamespace(&'static str),
    DuplicateMessageId { language: Language, id: String },
}

impl CatalogProblem {
    pub(crate) fn diagnostic(&self) -> String {
        match self {
            Self::Malformed(error) => error.to_string(),
            Self::DuplicateNamespace(namespace) => {
                format!("namespace {namespace} registered twice")
            }
            Self::DuplicateMessageId { language, id } => format!("{language}: {id} defined twice"),
        }
    }
}

pub(crate) fn fragments(lang: Language) -> impl Iterator<Item = &'static str> {
    REGISTERED_CATALOGS.iter().map(move |catalog| match lang {
        Language::En => catalog.en_ftl,
        Language::Uk => catalog.uk_ftl,
    })
}

pub(crate) fn catalog_problems(catalogs: &[MessageCatalog]) -> Vec<CatalogProblem> {
    let mut problems: Vec<CatalogProblem> = catalogs
        .iter()
        .filter_map(|catalog| catalog.check().err())
        .map(CatalogProblem::Malformed)
        .collect();
    let mut namespaces = BTreeSet::new();
    problems.extend(
        catalogs
            .iter()
            .filter(|catalog| !namespaces.insert(catalog.namespace))
            .map(|catalog| CatalogProblem::DuplicateNamespace(catalog.namespace)),
    );
    problems
}

pub(crate) fn registered_catalog_problems() -> Vec<CatalogProblem> {
    let mut problems = catalog_problems(REGISTERED_CATALOGS);
    for lang in [Language::En, Language::Uk] {
        problems.extend(
            duplicate_message_ids(catalog_language(lang), &app_catalogs(lang))
                .into_iter()
                .map(|id| CatalogProblem::DuplicateMessageId { language: lang, id }),
        );
    }
    problems
}

pub(crate) fn report_catalog_problems<T: 'static>(cx: &mut Context<T>) {
    cx.spawn(async move |_, cx| {
        let problems = cx
            .background_spawn(async { registered_catalog_problems() })
            .await;
        if problems.is_empty() {
            return;
        }
        let listed = problems
            .iter()
            .map(CatalogProblem::diagnostic)
            .collect::<Vec<_>>()
            .join("; ");
        tracing::error!(problems = %listed, "message catalogs conflict");
        cx.update(|cx| {
            cx.push_toast_full(
                ToastKind::Error,
                tr!("message_catalog_conflict", problems = listed),
                None,
                None,
                Duration::ZERO,
            );
        });
    })
    .detach();
}
#[cfg(test)]
mod tests {
    use fluent::FluentResource;
    use forge_registry::TriggerRegistry;
    use forge_runtime::TIMER_TICK_KIND;
    use forge_types::{DatePattern, MessageCode, MessageRef, TriggerConfig, Variant, VerbatimText};

    use super::*;
    use crate::i18n::install_language;
    use crate::user_text::{resolve_message, resolve_user_text};

    const GOOD_CODES: &[MessageCode] = &[MessageCode::from_static("alpha", "one")];
    const OTHER_CODES: &[MessageCode] = &[MessageCode::from_static("beta", "one")];
    const DUPLICATE_CODES: &[MessageCode] = &[
        MessageCode::from_static("alpha", "one"),
        MessageCode::from_static("alpha", "one"),
    ];

    const fn catalog(namespace: &'static str, codes: &'static [MessageCode]) -> MessageCatalog {
        MessageCatalog {
            namespace,
            codes,
            en_ftl: "",
            uk_ftl: "",
        }
    }

    fn ftl_ids(source: &str) -> BTreeSet<String> {
        source
            .lines()
            .filter(|line| !line.starts_with([' ', '#']))
            .filter_map(|line| line.split_once('=').map(|(id, _)| id.trim().to_owned()))
            .collect()
    }

    fn produced_codes() -> Vec<MessageCode> {
        let mut codes = vec![
            forge_types::UPSTREAM_FAILURE,
            forge_types::INTEGRATION_DISABLED,
            forge_types::NO_CHAT_PLATFORM_ENABLED,
            forge_types::NO_WHISPER_PLATFORM_ENABLED,
            forge_types::WHISPERS_UNSUPPORTED,
            forge_types::UNKNOWN_CHAT_TARGET,
            forge_platform_youtube::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
            forge_platform_kick::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
            forge_midi::vocabulary::messages::BACKEND_UNAVAILABLE,
            forge_hotkey::vocabulary::messages::PORTAL_UNAVAILABLE,
            forge_hotkey::vocabulary::messages::PERMISSION_DENIED,
            forge_hotkey::vocabulary::messages::COMBO_REJECTED,
            forge_hotkey::vocabulary::messages::ENGINE_STOPPED,
            forge_hotkey::vocabulary::messages::BACKEND_FAILED,
            forge_types::HEALTH_VIEWERS,
            forge_platform_twitch::vocabulary::messages::HEALTH_CHAT,
            forge_platform_twitch::vocabulary::messages::HEALTH_EVENTSUB,
            forge_platform_twitch::vocabulary::messages::HEALTH_API_CALLS,
            forge_platform_youtube::vocabulary::messages::HEALTH_LIVE_CHAT,
            forge_platform_youtube::vocabulary::messages::HEALTH_EVENTS,
            forge_platform_youtube::vocabulary::messages::HEALTH_API_CALLS,
            forge_platform_kick::vocabulary::messages::HEALTH_LIVE_CHAT,
            forge_platform_kick::vocabulary::messages::HEALTH_EVENTS,
            forge_platform_kick::vocabulary::messages::HEALTH_API_CALLS,
            forge_obs::vocabulary::messages::HEALTH_STREAM,
            forge_obs::vocabulary::messages::HEALTH_RECORDING,
            forge_obs::vocabulary::messages::HEALTH_CPU_FPS,
            forge_obs::vocabulary::messages::HEALTH_DROPPED_FRAMES,
            forge_vtube::vocabulary::messages::HEALTH_MODEL,
            forge_vtube::vocabulary::messages::HEALTH_EXPRESSIONS,
            forge_vtube::vocabulary::messages::HEALTH_ITEMS,
            forge_vtube::vocabulary::messages::HEALTH_TRACKING,
            forge_midi::vocabulary::messages::HEALTH_INPUT_PORTS,
            forge_midi::vocabulary::messages::HEALTH_OUTPUT_PORTS,
            forge_midi::vocabulary::messages::HEALTH_LAST_NOTE_ON,
            forge_midi::vocabulary::messages::HEALTH_EVENTS_PER_MINUTE,
            forge_hotkey::vocabulary::messages::HEALTH_REGISTERED,
            forge_hotkey::vocabulary::messages::HEALTH_LAST_TRIGGERED,
            forge_hotkey::vocabulary::messages::HEALTH_CONFLICTS,
            forge_hotkey::vocabulary::messages::HEALTH_PORTAL,
            forge_hotkey::vocabulary::messages::HEALTH_BACKEND,
            forge_discord::vocabulary::messages::HEALTH_WEBHOOK_LATENCY,
            forge_discord::vocabulary::messages::HEALTH_RATE_LIMIT,
            forge_discord::vocabulary::messages::HEALTH_LAST_SEND,
            forge_discord::vocabulary::messages::HEALTH_ERRORS,
        ];
        codes.extend(
            [
                "lifecycle.start_timed_out",
                "lifecycle.state_not_saved",
                "lifecycle.signed_out",
                "lifecycle.start_interrupted",
            ]
            .map(|raw| MessageCode::parse(raw).unwrap()),
        );
        codes
    }

    #[test]
    fn every_registered_catalog_declares_in_both_languages_exactly_its_codes() {
        for code in produced_codes() {
            let catalog = REGISTERED_CATALOGS
                .iter()
                .find(|catalog| catalog.namespace == code.namespace())
                .unwrap_or_else(|| panic!("no registered catalog for {code}"));
            assert!(catalog.codes.contains(&code), "{code} is not catalogued");
        }
        for catalog in REGISTERED_CATALOGS {
            let expected: BTreeSet<String> = catalog.ftl_keys().collect();
            assert_eq!(
                ftl_ids(catalog.en_ftl),
                expected,
                "{} en",
                catalog.namespace
            );
            assert_eq!(
                ftl_ids(catalog.uk_ftl),
                expected,
                "{} uk",
                catalog.namespace
            );
        }
    }

    #[test]
    fn the_registered_catalogs_have_no_problems() {
        assert_eq!(registered_catalog_problems(), Vec::new());
    }

    #[test]
    fn a_malformed_namespace_is_reported() {
        assert_eq!(
            catalog_problems(&[catalog("Bad-Space", GOOD_CODES)]),
            vec![CatalogProblem::Malformed(
                MessageCatalogError::MalformedNamespace("Bad-Space")
            )]
        );
    }

    #[test]
    fn a_code_from_another_namespace_is_reported() {
        assert_eq!(
            catalog_problems(&[catalog("alpha", OTHER_CODES)]),
            vec![CatalogProblem::Malformed(
                MessageCatalogError::ForeignCode {
                    namespace: "alpha",
                    code: MessageCode::from_static("beta", "one"),
                }
            )]
        );
    }

    #[test]
    fn a_code_listed_twice_is_reported() {
        assert_eq!(
            catalog_problems(&[catalog("alpha", DUPLICATE_CODES)]),
            vec![CatalogProblem::Malformed(
                MessageCatalogError::DuplicateCode(MessageCode::from_static("alpha", "one"))
            )]
        );
    }

    #[test]
    fn a_namespace_registered_twice_is_reported_for_the_second_registration() {
        assert_eq!(
            catalog_problems(&[catalog("alpha", GOOD_CODES), catalog("alpha", GOOD_CODES)]),
            vec![CatalogProblem::DuplicateNamespace("alpha")]
        );
    }

    #[test]
    fn distinct_well_formed_catalogs_have_no_problems() {
        assert_eq!(
            catalog_problems(&[catalog("alpha", GOOD_CODES), catalog("beta", OTHER_CODES)]),
            Vec::new()
        );
    }

    #[test]
    fn fragments_yield_each_registered_catalog_source_for_the_language() {
        let en: Vec<&str> = fragments(Language::En).collect();
        let uk: Vec<&str> = fragments(Language::Uk).collect();

        assert_eq!(
            (en, uk),
            (
                REGISTERED_CATALOGS.iter().map(|c| c.en_ftl).collect(),
                REGISTERED_CATALOGS.iter().map(|c| c.uk_ftl).collect(),
            )
        );
    }

    #[test]
    fn every_registered_catalog_parses_without_syntax_errors_in_both_languages() {
        for catalog in REGISTERED_CATALOGS {
            for (language, source) in [("en", catalog.en_ftl), ("uk", catalog.uk_ftl)] {
                assert!(
                    FluentResource::try_new(source.to_owned()).is_ok(),
                    "{} {language} has a syntax error",
                    catalog.namespace
                );
            }
        }
    }

    fn plain(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    fn every_trigger() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        forge_runtime::register_core_triggers(&mut registry).unwrap();
        forge_platform_twitch::register_twitch_triggers(&mut registry).unwrap();
        forge_platform_kick::register_kick_triggers(&mut registry).unwrap();
        forge_platform_youtube::register_youtube_triggers(&mut registry).unwrap();
        forge_obs::register_obs_triggers(&mut registry).unwrap();
        forge_vtube::register_vtube_triggers(&mut registry).unwrap();
        forge_midi::register_midi_triggers(&mut registry).unwrap();
        forge_hotkey::register_hotkey_triggers(&mut registry).unwrap();
        registry
    }

    fn config<const N: usize>(entries: [(&str, Variant); N]) -> TriggerConfig {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn assert_ukrainian_conditions(cases: Vec<(&str, TriggerConfig, &str)>) {
        install_language(Language::Uk);
        let registry = every_trigger();
        for (kind, config, expected) in cases {
            let descriptor = registry
                .get(kind)
                .unwrap_or_else(|| panic!("unknown trigger {kind}"));
            let shown = plain(resolve_user_text(&descriptor.condition_display(&config)).primary);
            assert_eq!(shown, expected, "{kind} {config:?}");
        }
    }

    #[test]
    fn shared_chat_conditions_render_in_ukrainian() {
        assert_ukrainian_conditions(vec![
            ("twitch.chat.message", config([]), "будь-який"),
            (
                "twitch.chat.message",
                config([("first_chatters_only", Variant::Bool(true))]),
                "нові глядачі чату",
            ),
            (
                "kick.chat.command",
                config([("phrase", text("!hi"))]),
                "\"!hi\"",
            ),
            ("youtube.chat.command", config([]), "будь-який"),
        ]);
    }

    #[test]
    fn core_conditions_render_in_ukrainian() {
        assert_ukrainian_conditions(vec![
            ("donation.received", config([]), "будь-який сервіс донатів"),
            (
                "donation.received",
                config([("provider", text("monobank"))]),
                "сервіс = monobank",
            ),
            ("script.event.custom", config([]), "будь-яка подія"),
            (
                "script.event.custom",
                config([("event_name", text("boss"))]),
                "подія = boss",
            ),
            (
                TIMER_TICK_KIND,
                config([("interval_minutes", Variant::Int(5))]),
                "кожні 5 хв",
            ),
        ]);
    }

    #[test]
    fn twitch_conditions_render_in_ukrainian_with_plural_and_status_branches() {
        assert_ukrainian_conditions(vec![
            (
                "twitch.support.cheer",
                config([("min_bits", Variant::Int(1))]),
                ">= 1 біт",
            ),
            (
                "twitch.support.cheer",
                config([("min_bits", Variant::Int(3))]),
                ">= 3 біти",
            ),
            (
                "twitch.support.cheer",
                config([("min_bits", Variant::Int(5))]),
                ">= 5 бітів",
            ),
            (
                "twitch.chat.cheer_message",
                config([
                    ("min_bits", Variant::Int(5)),
                    ("max_bits", Variant::Int(21)),
                ]),
                "5-21 біт",
            ),
            (
                "twitch.support.charity_donation",
                config([("min_amount_cents", Variant::Int(11))]),
                "сума >= 11 центів",
            ),
            (
                "twitch.support.hype_train_progress",
                config([("min_level", Variant::Int(2))]),
                "рівень >= 2",
            ),
            (
                "twitch.channel_points.redemption_updated",
                config([("status_filter", text("fulfilled"))]),
                "статус = виконано",
            ),
            (
                "twitch.channel_points.redemption_updated",
                config([("status_filter", text("custom"))]),
                "статус = custom",
            ),
            (
                "twitch.chat.whisper",
                config([("from_user", text("ann")), ("match_text", text("hi"))]),
                "від ann · текст збігається з \"hi\"",
            ),
        ]);
    }

    #[test]
    fn integration_conditions_render_in_ukrainian() {
        assert_ukrainian_conditions(vec![
            (
                "youtube.channel.user_banned",
                config([("ban_type_filter", text("permanent"))]),
                "лише постійне блокування",
            ),
            (
                forge_obs::vocabulary::trigger_ids::SCENE_CHANGED,
                config([("scene", text("Gameplay"))]),
                "сцена = Gameplay",
            ),
            (
                forge_obs::vocabulary::trigger_ids::RECORDING_STARTED,
                config([]),
                "запис розпочато",
            ),
            ("vtube.model.loaded", config([]), "будь-яка модель"),
            (
                "hotkey.global.pressed",
                config([]),
                "будь-яка гаряча клавіша",
            ),
        ]);
    }

    #[test]
    fn midi_conditions_render_in_ukrainian_with_joined_parts_and_direction_branches() {
        assert_ukrainian_conditions(vec![
            (
                "midi.input.note_on",
                config([]),
                "будь-яка нота · будь-який канал",
            ),
            (
                "midi.input.note_on",
                config([
                    ("note", Variant::Int(60)),
                    ("channel", Variant::Int(2)),
                    ("device", text("Pad")),
                ]),
                "нота=60 · канал=3 · пристрій=Pad",
            ),
            (
                "midi.device.connected",
                config([("direction", text("input"))]),
                "напрямок=вхід",
            ),
            (
                "midi.device.connected",
                config([("direction", text("thru"))]),
                "напрямок=thru",
            ),
        ]);
    }

    fn date_reason(name: &'static str) -> MessageRef {
        MessageRef::new(MessageCode::from_static("condition", name))
    }

    fn date_reasons() -> [(&'static str, MessageRef, &'static str); 5] {
        [
            (
                "",
                date_reason("date_pattern_empty"),
                "шаблон дати порожній",
            ),
            (
                "'HH",
                date_reason("date_pattern_unterminated_quote").with_arg("position", 0_usize),
                "текст у лапках, що починається на позиції 0, не закрито символом '",
            ),
            (
                "qq",
                date_reason("date_pattern_unsupported_letter")
                    .with_arg("letter", VerbatimText::new("q"))
                    .with_arg("position", 0_usize),
                "'q' на позиції 0 не є літерою шаблону дати; візьміть звичайний текст в одинарні лапки",
            ),
            (
                "HHH",
                date_reason("date_pattern_unsupported_length")
                    .with_arg("letter", VerbatimText::new("H"))
                    .with_arg("spelling", VerbatimText::new("HHH"))
                    .with_arg("position", 0_usize),
                "'HHH' на позиції 0 має непідтримувану довжину для 'H'",
            ),
            (
                "tt",
                date_reason("date_pattern_replaced_spelling")
                    .with_arg("spelling", VerbatimText::new("tt"))
                    .with_arg("replacement", VerbatimText::new("a"))
                    .with_arg("position", 0_usize),
                "'tt' на позиції 0 не підтримується; використайте 'a'",
            ),
        ]
    }

    #[test]
    fn date_pattern_reasons_read_like_the_parser_error_in_english() {
        install_language(Language::En);
        for (pattern, reason, _) in date_reasons() {
            let parser_error = DatePattern::parse(pattern).unwrap_err().to_string();
            assert_eq!(plain(resolve_message(&reason).primary), parser_error);
        }
    }

    #[test]
    fn date_pattern_reasons_render_in_ukrainian() {
        install_language(Language::Uk);
        for (pattern, reason, expected) in date_reasons() {
            assert_eq!(
                plain(resolve_message(&reason).primary),
                expected,
                "{pattern:?}"
            );
        }
    }
}
