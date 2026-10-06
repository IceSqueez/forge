use forge_components::{Platform, fmt_relative_time, tr};
use forge_events::Event;
use forge_platform_core::{BanDuration, BanListUnavailable, UnbanRefusal};
use time::{OffsetDateTime, UtcOffset};

use crate::chat_drawer::DASH;
use crate::scheduled_run_labels::local_stamp;

const BAN_CHANGE_KINDS: [(&str, Platform); 4] = [
    ("twitch.channel.ban", Platform::Twitch),
    ("twitch.channel.unban", Platform::Twitch),
    ("youtube.channel.user_banned", Platform::YouTube),
    ("kick.moderation.banned", Platform::Kick),
];

pub(crate) fn ban_change_platform(kind: &str) -> Option<Platform> {
    BAN_CHANGE_KINDS
        .iter()
        .find(|(change, _)| *change == kind)
        .map(|(_, platform)| *platform)
}

pub(crate) fn changed_platforms(events: &[Event]) -> Vec<Platform> {
    let mut platforms = Vec::new();
    for platform in events
        .iter()
        .filter_map(|event| ban_change_platform(&event.kind))
    {
        if !platforms.contains(&platform) {
            platforms.push(platform);
        }
    }
    platforms
}

pub(crate) fn unavailable_label(reason: BanListUnavailable) -> String {
    match reason {
        BanListUnavailable::NotConnected => tr!("chat_banned_not_connected"),
        BanListUnavailable::MissingScope => tr!("chat_banned_missing_permission"),
        BanListUnavailable::QuotaExhausted | BanListUnavailable::Transport => {
            tr!("chat_banned_try_later")
        }
    }
}

pub(crate) fn refusal_label(refusal: UnbanRefusal) -> String {
    match refusal {
        UnbanRefusal::BannedOutsideForge => tr!("chat_banned_outside_forge"),
    }
}

pub(crate) fn expiry_label(duration: BanDuration, offset: UtcOffset) -> String {
    match duration {
        BanDuration::Permanent => tr!("chat_banned_permanent"),
        BanDuration::Until(at) => tr!("chat_banned_until", time = local_stamp(at, offset)),
    }
}

pub(crate) fn banned_at_label(created_at: Option<OffsetDateTime>) -> String {
    match created_at {
        Some(at) => fmt_relative_time(Some(at)),
        None => DASH.to_owned(),
    }
}

pub(crate) fn optional_label(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DASH)
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use forge_components::Platform;
    use forge_events::{Event, EventSource};
    use forge_platform_core::{BanDuration, BanListUnavailable};
    use forge_storage::Language;
    use time::{OffsetDateTime, UtcOffset};

    use super::{
        ban_change_platform, changed_platforms, expiry_label, optional_label, unavailable_label,
    };
    use crate::chat_drawer::DASH;
    use crate::i18n::install_language;

    const MAR_15_2026_2330_UTC: i64 = 1_773_617_400;

    fn event(kind: &str) -> Event {
        Event::new(EventSource::Twitch, kind, serde_json::Value::Null)
    }

    #[test]
    fn ban_change_platform_recognises_only_the_ban_and_unban_kinds_of_each_platform() {
        for (kind, expected) in [
            ("twitch.channel.ban", Some(Platform::Twitch)),
            ("twitch.channel.unban", Some(Platform::Twitch)),
            ("youtube.channel.user_banned", Some(Platform::YouTube)),
            ("kick.moderation.banned", Some(Platform::Kick)),
            ("twitch.channel.ban.extra", None),
            ("twitch.channel", None),
            ("TWITCH.CHANNEL.BAN", None),
            ("twitch.chat.message", None),
            ("kick.moderation.unbanned", None),
            ("", None),
        ] {
            assert_eq!(ban_change_platform(kind), expected, "{kind:?}");
        }
    }

    #[test]
    fn changed_platforms_lists_each_platform_once_in_first_seen_order() {
        let batch = [
            event("kick.moderation.banned"),
            event("twitch.chat.message"),
            event("twitch.channel.ban"),
            event("kick.moderation.banned"),
            event("twitch.channel.unban"),
        ];

        assert_eq!(
            changed_platforms(&batch),
            [Platform::Kick, Platform::Twitch]
        );
    }

    #[test]
    fn changed_platforms_is_empty_for_a_batch_without_ban_changes() {
        let batch = [event("twitch.chat.message"), event("obs.scene.changed")];

        assert!(changed_platforms(&batch).is_empty());
        assert!(changed_platforms(&[]).is_empty());
    }

    #[test]
    fn unavailable_label_separates_reconnect_advice_from_transient_failures() {
        install_language(Language::En);
        for (reason, expected) in [
            (BanListUnavailable::NotConnected, "Not connected"),
            (
                BanListUnavailable::MissingScope,
                "Missing permission - reconnect to grant it",
            ),
            (
                BanListUnavailable::QuotaExhausted,
                "Could not load bans - try again later",
            ),
            (
                BanListUnavailable::Transport,
                "Could not load bans - try again later",
            ),
        ] {
            assert_eq!(unavailable_label(reason), expected, "{reason:?}");
        }
    }

    #[test]
    fn expiry_label_is_permanent_for_a_ban_without_an_end() {
        install_language(Language::En);

        assert_eq!(
            expiry_label(
                BanDuration::Permanent,
                UtcOffset::from_hms(3, 0, 0).unwrap()
            ),
            "Permanent"
        );
    }

    #[test]
    fn expiry_label_shows_the_end_in_the_given_offset_across_midnight() {
        install_language(Language::En);
        let ends = OffsetDateTime::from_unix_timestamp(MAR_15_2026_2330_UTC).unwrap();

        let label = expiry_label(
            BanDuration::Until(ends),
            UtcOffset::from_hms(3, 0, 0).unwrap(),
        );

        assert!(label.starts_with("until"), "{label:?}");
        assert!(label.contains("Mar 16, 2026 02:30"), "{label:?}");
    }

    #[test]
    fn optional_label_trims_the_value_and_falls_back_to_a_dash_when_blank() {
        for (value, expected) in [
            (None, DASH),
            (Some(""), DASH),
            (Some("   "), DASH),
            (Some("\t\n"), DASH),
            (Some(" spam links "), "spam links"),
            (
                Some("\u{0441}\u{043f}\u{0430}\u{043c} \u{1f6ab}"),
                "\u{0441}\u{043f}\u{0430}\u{043c} \u{1f6ab}",
            ),
        ] {
            assert_eq!(optional_label(value), expected, "{value:?}");
        }
    }
}
