use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Spacing,
    body_family, fmt_relative_time, modal, mono_family, overlay, spacing, tr,
};
use forge_storage::{ChatAuthorKey, ChatAuthorPage, ChatHistoryRepo};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, IntoElement, Pixels, Render, Rgba,
    SharedString, Subscription, Task, Window, div, prelude::*, px,
};

use super::{body_export_text, platform_display_name};
use crate::chat_author::AuthorKey;
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::chat_viewer_messages::{ViewerMessages, stored_author};
use crate::presentation::ActivePresentation;

const HISTORY_LOAD_CAP: usize = 100;
const HISTORY_MAX_H: Pixels = px(420.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryState {
    Loading,
    Failed,
    Loaded,
}

pub(crate) struct ViewerHistoryDismissed;

pub(crate) struct ViewerHistory {
    feed: Entity<ChatFeed>,
    messages: Entity<ViewerMessages>,
    repo: Arc<dyn ChatHistoryRepo>,
    rt_handle: tokio::runtime::Handle,
    viewer_name: SharedString,
    key: AuthorKey,
    state: HistoryState,
    stored: Vec<ChatMessage>,
    shown: Vec<ChatMessage>,
    total: u64,
    _load: Option<Task<()>>,
    _feed_obs: Subscription,
    _messages_obs: Subscription,
}

impl EventEmitter<ViewerHistoryDismissed> for ViewerHistory {}

fn moderated_ids_by(feed: &ChatFeed, key: &AuthorKey) -> HashSet<SharedString> {
    feed.messages()
        .iter()
        .filter(|message| {
            message.moderated
                && !message.id.is_empty()
                && message.author_key().as_ref() == Some(key)
        })
        .map(|message| message.id.clone())
        .collect()
}

fn mark_moderated(shown: &mut [ChatMessage], moderated_ids: &HashSet<SharedString>) -> bool {
    let mut changed = false;
    for message in shown
        .iter_mut()
        .filter(|message| !message.moderated && moderated_ids.contains(&message.id))
    {
        message.moderated = true;
        changed = true;
    }
    changed
}

fn merge_newest_first(
    known: Vec<ChatMessage>,
    arrived: Vec<ChatMessage>,
    cap: usize,
) -> Vec<ChatMessage> {
    let known_ids: HashSet<SharedString> = known
        .iter()
        .filter(|message| !message.id.is_empty())
        .map(|message| message.id.clone())
        .collect();
    let mut merged: Vec<ChatMessage> = arrived
        .into_iter()
        .filter(|message| message.id.is_empty() || !known_ids.contains(&message.id))
        .chain(known)
        .collect();
    merged.sort_by_key(|message| Reverse(message.received_at));
    merged.truncate(cap);
    merged
}

impl ViewerHistory {
    pub fn new(
        key: AuthorKey,
        feed: Entity<ChatFeed>,
        messages: Entity<ViewerMessages>,
        repo: Arc<dyn ChatHistoryRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let feed_obs = cx.observe(&feed, |this, _feed, cx| this.remark_moderated(cx));
        let messages_obs = cx.observe(&messages, |this, _messages, cx| {
            this.rebuild_shown(cx);
            cx.notify();
        });
        let mut history = Self {
            feed,
            messages,
            repo,
            rt_handle,
            viewer_name: SharedString::default(),
            key,
            state: HistoryState::Loading,
            stored: Vec::new(),
            shown: Vec::new(),
            total: 0,
            _load: None,
            _feed_obs: feed_obs,
            _messages_obs: messages_obs,
        };
        match stored_author(&history.key) {
            Some(query) => history.spawn_load(query, cx),
            None => {
                history.state = HistoryState::Loaded;
                history.rebuild_shown(cx);
            }
        }
        history
    }

    #[must_use]
    pub fn titled(mut self, viewer_name: impl Into<SharedString>) -> Self {
        self.viewer_name = viewer_name.into();
        self
    }

    fn spawn_load(&mut self, query: ChatAuthorKey, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(repo.author_page(&query, HISTORY_LOAD_CAP).await);
        });
        self._load = Some(cx.spawn(async move |this, cx| {
            let page = match rx.await {
                Ok(Ok(page)) => Some(page),
                Ok(Err(err)) => {
                    eprintln!("forge-desktop: viewer history load failed: {err}");
                    None
                }
                Err(_) => None,
            };
            this.update(cx, |this, cx| this.apply_loaded(page, cx)).ok();
        }));
    }

    fn apply_loaded(&mut self, page: Option<ChatAuthorPage>, cx: &mut Context<Self>) {
        self._load = None;
        match page {
            None => self.state = HistoryState::Failed,
            Some(page) => {
                self.stored = page.rows.iter().map(ChatMessage::from_row).collect();
                self.state = HistoryState::Loaded;
                let feed = self.feed.clone();
                let key = self.key.clone();
                self.messages.update(cx, |messages, cx| {
                    messages.adopt(&key, page.tally, feed.read(cx));
                    cx.notify();
                });
                self.rebuild_shown(cx);
            }
        }
        cx.notify();
    }

    fn rebuild_shown(&mut self, cx: &mut Context<Self>) {
        if self.state != HistoryState::Loaded {
            return;
        }
        let messages = self.messages.read(cx);
        self.total = messages.count(&self.key);
        let unsaved: Vec<ChatMessage> = messages.unsaved(&self.key).iter().rev().cloned().collect();
        let mut shown = merge_newest_first(self.stored.clone(), unsaved, HISTORY_LOAD_CAP);
        mark_moderated(&mut shown, &moderated_ids_by(self.feed.read(cx), &self.key));
        self.shown = shown;
    }

    fn remark_moderated(&mut self, cx: &mut Context<Self>) {
        let moderated_ids = moderated_ids_by(self.feed.read(cx), &self.key);
        if mark_moderated(&mut self.shown, &moderated_ids) {
            cx.notify();
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(ViewerHistoryDismissed);
    }

    fn render_note(text: impl Into<SharedString>, color: Rgba) -> AnyElement {
        div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .child(text.into())
            .into_any_element()
    }

    fn render_row(message: &ChatMessage, palette: &ForgePalette, density: Density) -> AnyElement {
        let text_color = if message.moderated {
            palette.text_faint
        } else {
            palette.text_primary
        };
        div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(fmt_relative_time(Some(message.received_at))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text_color)
                    .child(body_export_text(&message.body)),
            )
            .into_any_element()
    }

    fn render_body(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        match self.state {
            HistoryState::Loading => {
                Self::render_note(tr!("chat_drawer_history_loading"), palette.text_faint)
            }
            HistoryState::Failed => {
                Self::render_note(tr!("chat_drawer_history_failed"), palette.random)
            }
            HistoryState::Loaded if self.shown.is_empty() => {
                Self::render_note(tr!("chat_drawer_history_empty"), palette.text_faint)
            }
            HistoryState::Loaded => div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .children(self.truncated_note(palette))
                .child(
                    div()
                        .id("chat-viewer-history")
                        .max_h(HISTORY_MAX_H)
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(spacing(Spacing::Xs, density))
                        .children(
                            self.shown
                                .iter()
                                .map(|message| Self::render_row(message, palette, density)),
                        ),
                )
                .into_any_element(),
        }
    }

    fn truncated_note(&self, palette: &ForgePalette) -> Option<AnyElement> {
        let shown = self.shown.len() as u64;
        (self.total > shown).then(|| {
            let shown = i64::try_from(shown).unwrap_or(i64::MAX);
            let total = i64::try_from(self.total).unwrap_or(i64::MAX);
            Self::render_note(
                tr!(
                    "chat_drawer_history_truncated",
                    shown = shown,
                    total = total
                ),
                palette.text_faint,
            )
        })
    }
}

impl Render for ViewerHistory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let card = modal(
            self.viewer_name.clone(),
            self.render_body(&palette, density),
            &palette,
        )
        .size(ModalSize::Md)
        .header_icon(Icon::History, palette.brand)
        .subtitle(platform_display_name(self.key.platform))
        .on_close(
            "chat-viewer-history-close",
            cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss(cx)),
        );
        let view = cx.entity();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                overlay(card, &palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("chat-viewer-history-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.dismiss(cx));
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_components::{ChatBody, Platform};
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::{ChatAuthorKey, StorageError};
    use forge_types::{ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow};
    use gpui::{AppContext as _, Entity, TestAppContext};
    use time::OffsetDateTime;

    use super::{
        HISTORY_LOAD_CAP, HistoryState, ViewerHistory, feed_messages_by, history_query,
        mark_moderated, merge_newest_first, moderated_ids_by,
    };
    use crate::chat::ChatView;
    use crate::chat::tests::{message, mount_with_chat_history, push_each};
    use crate::chat_author::AuthorKey;
    use crate::chat_feed::{ChatFeed, ChatMessage};
    use crate::test_support::{pump, runtime};

    type Author = (Platform, Option<&'static str>, &'static str);

    const ANN: Author = (Platform::Twitch, Some("42"), "ann");
    const BOB: Author = (Platform::Twitch, Some("7"), "bob");

    fn key_of((platform, id, name): Author) -> AuthorKey {
        match id {
            Some(id) => AuthorKey::by_viewer_id(platform, id),
            None => AuthorKey::by_name(platform, name),
        }
    }

    fn line(id: &str, at: i64, (platform, author_id, name): Author) -> ChatMessage {
        ChatMessage {
            id: id.to_owned().into(),
            event_id: EventId::new(),
            timestamp: "00:00:00".into(),
            received_at: OffsetDateTime::from_unix_timestamp(at).unwrap(),
            platform,
            badges: vec![],
            username: name.into(),
            author_id: author_id.map(Into::into),
            author_color: None,
            body: ChatBody::Message("hi".into()),
            is_event: false,
            is_bot: false,
            moderated: false,
            reply: None,
        }
    }

    fn stored(id: &str, at: i64, (_, author_id, name): Author) -> UnifiedChatRow {
        UnifiedChatRow {
            id: id.to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: OffsetDateTime::from_unix_timestamp(at).unwrap(),
            author: name.to_owned(),
            author_id: author_id.map(str::to_owned),
            author_color: None,
            body_segments: vec![ChatSegment::Text {
                text: "hi".to_owned(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn ids(messages: &[ChatMessage]) -> Vec<String> {
        messages.iter().map(|m| m.id.to_string()).collect()
    }

    fn feed_of(messages: Vec<ChatMessage>) -> ChatFeed {
        let mut feed = ChatFeed::new();
        for message in messages {
            feed.push(message);
        }
        feed
    }

    fn query_of((_, author_id, _): Author) -> ChatAuthorKey {
        ChatAuthorKey {
            source: ChatSource::Twitch,
            author_id: author_id.unwrap().to_owned(),
        }
    }

    fn repo_serving(entries: Vec<(Author, Vec<UnifiedChatRow>)>) -> MockChatHistoryRepo {
        let mut repo = MockChatHistoryRepo::new();
        for (author, rows) in entries {
            let query = query_of(author);
            repo.expect_list_recent_messages_by_author()
                .withf(move |asked, limit| *asked == query && *limit == HISTORY_LOAD_CAP)
                .returning(move |_, _| Ok(rows.clone()));
        }
        repo
    }

    fn mount_history(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        key: AuthorKey,
        repo: MockChatHistoryRepo,
        live: Vec<ChatMessage>,
    ) -> (Entity<ChatFeed>, Entity<ViewerHistory>) {
        let feed = cx.new(|_| feed_of(live));
        let history = cx.new(|cx| {
            ViewerHistory::new(key, feed.clone(), Arc::new(repo), rt.handle().clone(), cx)
        });
        (feed, history)
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        cx.run_until_parked();
        pump(rt);
        cx.run_until_parked();
    }

    fn state(cx: &mut TestAppContext, history: &Entity<ViewerHistory>) -> HistoryState {
        history.read_with(cx, |history, _| history.state.clone())
    }

    fn loaded_ids(cx: &mut TestAppContext, history: &Entity<ViewerHistory>) -> Vec<String> {
        match state(cx, history) {
            HistoryState::Loaded(messages) => ids(&messages),
            other => panic!("expected a loaded history, got {other:?}"),
        }
    }

    #[test]
    fn history_query_asks_by_viewer_id_on_the_viewers_platform_and_skips_name_only_keys() {
        let cases = [
            (
                AuthorKey::by_viewer_id(Platform::Twitch, "42"),
                Some((ChatSource::Twitch, "42")),
            ),
            (
                AuthorKey::by_viewer_id(Platform::YouTube, "UCx"),
                Some((ChatSource::YouTube, "UCx")),
            ),
            (
                AuthorKey::by_viewer_id(Platform::Kick, "9"),
                Some((ChatSource::Kick, "9")),
            ),
            (AuthorKey::by_name(Platform::Kick, "ann"), None),
        ];
        for (key, expected) in cases {
            let expected = expected.map(|(source, id)| ChatAuthorKey {
                source,
                author_id: id.to_owned(),
            });
            assert_eq!(history_query(&key), expected, "{key:?}");
        }
    }

    #[test]
    fn feed_messages_by_keeps_only_the_viewers_chat_lines_newest_first() {
        let mut event = line("e1", 3, ANN);
        event.is_event = true;
        let same_id_elsewhere = line("y1", 4, (Platform::YouTube, Some("42"), "ann"));
        let feed = feed_of(vec![
            line("m1", 1, ANN),
            line("b1", 2, BOB),
            event,
            same_id_elsewhere,
            line("m2", 5, ANN),
        ]);

        let found = feed_messages_by(&feed, &key_of(ANN), feed.start_seq());

        assert_eq!(ids(&found), ["m2", "m1"]);
    }

    #[test]
    fn feed_messages_by_scans_from_the_given_seq_inclusive() {
        let feed = feed_of(vec![
            line("m0", 0, ANN),
            line("m1", 1, ANN),
            line("m2", 2, ANN),
        ]);
        let cases: [(u64, &[&str]); 4] = [
            (0, &["m2", "m1", "m0"]),
            (1, &["m2", "m1"]),
            (2, &["m2"]),
            (3, &[]),
        ];
        for (from_seq, expected) in cases {
            let found = feed_messages_by(&feed, &key_of(ANN), from_seq);
            assert_eq!(ids(&found), expected, "from seq {from_seq}");
        }
    }

    #[test]
    fn merge_drops_an_arrived_message_whose_id_is_already_known() {
        let merged = merge_newest_first(
            vec![line("m1", 10, ANN)],
            vec![line("m1", 10, ANN), line("m2", 20, ANN)],
            HISTORY_LOAD_CAP,
        );

        assert_eq!(ids(&merged), ["m2", "m1"]);
    }

    #[test]
    fn merge_never_treats_messages_without_an_id_as_duplicates() {
        let merged = merge_newest_first(
            vec![line("", 10, ANN)],
            vec![line("", 20, ANN)],
            HISTORY_LOAD_CAP,
        );

        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_orders_both_inputs_together_newest_first() {
        let merged = merge_newest_first(
            vec![line("k3", 30, ANN), line("k1", 10, ANN)],
            vec![line("a4", 40, ANN), line("a2", 20, ANN)],
            HISTORY_LOAD_CAP,
        );

        assert_eq!(ids(&merged), ["a4", "k3", "a2", "k1"]);
    }

    #[test]
    fn merge_keeps_only_the_newest_messages_up_to_the_cap() {
        const CAP: usize = 3;
        for total in [CAP - 1, CAP, CAP + 1] {
            let all: Vec<ChatMessage> = (0..total)
                .map(|ix| line(&format!("m{ix}"), ix as i64, ANN))
                .collect();
            let (known, arrived) = all.split_at(total / 2);

            let merged = merge_newest_first(known.to_vec(), arrived.to_vec(), CAP);

            let expected: Vec<String> = (0..total)
                .rev()
                .take(CAP)
                .map(|ix| format!("m{ix}"))
                .collect();
            assert_eq!(ids(&merged), expected, "{total} messages");
        }
    }

    fn loaded_moderated_ids(
        cx: &mut TestAppContext,
        history: &Entity<ViewerHistory>,
    ) -> Vec<String> {
        match state(cx, history) {
            HistoryState::Loaded(messages) => messages
                .iter()
                .filter(|m| m.moderated)
                .map(|m| m.id.to_string())
                .collect(),
            other => panic!("expected a loaded history, got {other:?}"),
        }
    }

    #[gpui::test]
    fn a_viewer_with_an_id_shows_stored_and_live_lines_newest_first_without_duplicates(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let repo = repo_serving(vec![(
            ANN,
            vec![stored("m3", 30, ANN), stored("m1", 10, ANN)],
        )]);
        let live = vec![
            line("m1", 10, ANN),
            line("b9", 40, BOB),
            line("m5", 50, ANN),
        ];
        let (_feed, history) = mount_history(cx, &rt, key_of(ANN), repo, live);

        settle(cx, &rt);

        assert_eq!(loaded_ids(cx, &history), ["m5", "m3", "m1"]);
    }

    #[gpui::test]
    fn a_viewer_known_only_by_name_needs_an_id_instead_of_loading(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_feed, history) = mount_history(
            cx,
            &rt,
            AuthorKey::by_name(Platform::Kick, "ann"),
            MockChatHistoryRepo::new(),
            vec![],
        );

        settle(cx, &rt);

        assert!(matches!(state(cx, &history), HistoryState::NeedsViewerId));
    }

    #[gpui::test]
    fn a_failed_load_shows_the_failed_state(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_list_recent_messages_by_author()
            .returning(|_, _| {
                Err(StorageError::Connection {
                    reason: "closed".into(),
                })
            });
        let (_feed, history) = mount_history(cx, &rt, key_of(ANN), repo, vec![line("m1", 10, ANN)]);

        settle(cx, &rt);

        assert!(matches!(state(cx, &history), HistoryState::Failed));
    }

    #[gpui::test]
    fn lines_the_viewer_sends_while_open_are_added_once_newest_first(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo = repo_serving(vec![(ANN, vec![])]);
        let (feed, history) = mount_history(cx, &rt, key_of(ANN), repo, vec![line("", 10, ANN)]);
        settle(cx, &rt);

        feed.update(cx, |feed, cx| {
            feed.push(line("m2", 20, ANN));
            feed.push(line("b3", 30, BOB));
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(loaded_ids(cx, &history), ["m2", ""]);
    }

    #[gpui::test]
    fn a_line_moderated_in_the_feed_while_open_dims_in_the_history(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo = repo_serving(vec![(ANN, vec![])]);
        let (feed, history) = mount_history(
            cx,
            &rt,
            key_of(ANN),
            repo,
            vec![line("m1", 10, ANN), line("m2", 20, ANN)],
        );
        settle(cx, &rt);

        feed.update(cx, |feed, cx| {
            feed.mark_deleted("m1");
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(loaded_moderated_ids(cx, &history), ["m1"]);
    }

    #[gpui::test]
    fn a_stored_line_already_moderated_in_the_feed_loads_dimmed(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo = repo_serving(vec![(
            ANN,
            vec![stored("m2", 20, ANN), stored("m1", 10, ANN)],
        )]);
        let mut moderated = line("m1", 10, ANN);
        moderated.moderated = true;
        let (_feed, history) = mount_history(cx, &rt, key_of(ANN), repo, vec![moderated]);

        settle(cx, &rt);

        assert_eq!(loaded_moderated_ids(cx, &history), ["m1"]);
    }

    #[test]
    fn moderated_ids_by_collects_only_the_viewers_moderated_lines_that_carry_an_id() {
        let moderated = |mut message: ChatMessage| {
            message.moderated = true;
            message
        };
        let feed = feed_of(vec![
            moderated(line("m1", 1, ANN)),
            line("m2", 2, ANN),
            moderated(line("b1", 3, BOB)),
            moderated(line("", 4, ANN)),
            moderated(line("y1", 5, (Platform::YouTube, Some("42"), "ann"))),
        ]);

        let found = moderated_ids_by(&feed, &key_of(ANN));

        assert_eq!(found, HashSet::from(["m1".into()]));
    }

    #[test]
    fn mark_moderated_dims_only_shown_lines_whose_id_was_moderated() {
        let mut shown = vec![line("m1", 1, ANN), line("m2", 2, ANN)];

        let changed = mark_moderated(&mut shown, &HashSet::from(["m1".into(), "x9".into()]));

        assert!(changed);
        assert_eq!(
            shown.iter().map(|m| m.moderated).collect::<Vec<_>>(),
            [true, false]
        );
    }

    #[test]
    fn mark_moderated_reports_no_change_when_nothing_is_newly_moderated() {
        let mut already = line("m1", 1, ANN);
        already.moderated = true;
        let cases: [(&str, Vec<ChatMessage>, Vec<&str>); 3] = [
            ("no moderated ids", vec![line("m1", 1, ANN)], vec![]),
            ("id not shown", vec![line("m1", 1, ANN)], vec!["x9"]),
            ("already dimmed", vec![already], vec!["m1"]),
        ];
        for (name, mut shown, ids) in cases {
            let ids: HashSet<_> = ids.into_iter().map(Into::into).collect();

            assert!(!mark_moderated(&mut shown, &ids), "{name}");
        }
    }

    fn chatter(ix: usize, viewer_id: &str) -> ChatMessage {
        let mut chatter = message(ix, false);
        chatter.author_id = Some(viewer_id.to_owned().into());
        chatter
    }

    fn viewer(viewer_id: &str) -> AuthorKey {
        AuthorKey::by_viewer_id(Platform::Twitch, viewer_id)
    }

    fn counting_repo() -> (MockChatHistoryRepo, Arc<AtomicUsize>) {
        let queries = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&queries);
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_list_recent_messages_by_author()
            .returning(move |_, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(vec![])
            });
        (repo, queries)
    }

    fn open_dialog(
        cx: &mut TestAppContext,
        view: &Entity<ChatView>,
    ) -> Option<Entity<ViewerHistory>> {
        view.read_with(cx, |view, _| {
            view.viewer_history.as_ref().map(|host| host.view.clone())
        })
    }

    fn dialog_viewer(cx: &mut TestAppContext, view: &Entity<ChatView>) -> Option<AuthorKey> {
        let dialog = open_dialog(cx, view)?;
        Some(dialog.read_with(cx, |history, _| history.key.clone()))
    }

    fn open_history_for(cx: &mut TestAppContext, view: &Entity<ChatView>, viewer_id: &str) {
        view.update(cx, |view, cx| {
            view.open_viewer_history(viewer(viewer_id), "user0".to_owned(), cx);
        });
    }

    #[gpui::test]
    fn opening_the_history_shows_a_dialog_for_that_viewer(cx: &mut TestAppContext) {
        let rt = runtime();
        let (repo, _queries) = counting_repo();
        let (feed, view) = mount_with_chat_history(cx, &rt, repo);
        push_each(cx, &feed, vec![chatter(0, "10"), chatter(1, "11")]);

        open_history_for(cx, &view, "10");
        settle(cx, &rt);

        assert_eq!(dialog_viewer(cx, &view), Some(viewer("10")));
    }

    #[gpui::test]
    fn opening_the_history_queries_storage_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let (repo, queries) = counting_repo();
        let (feed, view) = mount_with_chat_history(cx, &rt, repo);
        push_each(cx, &feed, vec![chatter(0, "10")]);

        open_history_for(cx, &view, "10");
        settle(cx, &rt);

        assert_eq!(queries.load(Ordering::SeqCst), 1);
    }

    #[gpui::test]
    fn no_storage_query_happens_while_the_history_dialog_is_closed(cx: &mut TestAppContext) {
        let rt = runtime();
        let (repo, queries) = counting_repo();
        let (feed, view) = mount_with_chat_history(cx, &rt, repo);

        push_each(cx, &feed, vec![chatter(0, "10"), chatter(1, "11")]);
        view.update(cx, |view, cx| view.open_viewer(viewer("10"), cx));
        view.update(cx, |view, cx| view.select_viewer(viewer("11"), cx));
        push_each(cx, &feed, vec![chatter(2, "12")]);
        settle(cx, &rt);

        assert_eq!(
            (queries.load(Ordering::SeqCst), dialog_viewer(cx, &view)),
            (0, None)
        );
    }

    #[gpui::test]
    fn dismissing_the_history_dialog_closes_it_and_drops_the_history(cx: &mut TestAppContext) {
        let rt = runtime();
        let (repo, _queries) = counting_repo();
        let (feed, view) = mount_with_chat_history(cx, &rt, repo);
        push_each(cx, &feed, vec![chatter(0, "10")]);
        open_history_for(cx, &view, "10");
        settle(cx, &rt);
        let dialog = open_dialog(cx, &view).unwrap();
        let released = dialog.downgrade();

        dialog.update(cx, |history, cx| history.dismiss(cx));
        drop(dialog);
        cx.run_until_parked();

        assert_eq!(
            (
                open_dialog(cx, &view).is_none(),
                released.upgrade().is_none()
            ),
            (true, true)
        );
    }

    #[gpui::test]
    fn the_open_history_stays_on_its_viewer_when_the_card_shows_someone_else(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (repo, queries) = counting_repo();
        let (feed, view) = mount_with_chat_history(cx, &rt, repo);
        push_each(cx, &feed, vec![chatter(0, "10")]);
        open_history_for(cx, &view, "10");
        settle(cx, &rt);

        view.update(cx, |view, cx| view.select_viewer(viewer("11"), cx));
        push_each(cx, &feed, vec![chatter(1, "11")]);
        settle(cx, &rt);

        assert_eq!(
            (dialog_viewer(cx, &view), queries.load(Ordering::SeqCst)),
            (Some(viewer("10")), 1)
        );
    }
}
