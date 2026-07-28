use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;

use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DetailSection, SectionIcon, TokenColor,
    TrailingToken,
};

use crate::client::DiscordClient;

pub(crate) const RECENT_POSTS_CAP: usize = 20;

#[derive(Debug, Clone)]
pub(crate) struct SendRecord {
    pub webhook_name: String,
    pub message_id: Option<String>,
    pub had_embed: bool,
    pub ok: bool,
    pub sent_at: OffsetDateTime,
}

#[derive(Default)]
pub(crate) struct DiscordContentSnapshot {
    pub webhook_names: Vec<String>,
    pub webhook_last_ok: HashMap<String, bool>,
    pub recent_posts: VecDeque<SendRecord>,
}

pub(crate) fn make_content_state() -> Arc<Mutex<DiscordContentSnapshot>> {
    Arc::new(Mutex::new(DiscordContentSnapshot::default()))
}

#[derive(Debug, Clone)]
pub struct WebhookPost {
    pub webhook_name: String,
    pub had_embed: bool,
    pub ok: bool,
    pub sent_at: OffsetDateTime,
}

impl DiscordClient {
    pub fn webhook_names(&self) -> Vec<String> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        snap.webhook_names.clone()
    }

    /// Newest first, capped at the in-memory send history; empty after a restart.
    pub fn recent_posts(&self) -> Vec<WebhookPost> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        snap.recent_posts
            .iter()
            .map(|record| WebhookPost {
                webhook_name: record.webhook_name.clone(),
                had_embed: record.had_embed,
                ok: record.ok,
                sent_at: record.sent_at,
            })
            .collect()
    }
}

fn push_send_record(
    snap: &mut DiscordContentSnapshot,
    webhook_name: &str,
    message_id: Option<String>,
    had_embed: bool,
    ok: bool,
) {
    snap.webhook_last_ok.insert(webhook_name.to_owned(), ok);
    snap.recent_posts.push_front(SendRecord {
        webhook_name: webhook_name.to_owned(),
        message_id,
        had_embed,
        ok,
        sent_at: OffsetDateTime::now_utc(),
    });
    if snap.recent_posts.len() > RECENT_POSTS_CAP {
        snap.recent_posts.pop_back();
    }
}

pub(crate) fn record_send(
    snap: &mut DiscordContentSnapshot,
    webhook_name: &str,
    message_id: Option<String>,
    had_embed: bool,
    ok: bool,
) {
    if !snap.webhook_names.contains(&webhook_name.to_owned()) {
        snap.webhook_names.push(webhook_name.to_owned());
    }
    push_send_record(snap, webhook_name, message_id, had_embed, ok);
}

/// Records the post and health signal without registering `webhook_name` in the saved-webhook list.
pub(crate) fn record_test_send(
    snap: &mut DiscordContentSnapshot,
    webhook_name: &str,
    message_id: Option<String>,
    had_embed: bool,
    ok: bool,
) {
    push_send_record(snap, webhook_name, message_id, had_embed, ok);
}

impl BuiltinContent for DiscordClient {
    fn sections(&self) -> Vec<DetailSection> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());

        let webhook_items: Vec<ContentListItem> = snap
            .webhook_names
            .iter()
            .map(|name| {
                let last_ok = snap.webhook_last_ok.get(name).copied().unwrap_or(false);
                ContentListItem {
                    icon: SectionIcon::new("webhook"),
                    icon_tint: None,
                    name: name.clone(),
                    monospace_name: true,
                    active: last_ok,
                    active_label: if last_ok { Some("OK".to_owned()) } else { None },
                    trailing: vec![if last_ok {
                        TrailingToken::Badge("READY".to_owned(), TokenColor::Green)
                    } else {
                        TrailingToken::Badge("ERROR".to_owned(), TokenColor::Red)
                    }],
                    enabled: true,
                }
            })
            .collect();

        let post_items: Vec<ContentListItem> = snap
            .recent_posts
            .iter()
            .map(|r| {
                let short_id = r
                    .message_id
                    .as_deref()
                    .map(|id| id.chars().take(8).collect::<String>())
                    .unwrap_or_else(|| "no-id".to_owned());
                let ts = r
                    .sent_at
                    .format(
                        &time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                            .unwrap_or_default(),
                    )
                    .unwrap_or_else(|_| "--:--:--".to_owned());
                ContentListItem {
                    icon: SectionIcon::new("message"),
                    icon_tint: None,
                    name: format!("{}: {short_id}", r.webhook_name),
                    monospace_name: true,
                    active: r.ok,
                    active_label: if r.had_embed {
                        Some("embed".to_owned())
                    } else {
                        None
                    },
                    trailing: vec![TrailingToken::Label(ts)],
                    enabled: true,
                }
            })
            .collect();

        let webhook_count = webhook_items.len().to_string();
        let post_count = post_items.len().to_string();

        vec![DetailSection::TwoColumnLists {
            left: Box::new(ContentList {
                title: "Webhooks".to_owned(),
                icon: SectionIcon::new("webhook"),
                inline_label: None,
                count_label: Some(webhook_count),
                visible_rows: None,
                row_padding_y_px: 7,
                refreshable: false,
                items: webhook_items,
                footer: None,
            }),
            right: Box::new(ContentList {
                title: "Recent Posts".to_owned(),
                icon: SectionIcon::new("message"),
                inline_label: None,
                count_label: Some(post_count),
                visible_rows: None,
                row_padding_y_px: 7,
                refreshable: false,
                items: post_items,
                footer: None,
            }),
        }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{BuiltinContent, DetailSection};

    use super::*;
    use crate::client::DiscordClient;

    #[test]
    fn sections_returns_two_column_lists() {
        let c = DiscordClient::new_for_test();
        let content: &dyn BuiltinContent = &*c;
        let sections = content.sections();
        assert_eq!(sections.len(), 1);
        assert!(matches!(sections[0], DetailSection::TwoColumnLists { .. }));
    }

    #[test]
    fn record_send_populates_webhooks_and_recent_posts() {
        let snap_arc = make_content_state();
        {
            let mut snap = snap_arc.lock().unwrap();
            record_send(&mut snap, "alerts", Some("msg123".to_owned()), false, true);
        }
        let snap = snap_arc.lock().unwrap();
        assert_eq!(snap.webhook_names, vec!["alerts"]);
        assert_eq!(snap.recent_posts.len(), 1);
        assert_eq!(snap.recent_posts[0].webhook_name, "alerts");
        assert_eq!(snap.recent_posts[0].message_id.as_deref(), Some("msg123"));
        assert!(snap.recent_posts[0].ok);
    }

    #[test]
    fn recent_posts_capped_at_twenty() {
        let snap_arc = make_content_state();
        let mut snap = snap_arc.lock().unwrap();
        for i in 0..25 {
            record_send(&mut snap, "w", Some(i.to_string()), false, true);
        }
        assert_eq!(snap.recent_posts.len(), RECENT_POSTS_CAP);
    }

    #[test]
    fn record_send_tracks_last_ok_per_webhook() {
        let snap_arc = make_content_state();
        let mut snap = snap_arc.lock().unwrap();
        record_send(&mut snap, "alerts", None, false, true);
        record_send(&mut snap, "alerts", None, false, false);
        assert_eq!(snap.webhook_last_ok.get("alerts"), Some(&false));
    }
}
