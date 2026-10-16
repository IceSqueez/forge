use forge_components::{ForgePalette, PlatformMark, RoleBadge, SendTarget, tr};
use forge_types::PlatformId;
use gpui::{Rgba, SharedString};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Moderator,
    Vip,
    Subscriber,
    Bot,
    Broadcaster,
    Partner,
    Premium,
    Founder,
    Turbo,
    HypeTrain,
    Bits,
    BitsLeader,
}

pub fn platform_color(platform: PlatformId, palette: &ForgePalette) -> Rgba {
    match platform {
        PlatformId::Twitch => palette.platform_twitch,
        PlatformId::YouTube => palette.platform_youtube,
        PlatformId::Kick => palette.platform_kick,
    }
}

pub fn platform_letter(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "T",
        PlatformId::YouTube => "Y",
        PlatformId::Kick => "K",
    }
}

pub fn platform_mark(platform: PlatformId, palette: &ForgePalette) -> PlatformMark {
    PlatformMark {
        letter: SharedString::new_static(platform_letter(platform)),
        color: platform_color(platform, palette),
    }
}

pub fn send_target(platform: PlatformId, palette: &ForgePalette) -> SendTarget {
    SendTarget {
        id: send_target_id(platform),
        mark: platform_mark(platform, palette),
    }
}

pub fn send_target_id(platform: PlatformId) -> SharedString {
    SharedString::new_static(platform.as_str())
}

pub fn send_target_platforms(ids: &[SharedString]) -> Vec<PlatformId> {
    ids.iter().filter_map(|id| PlatformId::parse(id)).collect()
}

pub fn badge_color(kind: BadgeKind, palette: &ForgePalette) -> Rgba {
    match kind {
        BadgeKind::Broadcaster => palette.warning,
        BadgeKind::Moderator => palette.success,
        BadgeKind::Vip => palette.brand,
        BadgeKind::Subscriber => palette.bits,
        BadgeKind::Bot => palette.info,
        BadgeKind::Partner => palette.accent_teal,
        BadgeKind::Premium => palette.accent_pink_light,
        BadgeKind::Founder => palette.accent_pink_light,
        BadgeKind::Turbo => palette.brand,
        BadgeKind::HypeTrain => palette.warning,
        BadgeKind::Bits => palette.bits,
        BadgeKind::BitsLeader => palette.bits,
    }
}

pub fn badge_label(kind: BadgeKind) -> String {
    match kind {
        BadgeKind::Broadcaster => tr!("chat_badge_owner"),
        BadgeKind::Moderator => tr!("chat_badge_mod"),
        BadgeKind::Vip => tr!("chat_badge_vip"),
        BadgeKind::Subscriber => tr!("chat_badge_sub"),
        BadgeKind::Bot => tr!("chat_badge_bot"),
        BadgeKind::Partner => tr!("chat_badge_partner"),
        BadgeKind::Premium => tr!("chat_badge_prime"),
        BadgeKind::Founder => tr!("chat_badge_founder"),
        BadgeKind::Turbo => tr!("chat_badge_turbo"),
        BadgeKind::HypeTrain => tr!("chat_badge_hype"),
        BadgeKind::Bits => tr!("chat_badge_bits"),
        BadgeKind::BitsLeader => tr!("chat_badge_bits_leader"),
    }
}

pub fn role_badge(kind: BadgeKind, palette: &ForgePalette) -> RoleBadge {
    RoleBadge {
        label: badge_label(kind).into(),
        color: badge_color(kind, palette),
    }
}
