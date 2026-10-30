use std::collections::BTreeSet;
use std::time::Duration;

use forge_components::{PushToast, ToastKind, duplicate_message_ids, tr};
use forge_registry::CONFIG_MESSAGES;
use forge_runtime::{CORE_MESSAGES, LIFECYCLE_MESSAGES};
use forge_storage::Language;
use forge_types::{COMMON_MESSAGES, MessageCatalog, MessageCatalogError};
use gpui::{AppContext, Context};

use crate::i18n::{app_catalogs, catalog_language};

pub const REGISTERED_CATALOGS: &[MessageCatalog] = &[
    COMMON_MESSAGES,
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
    forge_donatello::DONATELLO_MESSAGES,
    forge_monobank::MONOBANK_MESSAGES,
    forge_speak_queue::vocabulary::messages::SPEAK_MESSAGES,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogProblem {
    Malformed(MessageCatalogError),
    DuplicateNamespace(&'static str),
    DuplicateMessageId { language: Language, id: String },
}

impl CatalogProblem {
    pub fn diagnostic(&self) -> String {
        match self {
            Self::Malformed(error) => error.to_string(),
            Self::DuplicateNamespace(namespace) => {
                format!("namespace {namespace} registered twice")
            }
            Self::DuplicateMessageId { language, id } => format!("{language}: {id} defined twice"),
        }
    }
}

pub fn fragments(lang: Language) -> impl Iterator<Item = &'static str> {
    REGISTERED_CATALOGS.iter().map(move |catalog| match lang {
        Language::En => catalog.en_ftl,
        Language::Uk => catalog.uk_ftl,
    })
}

pub fn catalog_problems(catalogs: &[MessageCatalog]) -> Vec<CatalogProblem> {
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

pub fn registered_catalog_problems() -> Vec<CatalogProblem> {
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

pub fn report_catalog_problems<T: 'static>(cx: &mut Context<T>) {
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
    use forge_types::MessageCode;

    use super::*;

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
            .filter_map(|line| line.split_once(" = ").map(|(id, _)| id.trim().to_owned()))
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
}
