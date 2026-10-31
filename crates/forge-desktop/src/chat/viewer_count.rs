use super::{ChatView, VIEWER_REFRESH};
use crate::chat_author::AuthorKey;
use crate::chat_viewer_messages::stored_author;
use crate::home_stats::HomeStats;
use crate::window_presence::PresenceGate;
use forge_components::{
    ActivePresentation, FONT_XS, Spacing, body_family, spacing, status_dot, tr,
};
use forge_storage::{
    ChatAuthorKey, ChatAuthorTally, ChatHistoryRepo, StorageError, Viewer, ViewerRepo,
};
use gpui::{App, Context, Entity, Pixels, Subscription, Window, div, prelude::*, px};
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;

const VIEWER_DOT: Pixels = px(6.0);

pub(super) struct ChatViewerCount {
    home_stats: Entity<HomeStats>,
    _stats_obs: Subscription,
}

impl ChatViewerCount {
    pub(super) fn new(home_stats: Entity<HomeStats>, cx: &mut Context<Self>) -> Self {
        let stats_obs = cx.observe(&home_stats, |_, _, cx| cx.notify());
        Self {
            home_stats,
            _stats_obs: stats_obs,
        }
    }
}

impl Render for ChatViewerCount {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let viewer_count = self.home_stats.read(cx).viewers_display();
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(palette.success, VIEWER_DOT))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(format!("{} {}", viewer_count, tr!("chat_viewers_unit"))),
            )
    }
}

pub(super) struct StaleViewer {
    pub(super) key: AuthorKey,
    pub(super) seen_at: OffsetDateTime,
}

pub(super) async fn read_viewers(
    repo: &dyn ViewerRepo,
    authors: &[ChatAuthorKey],
) -> Result<Vec<Viewer>, StorageError> {
    let mut viewers = Vec::with_capacity(authors.len());
    for author in authors {
        if let Some(viewer) = repo.get(author.source, &author.author_id).await? {
            viewers.push(viewer);
        }
    }
    Ok(viewers)
}

pub(super) fn note_refreshed(
    stale: Vec<StaleViewer>,
    viewers: &[Viewer],
    refreshed: &mut HashMap<AuthorKey, OffsetDateTime>,
) {
    for StaleViewer { key, seen_at } in stale {
        let caught_up = stored_author(&key).is_some_and(|author| {
            viewers.iter().any(|viewer| {
                viewer.platform == author.source
                    && viewer.viewer_id == author.author_id
                    && viewer.last_seen_at >= seen_at
            })
        });
        if caught_up {
            refreshed.insert(key, seen_at);
        }
    }
}

impl ChatView {
    pub(super) fn spawn_viewer_refresh(
        repo: Arc<dyn ViewerRepo>,
        chat_history: Arc<dyn ChatHistoryRepo>,
        rt_handle: tokio::runtime::Handle,
        mut feed_changes: tokio::sync::watch::Receiver<()>,
        cx: &mut Context<Self>,
    ) {
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            let mut refreshed: HashMap<AuthorKey, OffsetDateTime> = HashMap::new();
            loop {
                presence.until_visible().await;
                let Ok((keys, stale)) = this.update(cx, |this, cx| {
                    (
                        this.stored_author_keys(cx),
                        this.stale_viewers(&refreshed, cx),
                    )
                }) else {
                    break;
                };
                let repo = Arc::clone(&repo);
                let chat_history = Arc::clone(&chat_history);
                let (tx, rx) = tokio::sync::oneshot::channel();
                let queries: Vec<_> = keys.iter().filter_map(stored_author).collect();
                let lookups: Vec<_> = stale.iter().filter_map(|s| stored_author(&s.key)).collect();
                rt_handle.spawn(async move {
                    let viewers = read_viewers(repo.as_ref(), &lookups).await;
                    let tallies = if queries.is_empty() {
                        Ok(Vec::new())
                    } else {
                        chat_history.author_tallies(&queries).await
                    };
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the view may close before the result arrives"
                    )]
                    let _ = tx.send((viewers, tallies));
                });
                if let Ok((viewers, tallies)) = rx.await {
                    let viewers = viewers
                        .inspect_err(|err| {
                            tracing::warn!(error = %err, "viewer snapshot load failed");
                        })
                        .ok();
                    if let Some(list) = &viewers {
                        note_refreshed(stale, list, &mut refreshed);
                    }
                    let tallies = tallies
                        .inspect_err(|err| {
                            tracing::warn!(error = %err, "viewer message tally load failed");
                        })
                        .ok();
                    let applied = this.update(cx, |this, cx| {
                        if let Some(list) = viewers {
                            this.apply_viewers(list, cx);
                        }
                        if let Some(tallies) = tallies {
                            this.apply_tallies(keys, tallies, cx);
                        }
                    });
                    if applied.is_err() {
                        break;
                    }
                }
                cx.background_executor().timer(VIEWER_REFRESH).await;
                if feed_changes.changed().await.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_viewers(&mut self, viewers: Vec<Viewer>, cx: &mut Context<Self>) {
        if self.viewers.absorb(viewers) {
            cx.notify();
        }
    }

    pub(super) fn stale_viewers(
        &self,
        refreshed: &HashMap<AuthorKey, OffsetDateTime>,
        cx: &App,
    ) -> Vec<StaleViewer> {
        self.feed
            .read(cx)
            .authors()
            .newest_first()
            .filter(|(key, activity)| {
                stored_author(key).is_some()
                    && refreshed.get(*key) != Some(&activity.last_received_at)
            })
            .map(|(key, activity)| StaleViewer {
                key: key.clone(),
                seen_at: activity.last_received_at,
            })
            .collect()
    }

    fn stored_author_keys(&self, cx: &App) -> Vec<AuthorKey> {
        self.feed
            .read(cx)
            .authors()
            .newest_first()
            .map(|(key, _)| key)
            .filter(|key| stored_author(key).is_some())
            .cloned()
            .collect()
    }

    fn apply_tallies(
        &mut self,
        keys: Vec<AuthorKey>,
        tallies: Vec<ChatAuthorTally>,
        cx: &mut Context<Self>,
    ) {
        if keys.len() != tallies.len() {
            tracing::warn!(
                keys = keys.len(),
                tallies = tallies.len(),
                "viewer message tally does not match the requested authors"
            );
            return;
        }
        let feed = self.feed.clone();
        self.viewer_messages.update(cx, |messages, cx| {
            messages.replace_tallies(keys.into_iter().zip(tallies), feed.read(cx));
            cx.notify();
        });
    }

    pub(super) fn absorb_viewer_messages(&mut self, cx: &mut Context<Self>) {
        let feed = self.feed.clone();
        self.viewer_messages.update(cx, |messages, cx| {
            if messages.absorb(feed.read(cx)) {
                cx.notify();
            }
        });
    }
}
