use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Spacing,
    body_family, fmt_relative_time, ghost_button, modal, mono_family, overlay, spacing, tr,
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
    older_failed: bool,
    full_tally: Option<ChatAuthorTally>,
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
            older_failed: false,
            full_tally: None,
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
        let first_page = self.state == HistoryState::Loading;
        let Some(page) = page else {
            if first_page {
                self.state = HistoryState::Failed;
            } else {
                self.older_failed = true;
                self.publish_tally(false, cx);
                self.sync_rows(cx);
            }
            cx.notify();
            return;
        };
        self.older_failed = false;
        self.stored
            .extend(page.rows.iter().map(ChatMessage::from_row));
        self.older = page.older;
        self.state = HistoryState::Loaded;
        if first_page {
            self.full_tally = Some(page.tally);
        }
        self.publish_tally(first_page, cx);
        self.sync_rows(cx);
        self.load_more_if_near(cx);
        cx.notify();
    }

    fn publish_tally(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(full) = self.full_tally else {
            return;
        };
        let key = self.key.clone();
        let settled = settled_tally(
            full,
            self.stored.len(),
            self.older.is_none() || self.older_failed,
        );
        if force || self.messages.read(cx).stored_tally(&key) != Some(settled) {
            let feed = self.feed.clone();
            self.messages.update(cx, |messages, cx| {
                messages.adopt(&key, settled, feed.read(cx));
                cx.notify();
            });
        }
    }

    fn retry_older(&mut self, cx: &mut Context<Self>) {
        if !self.older_failed || self._load.is_some() {
            return;
        }
        let Some(query) = stored_author(&self.key) else {
            return;
        };
        self.older_failed = false;
        self.spawn_load(query, self.older, cx);
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
            && !self.older_failed
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

    fn render_older_failed(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        div()
            .w_full()
            .flex_none()
            .pt(spacing(Spacing::Xs, density))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(Self::render_note(
                tr!("chat_drawer_history_older_failed"),
                palette.random,
            ))
            .child(
                ghost_button(tr!("chat_drawer_history_retry"), palette).on_click(
                    "chat-viewer-history-retry",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_older(cx)),
                ),
            )
            .into_any_element()
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
                .when(self.older_failed, |column| {
                    column.child(self.render_older_failed(palette, cx))
                })
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
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use forge_components::{ChatBody, FORGE_DEFAULT, Platform};
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::{
        ChatAuthorKey, ChatAuthorPage, ChatAuthorTally, ChatHistoryCursor, StorageError,
    };
    use forge_types::{ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow};
    use gpui::{AppContext as _, Entity, TestAppContext};
    use time::OffsetDateTime;

    use super::{
        HISTORY_PAGE_SIZE, HistoryState, ListedRows, RowChange, ViewerHistory, history_row,
        moderated_ids_by, row_change, settled_tally,
    };
    use crate::chat::tests::{message, mount_with_chat_history, push_each};
    use crate::chat::{ChatView, VIEWER_REFRESH};
    use crate::chat_author::AuthorKey;
    use crate::chat_drawer::author_summary;
    use crate::chat_feed::{ChatFeed, ChatMessage};
    use crate::chat_viewer_messages::ViewerMessages;
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

    fn rows(unsaved: usize, total: usize) -> ListedRows {
        ListedRows { unsaved, total }
    }

    #[test]
    fn row_change_splices_only_newest_unsaved_lines_onto_a_listed_history() {
        let cases = [
            (
                "nothing listed yet",
                rows(0, 0),
                rows(0, 0),
                RowChange::Same,
            ),
            ("unchanged", rows(1, 5), rows(1, 5), RowChange::Same),
            (
                "new live lines",
                rows(0, 5),
                rows(2, 7),
                RowChange::NewestAdded(2),
            ),
            ("first fill", rows(0, 0), rows(2, 2), RowChange::Rebuilt),
            (
                "lines flushed into storage",
                rows(2, 7),
                rows(0, 7),
                RowChange::Rebuilt,
            ),
            (
                "count settled lower",
                rows(0, 130),
                rows(0, 100),
                RowChange::Rebuilt,
            ),
            (
                "stored count grew",
                rows(0, 5),
                rows(1, 7),
                RowChange::Rebuilt,
            ),
            (
                "unsaved line dropped",
                rows(1, 5),
                rows(0, 6),
                RowChange::Rebuilt,
            ),
        ];
        for (name, before, after, expected) in cases {
            assert_eq!(row_change(before, after), expected, "{name}");
        }
    }

    #[test]
    fn history_rows_run_from_the_newest_unsaved_line_into_the_stored_rows() {
        let unsaved = [line("u1", 30, ANN), line("u2", 40, ANN)];
        let stored = [line("s2", 20, ANN), line("s1", 10, ANN)];
        let cases: [(&[ChatMessage], &[&str]); 2] =
            [(&unsaved, &["u2", "u1", "s2", "s1"]), (&[], &["s2", "s1"])];
        for (unsaved, expected) in cases {
            let listed: Vec<String> = (0..expected.len() + 1)
                .map_while(|ix| history_row(ix, unsaved, &stored))
                .map(|message| message.id.to_string())
                .collect();
            assert_eq!(listed, expected);
        }
    }

    #[test]
    fn settled_tally_matches_the_loaded_rows_only_once_storage_has_no_older_page() {
        let tally = ChatAuthorTally {
            messages: 130,
            newest_at: Some(OffsetDateTime::from_unix_timestamp(10).unwrap()),
        };
        for (loaded, exhausted, expected) in [
            (100, true, 100),
            (130, true, 130),
            (100, false, 130),
            (0, true, 0),
        ] {
            assert_eq!(
                settled_tally(tally, loaded, exhausted),
                ChatAuthorTally {
                    messages: expected,
                    ..tally
                },
                "loaded={loaded} exhausted={exhausted}"
            );
        }
    }

    struct PagedStore {
        rows: Vec<UnifiedChatRow>,
        kept: Arc<AtomicUsize>,
    }

    impl PagedStore {
        fn tally(rows: &[UnifiedChatRow], kept: usize) -> ChatAuthorTally {
            ChatAuthorTally {
                messages: kept as u64,
                newest_at: rows.first().map(|row| row.received_at),
            }
        }

        fn page(
            rows: &[UnifiedChatRow],
            kept: usize,
            older_than: Option<ChatHistoryCursor>,
            limit: usize,
        ) -> ChatAuthorPage {
            let from = older_than.map_or(0, |ChatHistoryCursor(ix)| ix as usize);
            let to = (from + limit).min(kept);
            ChatAuthorPage {
                tally: Self::tally(rows, kept),
                rows: rows.get(from..to).unwrap_or_default().to_vec(),
                older: (to < kept).then_some(ChatHistoryCursor(to as i64)),
            }
        }

        fn into_repo(self) -> MockChatHistoryRepo {
            let mut repo = MockChatHistoryRepo::new();
            let (rows, kept) = (self.rows.clone(), Arc::clone(&self.kept));
            repo.expect_author_tallies().returning(move |authors| {
                let kept = kept.load(Ordering::SeqCst);
                Ok(authors.iter().map(|_| Self::tally(&rows, kept)).collect())
            });
            let (rows, kept) = (self.rows, self.kept);
            repo.expect_author_page()
                .returning(move |_, older_than, limit| {
                    Ok(Self::page(
                        &rows,
                        kept.load(Ordering::SeqCst),
                        older_than,
                        limit,
                    ))
                });
            repo
        }
    }

    fn stored_newest_first(count: usize) -> Vec<UnifiedChatRow> {
        (0..count)
            .map(|ix| stored(&format!("s{ix}"), 1_000 - ix as i64, ANN))
            .collect()
    }

    fn event_by(id: &str, at: i64, author: Author) -> ChatMessage {
        let mut event = line(id, at, author);
        event.is_event = true;
        event
    }

    fn run_viewer_refresh(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        settle(cx, rt);
        cx.executor().advance_clock(VIEWER_REFRESH);
        settle(cx, rt);
    }

    fn tile_count(cx: &mut TestAppContext, view: &Entity<ChatView>) -> u64 {
        view.read_with(cx, |view, cx| {
            author_summary(
                &key_of(ANN),
                view.feed.read(cx).authors(),
                &view.viewers,
                view.viewer_messages.read(cx),
                &FORGE_DEFAULT,
            )
            .map(|summary| summary.message_count)
        })
        .unwrap()
    }

    fn open_ann_history(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        view: &Entity<ChatView>,
    ) -> Entity<ViewerHistory> {
        view.update(cx, |view, cx| {
            view.open_viewer_history(key_of(ANN), "ann".to_owned(), cx);
        });
        settle(cx, rt);
        open_dialog(cx, view).unwrap()
    }

    fn scroll_to_the_end(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        history: &Entity<ViewerHistory>,
    ) {
        for _ in 0..HISTORY_PAGE_SIZE {
            history.update(cx, |history, cx| {
                history.on_scrolled(history.listed.total, cx);
            });
            settle(cx, rt);
            if history.read_with(cx, |history, _| history.older.is_none()) {
                return;
            }
        }
    }

    fn tile_and_dialog_rows(
        cx: &mut TestAppContext,
        view: &Entity<ChatView>,
        history: &Entity<ViewerHistory>,
    ) -> (u64, u64) {
        (tile_count(cx, view), shown(cx, history).len() as u64)
    }

    #[gpui::test]
    fn the_tile_matches_the_rows_the_dialog_scrolls_through_past_one_page_ignoring_events(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let store = PagedStore {
            rows: stored_newest_first(130),
            kept: Arc::new(AtomicUsize::new(130)),
        };
        let (feed, view) = mount_with_chat_history(cx, &rt, store.into_repo());
        push_each(
            cx,
            &feed,
            vec![line("s0", 1_000, ANN), event_by("cheer", 2_000, ANN)],
        );
        run_viewer_refresh(cx, &rt);
        let history = open_ann_history(cx, &rt, &view);

        scroll_to_the_end(cx, &rt, &history);

        assert_eq!(tile_and_dialog_rows(cx, &view, &history), (130, 130));
    }

    #[gpui::test]
    fn the_tile_matches_the_dialog_rows_with_unsaved_live_lines_newer_than_storage(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let store = PagedStore {
            rows: stored_newest_first(2),
            kept: Arc::new(AtomicUsize::new(2)),
        };
        let (feed, view) = mount_with_chat_history(cx, &rt, store.into_repo());
        push_each(
            cx,
            &feed,
            vec![
                line("s0", 1_000, ANN),
                line("live1", 2_000, ANN),
                event_by("sub", 2_500, ANN),
                line("live2", 3_000, ANN),
            ],
        );
        run_viewer_refresh(cx, &rt);
        let history = open_ann_history(cx, &rt, &view);

        scroll_to_the_end(cx, &rt, &history);

        assert_eq!(tile_and_dialog_rows(cx, &view, &history), (4, 4));
    }

    #[gpui::test]
    fn the_tile_settles_to_the_dialog_rows_when_retention_prunes_rows_mid_scroll(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let kept = Arc::new(AtomicUsize::new(130));
        let store = PagedStore {
            rows: stored_newest_first(130),
            kept: Arc::clone(&kept),
        };
        let (feed, view) = mount_with_chat_history(cx, &rt, store.into_repo());
        push_each(
            cx,
            &feed,
            vec![line("s0", 1_000, ANN), line("live", 2_000, ANN)],
        );
        run_viewer_refresh(cx, &rt);
        let history = open_ann_history(cx, &rt, &view);

        kept.store(HISTORY_PAGE_SIZE, Ordering::SeqCst);
        scroll_to_the_end(cx, &rt, &history);

        assert_eq!(
            tile_and_dialog_rows(cx, &view, &history),
            (HISTORY_PAGE_SIZE as u64 + 1, HISTORY_PAGE_SIZE as u64 + 1)
        );
    }

    #[gpui::test]
    fn the_tile_holds_its_count_while_the_dialog_is_open_and_follows_storage_after_close(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let kept = Arc::new(AtomicUsize::new(5));
        let store = PagedStore {
            rows: stored_newest_first(9),
            kept: Arc::clone(&kept),
        };
        let (feed, view) = mount_with_chat_history(cx, &rt, store.into_repo());
        push_each(cx, &feed, vec![line("s0", 1_000, ANN)]);
        run_viewer_refresh(cx, &rt);
        let history = open_ann_history(cx, &rt, &view);

        kept.store(9, Ordering::SeqCst);
        run_viewer_refresh(cx, &rt);
        let while_open = tile_and_dialog_rows(cx, &view, &history);
        history.update(cx, |history, cx| history.dismiss(cx));
        drop(history);
        run_viewer_refresh(cx, &rt);

        assert_eq!((while_open, tile_count(cx, &view)), ((5, 5), 9));
    }

    struct FlakyOlderPages {
        fail_older: Arc<AtomicBool>,
        asked: Arc<Mutex<Vec<Option<ChatHistoryCursor>>>>,
    }

    fn flaky_older_pages(total: usize) -> (MockChatHistoryRepo, FlakyOlderPages) {
        let flaky = FlakyOlderPages {
            fail_older: Arc::new(AtomicBool::new(true)),
            asked: Arc::new(Mutex::new(Vec::new())),
        };
        let rows = stored_newest_first(total);
        let mut repo = MockChatHistoryRepo::new();
        let tally_rows = rows.clone();
        repo.expect_author_tallies().returning(move |authors| {
            Ok(authors
                .iter()
                .map(|_| PagedStore::tally(&tally_rows, total))
                .collect())
        });
        let (fail_older, asked) = (Arc::clone(&flaky.fail_older), Arc::clone(&flaky.asked));
        repo.expect_author_page()
            .returning(move |_, older_than, limit| {
                asked.lock().unwrap().push(older_than);
                if older_than.is_some() && fail_older.load(Ordering::SeqCst) {
                    return Err(StorageError::Connection {
                        reason: "closed".into(),
                    });
                }
                Ok(PagedStore::page(&rows, total, older_than, limit))
            });
        (repo, flaky)
    }

    fn open_with_a_failing_older_page(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> (Entity<ChatView>, Entity<ViewerHistory>, FlakyOlderPages) {
        let (repo, flaky) = flaky_older_pages(HISTORY_PAGE_SIZE + 30);
        let (feed, view) = mount_with_chat_history(cx, rt, repo);
        push_each(cx, &feed, vec![line("s0", 1_000, ANN)]);
        run_viewer_refresh(cx, rt);
        let history = open_ann_history(cx, rt, &view);
        history.update(cx, |history, cx| {
            history.on_scrolled(history.listed.total, cx);
        });
        settle(cx, rt);
        (view, history, flaky)
    }

    fn older_requests(flaky: &FlakyOlderPages) -> usize {
        flaky
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter(|cursor| cursor.is_some())
            .count()
    }

    #[gpui::test]
    fn a_failed_older_page_keeps_the_loaded_rows_and_settles_the_tile_to_them(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();

        let (view, history, _flaky) = open_with_a_failing_older_page(cx, &rt);

        let (older_failed, state) =
            history.read_with(cx, |history, _| (history.older_failed, history.state));
        assert_eq!(
            (
                state,
                older_failed,
                tile_and_dialog_rows(cx, &view, &history)
            ),
            (
                HistoryState::Loaded,
                true,
                (HISTORY_PAGE_SIZE as u64, HISTORY_PAGE_SIZE as u64)
            )
        );
    }

    #[gpui::test]
    fn a_failed_older_page_is_not_requested_again_until_retried(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_view, history, flaky) = open_with_a_failing_older_page(cx, &rt);
        let failed_requests = older_requests(&flaky);

        for _ in 0..3 {
            history.update(cx, |history, cx| {
                history.on_scrolled(history.listed.total, cx);
            });
            settle(cx, &rt);
        }

        let wants_more = history.read_with(cx, |history, _| history.wants_more());
        assert_eq!(
            (failed_requests, older_requests(&flaky), wants_more),
            (1, 1, false)
        );
    }

    #[gpui::test]
    fn retrying_a_failed_older_page_asks_the_same_cursor_and_restores_the_full_tally(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (view, history, flaky) = open_with_a_failing_older_page(cx, &rt);
        flaky.fail_older.store(false, Ordering::SeqCst);

        history.update(cx, |history, cx| history.retry_older(cx));
        settle(cx, &rt);

        let asked = flaky.asked.lock().unwrap().clone();
        let older_failed = history.read_with(cx, |history, _| history.older_failed);
        assert_eq!(
            (
                asked[asked.len() - 2],
                asked[asked.len() - 1],
                older_failed,
                tile_and_dialog_rows(cx, &view, &history)
            ),
            (
                Some(ChatHistoryCursor(HISTORY_PAGE_SIZE as i64)),
                Some(ChatHistoryCursor(HISTORY_PAGE_SIZE as i64)),
                false,
                (HISTORY_PAGE_SIZE as u64 + 30, HISTORY_PAGE_SIZE as u64 + 30)
            )
        );
    }
}
