use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    MenuPlacement, OverlayPosition, ToastKind, badge, body_family, card, confirm_modal,
    drive_overlay_focus, fmt_relative_time, icon, menu_button, menu_divider, menu_item,
    mono_family, overlay, pad_tile, page_frame, status_dot, tr,
};
use forge_discord::{DiscordClient, DiscordSendHealth, WebhookPost};
use forge_runtime::EventBus;
use forge_storage::ActionRepo;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, Pixels, Point, Rgba, SharedString,
    Subscription, Task, Window, div, prelude::*, px,
};
use time::OffsetDateTime;

use crate::async_bridge::{self, BridgeFlow, drain_events};
use crate::builtin_sections::grow_cell;
use crate::discord_webhook_modal::{
    DiscordWebhookModal, DiscordWebhookModalEvent, WebhookDraft, WebhookModalLaunch,
};
use crate::discord_webhooks::{
    DISCORD_EVENT_PREFIX, WebhookRow, distinct_linked_actions, load_webhooks, name_is_taken,
};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const HERO_PAD_V: Pixels = px(14.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(14.0);
const HERO_MARGIN_B: Pixels = px(14.0);
const HERO_TILE: Pixels = px(40.0);
const HERO_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = px(15.0);
const HERO_BLURB_MT: Pixels = px(1.0);

const COUNT_TILE: Pixels = px(26.0);
const COUNT_TILE_RADIUS: Pixels = px(7.0);
const COUNT_GLYPH: Pixels = px(13.0);
const COUNT_GAP: Pixels = px(8.0);
const COUNT_MR: Pixels = px(4.0);
const COUNT_VALUE_FS: Pixels = px(12.0);
const COUNT_SUB_FS: Pixels = px(10.0);

const STAT_GAP: Pixels = px(10.0);
const STAT_MARGIN_B: Pixels = px(14.0);
const STAT_PAD_V: Pixels = px(10.0);
const STAT_PAD_H: Pixels = px(12.0);
const STAT_LABEL_MB: Pixels = px(4.0);
const STAT_HINT_MT: Pixels = px(2.0);

const COLUMN_GAP: Pixels = px(12.0);
const COLUMN_FLEX: f32 = 1.0;

const SECTION_LABEL_FS: Pixels = px(9.5);
const SECTION_LABEL_MT: Pixels = px(4.0);
const SECTION_LABEL_MB: Pixels = px(8.0);
const SECTION_HINT_FS: Pixels = px(10.0);
const LIST_GAP: Pixels = px(6.0);

const ROW_PAD_V: Pixels = px(9.0);
const ROW_PAD_H: Pixels = px(12.0);
const ROW_GAP: Pixels = px(9.0);
const ROW_GLYPH: Pixels = px(13.0);
const ROW_NAME_MIN_W: Pixels = px(78.0);
const ROW_NAME_FS: Pixels = px(12.0);
const ROW_SUMMARY_FS: Pixels = px(11.5);
const ROW_DOT: Pixels = px(6.0);
const KIND_BADGE_FS: Pixels = px(9.0);
const POST_TIME_FS: Pixels = px(10.5);
const EMPTY_PAD_V: Pixels = px(14.0);

const ADD_BAR_GLYPH: Pixels = px(13.0);

const FOOTER_FS: Pixels = px(10.5);
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_GAP: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);
const FOOTER_SEPARATOR: &str = "·";

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);
const HEADER_FS: Pixels = px(11.5);

const NO_VALUE: &str = "-";
const SUMMARY_INLINE_LIMIT: usize = 42;
const ACTION_JOIN: &str = ", ";

struct OpenModal {
    view: Entity<DiscordWebhookModal>,
    _sub: Subscription,
}

struct DeletePrompt {
    name: String,
    linked: usize,
}

pub struct DiscordScreenView {
    client: Arc<DiscordClient>,
    action_repo: Arc<dyn ActionRepo>,
    rt_handle: tokio::runtime::Handle,
    webhooks: Vec<WebhookRow>,
    posts: Vec<WebhookPost>,
    health: DiscordSendHealth,
    menu_open: Option<String>,
    menu_click_pos: Option<Point<Pixels>>,
    modal: Option<OpenModal>,
    delete_prompt: Option<DeletePrompt>,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    _bus_bridge: Task<()>,
}

impl DiscordScreenView {
    pub fn new(
        client: Arc<DiscordClient>,
        action_repo: Arc<dyn ActionRepo>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let bus_bridge = Self::spawn_bus_bridge(bus, cx);
        let mut view = Self {
            posts: client.recent_posts(),
            health: client.send_health(),
            client,
            action_repo,
            rt_handle,
            webhooks: Vec::new(),
            menu_open: None,
            menu_click_pos: None,
            modal: None,
            delete_prompt: None,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            _bus_bridge: bus_bridge,
        };
        view.load(cx);
        view
    }

    fn spawn_bus_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                if !batch
                    .iter()
                    .any(|event| event.kind.starts_with(DISCORD_EVENT_PREFIX))
                {
                    return BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.refresh_from_client(cx)) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        })
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let client = Arc::clone(&self.client);
        let actions = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            load_webhooks(client, actions),
            |this, result, cx| this.apply_webhooks(result, cx),
            cx,
        );
    }

    fn apply_webhooks(&mut self, result: Result<Vec<WebhookRow>, String>, cx: &mut Context<Self>) {
        match result {
            Ok(rows) => self.webhooks = rows,
            Err(message) => self.on_error(&message, cx),
        }
        self.refresh_from_client(cx);
    }

    fn refresh_from_client(&mut self, cx: &mut Context<Self>) {
        self.posts = self.client.recent_posts();
        self.health = self.client.send_health();
        cx.notify();
    }

    fn on_error(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(error = %message, "discord webhook operation failed");
        cx.push_toast(
            ToastKind::Error,
            tr!("discord_toast_error", message = message),
        );
        cx.notify();
    }

    fn last_send_at(&self) -> Option<OffsetDateTime> {
        self.posts.first().map(|post| post.sent_at)
    }

    fn linked_total(&self) -> usize {
        distinct_linked_actions(&self.webhooks)
    }

    fn linked_count(&self, name: &str) -> usize {
        self.webhooks
            .iter()
            .find(|row| row.name == name)
            .map_or(0, |row| row.linked_actions.len())
    }

    fn summary_of(row: &WebhookRow) -> String {
        if row.linked_actions.is_empty() {
            return tr!("discord_binding_no_actions");
        }
        let joined = row.linked_actions.join(ACTION_JOIN);
        if joined.chars().count() <= SUMMARY_INLINE_LIMIT {
            joined
        } else {
            tr!(
                "discord_binding_action_count",
                count = row.linked_actions.len() as i64
            )
        }
    }

    fn open_add_modal(&mut self, cx: &mut Context<Self>) {
        self.open_modal(
            WebhookModalLaunch {
                original_name: None,
                name: String::new(),
                url: String::new(),
            },
            cx,
        );
    }

    fn edit_webhook(&mut self, name: String, cx: &mut Context<Self>) {
        self.menu_open = None;
        let client = Arc::clone(&self.client);
        let lookup = name.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .webhook_url(&lookup)
                    .await
                    .map_err(|e| e.to_string())
                    .map(|url| (lookup, url))
            },
            |this, result, cx| match result {
                Ok((name, url)) => this.open_modal(
                    WebhookModalLaunch {
                        original_name: Some(name.clone()),
                        name,
                        url,
                    },
                    cx,
                ),
                Err(message) => this.on_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn open_modal(&mut self, launch: WebhookModalLaunch, cx: &mut Context<Self>) {
        let view = cx.new(|cx| DiscordWebhookModal::new(launch, cx));
        let sub = cx.subscribe(&view, Self::on_modal_event);
        self.modal = Some(OpenModal { view, _sub: sub });
        self.menu_open = None;
        cx.notify();
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _view: Entity<DiscordWebhookModal>,
        event: &DiscordWebhookModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DiscordWebhookModalEvent::Save(draft) => self.save_webhook(draft, cx),
            DiscordWebhookModalEvent::Test(draft) => self.test_draft(draft, cx),
            DiscordWebhookModalEvent::Cancel => self.close_modal(cx),
        }
    }

    fn save_webhook(&mut self, draft: &WebhookDraft, cx: &mut Context<Self>) {
        let name = draft.name.clone();
        if draft.original_name.is_none() && name_is_taken(&self.webhooks, &name) {
            cx.push_toast(
                ToastKind::Error,
                tr!("discord_toast_name_taken", name = name.as_str()),
            );
            cx.notify();
            return;
        }

        self.close_modal(cx);
        let client = Arc::clone(&self.client);
        let url = draft.url.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .save_webhook(&name, &url)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| match result {
                Ok(()) => {
                    cx.push_toast(ToastKind::Success, tr!("discord_toast_saved"));
                    this.load(cx);
                }
                Err(message) => this.on_error(&message, cx),
            },
            cx,
        );
    }

    fn test_draft(&mut self, draft: &WebhookDraft, cx: &mut Context<Self>) {
        let client = Arc::clone(&self.client);
        let name = draft.name.clone();
        let url = draft.url.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .post_test(&name, &url, &tr!("discord_test_content"))
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| {
                if let Some(open) = &this.modal {
                    open.view
                        .update(cx, |modal, cx| modal.set_testing(false, cx));
                }
                this.report_test(result, cx);
            },
            cx,
        );
    }

    fn test_webhook(&mut self, name: String, cx: &mut Context<Self>) {
        self.menu_open = None;
        let client = Arc::clone(&self.client);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .post_text(&name, &tr!("discord_test_content"))
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.report_test(result, cx),
            cx,
        );
        cx.notify();
    }

    fn report_test(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => cx.push_toast(ToastKind::Success, tr!("discord_toast_test_sent")),
            Err(message) => cx.push_toast(
                ToastKind::Error,
                tr!("discord_toast_test_failed", message = message.as_str()),
            ),
        }
        self.refresh_from_client(cx);
    }

    fn prompt_delete(&mut self, name: String, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.delete_prompt = Some(DeletePrompt {
            linked: self.linked_count(&name),
            name,
        });
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.delete_prompt = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.delete_prompt.take() else {
            return;
        };
        let client = Arc::clone(&self.client);
        let name = prompt.name;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .delete_webhook(&name)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| match result {
                Ok(()) => {
                    cx.push_toast(ToastKind::Success, tr!("discord_toast_deleted"));
                    this.load(cx);
                }
                Err(message) => this.on_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_menu(&mut self, name: &str, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open.as_deref() == Some(name) {
            None
        } else {
            self.menu_click_pos = Some(position);
            Some(name.to_owned())
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn render_hero(&self, palette: &ForgePalette) -> AnyElement {
        let count = self.webhooks.len();
        let badge_slot = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(COUNT_GAP)
            .mr(COUNT_MR)
            .child(
                div()
                    .flex_none()
                    .size(COUNT_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(COUNT_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Variable, COUNT_GLYPH, palette.brand)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(COUNT_VALUE_FS)
                            .text_color(palette.text_primary)
                            .child(tr!("discord_hero_webhooks", count = count as i64)),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(COUNT_SUB_FS)
                            .text_color(palette.text_faint)
                            .child(tr!("discord_hero_webhooks_sub")),
                    ),
            );

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(HERO_GAP)
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::BrandDiscord, HERO_GLYPH, palette.brand)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(HERO_TITLE_FS)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child(tr!("discord_hero_title")),
                    )
                    .child(
                        div()
                            .mt(HERO_BLURB_MT)
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("discord_hero_blurb")),
                    ),
            )
            .child(badge_slot);

        div()
            .w_full()
            .mb(HERO_MARGIN_B)
            .child(
                card(body, palette)
                    .padding_xy(HERO_PAD_V, HERO_PAD_H)
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_stats(&self, palette: &ForgePalette) -> AnyElement {
        let health = self.health;

        let (latency_value, latency_hint) = match health.latency_p50_ms {
            Some(ms) => (
                tr!("discord_stat_latency_value", ms = ms as i64),
                tr!(
                    "discord_stat_latency_hint",
                    count = health.latency_samples as i64
                ),
            ),
            None => (NO_VALUE.to_owned(), tr!("discord_stat_no_sends")),
        };

        let (budget_value, budget_hint, budget_ink) = if health.rate_limit_total == 0 {
            (
                NO_VALUE.to_owned(),
                tr!("discord_stat_budget_unknown"),
                palette.text_primary,
            )
        } else {
            let exhausted = health.rate_limit_used >= health.rate_limit_total;
            (
                tr!(
                    "discord_stat_budget_value",
                    used = health.rate_limit_used as i64,
                    total = health.rate_limit_total as i64
                ),
                tr!("discord_stat_budget_hint"),
                if exhausted {
                    palette.warning
                } else {
                    palette.text_primary
                },
            )
        };

        let (send_value, send_ink) = match health.last_send_ok {
            Some(true) => (tr!("discord_stat_send_ok"), palette.success),
            Some(false) => (tr!("discord_stat_send_failed"), palette.random),
            None => (tr!("discord_stat_send_none"), palette.text_faint),
        };
        let send_hint = match self.last_send_at() {
            Some(at) => fmt_relative_time(Some(at)),
            None => NO_VALUE.to_owned(),
        };

        let errors = health.errors_last_hour;
        let errors_ink = if errors == 0 {
            palette.success
        } else {
            palette.random
        };

        div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(STAT_GAP)
            .mb(STAT_MARGIN_B)
            .child(stat_card(
                tr!("discord_stat_latency"),
                latency_value,
                palette.text_primary,
                latency_hint,
                palette.text_faint,
                palette,
            ))
            .child(stat_card(
                tr!("discord_stat_budget"),
                budget_value,
                budget_ink,
                budget_hint,
                palette.text_faint,
                palette,
            ))
            .child(stat_card(
                tr!("discord_stat_send"),
                send_value,
                send_ink,
                send_hint,
                palette.text_faint,
                palette,
            ))
            .child(stat_card(
                tr!("discord_stat_errors"),
                errors.to_string(),
                errors_ink,
                tr!("discord_stat_errors_hint"),
                palette.text_faint,
                palette,
            ))
            .into_any_element()
    }

    fn render_bindings(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(tr!(
                "discord_section_bindings_count",
                count = self.webhooks.len() as i64
            ))
            .into_any_element();

        let mut list = div().w_full().flex().flex_col().gap(LIST_GAP);
        if self.webhooks.is_empty() {
            list = list.child(empty_row(tr!("discord_bindings_empty"), palette));
        }
        for (index, row) in self.webhooks.iter().enumerate() {
            list = list.child(self.render_binding(index, row, palette, cx));
        }
        list = list.child(self.render_add_bar(palette, cx));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_label(
                &tr!("discord_section_bindings"),
                palette,
                hint,
            ))
            .child(list)
            .into_any_element()
    }

    fn render_binding(
        &self,
        index: usize,
        row: &WebhookRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let summary = Self::summary_of(row);
        let summary_ink = if row.linked_actions.is_empty() {
            palette.text_faint
        } else {
            palette.text_muted
        };

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .child(icon(Icon::Variable, ROW_GLYPH, row_accent(index, palette)))
            .child(
                div()
                    .flex_none()
                    .min_w(ROW_NAME_MIN_W)
                    .font_family(body_family())
                    .text_size(ROW_NAME_FS)
                    .text_color(palette.text_primary)
                    .child(row.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(ROW_SUMMARY_FS)
                    .text_color(summary_ink)
                    .child(summary),
            )
            .child(self.render_row_menu(index, &row.name, palette, cx));

        div()
            .w_full()
            .child(
                card(body, palette)
                    .padding_xy(ROW_PAD_V, ROW_PAD_H)
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        index: usize,
        name: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.menu_open.as_deref() == Some(name);
        let view = cx.entity();
        let edit_name = name.to_owned();
        let test_name = name.to_owned();
        let delete_name = name.to_owned();
        let toggle_name = name.to_owned();

        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(vec![
                menu_item(
                    ("discord-menu-edit", index),
                    tr!("discord_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.edit_webhook(edit_name.clone(), cx)
                    }),
                )
                .icon(Icon::Edit)
                .into(),
                menu_item(
                    ("discord-menu-test", index),
                    tr!("discord_menu_test"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.test_webhook(test_name.clone(), cx)
                    }),
                )
                .icon(Icon::Send)
                .into(),
                menu_divider(),
                menu_item(
                    ("discord-menu-delete", index),
                    tr!("common_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.prompt_delete(delete_name.clone(), cx)
                    }),
                )
                .icon(Icon::Trash)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                ("discord-menu-trigger", index),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.toggle_menu(&toggle_name, event.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        pad_tile(
            "discord-add-webhook",
            icon(Icon::Plus, ADD_BAR_GLYPH, palette.brand),
            div()
                .flex()
                .items_center()
                .child(tr!("discord_add_binding")),
            palette,
        )
        .bar(palette)
        .title_color(palette.brand)
        .hover_border(palette.brand)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(cx)))
        .into_any_element()
    }

    fn render_posts(&self, palette: &ForgePalette) -> AnyElement {
        let last = match self.last_send_at() {
            Some(at) => fmt_relative_time(Some(at)),
            None => tr!("discord_posts_never"),
        };
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(last)
            .into_any_element();

        let mut list = div().w_full().flex().flex_col().gap(LIST_GAP);
        if self.posts.is_empty() {
            list = list.child(empty_row(tr!("discord_posts_empty"), palette));
        }
        for post in &self.posts {
            list = list.child(render_post(post, palette));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_label(&tr!("discord_section_posts"), palette, hint))
            .child(list)
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette) -> AnyElement {
        let (dot, right) = match self.health.last_send_ok {
            Some(true) => (palette.success, tr!("discord_footer_healthy")),
            Some(false) => (palette.random, tr!("discord_footer_failing")),
            None => (palette.text_faint, tr!("discord_footer_idle")),
        };

        div()
            .w_full()
            .mt(FOOTER_MT)
            .flex()
            .items_center()
            .justify_between()
            .py(FOOTER_PAD_V)
            .px(FOOTER_PAD_H)
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(FOOTER_GAP)
                    .child(footer_text(
                        tr!(
                            "discord_footer_webhooks",
                            count = self.webhooks.len() as i64
                        ),
                        palette,
                    ))
                    .child(footer_text(FOOTER_SEPARATOR.to_owned(), palette))
                    .child(footer_text(
                        tr!("discord_footer_linked", count = self.linked_total() as i64),
                        palette,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(FOOTER_GAP)
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(footer_text(right, palette)),
            )
            .into_any_element()
    }

    fn render_delete_confirm(
        &self,
        prompt: &DeletePrompt,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = if prompt.linked == 0 {
            tr!("discord_confirm_delete_body")
        } else {
            tr!(
                "discord_confirm_delete_body_linked",
                count = prompt.linked as i64
            )
        };
        let card = confirm_modal(
            tr!("discord_confirm_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.name.clone())
        .on_cancel(
            "discord-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "discord-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("discord-delete-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

fn render_post(post: &WebhookPost, palette: &ForgePalette) -> AnyElement {
    let (dot, kind_ink) = if post.ok {
        (palette.success, palette.text_muted)
    } else {
        (palette.random, palette.random)
    };
    let kind = if post.had_embed {
        tr!("discord_post_kind_embed")
    } else {
        tr!("discord_post_kind_message")
    };

    let body = div()
        .w_full()
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .child(status_dot(dot, ROW_DOT))
        .child(
            div()
                .flex_none()
                .min_w(ROW_NAME_MIN_W)
                .truncate()
                .font_family(mono_family())
                .text_size(ROW_NAME_FS)
                .text_color(palette.text_primary)
                .child(post.webhook_name.clone()),
        )
        .child(
            badge(
                palette.surface_overlay,
                kind_ink,
                kind,
                false,
                KIND_BADGE_FS,
            )
            .flex_none(),
        )
        .child(div().flex_1().min_w_0())
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(POST_TIME_FS)
                .text_color(palette.text_faint)
                .child(fmt_relative_time(Some(post.sent_at))),
        );

    div()
        .w_full()
        .child(
            card(body, palette)
                .padding_xy(ROW_PAD_V, ROW_PAD_H)
                .full_width(),
        )
        .into_any_element()
}

fn row_accent(index: usize, palette: &ForgePalette) -> Rgba {
    let wheel = [
        palette.random,
        palette.brand,
        palette.warning,
        palette.success,
        palette.info,
        palette.bits,
    ];
    wheel[index % wheel.len()]
}

fn empty_row(message: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .py(EMPTY_PAD_V)
        .font_family(body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_faint)
        .child(message)
}

fn footer_text(text: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FOOTER_FS)
        .text_color(palette.text_faint)
        .child(text)
}

fn stat_card(
    label: String,
    value: String,
    value_color: Rgba,
    hint: String,
    hint_color: Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(STAT_LABEL_MB)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .font_family(body_family())
                .text_size(FONT_SM)
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .mt(STAT_HINT_MT)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint),
        );

    grow_cell(
        card(body, palette)
            .padding_xy(STAT_PAD_V, STAT_PAD_H)
            .full_width()
            .full_height(),
        1.0,
    )
}

fn section_label(label: &str, palette: &ForgePalette, right: AnyElement) -> impl IntoElement {
    div()
        .w_full()
        .mt(SECTION_LABEL_MT)
        .mb(SECTION_LABEL_MB)
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(mono_family())
                .text_size(SECTION_LABEL_FS)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(right)
}

impl Render for DiscordScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        drive_overlay_focus(
            self.delete_prompt.is_some(),
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let header_right = div()
            .flex()
            .items_center()
            .gap(HEADER_GAP)
            .child(icon(Icon::BrandDiscord, HEADER_GLYPH, palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(HEADER_FS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "discord_header_summary",
                        webhooks = self.webhooks.len() as i64,
                        actions = self.linked_total() as i64
                    )),
            );

        let columns = div()
            .w_full()
            .flex()
            .items_start()
            .gap(COLUMN_GAP)
            .child(grow_cell(self.render_bindings(&palette, cx), COLUMN_FLEX))
            .child(grow_cell(self.render_posts(&palette), COLUMN_FLEX));

        let body = div()
            .id("discord-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(SCROLL_PAD_Y)
                    .px(SCROLL_PAD_X)
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette))
                    .child(self.render_stats(&palette))
                    .child(columns)
                    .child(self.render_footer(&palette)),
            );

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("discord_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("discord_hero_title")),
            ],
            &palette,
        )
        .header_right(header_right)
        .density(density)
        .body(body);

        let prompt = self
            .delete_prompt
            .as_ref()
            .map(|prompt| self.render_delete_confirm(prompt, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(self.modal.as_ref().map(|open| open.view.clone()))
            .children(prompt)
    }
}
