use forge_components::{fmt_uptime, tr};
use forge_platform_core::{ConnectionState, HealthMetricId, HealthText};

use crate::user_text::{resolve_message, resolve_user_text};

pub fn health_metric_label(id: &HealthMetricId) -> String {
    resolve_message(&id.label()).primary
}

pub fn health_text(text: &HealthText) -> String {
    match text {
        HealthText::Text(text) => resolve_user_text(text).inline(),
        HealthText::Count(count) => count.to_string(),
        HealthText::Span(span) => fmt_uptime(span.as_secs()),
        HealthText::Connection(state) => connection_label(*state),
    }
}

pub fn connection_label(state: ConnectionState) -> String {
    match state {
        ConnectionState::Connected => tr!("common_status_connected"),
        ConnectionState::Connecting => tr!("integration_state_connecting_title"),
        ConnectionState::Reconnecting => tr!("integration_state_reconnecting_title"),
        ConnectionState::Disconnected => tr!("common_status_not_connected"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_storage::Language;
    use forge_types::{HEALTH_VIEWERS, MessageCode, MessageRef, UpstreamDetail, UserText};

    use super::*;
    use crate::i18n::install_language;

    fn plain(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn connection_slots_read_the_header_labels_in_each_language() {
        for (language, state, expected) in [
            (Language::En, ConnectionState::Connected, "Connected"),
            (Language::En, ConnectionState::Connecting, "Connecting..."),
            (
                Language::En,
                ConnectionState::Reconnecting,
                "Reconnecting...",
            ),
            (Language::En, ConnectionState::Disconnected, "Not connected"),
            (Language::Uk, ConnectionState::Connected, "Підключено"),
            (Language::Uk, ConnectionState::Connecting, "Підключення..."),
            (
                Language::Uk,
                ConnectionState::Reconnecting,
                "Повторне підключення...",
            ),
            (Language::Uk, ConnectionState::Disconnected, "Не підключено"),
        ] {
            install_language(language);

            assert_eq!(
                (
                    connection_label(state),
                    health_text(&HealthText::Connection(state))
                ),
                (expected.to_owned(), expected.to_owned()),
                "{language:?} {state:?}"
            );
        }
    }

    #[test]
    fn a_count_slot_prints_every_digit_without_grouping() {
        install_language(Language::Uk);
        for (count, expected) in [
            (0, "0"),
            (1_234_567, "1234567"),
            (u64::MAX, "18446744073709551615"),
        ] {
            assert_eq!(health_text(&HealthText::Count(count)), expected);
        }
    }

    #[test]
    fn a_span_slot_renders_as_an_uptime_in_the_installed_language() {
        for (language, span, expected) in [
            (Language::En, Duration::from_secs(3_725), "1h 2m"),
            (Language::En, Duration::from_millis(61_900), "1m 1s"),
            (Language::Uk, Duration::from_secs(3_725), "1 год 2 хв"),
        ] {
            install_language(language);

            assert_eq!(
                plain(health_text(&HealthText::Span(span))),
                expected,
                "{language:?} {span:?}"
            );
        }
    }

    #[test]
    fn a_verbatim_text_slot_is_shown_as_stored_in_every_language() {
        for language in [Language::En, Language::Uk] {
            install_language(language);

            assert_eq!(
                health_text(&HealthText::unlocalized("#streamer")),
                "#streamer"
            );
        }
    }

    #[test]
    fn a_coded_text_slot_resolves_through_the_catalog_in_each_language() {
        let viewers = HealthText::from(UserText::from(MessageRef::new(HEALTH_VIEWERS)));
        for (language, expected) in [(Language::En, "Viewers"), (Language::Uk, "Глядачі")] {
            install_language(language);

            assert_eq!(health_text(&viewers), expected);
        }
    }

    #[test]
    fn a_coded_text_slot_inlines_its_upstream_detail_after_the_message() {
        install_language(Language::En);
        let message = MessageRef::new(HEALTH_VIEWERS)
            .with_detail(UpstreamDetail::from_redacted("stale by 30s"));

        assert_eq!(
            health_text(&HealthText::from(UserText::from(message))),
            "Viewers (stale by 30s)"
        );
    }

    #[test]
    fn a_metric_label_resolves_from_its_id_in_each_language() {
        for (language, expected) in [(Language::En, "Viewers"), (Language::Uk, "Глядачі")] {
            install_language(language);

            assert_eq!(health_metric_label(&HealthMetricId::VIEWERS), expected);
        }
    }

    #[test]
    fn a_metric_without_a_catalog_entry_shows_the_generic_failure_instead_of_its_key() {
        install_language(Language::En);
        let unknown = HealthMetricId::new(MessageCode::from_static("stub", "not_catalogued"));

        assert_eq!(
            health_metric_label(&unknown),
            "The service reported an error"
        );
    }
}
