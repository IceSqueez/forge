use forge_components::tr;
use forge_registry::KindPlatformContract;
use forge_types::{PermissionRung, PlatformId, PlatformScope};

pub(super) fn gated_platforms(
    contract: KindPlatformContract,
    scope: &PlatformScope,
) -> Vec<PlatformId> {
    let candidates = match contract {
        KindPlatformContract::PlatformSpecific(platform) => vec![platform],
        KindPlatformContract::Universal => PlatformId::ALL.to_vec(),
    };
    candidates
        .into_iter()
        .filter(|platform| scope.matches(Some(*platform)))
        .collect()
}

pub(super) fn rung_meanings(rung: PermissionRung, platforms: &[PlatformId]) -> Vec<String> {
    if rung == PermissionRung::Everyone {
        return vec![tr!("triggers_permission_meaning_everyone")];
    }
    platforms
        .iter()
        .map(|platform| rung_meaning(*platform, rung))
        .collect()
}

fn rung_meaning(platform: PlatformId, rung: PermissionRung) -> String {
    match (platform, rung) {
        (_, PermissionRung::Everyone) => tr!("triggers_permission_meaning_everyone"),
        (PlatformId::Twitch, PermissionRung::Subscriber) => {
            tr!("triggers_permission_meaning_twitch_subscriber")
        }
        (PlatformId::Twitch, PermissionRung::Vip) => tr!("triggers_permission_meaning_twitch_vip"),
        (PlatformId::Twitch, PermissionRung::Moderator) => {
            tr!("triggers_permission_meaning_twitch_moderator")
        }
        (PlatformId::Twitch, PermissionRung::Broadcaster) => {
            tr!("triggers_permission_meaning_twitch_broadcaster")
        }
        (PlatformId::YouTube, PermissionRung::Subscriber) => {
            tr!("triggers_permission_meaning_youtube_subscriber")
        }
        (PlatformId::YouTube, PermissionRung::Vip) => {
            tr!("triggers_permission_meaning_youtube_vip")
        }
        (PlatformId::YouTube, PermissionRung::Moderator) => {
            tr!("triggers_permission_meaning_youtube_moderator")
        }
        (PlatformId::YouTube, PermissionRung::Broadcaster) => {
            tr!("triggers_permission_meaning_youtube_broadcaster")
        }
        (PlatformId::Kick, PermissionRung::Subscriber) => {
            tr!("triggers_permission_meaning_kick_subscriber")
        }
        (PlatformId::Kick, PermissionRung::Vip) => tr!("triggers_permission_meaning_kick_vip"),
        (PlatformId::Kick, PermissionRung::Moderator) => {
            tr!("triggers_permission_meaning_kick_moderator")
        }
        (PlatformId::Kick, PermissionRung::Broadcaster) => {
            tr!("triggers_permission_meaning_kick_broadcaster")
        }
    }
}
