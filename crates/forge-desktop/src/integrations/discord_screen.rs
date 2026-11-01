mod sections;

use forge_components::Density;
use forge_components::FONT_BADGE;
use forge_components::FONT_HERO;
use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::FONT_XXS;
use forge_components::Spacing;
use forge_components::spacing;
use std::collections::HashSet;
use std::sync::Arc;

use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BORDER_THIN, BreadcrumbCrumb, ConfirmTone, FONT_XS, ForgePalette, Icon,
    MenuPlacement, OverlayPosition, SectionLabel, ToastKind, badge, card, confirm_modal,
    fmt_display_relative, icon, menu_button, menu_divider, menu_item, overlay, pad_tile,
    page_frame, scaled, status_dot, tr,
};
use forge_discord::{DiscordClient, DiscordSendHealth, WebhookPost};
use forge_runtime::EventBus;
use forge_storage::ActionRepo;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Point, Rgba, Subscription, Task,
    Window, div, prelude::*, px,
};
use time::OffsetDateTime;

use crate::app::async_bridge::{self, BridgeFlow, Loadable, drain_events};
use crate::app::navigation::NavRequested;
use crate::app::runtime_event;
use crate::integrations::discord_webhook_modal::{
    DiscordWebhookModal, DiscordWebhookModalEvent, WebhookDraft, WebhookModalLaunch,
};
use crate::integrations::discord_webhooks::{
    WebhookRow, distinct_linked_actions, load_webhooks, name_is_taken,
};
use crate::integrations::hub_crumb::hub_crumb;
use crate::shared::load_notice::{LoadNotice, NoticeWidth, load_notice};
use crate::shared::repo_failure::RepoFailure;
use forge_components::PushToast;
use forge_components::{grow_cell, stat_card};
use forge_host::integrations::BuiltinObject;
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

struct DiscordServices {
    client: Arc<DiscordClient>,
    integration: Option<IntegrationSlot<BuiltinObject>>,
    action_repo: Arc<dyn ActionRepo>,
    rt_handle: tokio::runtime::Handle,
}

struct WebhooksData {
    rows: Vec<WebhookRow>,
    load: Loadable<()>,
    posts: Vec<WebhookPost>,
    health: DiscordSendHealth,
    saving: bool,
    testing_rows: HashSet<String>,
}

#[derive(Default)]
struct RowMenuState {
    open: Option<String>,
    click_pos: Option<Point<Pixels>>,
}

pub(crate) struct DiscordScreenView {
    services: DiscordServices,
    data: WebhooksData,
    row_menu: RowMenuState,
    modal: Option<OpenModal>,
    delete_prompt: Option<DeletePrompt>,
    _bus_bridge: Task<()>,
}

impl DiscordScreenView {
    pub(crate) fn new(
        client: Arc<DiscordClient>,
        integration: Option<IntegrationSlot<BuiltinObject>>,
        action_repo: Arc<dyn ActionRepo>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let bus_bridge = Self::spawn_bus_bridge(bus, cx);
        let mut view = Self {
            data: WebhooksData {
                rows: Vec::new(),
                load: Loadable::Loading,
                posts: client.recent_posts(),
                health: client.send_health(),
                saving: false,
                testing_rows: HashSet::new(),
            },
            services: DiscordServices {
                client,
                integration,
                action_repo,
                rt_handle,
            },
            row_menu: RowMenuState::default(),
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
        let client = Arc::clone(&self.services.client);
        let actions = Arc::clone(&self.services.action_repo);
        async_bridge::run_async(
            &self.services.rt_handle,
            load_webhooks(client, actions),
            |this, result, cx| this.apply_webhooks(result, cx),
            cx,
        );
    }

    fn apply_webhooks(
        &mut self,
        result: Result<Vec<WebhookRow>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(rows) => {
                self.data.rows = rows;
                self.data.load = Loadable::Loaded(());
            }
            Err(failure) => {
                if !self.data.rows.is_empty() {
                    self.on_error(&failure.text(), cx);
                }
                self.data.load = Loadable::Failed(failure);
            }
        }
        self.refresh_from_client(cx);
    }

    fn retry_webhooks_load(&mut self, cx: &mut Context<Self>) {
        self.data.load = Loadable::Loading;
        self.load(cx);
        cx.notify();
    }

    fn refresh_from_client(&mut self, cx: &mut Context<Self>) {
        self.data.posts = self.services.client.recent_posts();
        self.data.health = self.services.client.send_health();
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
        self.data.posts.first().map(|post| post.sent_at)
    }

    fn linked_total(&self) -> usize {
        distinct_linked_actions(&self.data.rows)
    }

    fn linked_count(&self, name: &str) -> usize {
        self.data
            .rows
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
                count = i64::try_from(row.linked_actions.len()).unwrap_or(i64::MAX)
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
        self.row_menu.open = None;
        let client = Arc::clone(&self.services.client);
        let lookup = name.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                client
                    .webhook_url(&lookup)
                    .await
                    .map_err(RepoFailure::from)
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
                Err(message) => this.on_error(&message.text(), cx),
            },
            cx,
        );
        cx.notify();
    }

    fn open_modal(&mut self, launch: WebhookModalLaunch, cx: &mut Context<Self>) {
        let view = cx.new(|cx| DiscordWebhookModal::new(launch, cx));
        let sub = cx.subscribe(&view, Self::on_modal_event);
        self.modal = Some(OpenModal { view, _sub: sub });
        self.row_menu.open = None;
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
        if self.data.saving {
            return;
        }
        let name = draft.name.clone();
        if draft.original_name.is_none() && name_is_taken(&self.data.rows, &name) {
            cx.push_toast(
                ToastKind::Error,
                tr!("discord_toast_name_taken", name = name.as_str()),
            );
            cx.notify();
            return;
        }

        self.data.saving = true;
        let client = Arc::clone(&self.services.client);
        let url = draft.url.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                client
                    .save_webhook(&name, &url)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| {
                this.data.saving = false;
                match result {
                    Ok(()) => {
                        this.close_modal(cx);
                        cx.push_toast(ToastKind::Success, tr!("discord_toast_saved"));
                        if let Some(integration) = &this.services.integration {
                            integration.request_enable();
                        }
                        this.load(cx);
                    }
                    Err(message) => this.on_error(&message.text(), cx),
                }
            },
            cx,
        );
    }

    fn test_draft(&mut self, draft: &WebhookDraft, cx: &mut Context<Self>) {
        let client = Arc::clone(&self.services.client);
        let name = draft.name.clone();
        let url = draft.url.clone();
        let content = tr!("discord_test_content");
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                client
                    .post_test(&name, &url, &content)
                    .await
                    .map(|_| ())
                    .map_err(RepoFailure::from)
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
        self.row_menu.open = None;
        if !self.data.testing_rows.insert(name.clone()) {
            cx.notify();
            return;
        }
        let settled_name = name.clone();
        let client = Arc::clone(&self.services.client);
        let content = tr!("discord_test_content");
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                client
                    .post_text(&name, &content, forge_discord::MentionPolicy::default())
                    .await
                    .map(|_| ())
                    .map_err(RepoFailure::from)
            },
            move |this, result, cx| {
                this.data.testing_rows.remove(&settled_name);
                this.report_test(result, cx);
            },
            cx,
        );
        cx.notify();
    }

    fn report_test(&mut self, result: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => cx.push_toast(ToastKind::Success, tr!("discord_toast_test_sent")),
            Err(message) => cx.push_toast(
                ToastKind::Error,
                tr!("discord_toast_test_failed", message = message.text()),
            ),
        }
        self.refresh_from_client(cx);
    }

    fn prompt_delete(&mut self, name: String, cx: &mut Context<Self>) {
        self.row_menu.open = None;
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
        let client = Arc::clone(&self.services.client);
        let name = prompt.name;
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                client
                    .delete_webhook(&name)
                    .await
                    .map(|_| ())
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| match result {
                Ok(()) => {
                    cx.push_toast(ToastKind::Success, tr!("discord_toast_deleted"));
                    this.load(cx);
                }
                Err(message) => this.on_error(&message.text(), cx),
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_menu(&mut self, name: &str, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.row_menu.open = if self.row_menu.open.as_deref() == Some(name) {
            None
        } else {
            self.row_menu.click_pos = Some(position);
            Some(name.to_owned())
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.row_menu.open = None;
        cx.notify();
    }
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
                    .font_family(cx.body_family())
                    .text_size(HEADER_FS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "discord_header_summary",
                        webhooks = i64::try_from(self.data.rows.len()).unwrap_or(i64::MAX),
                        actions = i64::try_from(self.linked_total()).unwrap_or(i64::MAX)
                    )),
            );

        let columns = div()
            .w_full()
            .flex()
            .items_start()
            .gap(scaled(COLUMN_GAP, density))
            .child(grow_cell(self.render_bindings(&palette, cx), COLUMN_FLEX))
            .child(grow_cell(
                self.render_posts(&palette, density, cx),
                COLUMN_FLEX,
            ));

        let body = div()
            .id("discord-scroll")
            .flex_1()
            .h_full()
            .overflow_y_revealing_focus()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(SCROLL_PAD_Y, density))
                    .px(scaled(SCROLL_PAD_X, density))
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, density, cx))
                    .child(self.render_stats(&palette, density, cx))
                    .child(columns)
                    .child(self.render_footer(&palette, density, cx)),
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
    use crate::app::i18n::install_language;
    use crate::app::navigation::Screen;
    use crate::test_support::{
        StubActions, alive_tasks, error_toasts, hub_crumb_targets, install_presentation,
        pump_until, quiet_bus, runtime, test_backend, wait_until_worker_drains_to, worker_runtime,
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
                    .base_url(forge_discord::endpoint_surfaces::DISCORD_API),
                Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                backend as Arc<dyn CredentialsRepo>,
            )
            .expect("the discord gateway builds")
        };
        let handle = rt.handle().clone();

        let category = forge_discord::DISCORD_INTEGRATION.category;
        let targets = hub_crumb_targets(cx, Some(category), |_, cx| {
            DiscordScreenView::new(client, None, Arc::new(StubActions), bus, handle, cx)
        });

        assert_eq!(targets, vec![Screen::Integrations(Some(category))]);
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
        let background_tasks = alive_tasks(&rt);
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

    const VALID_WEBHOOK_URL: &str = "https://discord.com/api/webhooks/1/token";
    const STORE_REFUSED: &str = "the credential store is read-only";

    struct Counted {
        stores: Arc<std::sync::atomic::AtomicUsize>,
        lookups: Arc<std::sync::atomic::AtomicUsize>,
    }

    fn counted_creds(store_fails: bool) -> (MockCredentialsRepo, Counted) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let stores = Arc::new(AtomicUsize::new(0));
        let lookups = Arc::new(AtomicUsize::new(0));
        let (stored, looked_up) = (Arc::clone(&stores), Arc::clone(&lookups));
        let mut creds = MockCredentialsRepo::new();
        creds.expect_list_ids().returning(|| Ok(Vec::new()));
        creds.expect_load().returning(move |_| {
            looked_up.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        });
        creds.expect_store().returning(move |_, _| {
            stored.fetch_add(1, Ordering::SeqCst);
            if store_fails {
                Err(forge_storage::StorageError::Connection {
                    reason: STORE_REFUSED.to_owned(),
                })
            } else {
                Ok(())
            }
        });
        (creds, Counted { stores, lookups })
    }

    fn view_over(
        creds: MockCredentialsRepo,
        rt: &tokio::runtime::Runtime,
        cx: &mut TestAppContext,
    ) -> Entity<DiscordScreenView> {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let bus = quiet_bus(rt);
        let client = {
            let _entered = rt.enter();
            DiscordClient::new(
                "http://127.0.0.1:1/api",
                Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                Arc::new(creds) as Arc<dyn CredentialsRepo>,
            )
            .expect("the discord gateway builds")
        };
        let handle = rt.handle().clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                DiscordScreenView::new(client, None, Arc::new(StubActions), bus, handle, cx)
            })
        });
        settle_discord(&view, rt, cx);
        view
    }

    fn settle_discord(
        view: &Entity<DiscordScreenView>,
        rt: &tokio::runtime::Runtime,
        cx: &mut TestAppContext,
    ) {
        pump_until(rt, || {
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                !view.data.saving && view.data.testing_rows.is_empty()
            })
        });
    }

    fn draft() -> WebhookDraft {
        WebhookDraft {
            original_name: None,
            name: "alerts".to_owned(),
            url: VALID_WEBHOOK_URL.to_owned(),
        }
    }

    fn modal_open(view: &Entity<DiscordScreenView>, cx: &mut TestAppContext) -> bool {
        view.read_with(cx, |view, _| view.modal.is_some())
    }

    #[gpui::test]
    fn a_saved_webhook_closes_the_modal_only_once_the_write_landed(cx: &mut TestAppContext) {
        let rt = runtime();
        let (creds, _counted) = counted_creds(false);
        let view = view_over(creds, &rt, cx);
        view.update(cx, |view, cx| {
            view.open_add_modal(cx);
            view.save_webhook(&draft(), cx);
        });
        let open_while_writing = modal_open(&view, cx);

        settle_discord(&view, &rt, cx);

        assert_eq!((open_while_writing, modal_open(&view, cx)), (true, false));
    }

    #[gpui::test]
    fn a_refused_write_keeps_the_modal_open_and_reports_the_failure(cx: &mut TestAppContext) {
        let rt = runtime();
        let (creds, _counted) = counted_creds(true);
        let view = view_over(creds, &rt, cx);
        view.update(cx, |view, cx| {
            view.open_add_modal(cx);
            view.save_webhook(&draft(), cx);
        });

        settle_discord(&view, &rt, cx);

        assert_eq!(
            (modal_open(&view, cx), cx.update(|cx| error_toasts(cx))),
            (true, 1)
        );
    }

    #[gpui::test]
    fn a_second_save_while_one_runs_writes_nothing_more(cx: &mut TestAppContext) {
        let rt = runtime();
        let (creds, counted) = counted_creds(false);
        let view = view_over(creds, &rt, cx);

        view.update(cx, |view, cx| {
            view.save_webhook(&draft(), cx);
            view.save_webhook(&draft(), cx);
        });
        settle_discord(&view, &rt, cx);

        assert_eq!(counted.stores.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[gpui::test]
    fn a_second_row_test_while_one_runs_sends_nothing_more(cx: &mut TestAppContext) {
        let rt = runtime();
        let (creds, counted) = counted_creds(false);
        let view = view_over(creds, &rt, cx);

        view.update(cx, |view, cx| {
            view.test_webhook("alerts".to_owned(), cx);
            view.test_webhook("alerts".to_owned(), cx);
        });
        settle_discord(&view, &rt, cx);

        assert_eq!(counted.lookups.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[gpui::test]
    fn a_settled_row_test_can_be_sent_again(cx: &mut TestAppContext) {
        let rt = runtime();
        let (creds, counted) = counted_creds(false);
        let view = view_over(creds, &rt, cx);

        for _ in 0..2 {
            view.update(cx, |view, cx| view.test_webhook("alerts".to_owned(), cx));
            settle_discord(&view, &rt, cx);
        }

        assert_eq!(counted.lookups.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[gpui::test]
    fn a_test_on_one_row_leaves_another_row_free(cx: &mut TestAppContext) {
        let rt = runtime();
        let (creds, counted) = counted_creds(false);
        let view = view_over(creds, &rt, cx);

        view.update(cx, |view, cx| {
            view.test_webhook("alerts".to_owned(), cx);
            view.test_webhook("raids".to_owned(), cx);
        });
        settle_discord(&view, &rt, cx);

        assert_eq!(counted.lookups.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
