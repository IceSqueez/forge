use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Spacing,
    body_family, fmt_relative_time, modal, mono_family, overlay, spacing, tr,
};
use forge_storage::{
    ChatAuthorKey, ChatAuthorPage, ChatAuthorTally, ChatHistoryCursor, ChatHistoryRepo,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, IntoElement, ListAlignment,
    ListScrollEvent, ListSizingBehavior, ListState, Pixels, Render, Rgba, SharedString,
    Subscription, Task, Window, div, list, prelude::*, px,
};

use super::{body_export_text, platform_display_name};
use crate::chat_author::AuthorKey;
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::chat_viewer_messages::{ViewerMessages, stored_author};
use crate::presentation::ActivePresentation;

const HISTORY_PAGE_SIZE: usize = 100;
const HISTORY_PREFETCH_ROWS: usize = 25;
const HISTORY_MAX_H: Pixels = px(420.0);
const PENDING_ROW_TEXT: &str = "...";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryState {
    Loading,
    Failed,
    Loaded,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ListedRows {
    unsaved: usize,
    total: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowChange {
    Same,
    NewestAdded(usize),
    Rebuilt,
}

fn row_change(before: ListedRows, after: ListedRows) -> RowChange {
    if before == after {
        return RowChange::Same;
    }
    let added_unsaved = after.unsaved.checked_sub(before.unsaved);
    let added_total = after.total.checked_sub(before.total);
    match (added_unsaved, added_total) {
        (Some(added), Some(total_added)) if added == total_added && before.total > 0 => {
            RowChange::NewestAdded(added)
        }
        _ => RowChange::Rebuilt,
    }
}

fn history_row<'a>(
    ix: usize,
    unsaved: &'a [ChatMessage],
    stored: &'a [ChatMessage],
) -> Option<&'a ChatMessage> {
    match unsaved.len().checked_sub(ix + 1) {
        Some(oldest_first_ix) => unsaved.get(oldest_first_ix),
        None => stored.get(ix - unsaved.len()),
    }
}

fn settled_tally(tally: ChatAuthorTally, loaded: usize, exhausted: bool) -> ChatAuthorTally {
    if exhausted {
        ChatAuthorTally {
            messages: loaded as u64,
            ..tally
        }
    } else {
        tally
    }
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
    older: Option<ChatHistoryCursor>,
    moderated: HashSet<SharedString>,
    list_state: ListState,
    listed: ListedRows,
    visible_end: usize,
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
            this.sync_rows(cx);
            cx.notify();
        });
        let list_state = ListState::new(0, ListAlignment::Top, HISTORY_MAX_H);
        list_state.set_scroll_handler(cx.listener(|this, event: &ListScrollEvent, _, cx| {
            this.on_scrolled(event.visible_range.end, cx);
        }));
        let moderated = moderated_ids_by(feed.read(cx), &key);
        let mut history = Self {
            feed,
            messages,
            repo,
            rt_handle,
            viewer_name: SharedString::default(),
            key,
            state: HistoryState::Loading,
            stored: Vec::new(),
            older: None,
            moderated,
            list_state,
            listed: ListedRows::default(),
            visible_end: 0,
            _load: None,
            _feed_obs: feed_obs,
            _messages_obs: messages_obs,
        };
        match stored_author(&history.key) {
            Some(query) => history.spawn_load(query, None, cx),
            None => {
                history.state = HistoryState::Loaded;
                history.sync_rows(cx);
            }
        }
        history
    }

    #[must_use]
    pub fn titled(mut self, viewer_name: impl Into<SharedString>) -> Self {
        self.viewer_name = viewer_name.into();
        self
    }

    fn spawn_load(
        &mut self,
        query: ChatAuthorKey,
        older_than: Option<ChatHistoryCursor>,
        cx: &mut Context<Self>,
    ) {
        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                repo.author_page(&query, older_than, HISTORY_PAGE_SIZE)
                    .await,
            );
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
            this.update(cx, |this, cx| this.apply_page(page, cx)).ok();
        }));
    }

    fn apply_page(&mut self, page: Option<ChatAuthorPage>, cx: &mut Context<Self>) {
        self._load = None;
        let Some(page) = page else {
            self.state = HistoryState::Failed;
            cx.notify();
            return;
        };
        let first_page = self.state == HistoryState::Loading;
        self.stored
            .extend(page.rows.iter().map(ChatMessage::from_row));
        self.older = page.older;
        self.state = HistoryState::Loaded;
        let exhausted = self.older.is_none();
        let key = self.key.clone();
        let held = if first_page {
            Some(page.tally)
        } else {
            self.messages.read(cx).stored_tally(&key)
        };
        if let Some(held) = held {
            let settled = settled_tally(held, self.stored.len(), exhausted);
            if first_page || settled != held {
                let feed = self.feed.clone();
                self.messages.update(cx, |messages, cx| {
                    messages.adopt(&key, settled, feed.read(cx));
                    cx.notify();
                });
            }
        }
        self.sync_rows(cx);
        self.load_more_if_near(cx);
        cx.notify();
    }

    fn on_scrolled(&mut self, visible_end: usize, cx: &mut Context<Self>) {
        self.visible_end = visible_end;
        self.load_more_if_near(cx);
    }

    fn wants_more(&self) -> bool {
        self.state == HistoryState::Loaded
            && self._load.is_none()
            && self.older.is_some()
            && self.visible_end + HISTORY_PREFETCH_ROWS >= self.listed.unsaved + self.stored.len()
    }

    fn load_more_if_near(&mut self, cx: &mut Context<Self>) {
        if !self.wants_more() {
            return;
        }
        if let Some(query) = stored_author(&self.key) {
            self.spawn_load(query, self.older, cx);
        }
    }

    fn sync_rows(&mut self, cx: &mut Context<Self>) {
        if self.state != HistoryState::Loaded {
            return;
        }
        let messages = self.messages.read(cx);
        let next = ListedRows {
            unsaved: messages.unsaved(&self.key).len(),
            total: usize::try_from(messages.count(&self.key)).unwrap_or(usize::MAX),
        };
        match row_change(self.listed, next) {
            RowChange::Same => {}
            RowChange::NewestAdded(added) => self.list_state.splice(0..0, added),
            RowChange::Rebuilt => self.list_state.reset(next.total),
        }
        self.listed = next;
    }

    fn remark_moderated(&mut self, cx: &mut Context<Self>) {
        let moderated = moderated_ids_by(self.feed.read(cx), &self.key);
        if moderated != self.moderated {
            self.moderated = moderated;
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

    fn render_row(
        stamp: impl Into<SharedString>,
        body: impl Into<SharedString>,
        text_color: Rgba,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .w_full()
            .pb(spacing(Spacing::Xs, density))
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(stamp.into()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text_color)
                    .child(body.into()),
            )
            .into_any_element()
    }

    fn render_list_row(&self, ix: usize, cx: &Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let density = cx.density();
        let unsaved = self.messages.read(cx).unsaved(&self.key);
        match history_row(ix, unsaved, &self.stored) {
            Some(message) => {
                let dimmed = message.moderated || self.moderated.contains(&message.id);
                let text_color = if dimmed {
                    palette.text_faint
                } else {
                    palette.text_primary
                };
                Self::render_row(
                    fmt_relative_time(Some(message.received_at)),
                    body_export_text(&message.body),
                    text_color,
                    &palette,
                    density,
                )
            }
            None => Self::render_row(
                PENDING_ROW_TEXT,
                PENDING_ROW_TEXT,
                palette.text_faint,
                &palette,
                density,
            ),
        }
    }

    fn render_body(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        match self.state {
            HistoryState::Loading => {
                Self::render_note(tr!("chat_drawer_history_loading"), palette.text_faint)
            }
            HistoryState::Failed => {
                Self::render_note(tr!("chat_drawer_history_failed"), palette.random)
            }
            HistoryState::Loaded if self.listed.total == 0 => {
                Self::render_note(tr!("chat_drawer_history_empty"), palette.text_faint)
            }
            HistoryState::Loaded => div()
                .w_full()
                .flex()
                .flex_col()
                .max_h(HISTORY_MAX_H)
                .child(
                    list(
                        self.list_state.clone(),
                        cx.processor(|this, ix, _window, cx| this.render_list_row(ix, cx)),
                    )
                    .with_sizing_behavior(ListSizingBehavior::Infer)
                    .w_full()
                    .flex_grow_1()
                    .min_h(px(0.0)),
                )
                .into_any_element(),
        }
    }
}

impl Render for ViewerHistory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let card = modal(
            self.viewer_name.clone(),
            self.render_body(&palette, cx),
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
    use forge_storage::{ChatAuthorKey, ChatAuthorPage, ChatAuthorTally, StorageError};
    use forge_types::{ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow};
    use gpui::{AppContext as _, Entity, TestAppContext};
    use time::OffsetDateTime;

    use super::{HISTORY_PAGE_SIZE, HistoryState, ViewerHistory, history_row, moderated_ids_by};
    use crate::chat::ChatView;
    use crate::chat::tests::{message, mount_with_chat_history, push_each};
    use crate::chat_author::AuthorKey;
    use crate::chat_feed::{ChatFeed, ChatMessage};
    use crate::chat_viewer_messages::{ViewerMessages, stored_author};
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
            let tally = ChatAuthorTally {
                messages: rows.len() as u64,
                newest_at: rows.iter().map(|row| row.received_at).max(),
            };
            repo.expect_author_page()
                .withf(move |asked, older, limit| {
                    *asked == query && older.is_none() && *limit == HISTORY_PAGE_SIZE
                })
                .returning(move |_, _, _| {
                    Ok(ChatAuthorPage {
                        tally,
                        rows: rows.clone(),
                        older: None,
                    })
                });
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
        let messages = cx.new(|cx| {
            let mut messages = ViewerMessages::default();
            messages.absorb(feed.read(cx));
            messages
        });
        let history = cx.new(|cx| {
            ViewerHistory::new(
                key,
                feed.clone(),
                messages.clone(),
                Arc::new(repo),
                rt.handle().clone(),
                cx,
            )
        });
        (feed, history)
    }

    fn feed_and_absorb(
        cx: &mut TestAppContext,
        feed: &Entity<ChatFeed>,
        history: &Entity<ViewerHistory>,
        change: impl FnOnce(&mut ChatFeed),
    ) {
        feed.update(cx, |feed, cx| {
            change(feed);
            cx.notify();
        });
        let messages = history.read_with(cx, |history, _| history.messages.clone());
        messages.update(cx, |messages, cx| {
            messages.absorb(feed.read(cx));
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        cx.run_until_parked();
        pump(rt);
        cx.run_until_parked();
    }

    fn state(cx: &mut TestAppContext, history: &Entity<ViewerHistory>) -> HistoryState {
        history.read_with(cx, |history, _| history.state)
    }

    fn shown(cx: &mut TestAppContext, history: &Entity<ViewerHistory>) -> Vec<ChatMessage> {
        match state(cx, history) {
            HistoryState::Loaded => history.read_with(cx, |history, app| {
                let unsaved = history.messages.read(app).unsaved(&history.key);
                (0..history.listed.total)
                    .filter_map(|ix| history_row(ix, unsaved, &history.stored))
                    .map(|message| {
                        let mut message = message.clone();
                        message.moderated |= history.moderated.contains(&message.id);
                        message
                    })
                    .collect()
            }),
            other => panic!("expected a loaded history, got {other:?}"),
        }
    }

    fn loaded_ids(cx: &mut TestAppContext, history: &Entity<ViewerHistory>) -> Vec<String> {
        ids(&shown(cx, history))
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
            assert_eq!(stored_author(&key), expected, "{key:?}");
        }
    }

    fn loaded_moderated_ids(
        cx: &mut TestAppContext,
        history: &Entity<ViewerHistory>,
    ) -> Vec<String> {
        shown(cx, history)
            .iter()
            .filter(|m| m.moderated)
            .map(|m| m.id.to_string())
            .collect()
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
    fn a_viewer_known_only_by_name_shows_their_live_lines_without_a_storage_query(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let by_name: Author = (Platform::Twitch, None, "ann");
        let (_feed, history) = mount_history(
            cx,
            &rt,
            key_of(by_name),
            MockChatHistoryRepo::new(),
            vec![line("n1", 10, by_name), line("m1", 20, ANN)],
        );

        settle(cx, &rt);

        assert_eq!(loaded_ids(cx, &history), ["n1"]);
    }

    #[gpui::test]
    fn a_failed_load_shows_the_failed_state(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_author_page().returning(|_, _, _| {
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

        feed_and_absorb(cx, &feed, &history, |feed| {
            feed.push(line("m2", 20, ANN));
            feed.push(line("b3", 30, BOB));
        });

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

        feed_and_absorb(cx, &feed, &history, |feed| {
            feed.mark_deleted("m1");
        });

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
        repo.expect_author_page().returning(move |_, _, _| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(ChatAuthorPage::default())
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
