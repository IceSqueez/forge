use std::collections::HashMap;

use forge_components::{BadgeKind, ForgePalette, Platform, fmt_short_date, tr};
use forge_platform_core::FollowStatus;
use forge_types::IntegrationId;
use gpui::{Context, Rgba, SharedString, Task};

use super::platform_gate::platform_integration;
use super::{ChatView, INFINITY_GLYPH};
use crate::chat_author::{AuthorHandle, AuthorKey};
use crate::chat_drawer::DASH;
use crate::integrations::BuiltinRegistry;

pub(crate) struct FollowRequest {
    pub builtin: IntegrationId,
    pub viewer_id: SharedString,
}

pub(crate) fn follow_request(key: &AuthorKey) -> Option<FollowRequest> {
    let AuthorHandle::ViewerId(viewer_id) = &key.handle else {
        return None;
    };
    shows_follow(key.platform).then(|| FollowRequest {
        builtin: platform_integration(key.platform).builtin_id(),
        viewer_id: viewer_id.clone(),
    })
}

pub(crate) fn shows_follow(platform: Platform) -> bool {
    match platform {
        Platform::Twitch | Platform::YouTube => true,
        Platform::Kick => false,
    }
}

pub(crate) fn follow_display(
    platform: Platform,
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
        Some(FollowStatus::FollowedSince(since)) => {
            (fmt_short_date(&since).into(), palette.text_primary)
        }
        Some(FollowStatus::NotFollowing) => {
            (tr!("chat_follow_not_following").into(), palette.text_muted)
        }
        Some(FollowStatus::Hidden) => (tr!("chat_follow_hidden").into(), palette.text_muted),
        Some(FollowStatus::Unavailable) | None => (DASH.into(), palette.text_faint),
    })
}

#[derive(Default)]
pub(crate) struct FollowLookups {
    builtins: BuiltinRegistry,
    known: HashMap<AuthorKey, FollowStatus>,
    pending: Option<AuthorKey>,
    _request: Option<Task<()>>,
}

impl FollowLookups {
    pub fn new(builtins: BuiltinRegistry) -> Self {
        Self {
            builtins,
            ..Self::default()
        }
    }

    pub fn status_of(&self, key: &AuthorKey) -> Option<FollowStatus> {
        self.known.get(key).copied()
    }

    pub fn needs_request(&self, key: &AuthorKey) -> bool {
        !self.known.contains_key(key) && self.pending.as_ref() != Some(key)
    }

    fn begin(&mut self, key: AuthorKey, request: Task<()>) {
        self.pending = Some(key);
        self._request = Some(request);
    }

    pub fn settle(&mut self, key: AuthorKey, status: FollowStatus) -> bool {
        if self.pending.as_ref() != Some(&key) {
            return false;
        }
        self.pending = None;
        if status != FollowStatus::Unavailable {
            self.known.insert(key, status);
        }
        true
    }
}

impl ChatView {
    #[must_use]
    pub fn with_follow_lookups(mut self, builtins: BuiltinRegistry) -> Self {
        self.follows = FollowLookups::new(builtins);
        self
    }

    pub(super) fn request_selected_follow(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.selected_viewer.clone() {
            self.request_follow(&key, cx);
        }
    }

    fn request_follow(&mut self, key: &AuthorKey, cx: &mut Context<Self>) {
        let Some(request) = follow_request(key) else {
            return;
        };
        let role = self
            .feed
            .read(cx)
            .authors()
            .get(key)
            .and_then(|activity| activity.role);
        if role == Some(BadgeKind::Broadcaster)
            || !self.follows.needs_request(key)
            || !self.platform_running(key.platform, cx)
        {
            return;
        }
        let builtins = self.follows.builtins.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let lookup = builtins
                .get(&request.builtin)
                .and_then(|object| object.follow);
            let status = match lookup {
                Some(lookup) => lookup.follow_status(&request.viewer_id).await,
                None => FollowStatus::Unavailable,
            };
            let _ = tx.send(status);
        });
        let pending = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let status = rx.await.unwrap_or(FollowStatus::Unavailable);
            let _ = this.update(cx, |this, cx| {
                if this.follows.settle(pending, status) {
                    cx.notify();
                }
            });
        });
        self.follows.begin(key.clone(), task);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use forge_components::{BadgeKind, FORGE_DEFAULT, Platform, fmt_short_date, tr};
    use forge_platform_core::{
        BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
        DetailSection, FollowLookup, FollowStatus, HeaderAction, HealthMetric, HealthStream,
        HealthValue, QuickAction, QuickActions, SectionIcon,
    };
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::voice_aliases::MockVoiceAliasRepo;
    use forge_types::IntegrationId;
    use gpui::{AppContext as _, Entity, TestAppContext};
    use time::OffsetDateTime;

    use super::{FollowLookups, follow_display, follow_request};
    use crate::chat::ChatView;
    use crate::chat::tests::{chat_states, message, mount_gated, mount_in_window};
    use crate::chat_author::AuthorKey;
    use crate::chat_drawer::DASH;
    use crate::home_stats::Integration;
    use crate::integration_lifecycle::IntegrationLifecycle;
    use crate::integration_supervisor::LifecycleState;
    use crate::integrations::{BuiltinObject, BuiltinRegistry};
    use crate::test_support::{pump, runtime, switch_lifecycle};

    struct StubBuiltin {
        id: IntegrationId,
    }

    impl BuiltinStatus for StubBuiltin {
        fn id(&self) -> &IntegrationId {
            &self.id
        }
        fn display_name(&self) -> &str {
            "stub"
        }
        fn version(&self) -> Option<&str> {
            None
        }
        fn connection(&self) -> ConnectionState {
            ConnectionState::Connected
        }
        fn uptime(&self) -> Option<std::time::Duration> {
            None
        }
        fn endpoint(&self) -> Option<&str> {
            None
        }
        fn capability_flags(&self) -> CapabilityFlags {
            CapabilityFlags {
                limited: false,
                label: None,
            }
        }
        fn header_actions(&self) -> Vec<HeaderAction> {
            Vec::new()
        }
    }

    impl BuiltinHealth for StubBuiltin {
        fn metrics(&self) -> [HealthMetric; 4] {
            std::array::from_fn(|i| HealthMetric {
                label: format!("metric{i}"),
                value: HealthValue::Text {
                    primary: String::new(),
                    secondary: None,
                },
            })
        }
        fn stream(&self) -> HealthStream {
            Box::pin(futures_util::stream::empty())
        }
    }

    impl BuiltinContent for StubBuiltin {
        fn sections(&self) -> Vec<DetailSection> {
            Vec::new()
        }
    }

    impl QuickActions for StubBuiltin {
        fn actions(&self) -> Vec<QuickAction> {
            Vec::new()
        }
    }

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
        for integration in [Integration::Twitch, Integration::YouTube, Integration::Kick] {
            let stub = Arc::new(StubBuiltin {
                id: integration.builtin_id(),
            });
            registry.install(BuiltinObject {
                icon: SectionIcon::new("bug"),
                status: stub.clone(),
                health: stub.clone(),
                content: stub.clone(),
                quick: stub,
                control: None,
                collections: None,
                obs_client: None,
                vtube_client: None,
                follow: Some(Arc::clone(lookup) as Arc<dyn FollowLookup>),
            });
        }
        registry
    }

    fn since() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        cx.run_until_parked();
        pump(rt);
        cx.run_until_parked();
    }

    fn mount_following(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        lookup: &Arc<RecordingLookup>,
        lifecycle: Option<Entity<IntegrationLifecycle>>,
    ) -> (Entity<crate::chat_feed::ChatFeed>, Entity<ChatView>) {
        let (feed, view) = mount_gated(cx, rt, lifecycle, MockChatHistoryRepo::new());
        let registry = registry_with(lookup);
        view.update(cx, |view, _| {
            view.follows = FollowLookups::new(registry);
        });
        (feed, view)
    }

    fn open(cx: &mut TestAppContext, view: &Entity<ChatView>, key: &AuthorKey) {
        let key = key.clone();
        view.update(cx, |view, cx| view.open_viewer(key, cx));
    }

    fn known(
        cx: &mut TestAppContext,
        view: &Entity<ChatView>,
        key: &AuthorKey,
    ) -> Option<FollowStatus> {
        view.read_with(cx, |view, _| view.follows.status_of(key))
    }

    #[test]
    fn follow_request_targets_the_platform_builtin_with_the_bare_viewer_id() {
        let cases = [
            (Platform::Twitch, "42", "twitch"),
            (Platform::YouTube, "UCabc", "youtube"),
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
            AuthorKey::by_viewer_id(Platform::Kick, "9"),
            AuthorKey::by_name(Platform::Twitch, "ann"),
            AuthorKey::by_name(Platform::YouTube, "ann"),
        ];
        for key in skipped {
            assert!(follow_request(&key).is_none(), "{key:?}");
        }
    }

    #[test]
    fn follow_display_renders_each_status_with_its_tone() {
        let p = FORGE_DEFAULT;
        let followed = fmt_short_date(&since());
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
            let (shown, tone) = follow_display(Platform::Twitch, None, status, &p).unwrap();
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
            let (shown, tone) =
                follow_display(Platform::YouTube, Some(BadgeKind::Broadcaster), status, &p)
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
            assert!(follow_display(Platform::Kick, role, status, &p).is_none());
        }
    }

    #[test]
    fn settle_caches_every_definitive_status_and_stops_requesting() {
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");
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
    fn settle_never_caches_unavailable_and_allows_a_retry() {
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");
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
        let stale = AuthorKey::by_viewer_id(Platform::Twitch, "42");
        let current = AuthorKey::by_viewer_id(Platform::Twitch, "7");
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
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");
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
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");

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
            &AuthorKey::by_viewer_id(Platform::YouTube, "UCabc"),
        );
        settle(cx, &rt);

        assert_eq!(lookup.asked(), ["UCabc"]);
    }

    #[gpui::test]
    fn a_cached_answer_is_not_asked_for_again(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::Hidden);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");

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
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");

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
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");

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

        crate::chat::tests::push_each(cx, &feed, vec![line]);
        settle(cx, &rt);

        assert!(lookup.asked().is_empty());
    }

    #[gpui::test]
    fn viewers_that_cannot_have_a_follow_date_are_never_queried(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (_feed, view) = mount_following(cx, &rt, &lookup, None);

        open(cx, &view, &AuthorKey::by_viewer_id(Platform::Kick, "9"));
        open(cx, &view, &AuthorKey::by_name(Platform::Twitch, "ann"));
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
        crate::chat::tests::push_each(cx, &feed, vec![line]);

        open(cx, &view, &AuthorKey::by_viewer_id(Platform::Twitch, "42"));
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
                    Integration::Twitch,
                    LifecycleState::Failed("down".to_owned()),
                ),
                (Integration::YouTube, LifecycleState::Running),
            ]))
        });
        let (_feed, view) = mount_following(cx, &rt, &lookup, Some(lifecycle));

        open(cx, &view, &AuthorKey::by_viewer_id(Platform::Twitch, "42"));
        settle(cx, &rt);

        assert!(lookup.asked().is_empty());
    }

    #[gpui::test]
    fn a_selected_viewer_is_requested_once_its_platform_starts_running(cx: &mut TestAppContext) {
        let rt = runtime();
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[(
                Integration::Twitch,
                LifecycleState::Starting,
            )]))
        });
        let (_feed, view) = mount_following(cx, &rt, &lookup, Some(lifecycle.clone()));
        let key = AuthorKey::by_viewer_id(Platform::Twitch, "42");
        open(cx, &view, &key);
        settle(cx, &rt);
        assert!(lookup.asked().is_empty());

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[(Integration::Twitch, LifecycleState::Running)]),
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
        view.update(vcx, |view, _| {
            view.follows = FollowLookups::new(registry);
        });

        view.update(vcx, |view, cx| {
            view.select_viewer(AuthorKey::by_viewer_id(Platform::Twitch, "1"), cx)
        });
        vcx.run_until_parked();
        pump(&rt);
        vcx.run_until_parked();
        view.update_in(vcx, |view, window, cx| {
            view.open_whisper(AuthorKey::by_viewer_id(Platform::Twitch, "2"), window, cx)
        });
        vcx.run_until_parked();
        pump(&rt);
        vcx.run_until_parked();

        assert_eq!(lookup.asked(), ["1", "2"]);
    }
}
