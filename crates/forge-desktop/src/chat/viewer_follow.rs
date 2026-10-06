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
