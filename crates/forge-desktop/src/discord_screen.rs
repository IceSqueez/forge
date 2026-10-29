use forge_components::Density;
use forge_components::FONT_BADGE;
use forge_components::FONT_HERO;
use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::FONT_XXS;
use forge_components::Spacing;
use forge_components::spacing;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, FONT_XS, ForgePalette, Icon, MenuPlacement,
    OverlayPosition, SectionLabel, ToastKind, badge, body_family, card, confirm_modal,
    fmt_display_relative, icon, menu_button, menu_divider, menu_item, mono_family, overlay,
    pad_tile, page_frame, scaled, status_dot, tr,
};
use forge_discord::{DiscordClient, DiscordSendHealth, WebhookPost};
use forge_runtime::EventBus;
use forge_storage::ActionRepo;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Point, Rgba, Subscription, Task,
    Window, div, prelude::*, px,
};
use time::OffsetDateTime;

use crate::async_bridge::{self, BridgeFlow, drain_events};
use crate::discord_webhook_modal::{
    DiscordWebhookModal, DiscordWebhookModalEvent, WebhookDraft, WebhookModalLaunch,
};
use crate::discord_webhooks::{WebhookRow, distinct_linked_actions, load_webhooks, name_is_taken};
use crate::hub_crumb::hub_crumb;
use crate::integrations::BuiltinObject;
use crate::navigation::NavRequested;
use crate::runtime_event;
use forge_components::ActivePresentation;
use forge_components::PushToast;
use forge_components::{grow_cell, stat_card};
use forge_runtime::IntegrationSlot;

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const HERO_PAD_V: Pixels = px(14.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(14.0);
const HERO_MARGIN_B: Pixels = px(14.0);
const HERO_TILE: Pixels = px(40.0);
const HERO_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = FONT_HERO;
const HERO_BLURB_MT: Pixels = px(1.0);

const COUNT_TILE: Pixels = px(26.0);
const COUNT_TILE_RADIUS: Pixels = px(7.0);
const COUNT_GLYPH: Pixels = px(13.0);
const COUNT_GAP: Pixels = px(8.0);
const COUNT_VALUE_FS: Pixels = FONT_XS;
const COUNT_SUB_FS: Pixels = FONT_MICRO;

const STAT_MARGIN_B: Pixels = px(14.0);

const COLUMN_GAP: Pixels = px(12.0);
const COLUMN_FLEX: f32 = 1.0;

const SECTION_HINT_FS: Pixels = FONT_MICRO;

const ROW_PAD_V: Pixels = px(9.0);
const ROW_PAD_H: Pixels = px(12.0);
const ROW_GAP: Pixels = px(9.0);
const ROW_GLYPH: Pixels = px(13.0);
const ROW_NAME_MIN_W: Pixels = px(78.0);
const ROW_NAME_FS: Pixels = FONT_XS;
const ROW_SUMMARY_FS: Pixels = FONT_LABEL;
const ROW_DOT: Pixels = px(6.0);
const KIND_BADGE_FS: Pixels = FONT_BADGE;
const POST_TIME_FS: Pixels = FONT_XXS;
const EMPTY_PAD_V: Pixels = px(14.0);

const ADD_BAR_GLYPH: Pixels = px(13.0);

const FOOTER_FS: Pixels = FONT_XXS;
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);
const FOOTER_SEPARATOR: &str = "·";

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);
const HEADER_FS: Pixels = FONT_LABEL;

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
    integration: Option<IntegrationSlot<BuiltinObject>>,
    action_repo: Arc<dyn ActionRepo>,
    rt_handle: tokio::runtime::Handle,
    webhooks: Vec<WebhookRow>,
    posts: Vec<WebhookPost>,
    health: DiscordSendHealth,
    menu_open: Option<String>,
    menu_click_pos: Option<Point<Pixels>>,
    modal: Option<OpenModal>,
    delete_prompt: Option<DeletePrompt>,
    _bus_bridge: Task<()>,
}

impl DiscordScreenView {
    pub fn new(
        client: Arc<DiscordClient>,
        integration: Option<IntegrationSlot<BuiltinObject>>,
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
            integration,
            action_repo,
            rt_handle,
            webhooks: Vec::new(),
            menu_open: None,
            menu_click_pos: None,
            modal: None,
            delete_prompt: None,
            _bus_bridge: bus_bridge,
        };
        view.load(cx);
        view
    }

    fn spawn_bus_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                if !batch.iter().any(runtime_event::is_discord_event) {
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
                    if let Some(integration) = &this.integration {
                        integration.request_enable();
                    }
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
        let content = tr!("discord_test_content");
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .post_test(&name, &url, &content)
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
        let content = tr!("discord_test_content");
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                client
                    .post_text(&name, &content)
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

    fn render_hero(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let count = self.webhooks.len();
        let badge_slot = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(scaled(COUNT_GAP, density))
            .mr(spacing(Spacing::Xxs, density))
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
            .gap(scaled(HERO_GAP, density))
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
            .mb(scaled(HERO_MARGIN_B, density))
            .child(
                card(body, palette)
                    .padding_xy(scaled(HERO_PAD_V, density), scaled(HERO_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_stats(&self, palette: &ForgePalette, density: Density) -> AnyElement {
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
            Some(false) => (tr!("discord_stat_send_failed"), palette.danger),
            None => (NO_VALUE.to_owned(), palette.text_primary),
        };
        let send_hint = match self.last_send_at() {
            Some(at) => fmt_display_relative(Some(at)),
            None => tr!("discord_stat_send_none"),
        };

        let errors = health.errors_last_hour;
        let errors_ink = if errors == 0 {
            palette.success
        } else {
            palette.danger
        };

        div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(spacing(Spacing::Sm, density))
            .mb(scaled(STAT_MARGIN_B, density))
            .child(stat_card(
                tr!("discord_stat_latency"),
                latency_value,
                palette.text_primary,
                latency_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("discord_stat_budget"),
                budget_value,
                budget_ink,
                budget_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("discord_stat_send"),
                send_value,
                send_ink,
                send_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("discord_stat_errors"),
                errors.to_string(),
                errors_ink,
                tr!("discord_stat_errors_hint"),
                palette.text_faint,
                palette,
                density,
            ))
            .into_any_element()
    }

    fn render_bindings(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(tr!(
                "discord_section_bindings_count",
                count = self.webhooks.len() as i64
            ))
            .into_any_element();

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if self.webhooks.is_empty() {
            list = list.child(empty_row(tr!("discord_bindings_empty"), palette, density));
        }
        for (index, row) in self.webhooks.iter().enumerate() {
            list = list.child(self.render_binding(index, row, palette, cx));
        }
        list = list.child(self.render_add_bar(palette, cx));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("discord_section_bindings"), palette)
                    .row()
                    .trailing(Some(hint)),
            )
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
        let density = cx.density();
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
            .gap(scaled(ROW_GAP, density))
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
                    .padding_xy(scaled(ROW_PAD_V, density), scaled(ROW_PAD_H, density))
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
                .color(palette.danger)
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

    fn render_posts(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let last = match self.last_send_at() {
            Some(at) => fmt_display_relative(Some(at)),
            None => tr!("discord_posts_never"),
        };
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(last)
            .into_any_element();

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if self.posts.is_empty() {
            list = list.child(empty_row(tr!("discord_posts_empty"), palette, density));
        }
        for post in &self.posts {
            list = list.child(render_post(post, palette, density));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("discord_section_posts"), palette)
                    .row()
                    .trailing(Some(hint)),
            )
            .child(list)
            .into_any_element()
    }

    fn render_footer(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (dot, right) = match self.health.last_send_ok {
            Some(true) => (palette.success, tr!("discord_footer_healthy")),
            Some(false) => (palette.danger, tr!("discord_footer_failing")),
            None => (palette.text_faint, tr!("discord_footer_idle")),
        };

        div()
            .w_full()
            .mt(FOOTER_MT)
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(FOOTER_PAD_V, density))
            .px(scaled(FOOTER_PAD_H, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
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
                    .gap(spacing(Spacing::Xs, density))
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
            .on_dismiss("discord-delete-dismiss", move |_window, cx| {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

fn render_post(post: &WebhookPost, palette: &ForgePalette, density: Density) -> AnyElement {
    let (dot, kind_ink) = if post.ok {
        (palette.success, palette.text_muted)
    } else {
        (palette.danger, palette.danger)
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
        .gap(scaled(ROW_GAP, density))
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
                .child(fmt_display_relative(Some(post.sent_at))),
        );

    div()
        .w_full()
        .child(
            card(body, palette)
                .padding_xy(scaled(ROW_PAD_V, density), scaled(ROW_PAD_H, density))
                .full_width(),
        )
        .into_any_element()
}

fn row_accent(index: usize, palette: &ForgePalette) -> Rgba {
    let wheel = [
        palette.danger,
        palette.brand,
        palette.warning,
        palette.success,
        palette.info,
        palette.accent_peach,
    ];
    wheel[index % wheel.len()]
}

fn empty_row(message: String, palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .w_full()
        .py(scaled(EMPTY_PAD_V, density))
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

impl EventEmitter<NavRequested> for DiscordScreenView {}

impl Render for DiscordScreenView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = div()
            .flex()
            .items_center()
            .gap(scaled(HEADER_GAP, density))
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
            .gap(scaled(COLUMN_GAP, density))
            .child(grow_cell(self.render_bindings(&palette, cx), COLUMN_FLEX))
            .child(grow_cell(self.render_posts(&palette, density), COLUMN_FLEX));

        let body = div()
            .id("discord-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(SCROLL_PAD_Y, density))
                    .px(scaled(SCROLL_PAD_X, density))
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, density))
                    .child(self.render_stats(&palette, density))
                    .child(columns)
                    .child(self.render_footer(&palette, density)),
            );

        let frame = page_frame(
            vec![
                hub_crumb(Some(forge_discord::DISCORD_INTEGRATION.category), cx),
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

#[cfg(test)]
mod tests {
    use forge_storage::CredentialsRepo;
    use gpui::TestAppContext;

    use forge_storage::credentials::MockCredentialsRepo;
    use forge_storage::{CredentialId, Language};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::i18n::install_language;
    use crate::navigation::Screen;
    use crate::test_support::{
        StubActions, alive_tasks, hub_crumb_targets, install_presentation, quiet_bus, runtime,
        test_backend, wait_until_worker_drains_to, worker_runtime,
    };

    #[gpui::test]
    fn the_crumb_leads_back_to_the_discord_category_on_the_hub(cx: &mut TestAppContext) {
        let rt = runtime();
        let bus = quiet_bus(&rt);
        let (backend, _writes) = test_backend();
        let client = {
            let _entered = rt.enter();
            DiscordClient::new(
                forge_platform_core::PlatformEndpoints::default()
                    .base_url(forge_platform_core::EndpointSurface::DiscordApi),
                Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                backend as Arc<dyn CredentialsRepo>,
            )
            .expect("the discord gateway builds")
        };
        let handle = rt.handle().clone();

        let targets = hub_crumb_targets(cx, |_, cx| {
            DiscordScreenView::new(client, None, Arc::new(StubActions), bus, handle, cx)
        });

        let category = forge_discord::DISCORD_INTEGRATION.category;
        assert!(!targets.is_empty(), "no click reached the crumb");
        assert!(
            targets
                .iter()
                .all(|screen| *screen == Screen::Integrations(Some(category))),
            "{targets:?}"
        );
    }

    const POSTED_REPLY: &str = r#"{"id":"1"}"#;

    fn header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|w| w == b"\r\n\r\n")
    }

    fn content_length(head: &[u8]) -> usize {
        String::from_utf8_lossy(head)
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().ok())
                    .flatten()
            })
            .unwrap_or_default()
    }

    async fn first_posted_body(listener: TcpListener) -> Option<serde_json::Value> {
        let (mut socket, _) = listener.accept().await.ok()?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let body = loop {
            let read = socket.read(&mut chunk).await.ok()?;
            if read == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..read]);
            if let Some(end) = header_end(&buf) {
                let start = end + 4;
                let len = content_length(&buf[..end]);
                if buf.len() >= start + len {
                    break buf[start..start + len].to_vec();
                }
            }
        };
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{POSTED_REPLY}",
            POSTED_REPLY.len()
        );
        socket.write_all(reply.as_bytes()).await.ok()?;
        serde_json::from_slice(&body).ok()
    }

    fn webhook_pointing_at(url: String) -> MockCredentialsRepo {
        let mut creds = MockCredentialsRepo::new();
        creds
            .expect_list_ids()
            .returning(|| Ok(vec![CredentialId::new("discord:alerts")]));
        creds
            .expect_load()
            .returning(move |_| Ok(Some(serde_json::json!({ "url": url }).to_string())));
        creds.expect_store().returning(|_, _| Ok(()));
        creds
    }

    #[gpui::test]
    fn testing_a_saved_webhook_posts_the_message_in_the_ui_language_from_a_worker_thread(
        cx: &mut TestAppContext,
    ) {
        let rt = worker_runtime();
        let bus = quiet_bus(&rt);
        let listener = rt
            .block_on(TcpListener::bind("127.0.0.1:0"))
            .expect("a loopback listener binds");
        let addr = listener
            .local_addr()
            .expect("a bound listener has an address");
        let creds = webhook_pointing_at("https://discord.com/api/webhooks/1/token".to_owned());
        let client = {
            let _entered = rt.enter();
            DiscordClient::new(
                &format!("http://{addr}/api"),
                Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                Arc::new(creds) as Arc<dyn CredentialsRepo>,
            )
            .expect("the discord gateway builds")
        };
        let background_tasks = alive_tasks(&rt);
        let posted = rt.spawn(first_posted_body(listener));
        install_language(Language::Uk);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let handle = rt.handle().clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                DiscordScreenView::new(client, None, Arc::new(StubActions), bus, handle, cx)
            })
        });

        view.update(cx, |view, cx| view.test_webhook("alerts".to_owned(), cx));
        let body = rt.block_on(posted).ok().flatten();
        wait_until_worker_drains_to(&rt, background_tasks);
        cx.run_until_parked();

        assert_eq!(
            body.map(|body| body["content"].clone()),
            Some(serde_json::json!("Тестове повідомлення з forge."))
        );
    }
}
