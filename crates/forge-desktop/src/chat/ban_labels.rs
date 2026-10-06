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
