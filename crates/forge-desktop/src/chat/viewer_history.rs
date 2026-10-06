use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Platform, Spacing,
    body_family, fmt_relative_time, modal, mono_family, overlay, spacing, tr,
};
use forge_storage::{ChatAuthorKey, ChatHistoryRepo};
use forge_types::{ChatSource, UnifiedChatRow};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, IntoElement, Pixels, Render, Rgba,
    SharedString, Subscription, Task, Window, div, prelude::*, px,
};

use super::{body_export_text, platform_display_name};
use crate::chat_author::{AuthorHandle, AuthorKey};
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::presentation::ActivePresentation;

const HISTORY_LOAD_CAP: usize = 100;
const HISTORY_MAX_H: Pixels = px(420.0);

#[derive(Clone, Debug)]
enum HistoryState {
    Hidden,
    NeedsViewerId,
    Loading,
    Failed,
    Loaded(Vec<ChatMessage>),
}

pub(crate) struct ViewerHistoryDismissed;

pub(crate) struct ViewerHistory {
    feed: Entity<ChatFeed>,
    repo: Arc<dyn ChatHistoryRepo>,
    rt_handle: tokio::runtime::Handle,
    viewer_name: SharedString,
    key: Option<AuthorKey>,
    state: HistoryState,
    scanned_through: u64,
    _load: Option<Task<()>>,
    _feed_obs: Option<Subscription>,
}

impl EventEmitter<ViewerHistoryDismissed> for ViewerHistory {}

fn history_query(key: &AuthorKey) -> Option<ChatAuthorKey> {
    match &key.handle {
        AuthorHandle::ViewerId(id) => Some(ChatAuthorKey {
            source: chat_source(key.platform),
            author_id: id.to_string(),
        }),
        AuthorHandle::Name(_) => None,
    }
}

fn chat_source(platform: Platform) -> ChatSource {
    match platform {
        Platform::Twitch => ChatSource::Twitch,
        Platform::YouTube => ChatSource::YouTube,
        Platform::Kick => ChatSource::Kick,
    }
}

fn feed_messages_by(feed: &ChatFeed, key: &AuthorKey, from_seq: u64) -> Vec<ChatMessage> {
    let mut found: Vec<ChatMessage> = (from_seq.max(feed.start_seq())..feed.end_seq())
        .filter_map(|seq| feed.get(seq))
        .filter(|message| !message.is_event && message.author_key().as_ref() == Some(key))
        .cloned()
        .collect();
    found.reverse();
    found
}

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
        feed: Entity<ChatFeed>,
        repo: Arc<dyn ChatHistoryRepo>,
        rt_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            feed,
            repo,
            rt_handle,
            viewer_name: SharedString::default(),
            key: None,
            state: HistoryState::Hidden,
            scanned_through: 0,
            _load: None,
            _feed_obs: None,
        }
    }

    #[must_use]
    pub fn titled(mut self, viewer_name: impl Into<SharedString>) -> Self {
        self.viewer_name = viewer_name.into();
        self
    }

    pub fn show(&mut self, key: Option<AuthorKey>, cx: &mut Context<Self>) {
        if self.key == key {
            self.absorb_live(cx);
            return;
        }
        self.key = key.clone();
        self._load = None;
        self._feed_obs = key
            .is_some()
            .then(|| cx.observe(&self.feed, |this, _feed, cx| this.absorb_live(cx)));
        self.state = match key {
            None => HistoryState::Hidden,
            Some(key) => match history_query(&key) {
                None => HistoryState::NeedsViewerId,
                Some(query) => {
                    self.spawn_load(key, query, cx);
                    HistoryState::Loading
                }
            },
        };
        cx.notify();
    }

    fn spawn_load(&mut self, key: AuthorKey, query: ChatAuthorKey, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                repo.list_recent_messages_by_author(&query, HISTORY_LOAD_CAP)
                    .await,
            );
        });
        self._load = Some(cx.spawn(async move |this, cx| {
            let rows = match rx.await {
                Ok(Ok(rows)) => Some(rows),
                Ok(Err(err)) => {
                    eprintln!("forge-desktop: viewer history load failed: {err}");
                    None
                }
                Err(_) => None,
            };
            this.update(cx, |this, cx| this.apply_loaded(&key, rows, cx))
                .ok();
        }));
    }

    fn apply_loaded(
        &mut self,
        key: &AuthorKey,
        rows: Option<Vec<UnifiedChatRow>>,
        cx: &mut Context<Self>,
    ) {
        if self.key.as_ref() != Some(key) {
            return;
        }
        self._load = None;
        let feed = self.feed.read(cx);
        self.state = match rows {
            None => HistoryState::Failed,
            Some(rows) => {
                let stored = rows.iter().map(ChatMessage::from_row).collect();
                let live = feed_messages_by(feed, key, feed.start_seq());
                let mut merged = merge_newest_first(stored, live, HISTORY_LOAD_CAP);
                mark_moderated(&mut merged, &moderated_ids_by(feed, key));
                HistoryState::Loaded(merged)
            }
        };
        self.scanned_through = feed.end_seq();
        cx.notify();
    }

    fn absorb_live(&mut self, cx: &mut Context<Self>) {
        let (Some(key), HistoryState::Loaded(known)) = (&self.key, &mut self.state) else {
            return;
        };
        let feed = self.feed.read(cx);
        let arrived = feed_messages_by(feed, key, self.scanned_through);
        self.scanned_through = feed.end_seq();
        let remarked = mark_moderated(known, &moderated_ids_by(feed, key));
        if arrived.is_empty() && !remarked {
            return;
        }
        if !arrived.is_empty() {
            *known = merge_newest_first(std::mem::take(known), arrived, HISTORY_LOAD_CAP);
        }
        cx.notify();
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
        match &self.state {
            HistoryState::Hidden => div().into_any_element(),
            HistoryState::NeedsViewerId => {
                Self::render_note(tr!("chat_drawer_history_needs_id"), palette.text_faint)
            }
            HistoryState::Loading => {
                Self::render_note(tr!("chat_drawer_history_loading"), palette.text_faint)
            }
            HistoryState::Failed => {
                Self::render_note(tr!("chat_drawer_history_failed"), palette.random)
            }
            HistoryState::Loaded(messages) if messages.is_empty() => {
                Self::render_note(tr!("chat_drawer_history_empty"), palette.text_faint)
            }
            HistoryState::Loaded(messages) => div()
                .id("chat-viewer-history")
                .max_h(HISTORY_MAX_H)
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .children(
                    messages
                        .iter()
                        .map(|message| Self::render_row(message, palette, density)),
                )
                .into_any_element(),
        }
    }
}

impl Render for ViewerHistory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(key) = self.key.as_ref() else {
            return div().into_any_element();
        };
        let palette = cx.palette();
        let density = cx.density();
        let card = modal(
            self.viewer_name.clone(),
            self.render_body(&palette, density),
            &palette,
        )
        .size(ModalSize::Md)
        .header_icon(Icon::History, palette.brand)
        .subtitle(platform_display_name(key.platform))
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
    use std::sync::Arc;

    use forge_components::{ChatBody, Platform};
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::{ChatAuthorKey, StorageError};
    use forge_types::{ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow};
    use gpui::{AppContext as _, Entity, TestAppContext};
    use time::OffsetDateTime;

    use super::{
        HISTORY_LOAD_CAP, HistoryState, ViewerHistory, feed_messages_by, history_query,
        merge_newest_first,
    };
    use crate::chat::ChatView;
    use crate::chat::tests::{message, mount, push_each};
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
        repo: MockChatHistoryRepo,
        live: Vec<ChatMessage>,
    ) -> (Entity<ChatFeed>, Entity<ViewerHistory>) {
        let feed = cx.new(|_| feed_of(live));
        let history =
            cx.new(|_| ViewerHistory::new(feed.clone(), Arc::new(repo), rt.handle().clone()));
        (feed, history)
    }

    fn show(cx: &mut TestAppContext, history: &Entity<ViewerHistory>, key: Option<AuthorKey>) {
        history.update(cx, |history, cx| history.show(key, cx));
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
        let (_feed, history) = mount_history(cx, &rt, repo, live);

        show(cx, &history, Some(key_of(ANN)));
        settle(cx, &rt);

        assert_eq!(loaded_ids(cx, &history), ["m5", "m3", "m1"]);
    }

    #[gpui::test]
    fn a_viewer_known_only_by_name_needs_an_id_instead_of_loading(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_feed, history) = mount_history(cx, &rt, MockChatHistoryRepo::new(), vec![]);

        show(
            cx,
            &history,
            Some(AuthorKey::by_name(Platform::Kick, "ann")),
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
        let (_feed, history) = mount_history(cx, &rt, repo, vec![line("m1", 10, ANN)]);

        show(cx, &history, Some(key_of(ANN)));
        settle(cx, &rt);

        assert!(matches!(state(cx, &history), HistoryState::Failed));
    }

    #[gpui::test]
    fn a_load_result_for_a_viewer_no_longer_shown_is_ignored(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo = repo_serving(vec![(ANN, vec![]), (BOB, vec![])]);
        let (_feed, history) = mount_history(cx, &rt, repo, vec![]);
        show(cx, &history, Some(key_of(ANN)));
        show(cx, &history, Some(key_of(BOB)));

        history.update(cx, |history, cx| {
            history.apply_loaded(&key_of(ANN), Some(vec![stored("m1", 10, ANN)]), cx);
        });

        assert!(matches!(state(cx, &history), HistoryState::Loading));
    }

    #[gpui::test]
    fn showing_the_same_viewer_again_adds_only_the_lines_that_arrived_since(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let repo = repo_serving(vec![(ANN, vec![])]);
        let (feed, history) = mount_history(cx, &rt, repo, vec![line("", 10, ANN)]);
        show(cx, &history, Some(key_of(ANN)));
        settle(cx, &rt);

        feed.update(cx, |feed, _| {
            feed.push(line("m2", 20, ANN));
            feed.push(line("b3", 30, BOB));
        });
        show(cx, &history, Some(key_of(ANN)));

        assert_eq!(loaded_ids(cx, &history), ["m2", ""]);
    }

    #[gpui::test]
    fn switching_viewers_resets_to_loading_before_the_new_viewers_lines_arrive(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let repo = repo_serving(vec![
            (ANN, vec![stored("a1", 10, ANN)]),
            (BOB, vec![stored("b1", 20, BOB)]),
        ]);
        let (_feed, history) = mount_history(cx, &rt, repo, vec![]);
        show(cx, &history, Some(key_of(ANN)));
        settle(cx, &rt);

        show(cx, &history, Some(key_of(BOB)));

        assert!(matches!(state(cx, &history), HistoryState::Loading));
    }

    #[gpui::test]
    fn switching_viewers_replaces_the_previous_viewers_lines(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo = repo_serving(vec![
            (ANN, vec![stored("a1", 10, ANN)]),
            (BOB, vec![stored("b1", 20, BOB)]),
        ]);
        let (_feed, history) = mount_history(cx, &rt, repo, vec![]);
        show(cx, &history, Some(key_of(ANN)));
        settle(cx, &rt);

        show(cx, &history, Some(key_of(BOB)));
        settle(cx, &rt);

        assert_eq!(loaded_ids(cx, &history), ["b1"]);
    }

    #[gpui::test]
    fn clearing_the_viewer_hides_a_loaded_history(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo = repo_serving(vec![(ANN, vec![stored("a1", 10, ANN)])]);
        let (_feed, history) = mount_history(cx, &rt, repo, vec![]);
        show(cx, &history, Some(key_of(ANN)));
        settle(cx, &rt);

        show(cx, &history, None);

        assert!(matches!(state(cx, &history), HistoryState::Hidden));
    }

    fn shown_key(cx: &mut TestAppContext, view: &Entity<ChatView>) -> Option<AuthorKey> {
        let history = view.read_with(cx, |view, _| view.viewer_history.clone());
        history.read_with(cx, |history, _| history.key.clone())
    }

    #[gpui::test]
    fn without_a_selection_the_history_follows_the_newest_chatter(cx: &mut TestAppContext) {
        let rt = runtime();
        let (feed, view) = mount(cx, &rt);

        push_each(cx, &feed, vec![message(0, false), message(1, false)]);

        assert_eq!(
            shown_key(cx, &view),
            Some(AuthorKey::by_name(Platform::Twitch, "user1"))
        );
    }

    #[gpui::test]
    fn picking_a_viewer_points_the_history_at_that_viewer(cx: &mut TestAppContext) {
        type Pick = fn(&mut ChatView, AuthorKey, &mut gpui::Context<ChatView>);
        let picks: [(&str, Pick); 2] = [
            ("open_viewer", ChatView::open_viewer),
            ("select_viewer", ChatView::select_viewer),
        ];
        for (name, pick) in picks {
            let rt = runtime();
            let (feed, view) = mount(cx, &rt);
            push_each(cx, &feed, vec![message(0, false), message(1, false)]);
            let first = AuthorKey::by_name(Platform::Twitch, "user0");

            view.update(cx, |view, cx| pick(view, first.clone(), cx));

            assert_eq!(shown_key(cx, &view), Some(first), "{name}");
        }
    }
}
