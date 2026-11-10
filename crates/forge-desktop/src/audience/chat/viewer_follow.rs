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
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use crate::integrations::platform_visuals::BadgeKind;
    use forge_components::{DateShape, FORGE_DEFAULT, fmt_display, tr};
    use forge_platform_core::{FollowLookup, FollowStatus};
    use forge_registry::TriggerRegistry;
    use forge_runtime::{
        EventBus, FollowKnowledgeHandle, FollowKnowledgeParts, LifecycleState,
        spawn_follow_knowledge,
    };
    use forge_storage::{
        DataProvider, FollowKnowledgeEntry, FollowKnowledgeKey, FollowKnowledgeRepo, KnownFollow,
        MockChatHistoryRepo, MockVoiceAliasRepo,
    };
    use forge_storage_sqlite::SqliteBackend;
    use forge_storage_sqlite::test_support::{Sandboxed, sandboxed_backend};
    use forge_types::{IntegrationId, PlatformId};
    use gpui::{AppContext as _, Entity, Task, TestAppContext};
    use time::OffsetDateTime;

    use super::{follow_display, follow_request};
    use crate::audience::chat::ChatView;
    use crate::audience::chat::tests::{
        chat_states, message, mount_gated, mount_in_window, push_each,
    };
    use crate::audience::chat_author::AuthorKey;
    use crate::audience::chat_drawer::DASH;
    use crate::audience::chat_feed::{ChatFeed, ChatMessage};
    use crate::integrations::integration_lifecycle::IntegrationLifecycle;
    use crate::test_support::{
        StubEventLog, pump_until, runtime, settle_runtime_work, settle_until, switch_lifecycle,
    };
    use forge_host::integrations::{BuiltinObject, BuiltinRegistry};

    const VIEWER_ID: &str = "42";
    const STORED_LIFETIME: time::Duration = time::Duration::days(1);

    struct RecordingLookup {
        asked: Mutex<Vec<String>>,
        answer: FollowStatus,
    }

    impl RecordingLookup {
        fn answering(answer: FollowStatus) -> Arc<Self> {
            Arc::new(Self {
                asked: Mutex::new(Vec::new()),
                answer,
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
            self.answer
        }
    }

    struct Knowledge {
        handle: FollowKnowledgeHandle,
        repo: Arc<dyn FollowKnowledgeRepo>,
        _bus: Arc<EventBus>,
        _store: Sandboxed<SqliteBackend>,
    }

    impl Knowledge {
        fn spawn(rt: &tokio::runtime::Runtime) -> Self {
            let store = rt.block_on(sandboxed_backend());
            let _enter = rt.enter();
            let bus = EventBus::new(Arc::new(StubEventLog));
            let repo = store.follow_knowledge_repo();
            let handle = spawn_follow_knowledge(FollowKnowledgeParts {
                bus: Arc::clone(&bus),
                repo: Arc::clone(&repo),
                event_log: store.event_log_repo(),
                triggers: Arc::new(TriggerRegistry::new()),
            });
            Self {
                handle,
                repo,
                _bus: bus,
                _store: store,
            }
        }

        fn store(&self, rt: &tokio::runtime::Runtime, key: &AuthorKey, follow: KnownFollow) {
            let observed_at = OffsetDateTime::now_utc();
            let entry = FollowKnowledgeEntry {
                key: FollowKnowledgeKey {
                    platform: key.platform,
                    viewer_id: viewer_id_of(key),
                },
                follow,
                observed_at,
                fresh_until: observed_at + STORED_LIFETIME,
            };
            rt.block_on(self.repo.upsert(&entry)).unwrap();
        }

        fn record(&self, rt: &tokio::runtime::Runtime, key: &AuthorKey, status: FollowStatus) {
            let elsewhere = RecordingLookup::answering(status);
            let recorded = rt.block_on(self.handle.resolve(
                key.platform,
                &viewer_id_of(key),
                elsewhere.as_ref(),
            ));
            assert_eq!(recorded, status);
        }
    }

    struct Following {
        feed: Entity<ChatFeed>,
        view: Entity<ChatView>,
        knowledge: Knowledge,
        lookup: Arc<RecordingLookup>,
    }

    impl Following {
        fn mount(
            cx: &mut TestAppContext,
            rt: &tokio::runtime::Runtime,
            answer: FollowStatus,
            lifecycle: Option<Entity<IntegrationLifecycle>>,
        ) -> Self {
            let knowledge = Knowledge::spawn(rt);
            let lookup = RecordingLookup::answering(answer);
            let (feed, view) = mount_gated(cx, rt, lifecycle, MockChatHistoryRepo::new());
            let registry = registry_with(&lookup);
            let handle = knowledge.handle.clone();
            view.update(cx, |view, cx| {
                view.drawer.update(cx, |drawer, cx| {
                    drawer.set_follow_knowledge(registry, handle, cx)
                })
            });
            settle_runtime_work(cx, rt);
            Self {
                feed,
                view,
                knowledge,
                lookup,
            }
        }

        fn post(&self, cx: &mut TestAppContext, line: ChatMessage) -> AuthorKey {
            let key = line.author_key().unwrap();
            push_each(cx, &self.feed, vec![line]);
            key
        }

        fn open(&self, cx: &mut TestAppContext, key: &AuthorKey) {
            let key = key.clone();
            self.view.update(cx, |view, cx| {
                view.drawer
                    .update(cx, |drawer, cx| drawer.open_viewer(key, cx))
            });
        }

        fn point_at(&self, cx: &mut TestAppContext, key: &AuthorKey) {
            let key = key.clone();
            self.view.update(cx, |view, cx| {
                view.drawer
                    .update(cx, |drawer, cx| drawer.point_at_viewer(key, cx))
            });
        }

        fn shown(&self, cx: &mut TestAppContext, key: &AuthorKey) -> Option<FollowStatus> {
            shown_on(&self.view, cx, key)
        }

        fn settle_shown(
            &self,
            cx: &mut TestAppContext,
            rt: &tokio::runtime::Runtime,
            key: &AuthorKey,
            expected: FollowStatus,
        ) {
            let view = self.view.clone();
            settle_until(cx, rt, |cx| shown_on(&view, cx, key) == Some(expected));
        }

        fn resolve_started(&self, cx: &mut TestAppContext) -> bool {
            self.view.read_with(cx, |view, cx| {
                view.drawer.read(cx).follows._resolve.is_some()
            })
        }

        fn finish_resolve(&self, cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
            let task = self.view.update(cx, |view, cx| {
                view.drawer
                    .update(cx, |drawer, _| drawer.follows._resolve.take())
            });
            await_task(cx, rt, task.unwrap());
        }

        fn finish_peek(&self, cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
            let task = self.view.update(cx, |view, cx| {
                view.drawer
                    .update(cx, |drawer, _| drawer.follows._peek.take())
            });
            await_task(cx, rt, task.unwrap());
        }

        fn settle_asked(&self, cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
            let lookup = Arc::clone(&self.lookup);
            settle_until(cx, rt, |_| !lookup.asked().is_empty());
        }
    }

    fn await_task(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime, task: Task<()>) {
        let done = Rc::new(Cell::new(false));
        let finished = Rc::clone(&done);
        cx.spawn(async move |_| {
            task.await;
            finished.set(true);
        })
        .detach();
        settle_until(cx, rt, |_| done.get());
    }

    fn shown_on(
        view: &Entity<ChatView>,
        cx: &mut TestAppContext,
        key: &AuthorKey,
    ) -> Option<FollowStatus> {
        view.read_with(cx, |view, cx| view.drawer.read(cx).follows.status_of(key))
    }

    fn viewer_id_of(key: &AuthorKey) -> String {
        follow_request(key).unwrap().viewer_id.to_string()
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
                ..crate::integrations::unavailable_builtin::unavailable_builtin(&integration)
            });
        }
        registry
    }

    fn line_from(ix: usize, viewer_id: &str) -> ChatMessage {
        let mut line = message(ix, false);
        line.author_id = Some(viewer_id.to_owned().into());
        line
    }

    fn twitch_viewer() -> AuthorKey {
        AuthorKey::by_viewer_id(PlatformId::Twitch, VIEWER_ID)
    }

    fn since() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn twitch_lifecycle(
        cx: &mut TestAppContext,
        state: LifecycleState,
    ) -> Entity<IntegrationLifecycle> {
        cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[(PlatformId::Twitch.integration_id(), state)]))
        })
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

    #[gpui::test]
    fn opening_a_viewer_resolves_once_with_the_bare_id_and_shows_the_answer(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::FollowedSince(since()), None);
        let key = rig.post(cx, line_from(1, VIEWER_ID));

        rig.open(cx, &key);
        rig.finish_resolve(cx, &rt);

        assert_eq!(
            (rig.lookup.asked(), rig.shown(cx, &key)),
            (
                vec![VIEWER_ID.to_owned()],
                Some(FollowStatus::FollowedSince(since()))
            )
        );
    }

    #[gpui::test]
    fn pointing_at_a_viewer_resolves_it_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);

        rig.point_at(cx, &twitch_viewer());
        rig.finish_resolve(cx, &rt);

        assert_eq!(rig.lookup.asked(), [VIEWER_ID]);
    }

    #[gpui::test]
    fn opening_a_whisper_resolves_the_viewer_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let knowledge = Knowledge::spawn(&rt);
        let lookup = RecordingLookup::answering(FollowStatus::NotFollowing);
        let (view, vcx) = mount_in_window(cx, &rt, MockVoiceAliasRepo::new());
        let registry = registry_with(&lookup);
        let handle = knowledge.handle.clone();
        view.update(vcx, |view, cx| {
            view.drawer.update(cx, |drawer, cx| {
                drawer.set_follow_knowledge(registry, handle, cx)
            })
        });

        view.update_in(vcx, |view, window, cx| {
            view.drawer.update(cx, |drawer, cx| {
                drawer.open_whisper(twitch_viewer(), window, cx)
            })
        });
        pump_until(&rt, || !lookup.asked().is_empty());

        assert_eq!(lookup.asked(), [VIEWER_ID]);
    }

    #[gpui::test]
    fn youtube_viewers_are_resolved_with_their_channel_id(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);

        rig.point_at(cx, &AuthorKey::by_viewer_id(PlatformId::YouTube, "UCabc"));
        rig.finish_resolve(cx, &rt);

        assert_eq!(rig.lookup.asked(), ["UCabc"]);
    }

    #[gpui::test]
    fn a_fresh_stored_status_is_shown_without_a_lookup(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);
        rig.knowledge
            .store(&rt, &twitch_viewer(), KnownFollow::Hidden);
        let key = rig.post(cx, line_from(1, VIEWER_ID));

        rig.open(cx, &key);
        rig.finish_resolve(cx, &rt);

        assert_eq!(
            (rig.lookup.asked(), rig.shown(cx, &key)),
            (Vec::<String>::new(), Some(FollowStatus::Hidden))
        );
    }

    #[gpui::test]
    fn the_passive_newest_author_card_shows_a_stored_status_without_resolving(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);
        rig.knowledge
            .store(&rt, &twitch_viewer(), KnownFollow::FollowingSince(since()));

        let key = rig.post(cx, line_from(1, VIEWER_ID));
        rig.settle_shown(cx, &rt, &key, FollowStatus::FollowedSince(since()));

        assert!(!rig.resolve_started(cx));
    }

    #[gpui::test]
    fn a_store_change_updates_the_displayed_card_without_reopening(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);
        let key = rig.post(cx, line_from(1, VIEWER_ID));
        rig.finish_peek(cx, &rt);
        assert_eq!(rig.shown(cx, &key), None);

        rig.knowledge
            .record(&rt, &key, FollowStatus::FollowedSince(since()));

        rig.settle_shown(cx, &rt, &key, FollowStatus::FollowedSince(since()));
    }

    #[gpui::test]
    fn the_broadcaster_is_never_resolved(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);
        let mut line = line_from(1, VIEWER_ID);
        line.badges = vec![BadgeKind::Broadcaster];
        let key = rig.post(cx, line);

        rig.open(cx, &key);

        assert!(!rig.resolve_started(cx));
    }

    #[gpui::test]
    fn viewers_without_a_followable_id_are_never_resolved(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);

        rig.open(cx, &AuthorKey::by_viewer_id(PlatformId::Kick, "9"));
        rig.open(cx, &AuthorKey::by_name(PlatformId::Twitch, "ann"));

        assert!(!rig.resolve_started(cx));
    }

    #[gpui::test]
    fn a_platform_that_is_not_running_is_not_resolved(cx: &mut TestAppContext) {
        let rt = runtime();
        let lifecycle = twitch_lifecycle(
            cx,
            LifecycleState::Failed(crate::test_support::start_failure("down")),
        );
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, Some(lifecycle));

        rig.open(cx, &twitch_viewer());

        assert!(!rig.resolve_started(cx));
    }

    #[gpui::test]
    fn a_selected_viewer_is_resolved_once_its_platform_starts_running(cx: &mut TestAppContext) {
        let rt = runtime();
        let lifecycle = twitch_lifecycle(cx, LifecycleState::Starting);
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, Some(lifecycle.clone()));
        rig.open(cx, &twitch_viewer());
        assert!(!rig.resolve_started(cx));

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[(PlatformId::Twitch.integration_id(), LifecycleState::Running)]),
        );
        rig.settle_asked(cx, &rt);

        assert_eq!(rig.lookup.asked(), [VIEWER_ID]);
    }

    #[gpui::test]
    fn an_unavailable_lookup_leaves_the_card_without_a_status(cx: &mut TestAppContext) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::Unavailable, None);
        let key = rig.post(cx, line_from(1, VIEWER_ID));

        rig.open(cx, &key);
        rig.finish_resolve(cx, &rt);

        assert_eq!(
            (rig.lookup.asked(), rig.shown(cx, &key)),
            (vec![VIEWER_ID.to_owned()], None)
        );
    }

    #[gpui::test]
    fn switching_the_displayed_viewer_discards_a_late_peek_for_the_old_one(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let rig = Following::mount(cx, &rt, FollowStatus::NotFollowing, None);
        rig.knowledge
            .store(&rt, &twitch_viewer(), KnownFollow::FollowingSince(since()));
        rig.post(cx, line_from(1, VIEWER_ID));

        let name_only = rig.post(cx, message(2, false));
        rig.finish_peek(cx, &rt);

        assert_eq!(rig.shown(cx, &name_only), None);
    }
}
