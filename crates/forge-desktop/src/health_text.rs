use forge_components::{fmt_uptime, tr};
use forge_platform_core::{ConnectionState, HealthMetricId, HealthText};

use crate::user_text::{resolve_message, resolve_user_text};

pub(crate) fn health_metric_label(id: &HealthMetricId) -> String {
    resolve_message(&id.label()).primary
}

pub(crate) fn health_text(text: &HealthText) -> String {
    match text {
        HealthText::Text(text) => resolve_user_text(text).inline(),
        HealthText::Count(count) => count.to_string(),
        HealthText::Span(span) => fmt_uptime(span.as_secs()),
        HealthText::Connection(state) => connection_label(*state),
    }
}

pub(crate) fn connection_label(state: ConnectionState) -> String {
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
    use forge_types::{
        HEALTH_VIEWERS, MessageCode, MessageRef, UpstreamDetail, UserText, VerbatimText,
    };

    use super::*;
    use crate::i18n::install_language;

    fn plain(text: String) -> String {
        text.replace(crate::test_support::FORMAT_ISOLATES, "")
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
                health_text(&HealthText::Text(VerbatimText::new("#streamer").into())),
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

    fn coded(raw_code: &str) -> MessageRef {
        MessageRef::new(MessageCode::parse(raw_code).unwrap())
    }

    fn rendered(message: MessageRef) -> String {
        plain(health_text(&HealthText::from(UserText::from(message))))
    }

    #[test]
    fn the_cpu_tile_pads_both_figures_to_one_decimal_in_each_language() {
        for (language, point) in [(Language::En, "."), (Language::Uk, ",")] {
            install_language(language);
            for (cpu, fps, expected) in [
                (12.3, 59.9, "12.3% \u{b7} 59.9"),
                (0.0, 60.0, "0.0% \u{b7} 60.0"),
            ] {
                let message = coded("obs.health_cpu_fps_value")
                    .with_arg("cpu", cpu)
                    .with_arg("fps", fps);

                assert_eq!(
                    rendered(message),
                    expected.replace('.', point),
                    "{language:?}"
                );
            }
        }
    }

    #[test]
    fn the_dropped_share_pads_to_two_decimals_in_each_language() {
        for (language, point) in [(Language::En, "."), (Language::Uk, ",")] {
            install_language(language);
            for (percent, expected) in [(0.0, "0.00%"), (0.5, "0.50%"), (100.0, "100.00%")] {
                let message = coded("obs.health_dropped_share").with_arg("percent", percent);

                assert_eq!(
                    rendered(message),
                    expected.replace('.', point),
                    "{language:?}"
                );
            }
        }
    }

    #[test]
    fn a_figure_with_more_digits_than_the_tile_declares_is_cut_to_the_declared_digits() {
        install_language(Language::En);
        let cpu = coded("obs.health_cpu_fps_value")
            .with_arg("cpu", 12.3456)
            .with_arg("fps", 59.94);
        let share = coded("obs.health_dropped_share").with_arg("percent", 100.0 / 3.0);

        assert_eq!(
            (rendered(cpu), rendered(share)),
            ("12.3% \u{b7} 59.9".to_owned(), "33.33%".to_owned())
        );
    }

    #[test]
    fn the_dropped_count_follows_each_languages_plural_rules() {
        for (language, count, expected) in [
            (Language::En, 1, "1 frame"),
            (Language::En, 0, "0 frames"),
            (Language::En, 21, "21 frames"),
            (Language::Uk, 1, "1 кадр"),
            (Language::Uk, 3, "3 кадри"),
            (Language::Uk, 5, "5 кадрів"),
            (Language::Uk, 11, "11 кадрів"),
            (Language::Uk, 22, "22 кадри"),
        ] {
            install_language(language);
            let message = coded("obs.health_dropped_count").with_arg("count", count);

            assert_eq!(rendered(message), expected, "{language:?}");
        }
    }

    #[test]
    fn the_eventsub_tile_follows_each_languages_plural_rules() {
        for (language, count, expected) in [
            (Language::En, 1, "1 subscription"),
            (Language::En, 2, "2 subscriptions"),
            (Language::En, 0, "0 subscriptions"),
            (Language::Uk, 1, "1 підписка"),
            (Language::Uk, 21, "21 підписка"),
            (Language::Uk, 3, "3 підписки"),
            (Language::Uk, 24, "24 підписки"),
            (Language::Uk, 5, "5 підписок"),
            (Language::Uk, 11, "11 підписок"),
            (Language::Uk, 0, "0 підписок"),
        ] {
            install_language(language);
            let message = coded("twitch.health_eventsub_subs").with_arg("count", count);

            assert_eq!(rendered(message), expected, "{language:?}");
        }
    }

    #[test]
    fn the_placeholder_slot_reads_the_same_dash_in_every_language() {
        for language in [Language::En, Language::Uk] {
            install_language(language);

            assert_eq!(health_text(&HealthText::placeholder()), "-");
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
