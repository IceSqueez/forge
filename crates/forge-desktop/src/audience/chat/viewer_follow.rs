use crate::integrations::platform_visuals::BadgeKind;

use forge_components::{DateShape, ForgePalette, fmt_display, tr};
use forge_platform_core::FollowStatus;
use forge_runtime::FollowKnowledgeHandle;
use forge_types::{IntegrationId, PlatformId};
use gpui::{Context, Rgba, SharedString, Task};

use super::INFINITY_GLYPH;
use super::viewer_drawer::ViewerDrawer;
use crate::audience::chat_author::{AuthorHandle, AuthorKey};
use crate::audience::chat_drawer::{DASH, displayed_viewer};
use forge_host::integration_catalog::chat_limits;
use forge_host::integrations::BuiltinRegistry;

pub(crate) struct FollowRequest {
    pub builtin: IntegrationId,
    pub viewer_id: SharedString,
}

pub(crate) fn follow_request(key: &AuthorKey) -> Option<FollowRequest> {
    let AuthorHandle::ViewerId(viewer_id) = &key.handle else {
        return None;
    };
    shows_follow(key.platform).then(|| FollowRequest {
        builtin: key.platform.integration_id(),
        viewer_id: viewer_id.clone(),
    })
}

pub(crate) fn shows_follow(platform: PlatformId) -> bool {
    chat_limits(platform).follow_lookup
}

pub(crate) fn follow_display(
    platform: PlatformId,
    role: Option<BadgeKind>,
    status: Option<FollowStatus>,
    palette: &ForgePalette,
) -> Option<(SharedString, Rgba)> {
    if !shows_follow(platform) {
        return None;
    }
    if role == Some(BadgeKind::Broadcaster) {
        return Some((INFINITY_GLYPH.into(), palette.text_primary));
    }
    Some(match status {
        Some(FollowStatus::FollowedSince(since)) => (
            fmt_display(DateShape::Date, since).into(),
            palette.text_primary,
        ),
        Some(FollowStatus::NotFollowing) => {
            (tr!("chat_follow_not_following").into(), palette.text_muted)
        }
        Some(FollowStatus::Hidden) => (tr!("chat_follow_hidden").into(), palette.text_muted),
        Some(FollowStatus::Unavailable) | None => (DASH.into(), palette.text_faint),
    })
}

struct FollowSource {
    builtins: BuiltinRegistry,
    knowledge: FollowKnowledgeHandle,
}

#[derive(Default)]
pub(crate) struct DisplayedFollow {
    source: Option<FollowSource>,
    viewer: Option<AuthorKey>,
    status: Option<FollowStatus>,
    _peek: Option<Task<()>>,
    _resolve: Option<Task<()>>,
    _changes: Option<Task<()>>,
}

impl DisplayedFollow {
    pub(crate) fn status_of(&self, key: &AuthorKey) -> Option<FollowStatus> {
        if self.viewer.as_ref() == Some(key) {
            self.status
        } else {
            None
        }
    }

    pub(crate) fn retarget(&mut self, viewer: Option<AuthorKey>) -> bool {
        if self.viewer == viewer {
            return false;
        }
        self.viewer = viewer;
        self.status = None;
        true
    }

    pub(crate) fn settle(&mut self, key: &AuthorKey, status: Option<FollowStatus>) -> bool {
        if self.viewer.as_ref() != Some(key) || self.status == status {
            return false;
        }
        self.status = status;
        true
    }
}

impl ViewerDrawer {
    pub(super) fn set_follow_knowledge(
        &mut self,
        builtins: BuiltinRegistry,
        knowledge: FollowKnowledgeHandle,
        cx: &mut Context<Self>,
    ) {
        let mut changes = knowledge.changes();
        self.follows.source = Some(FollowSource {
            builtins,
            knowledge,
        });
        self.follows._changes = Some(cx.spawn(async move |this, cx| {
            while changes.changed().await {
                if this
                    .update(cx, |this, cx| this.peek_displayed_follow(cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.follows.viewer = None;
        self.sync_displayed_follow(cx);
    }

    pub(super) fn sync_displayed_follow(&mut self, cx: &mut Context<Self>) {
        let displayed =
            displayed_viewer(self.selected_viewer.as_ref(), self.feed.read(cx).authors());
        if self.follows.retarget(displayed) {
            self.peek_displayed_follow(cx);
            cx.notify();
        }
    }

    fn peek_displayed_follow(&mut self, cx: &mut Context<Self>) {
        let Some(source) = &self.follows.source else {
            return;
        };
        let Some(key) = self.follows.viewer.clone() else {
            return;
        };
        let Some(request) = follow_request(&key) else {
            return;
        };
        let knowledge = source.knowledge.clone();
        let platform = key.platform;
        let reply = crate::app::async_bridge::on_runtime(&self.services.rt_handle, async move {
            knowledge.peek(platform, &request.viewer_id).await
        });
        self.follows._peek = Some(cx.spawn(async move |this, cx| {
            let Ok(status) = reply.await else {
                return;
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                if this.follows.settle(&key, status) {
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn request_selected_follow(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.selected_viewer.clone() {
            self.resolve_follow(key, cx);
        }
    }

    fn resolve_follow(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        let Some(source) = &self.follows.source else {
            return;
        };
        let Some(request) = follow_request(&key) else {
            return;
        };
        let role = self
            .feed
            .read(cx)
            .authors()
            .get(&key)
            .and_then(|activity| activity.role);
        if role == Some(BadgeKind::Broadcaster) || !self.platform_running(key.platform, cx) {
            return;
        }
        let builtins = source.builtins.clone();
        let knowledge = source.knowledge.clone();
        let platform = key.platform;
        let reply = crate::app::async_bridge::on_runtime(&self.services.rt_handle, async move {
            let lookup = builtins
                .get(&request.builtin)
                .and_then(|object| object.follow);
            match lookup {
                Some(lookup) => {
                    knowledge
                        .resolve(platform, &request.viewer_id, lookup.as_ref())
                        .await
                }
                None => FollowStatus::Unavailable,
            }
        });
        self.follows._resolve = Some(cx.spawn(async move |this, cx| {
            let Ok(status) = reply.await else {
                return;
            };
            if status == FollowStatus::Unavailable {
                return;
            }
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                if this.follows.settle(&key, Some(status)) {
                    cx.notify();
                }
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::integrations::platform_visuals::BadgeKind;
    use forge_components::{DateShape, FORGE_DEFAULT, fmt_display, tr};
    use forge_platform_core::{FollowLookup, FollowStatus};
    use forge_storage::MockChatHistoryRepo;
    use forge_storage::MockVoiceAliasRepo;
    use forge_types::{IntegrationId, PlatformId};
    use gpui::{AppContext as _, Entity, TestAppContext};
    use time::OffsetDateTime;

    use super::{FOLLOW_STATUS_TTL, FollowLookups, KnownFollow, follow_display, follow_request};
    use crate::audience::chat::ChatView;
    use crate::audience::chat::tests::{chat_states, message, mount_gated, mount_in_window};
    use crate::audience::chat_author::AuthorKey;
    use crate::audience::chat_drawer::DASH;
    use crate::integrations::integration_lifecycle::IntegrationLifecycle;
    use crate::test_support::{pump, runtime, switch_lifecycle};
    use forge_host::integrations::{BuiltinObject, BuiltinRegistry};
    use forge_runtime::LifecycleState;

    struct RecordingLookup {
        asked: Mutex<Vec<String>>,
        answer: Mutex<FollowStatus>,
    }

    impl RecordingLookup {
        fn answering(status: FollowStatus) -> Arc<Self> {
            Arc::new(Self {
                asked: Mutex::new(Vec::new()),
                answer: Mutex::new(status),
            })
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl FollowLookup for RecordingLookup {
        async fn follow_status(&self, viewer_id: &str) -> FollowStatus {
            self.asked.lock().unwrap().push(viewer_id.to_owned());
            *self.answer.lock().unwrap()
        }
    }

    fn registry_with(lookup: &Arc<RecordingLookup>) -> BuiltinRegistry {
        let registry = BuiltinRegistry::default();
        for integration in [
            PlatformId::Twitch.integration_id(),
            PlatformId::YouTube.integration_id(),
            PlatformId::Kick.integration_id(),
        ] {
            registry.install(BuiltinObject {
                follow: Some(Arc::clone(lookup) as Arc<dyn FollowLookup>),
                ..crate::integrations::unavailable_builtin::unavailable_builtin(
                    &integration.clone(),
                )
            });
        }
        registry
    }

    fn since() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        crate::test_support::settle_runtime_work(cx, rt);
    }

    fn mount_following(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        lookup: &Arc<RecordingLookup>,
        lifecycle: Option<Entity<IntegrationLifecycle>>,
    ) -> (
        Entity<crate::audience::chat_feed::ChatFeed>,
        Entity<ChatView>,
    ) {
        let (feed, view) = mount_gated(cx, rt, lifecycle, MockChatHistoryRepo::new());
        let registry = registry_with(lookup);
        view.update(cx, |view, cx| {
            view.drawer
                .update(cx, |drawer, _| drawer.set_follow_lookups(registry));
        });
        (feed, view)
    }

    fn open(cx: &mut TestAppContext, view: &Entity<ChatView>, key: &AuthorKey) {
        let key = key.clone();
        view.update(cx, |view, cx| {
            view.drawer
                .update(cx, |drawer, cx| drawer.open_viewer(key, cx))
        });
    }

    fn known(
        cx: &mut TestAppContext,
        view: &Entity<ChatView>,
        key: &AuthorKey,
    ) -> Option<FollowStatus> {
        view.read_with(cx, |view, cx| view.drawer.read(cx).follows.status_of(key))
    }

    #[test]
    fn follow_request_targets_the_platform_builtin_with_the_bare_viewer_id() {
        let cases = [
            (PlatformId::Twitch, "42", "twitch"),
            (PlatformId::YouTube, "UCabc", "youtube"),
        ];
        for (platform, id, builtin) in cases {
            let request = follow_request(&AuthorKey::by_viewer_id(platform, id)).unwrap();
            assert_eq!(request.builtin, IntegrationId::new(builtin));
            assert_eq!(request.viewer_id.as_ref(), id);
        }
    }

    #[test]
    fn follow_request_is_skipped_for_kick_and_name_only_authors() {
        let skipped = [
            AuthorKey::by_viewer_id(PlatformId::Kick, "9"),
            AuthorKey::by_name(PlatformId::Twitch, "ann"),
            AuthorKey::by_name(PlatformId::YouTube, "ann"),
        ];
        for key in skipped {
            assert!(follow_request(&key).is_none(), "{key:?}");
        }
    }

    #[test]
    fn follow_display_renders_each_status_with_its_tone() {
        let p = FORGE_DEFAULT;
        let followed = fmt_display(DateShape::Date, since());
        let cases = [
            (
                Some(FollowStatus::FollowedSince(since())),
                followed,
                p.text_primary,
            ),
            (
                Some(FollowStatus::NotFollowing),
                tr!("chat_follow_not_following"),
                p.text_muted,
            ),
            (
                Some(FollowStatus::Hidden),
                tr!("chat_follow_hidden"),
                p.text_muted,
            ),
            (
                Some(FollowStatus::Unavailable),
                DASH.to_owned(),
                p.text_faint,
            ),
            (None, DASH.to_owned(), p.text_faint),
        ];
        for (status, text, color) in cases {
            let (shown, tone) = follow_display(PlatformId::Twitch, None, status, &p).unwrap();
            assert_eq!(shown.as_ref(), text, "{status:?}");
            assert_eq!(tone, color, "{status:?}");
        }
    }

    #[test]
    fn follow_display_shows_infinity_for_the_broadcaster_whatever_the_status() {
        let p = FORGE_DEFAULT;
        for status in [
            None,
            Some(FollowStatus::NotFollowing),
            Some(FollowStatus::Hidden),
        ] {
            let (shown, tone) = follow_display(
                PlatformId::YouTube,
                Some(BadgeKind::Broadcaster),
                status,
                &p,
            )
            .unwrap();
            assert_eq!(shown.as_ref(), "\u{221e}");
            assert_eq!(tone, p.text_primary);
        }
    }

    #[test]
    fn follow_display_hides_the_tile_on_kick_even_for_the_broadcaster() {
        let p = FORGE_DEFAULT;
        for role in [None, Some(BadgeKind::Broadcaster)] {
            let status = Some(FollowStatus::FollowedSince(since()));
            assert!(follow_display(PlatformId::Kick, role, status, &p).is_none());
        }
    }

    #[test]
    fn settle_caches_every_definitive_status_and_stops_requesting() {
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        for status in [
            FollowStatus::FollowedSince(since()),
            FollowStatus::NotFollowing,
            FollowStatus::Hidden,
        ] {
            let mut lookups = FollowLookups {
                pending: Some(key.clone()),
                ..FollowLookups::default()
            };

            assert!(lookups.settle(key.clone(), status));

            assert_eq!(lookups.status_of(&key), Some(status));
            assert!(!lookups.needs_request(&key));
        }
    }

    #[test]
    fn a_status_past_its_lifetime_stays_shown_but_is_asked_for_again() {
        let Some(checked_at) = std::time::Instant::now().checked_sub(FOLLOW_STATUS_TTL) else {
            return;
        };
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        let mut lookups = FollowLookups::default();
        lookups.known.insert(
            key.clone(),
            KnownFollow {
                status: FollowStatus::NotFollowing,
                checked_at,
            },
        );

        assert_eq!(
            (lookups.status_of(&key), lookups.needs_request(&key)),
            (Some(FollowStatus::NotFollowing), true)
        );
    }

    #[test]
    fn a_stale_status_with_its_refresh_in_flight_is_not_asked_for_twice() {
        let Some(checked_at) = std::time::Instant::now().checked_sub(FOLLOW_STATUS_TTL) else {
            return;
        };
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        let mut lookups = FollowLookups {
            pending: Some(key.clone()),
            ..FollowLookups::default()
        };
        lookups.known.insert(
            key.clone(),
            KnownFollow {
                status: FollowStatus::NotFollowing,
                checked_at,
            },
        );

        assert!(!lookups.needs_request(&key));
    }

    #[test]
    fn settle_never_caches_unavailable_and_allows_a_retry() {
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        let mut lookups = FollowLookups {
            pending: Some(key.clone()),
            ..FollowLookups::default()
        };

        assert!(lookups.settle(key.clone(), FollowStatus::Unavailable));

        assert_eq!(lookups.status_of(&key), None);
        assert!(lookups.needs_request(&key));
    }

    #[test]
    fn settle_drops_a_result_for_a_key_that_is_not_pending() {
        let stale = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        let current = AuthorKey::by_viewer_id(PlatformId::Twitch, "7");
        let mut lookups = FollowLookups {
            pending: Some(current.clone()),
            ..FollowLookups::default()
        };

        assert!(!lookups.settle(stale.clone(), FollowStatus::NotFollowing));

        assert_eq!(lookups.status_of(&stale), None);
        assert!(!lookups.needs_request(&current));
    }

    #[test]
    fn settle_with_nothing_pending_is_dropped() {
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        let mut lookups = FollowLookups::default();

        assert!(!lookups.settle(key.clone(), FollowStatus::Hidden));

        assert_eq!(lookups.status_of(&key), None);
    }

    #[gpui::test]
    fn opening_a_viewer_asks_the_lookup_with_the_bare_id_and_caches_the_answer(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::FollowedSince(since()));
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");

        open(cx, &view, &key);
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["42"]);
        assert_eq!(
            known(cx, &view, &key),
            Some(FollowStatus::FollowedSince(since()))
        );
    }

    #[gpui::test]
    fn youtube_viewers_are_looked_up_with_their_channel_id(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);

        open(
            cx,
            &view,
            &AuthorKey::by_viewer_id(PlatformId::YouTube, "UCabc"),
        );
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["UCabc"]);
    }

    #[gpui::test]
    fn a_cached_answer_is_not_asked_for_again(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::Hidden);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");

        open(cx, &view, &key);
        settle(cx, &rt);
        open(cx, &view, &key);
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["42"]);
    }

    #[gpui::test]
    fn reopening_the_same_viewer_while_the_lookup_is_in_flight_asks_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");

        open(cx, &view, &key);
        open(cx, &view, &key);
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["42"]);
    }

    #[gpui::test]
    fn an_unavailable_answer_is_retried_on_the_next_open(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::Unavailable);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");

        open(cx, &view, &key);
        settle(cx, &rt);
        *lookup.answer.lock().unwrap() = FollowStatus::NotFollowing;
        open(cx, &view, &key);
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["42", "42"]);
        assert_eq!(known(cx, &view, &key), Some(FollowStatus::NotFollowing));
    }

    #[gpui::test]
    fn the_passive_newest_author_card_never_queries(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (feed, _view) = mount_following(cx, &rt, &lookup, None);
        let mut line = message(1, false);
        line.author_id = Some("42".into());

        crate::audience::chat::tests::push_each(cx, &feed, vec![line]);
        settle(cx, &rt);

        assert!(lookup.asked().is_empty());
    }

    #[gpui::test]
    fn viewers_that_cannot_have_a_follow_date_are_never_queried(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);

        open(cx, &view, &AuthorKey::by_viewer_id(PlatformId::Kick, "9"));
        open(cx, &view, &AuthorKey::by_name(PlatformId::Twitch, "ann"));
        settle(cx, &rt);

        assert!(lookup.asked().is_empty());
    }

    #[gpui::test]
    fn the_broadcaster_is_never_queried(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (feed, view) = mount_following(cx, &rt, &lookup, None);
        let mut line = message(1, false);
        line.author_id = Some("42".into());
        line.badges = vec![BadgeKind::Broadcaster];
        crate::audience::chat::tests::push_each(cx, &feed, vec![line]);

        open(
            cx,
            &view,
            &AuthorKey::by_viewer_id(PlatformId::Twitch, "42"),
        );
        settle(cx, &rt);

        assert!(lookup.asked().is_empty());
    }

    #[gpui::test]
    fn a_platform_that_is_not_running_is_not_queried(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[
                (
                    PlatformId::Twitch.integration_id(),
                    LifecycleState::Failed(crate::test_support::start_failure("down")),
                ),
                (
                    PlatformId::YouTube.integration_id(),
                    LifecycleState::Running,
                ),
            ]))
        });
        let (_feed, view) = mount_following(cx, &rt, &lookup, Some(lifecycle));

        open(
            cx,
            &view,
            &AuthorKey::by_viewer_id(PlatformId::Twitch, "42"),
        );
        settle(cx, &rt);

        assert!(lookup.asked().is_empty());
    }

    #[gpui::test]
    fn a_selected_viewer_is_requested_once_its_platform_starts_running(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[(
                PlatformId::Twitch.integration_id(),
                LifecycleState::Starting,
            )]))
        });
        let (_feed, view) = mount_following(cx, &rt, &lookup, Some(lifecycle.clone()));
        let key = AuthorKey::by_viewer_id(PlatformId::Twitch, "42");
        open(cx, &view, &key);
        settle(cx, &rt);
        assert!(lookup.asked().is_empty());

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[(PlatformId::Twitch.integration_id(), LifecycleState::Running)]),
        );
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["42"]);
        assert_eq!(known(cx, &view, &key), Some(FollowStatus::NotFollowing));
    }

    #[gpui::test]
    fn select_viewer_and_open_whisper_also_request_the_follow_date(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (view, vcx) = mount_in_window(cx, &rt, MockVoiceAliasRepo::new());
        let registry = registry_with(&lookup);
        view.update(vcx, |view, cx| {
            view.drawer
                .update(cx, |drawer, _| drawer.set_follow_lookups(registry));
        });

        view.update(vcx, |view, cx| {
            view.drawer.update(cx, |drawer, cx| {
                drawer.open_viewer(AuthorKey::by_viewer_id(PlatformId::Twitch, "1"), cx)
            })
        });
        vcx.run_until_parked();
        pump(&rt);
        vcx.run_until_parked();
        view.update_in(vcx, |view, window, cx| {
            view.drawer.update(cx, |drawer, cx| {
                drawer.open_whisper(AuthorKey::by_viewer_id(PlatformId::Twitch, "2"), window, cx)
            })
        });
        vcx.run_until_parked();
        pump(&rt);
        vcx.run_until_parked();

        assert_eq!(lookup.asked(), ["1", "2"]);
    }
}
