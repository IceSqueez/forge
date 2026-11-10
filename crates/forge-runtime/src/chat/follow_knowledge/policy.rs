use std::sync::Arc;

use forge_events::Event;
use forge_platform_core::FollowStatus;
use forge_registry::{
    ActorRole, ActorSlot, CanonicalVariable, ChatEventKind, TriggerKindDescriptor, TriggerRegistry,
    kind_matches_prefix,
};
use forge_storage::{FollowKnowledgeEntry, FollowKnowledgeKey, KnownFollow};
use forge_types::Variant;
use time::{Duration, OffsetDateTime};

pub(super) const FOLLOWING_LIFETIME: Duration = Duration::days(7);
const HIDDEN_LIFETIME: Duration = Duration::days(7);
const NOT_FOLLOWING_LIFETIME: Duration = Duration::hours(1);

fn lifetime(follow: KnownFollow) -> Duration {
    match follow {
        KnownFollow::FollowingSince(_) => FOLLOWING_LIFETIME,
        KnownFollow::Hidden => HIDDEN_LIFETIME,
        KnownFollow::NotFollowing => NOT_FOLLOWING_LIFETIME,
    }
}

pub(super) fn observed(
    key: FollowKnowledgeKey,
    follow: KnownFollow,
    observed_at: OffsetDateTime,
) -> FollowKnowledgeEntry {
    FollowKnowledgeEntry {
        key,
        follow,
        observed_at,
        fresh_until: observed_at.saturating_add(lifetime(follow)),
    }
}

pub(super) fn is_fresh(entry: &FollowKnowledgeEntry, now: OffsetDateTime) -> bool {
    entry.fresh_until > now
}

pub(super) fn known(status: FollowStatus) -> Option<KnownFollow> {
    match status {
        FollowStatus::FollowedSince(since) => Some(KnownFollow::FollowingSince(since)),
        FollowStatus::NotFollowing => Some(KnownFollow::NotFollowing),
        FollowStatus::Hidden => Some(KnownFollow::Hidden),
        FollowStatus::Unavailable => None,
    }
}

pub(super) fn status(follow: KnownFollow) -> FollowStatus {
    match follow {
        KnownFollow::FollowingSince(since) => FollowStatus::FollowedSince(since),
        KnownFollow::NotFollowing => FollowStatus::NotFollowing,
        KnownFollow::Hidden => FollowStatus::Hidden,
    }
}

pub(super) struct FollowTranslation {
    registry: Arc<TriggerRegistry>,
    follow_kinds: Vec<String>,
    logged_follow_kinds: Vec<String>,
}

impl FollowTranslation {
    pub(super) fn new(registry: Arc<TriggerRegistry>) -> Self {
        let follow_descriptors: Vec<&dyn TriggerKindDescriptor> = registry
            .all()
            .filter(|descriptor| descriptor.chat_event() == Some(ChatEventKind::Follow))
            .collect();
        let follow_kinds = follow_descriptors
            .iter()
            .map(|descriptor| descriptor.id().to_owned())
            .collect();
        let logged_follow_kinds = follow_descriptors
            .iter()
            .filter_map(|descriptor| descriptor.event_filter().kind_prefix)
            .collect();
        Self {
            registry,
            follow_kinds,
            logged_follow_kinds,
        }
    }

    pub(super) fn logged_follow_kinds(&self) -> &[String] {
        &self.logged_follow_kinds
    }

    pub(super) fn entry_of(&self, event: &Event) -> Option<FollowKnowledgeEntry> {
        if event.replay {
            return None;
        }
        let platform = event.source.to_platform_id()?;
        let descriptor = self.follow_descriptor_of(event)?;
        let viewer_id = principal_id(descriptor, event)?;
        let since = descriptor.follow_moment(event).unwrap_or(event.timestamp);
        Some(observed(
            FollowKnowledgeKey {
                platform,
                viewer_id,
            },
            KnownFollow::FollowingSince(since),
            event.timestamp,
        ))
    }

    fn follow_descriptor_of(&self, event: &Event) -> Option<&dyn TriggerKindDescriptor> {
        self.follow_kinds
            .iter()
            .filter_map(|kind_id| self.registry.get(kind_id))
            .find(|descriptor| {
                let filter = descriptor.event_filter();
                filter.source.is_none_or(|source| source == event.source)
                    && filter
                        .kind_prefix
                        .as_deref()
                        .is_none_or(|prefix| kind_matches_prefix(&event.kind, prefix))
            })
    }
}

fn principal_id(descriptor: &dyn TriggerKindDescriptor, event: &Event) -> Option<String> {
    let args = descriptor.build_arg_stack(event);
    match args.get(CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id).name()) {
        Some(Variant::String(id)) if !id.is_empty() => Some(id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use forge_types::PlatformId;

    use super::*;

    const OBSERVED_UNIX_SECS: i64 = 1_791_115_200;

    fn key() -> FollowKnowledgeKey {
        FollowKnowledgeKey {
            platform: PlatformId::Twitch,
            viewer_id: "viewer".to_owned(),
        }
    }

    #[test]
    fn each_known_status_stays_fresh_until_its_lifetime_ends() {
        let observed_at = OffsetDateTime::from_unix_timestamp(OBSERVED_UNIX_SECS).unwrap();
        let following_since = observed_at - Duration::days(30);
        for (follow, lifetime) in [
            (
                KnownFollow::FollowingSince(following_since),
                Duration::days(7),
            ),
            (KnownFollow::Hidden, Duration::days(7)),
            (KnownFollow::NotFollowing, Duration::hours(1)),
        ] {
            let entry = observed(key(), follow, observed_at);
            let end = observed_at + lifetime;
            assert!(
                is_fresh(&entry, end - Duration::nanoseconds(1)),
                "{follow:?} must still be fresh just before its lifetime ends"
            );
            assert!(
                !is_fresh(&entry, end),
                "{follow:?} must expire once its lifetime ends"
            );
        }
    }
}
