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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;

    use forge_storage::Language;

    use super::*;
    use crate::i18n::install_language;

    fn only(platforms: &[PlatformId]) -> PlatformScope {
        PlatformScope::only(platforms.iter().copied().collect::<BTreeSet<_>>()).unwrap()
    }

    #[test]
    fn gated_platforms_keeps_the_contract_platforms_the_scope_admits() {
        use PlatformId::{Kick, Twitch, YouTube};
        for (contract, scope, expected) in [
            (
                KindPlatformContract::Universal,
                PlatformScope::Any,
                vec![Twitch, YouTube, Kick],
            ),
            (
                KindPlatformContract::Universal,
                only(&[Kick, YouTube]),
                vec![YouTube, Kick],
            ),
            (
                KindPlatformContract::PlatformSpecific(Twitch),
                PlatformScope::Any,
                vec![Twitch],
            ),
            (
                KindPlatformContract::PlatformSpecific(Twitch),
                only(&[Kick]),
                vec![],
            ),
            (
                KindPlatformContract::PlatformSpecific(Kick),
                only(&[Twitch, Kick]),
                vec![Kick],
            ),
        ] {
            assert_eq!(
                gated_platforms(contract, &scope),
                expected,
                "{contract:?} under {scope:?}"
            );
        }
    }

    #[test]
    fn each_platform_explains_each_rung_in_its_own_terms() {
        use PermissionRung::{Broadcaster, Moderator, Subscriber, Vip};
        use PlatformId::{Kick, Twitch, YouTube};
        install_language(Language::En);
        for (platform, rung, prefix, marker) in [
            (Twitch, Subscriber, "Twitch:", "subscriber or founder"),
            (Twitch, Vip, "Twitch:", "VIP badge"),
            (Twitch, Moderator, "Twitch:", "moderator badge"),
            (Twitch, Broadcaster, "Twitch:", "owner only"),
            (YouTube, Subscriber, "YouTube:", "members"),
            (
                YouTube,
                Vip,
                "YouTube:",
                "only moderators and the channel owner",
            ),
            (YouTube, Moderator, "YouTube:", "channel moderators"),
            (YouTube, Broadcaster, "YouTube:", "owner only"),
            (Kick, Subscriber, "Kick:", "subscriber or founder"),
            (Kick, Vip, "Kick:", "VIP badge"),
            (Kick, Moderator, "Kick:", "moderator badge"),
            (Kick, Broadcaster, "Kick:", "owner only"),
        ] {
            let lines = rung_meanings(rung, &[platform]);

            assert!(
                lines.len() == 1 && lines[0].starts_with(prefix) && lines[0].contains(marker),
                "{platform:?} {rung:?}: {lines:?}"
            );
        }
    }

    #[test]
    fn rung_meanings_list_one_line_per_gated_platform_in_order() {
        install_language(Language::En);

        let lines = rung_meanings(
            PermissionRung::Moderator,
            &[PlatformId::Kick, PlatformId::Twitch],
        );

        assert_eq!(
            lines
                .iter()
                .map(|line| line.split(':').next().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["Kick", "Twitch"]
        );
    }

    #[test]
    fn the_everyone_rung_reads_as_one_shared_line_whatever_the_platforms() {
        install_language(Language::En);
        let shared = rung_meanings(PermissionRung::Everyone, &[PlatformId::Kick]);

        for platforms in [vec![], vec![PlatformId::Twitch], PlatformId::ALL.to_vec()] {
            assert_eq!(
                rung_meanings(PermissionRung::Everyone, &platforms),
                shared,
                "{platforms:?}"
            );
        }
        assert_eq!(shared.len(), 1);
    }
}
