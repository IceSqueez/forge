use forge_components::{ForgePalette, tr};

use crate::chat_input_bar::SendTarget;
use crate::chat_row::{PlatformMark, RoleBadge};
use forge_types::PlatformId;
use gpui::{Rgba, SharedString};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BadgeKind {
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

pub(crate) fn platform_color(platform: PlatformId, palette: &ForgePalette) -> Rgba {
    match platform {
        PlatformId::Twitch => palette.platform_twitch,
        PlatformId::YouTube => palette.platform_youtube,
        PlatformId::Kick => palette.platform_kick,
    }
}

pub(crate) fn platform_letter(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "T",
        PlatformId::YouTube => "Y",
        PlatformId::Kick => "K",
    }
}

pub(crate) fn platform_mark(platform: PlatformId, palette: &ForgePalette) -> PlatformMark {
    PlatformMark {
        letter: SharedString::new_static(platform_letter(platform)),
        color: platform_color(platform, palette),
    }
}

pub(crate) fn send_target(platform: PlatformId, palette: &ForgePalette) -> SendTarget {
    SendTarget {
        id: send_target_id(platform),
        mark: platform_mark(platform, palette),
    }
}

pub(crate) fn send_target_id(platform: PlatformId) -> SharedString {
    SharedString::new_static(platform.as_str())
}

pub(crate) fn send_target_platforms(ids: &[SharedString]) -> Vec<PlatformId> {
    ids.iter().filter_map(|id| PlatformId::parse(id)).collect()
}

pub(crate) fn badge_color(kind: BadgeKind, palette: &ForgePalette) -> Rgba {
    match kind {
        BadgeKind::Broadcaster => palette.warning,
        BadgeKind::Moderator => palette.success,
        BadgeKind::Vip => palette.theme_brand,
        BadgeKind::Subscriber => palette.accent_peach,
        BadgeKind::Bot => palette.info,
        BadgeKind::Partner => palette.accent_teal,
        BadgeKind::Premium => palette.accent_pink_light,
        BadgeKind::Founder => palette.accent_pink_light,
        BadgeKind::Turbo => palette.theme_brand,
        BadgeKind::HypeTrain => palette.warning,
        BadgeKind::Bits => palette.accent_peach,
        BadgeKind::BitsLeader => palette.accent_peach,
    }
}

pub(crate) fn badge_label(kind: BadgeKind) -> String {
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

pub(crate) fn role_badge(kind: BadgeKind, palette: &ForgePalette) -> RoleBadge {
    RoleBadge {
        label: badge_label(kind).into(),
        color: badge_color(kind, palette),
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{FORGE_DEFAULT, ForgePalette};
    use forge_types::PlatformId;
    use gpui::{Rgba, SharedString, rgba};

    use super::{
        platform_color, platform_mark, send_target, send_target_id, send_target_platforms,
    };

    const TWITCH_SENTINEL: u32 = 0x0a0b0cff;
    const YOUTUBE_SENTINEL: u32 = 0x1a1b1cff;
    const KICK_SENTINEL: u32 = 0x2a2b2cff;

    fn sentinel_palette() -> ForgePalette {
        ForgePalette {
            platform_twitch: rgba(TWITCH_SENTINEL),
            platform_youtube: rgba(YOUTUBE_SENTINEL),
            platform_kick: rgba(KICK_SENTINEL),
            ..FORGE_DEFAULT
        }
    }

    fn expected_marks() -> [(PlatformId, &'static str, Rgba); 3] {
        [
            (PlatformId::Twitch, "T", rgba(TWITCH_SENTINEL)),
            (PlatformId::YouTube, "Y", rgba(YOUTUBE_SENTINEL)),
            (PlatformId::Kick, "K", rgba(KICK_SENTINEL)),
        ]
    }

    #[test]
    fn platform_color_reads_each_platforms_own_palette_field() {
        let palette = sentinel_palette();
        for (platform, _, color) in expected_marks() {
            assert_eq!(platform_color(platform, &palette), color, "{platform:?}");
        }
    }

    #[test]
    fn platform_mark_pairs_the_platform_letter_with_its_palette_color() {
        let palette = sentinel_palette();
        for (platform, letter, color) in expected_marks() {
            let mark = platform_mark(platform, &palette);
            assert_eq!(
                (mark.letter.as_ref(), mark.color),
                (letter, color),
                "{platform:?}"
            );
        }
    }

    #[test]
    fn send_target_carries_the_id_and_mark_of_the_same_platform() {
        let palette = sentinel_palette();
        for platform in PlatformId::ALL {
            let target = send_target(platform, &palette);
            assert_eq!(target.id, send_target_id(platform), "{platform:?}");
            assert_eq!(
                target.mark,
                platform_mark(platform, &palette),
                "{platform:?}"
            );
        }
    }

    #[test]
    fn send_target_ids_decode_back_to_their_platforms_in_order() {
        let ids: Vec<SharedString> = [PlatformId::Kick, PlatformId::Twitch, PlatformId::YouTube]
            .into_iter()
            .map(send_target_id)
            .collect();

        assert_eq!(
            send_target_platforms(&ids),
            vec![PlatformId::Kick, PlatformId::Twitch, PlatformId::YouTube]
        );
    }

    #[test]
    fn send_target_platforms_drops_ids_that_are_not_exact_platform_ids() {
        let ids: Vec<SharedString> = ["Twitch", "trovo", "", " kick", "youtube"]
            .into_iter()
            .map(SharedString::new_static)
            .collect();

        assert_eq!(send_target_platforms(&ids), vec![PlatformId::YouTube]);
    }
}
