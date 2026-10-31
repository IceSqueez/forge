use forge_types::PlatformId;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    ActivePresentation, BORDER_THIN, Icon, Radius, Spacing, drive_overlay_focus, empty_state,
    radius, spacing, tr,
};
use forge_platform_core::{BanListSource, BanPageToken};
use forge_runtime::{ActionEngineHandle, EventBus, LifecycleState};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, ScrollHandle, Subscription, Task, Window, div,
    prelude::*, px,
};

use super::ban_labels::changed_platforms;
use super::platform_gate::platform_integration;
use crate::async_bridge::{self, EventBatch};
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integrations::BuiltinRegistry;
use crate::navigable_list::{ListCursor, navigable_list};

mod ban_group;
mod row_keys;
mod row_view;
mod unban;

use ban_group::{BanGroup, GroupStatus, ListResult, list_pages, lists_only_what_forge_saw};
#[cfg(test)]
use ban_group::{BanRow, UnbanState};
use row_keys::{BanCursor, RowMenu};

const BAN_RELIST_DEBOUNCE: Duration = Duration::from_millis(750);
const UNBAN_RELIST_SUPPRESSION: Duration = Duration::from_secs(5);
const PLATFORM_ORDER: [PlatformId; 3] = [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick];

pub(crate) struct BannedPanel {
    builtins: BuiltinRegistry,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    groups: Vec<BanGroup>,
    visible: bool,
    last_request: u64,
    row_keys: ListCursor<BanCursor>,
    list_scroll: ScrollHandle,
    row_menu: Option<RowMenu>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
    _bus_watch: Option<Task<()>>,
    _lifecycle_obs: Option<Subscription>,
}

impl BannedPanel {
    pub(crate) fn new(
        builtins: BuiltinRegistry,
        lifecycle: Option<Entity<IntegrationLifecycle>>,
        bus: Option<Arc<EventBus>>,
        rt_handle: tokio::runtime::Handle,
        action_engine: ActionEngineHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let lifecycle_obs = lifecycle
            .as_ref()
            .map(|lifecycle| cx.observe(lifecycle, |this, _, cx| this.sync_platforms(cx)));
        let bus_watch = bus.map(|bus| Self::watch_ban_changes(&bus, cx));
        let mut this = Self {
            builtins,
            lifecycle,
            rt_handle,
            action_engine,
            groups: Vec::new(),
            visible: true,
            last_request: 0,
            row_keys: ListCursor::new(cx),
            list_scroll: ScrollHandle::new(),
            row_menu: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
            _bus_watch: bus_watch,
            _lifecycle_obs: lifecycle_obs,
        };
        this.sync_platforms(cx);
        this
    }

    fn watch_ban_changes(bus: &Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        let mut subscription = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut subscription).await
            {
                let platforms = changed_platforms(&batch);
                if platforms.is_empty() {
                    continue;
                }
                let alive = this.update(cx, |this, cx| {
                    for platform in platforms {
                        this.schedule_relist(platform, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    fn platform_running(&self, platform: PlatformId, cx: &App) -> bool {
        match &self.lifecycle {
            Some(lifecycle) => {
                lifecycle
                    .read(cx)
                    .state_of(&platform_integration(platform).builtin_id())
                    == LifecycleState::Running
            }
            None => true,
        }
    }

    fn source_for(&self, platform: PlatformId) -> Option<Arc<dyn BanListSource>> {
        self.builtins
            .get(&platform_integration(platform).builtin_id())
            .and_then(|object| object.ban_list)
    }

    fn group_mut(&mut self, platform: PlatformId) -> Option<&mut BanGroup> {
        self.groups
            .iter_mut()
            .find(|group| group.platform == platform)
    }

    fn sync_platforms(&mut self, cx: &mut Context<Self>) {
        let shown: Vec<PlatformId> = PLATFORM_ORDER
            .into_iter()
            .filter(|platform| {
                self.platform_running(*platform, cx) && self.source_for(*platform).is_some()
            })
            .collect();
        self.groups.retain(|group| shown.contains(&group.platform));
        for platform in shown {
            if self.group_mut(platform).is_none() {
                self.relist(platform, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            self.row_keys.reclaim_focus();
            let stale: Vec<PlatformId> = self
                .groups
                .iter()
                .filter(|group| group.stale || lists_only_what_forge_saw(group.platform))
                .map(|group| group.platform)
                .collect();
            for platform in stale {
                self.list_group(platform, None, cx);
            }
        } else {
            self.row_menu = None;
        }
        cx.notify();
    }

    pub(crate) fn note_moderated(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        if lists_only_what_forge_saw(platform) {
            self.schedule_relist(platform, cx);
        }
    }

    fn relist(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        if self.visible {
            self.list_group(platform, None, cx);
        } else if self.platform_running(platform, cx) && self.source_for(platform).is_some() {
            self.ensure_group(platform).stale = true;
        }
    }

    fn schedule_relist(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        if !self.visible {
            self.relist(platform, cx);
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BAN_RELIST_DEBOUNCE).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| this.relist(platform, cx));
        });
        if let Some(group) = self.group_mut(platform) {
            group._relist = Some(task);
        }
    }

    fn ensure_group(&mut self, platform: PlatformId) -> &mut BanGroup {
        let rank = |platform: PlatformId| {
            PLATFORM_ORDER
                .iter()
                .position(|candidate| *candidate == platform)
                .unwrap_or(PLATFORM_ORDER.len())
        };
        let index = match self
            .groups
            .iter()
            .position(|group| group.platform == platform)
        {
            Some(index) => index,
            None => {
                let index = self
                    .groups
                    .iter()
                    .take_while(|group| rank(group.platform) < rank(platform))
                    .count();
                self.groups.insert(index, BanGroup::new(platform));
                index
            }
        };
        &mut self.groups[index]
    }

    fn list_group(
        &mut self,
        platform: PlatformId,
        after: Option<BanPageToken>,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.source_for(platform) else {
            self.groups.retain(|group| group.platform != platform);
            cx.notify();
            return;
        };
        self.last_request += 1;
        let request_id = self.last_request;
        let appending = after.is_some();
        let depth = match self.group_mut(platform) {
            Some(group) if !appending => group.pages,
            _ => 1,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let result = list_pages(source.as_ref(), after, depth).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(result);
        });
        let request = cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = this.update(cx, |this, cx| {
                    this.apply_listed(platform, request_id, appending, result, cx);
                });
            }
        });

        let group = self.ensure_group(platform);
        group.request_id = request_id;
        group.loading_more = appending;
        if !appending {
            group.stale = false;
            if group.status != GroupStatus::Loaded {
                group.status = GroupStatus::Loading;
            }
        }
        group._request = Some(request);
        cx.notify();
    }

    pub(crate) fn apply_listed(
        &mut self,
        platform: PlatformId,
        request_id: u64,
        appending: bool,
        result: ListResult,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let Some(group) = self.group_mut(platform) else {
            return;
        };
        if group.request_id != request_id {
            return;
        }
        let result = group.without_recent_unbans(result, now);
        group.apply(appending, result);
        cx.notify();
    }

    fn load_more(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        let Some(next) = self
            .group_mut(platform)
            .filter(|group| !group.loading_more)
            .and_then(|group| group.next.clone())
        else {
            return;
        };
        self.list_group(platform, Some(next), cx);
    }

    fn retry(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        if let Some(group) = self.group_mut(platform) {
            group.status = GroupStatus::Loading;
        }
        self.list_group(platform, None, cx);
    }
}

impl Render for BannedPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drop_row_menu_of_a_gone_row();
        drive_overlay_focus(
            self.row_menu.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        self.row_keys.claim_focus_once(window, cx);
        let palette = cx.palette();
        let density = cx.density();
        let mut content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .p(spacing(Spacing::Md, density));
        if self.groups.is_empty() {
            content = content.child(
                empty_state(tr!("chat_banned_no_platform"), &palette)
                    .density(density)
                    .glyph(Icon::Ban),
            );
        }
        let groups: Vec<AnyElement> = self
            .groups
            .iter()
            .map(|group| {
                div()
                    .flex()
                    .flex_col()
                    .rounded(radius(Radius::Md))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .overflow_hidden()
                    .child(Self::render_group_header(group, &palette, density))
                    .children(Self::render_group_notes(
                        group.platform,
                        group.rows.len(),
                        &palette,
                        density,
                    ))
                    .child(self.render_group_body(group, &palette, density, cx))
                    .into_any_element()
            })
            .collect();
        let focus = self.row_keys.focus().clone();
        let list = navigable_list(div().id("chat-banned-panel"), &focus, cx)
            .track_scroll(&self.list_scroll)
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(content.children(groups));
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(list)
            .children(self.render_row_menu(&palette, cx))
    }
}

#[cfg(test)]
pub(super) mod tests;
