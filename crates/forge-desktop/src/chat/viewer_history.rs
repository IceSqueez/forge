use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Platform, Spacing, body_family, fmt_relative_time,
    mono_family, section_label, spacing, tr,
};
use forge_storage::{ChatAuthorKey, ChatHistoryRepo};
use forge_types::{ChatSource, UnifiedChatRow};
use gpui::{
    AnyElement, Context, Entity, IntoElement, Pixels, Render, Rgba, SharedString, Task, Window,
    div, prelude::*, px,
};

use super::body_export_text;
use crate::chat_author::{AuthorHandle, AuthorKey};
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::presentation::ActivePresentation;

const HISTORY_LOAD_CAP: usize = 100;
const HISTORY_MAX_H: Pixels = px(180.0);

#[derive(Clone, Debug)]
enum HistoryState {
    Hidden,
    NeedsViewerId,
    Loading,
    Failed,
    Loaded(Vec<ChatMessage>),
}

pub(crate) struct ViewerHistory {
    feed: Entity<ChatFeed>,
    repo: Arc<dyn ChatHistoryRepo>,
    rt_handle: tokio::runtime::Handle,
    key: Option<AuthorKey>,
    state: HistoryState,
    scanned_through: u64,
    _load: Option<Task<()>>,
}

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
            key: None,
            state: HistoryState::Hidden,
            scanned_through: 0,
            _load: None,
        }
    }

    pub fn show(&mut self, key: Option<AuthorKey>, cx: &mut Context<Self>) {
        if self.key == key {
            self.absorb_live(cx);
            return;
        }
        self.key = key.clone();
        self._load = None;
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
                HistoryState::Loaded(merge_newest_first(stored, live, HISTORY_LOAD_CAP))
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
        if arrived.is_empty() {
            return;
        }
        *known = merge_newest_first(std::mem::take(known), arrived, HISTORY_LOAD_CAP);
        cx.notify();
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
}

impl Render for ViewerHistory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let body = match &self.state {
            HistoryState::Hidden => return div().into_any_element(),
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
                .gap(spacing(Spacing::Xxs, density))
                .children(
                    messages
                        .iter()
                        .map(|message| Self::render_row(message, &palette, density)),
                )
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(tr!("chat_drawer_history_title"), &palette))
            .child(body)
            .into_any_element()
    }
}
