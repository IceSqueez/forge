use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, ForgePalette,
    Icon, Radius, Spacing, badge, fmt_clock, icon, radius, spacing, tr, with_alpha,
};
use forge_platform_twitch::{TWITCH_BROADCASTER_SCOPES, TwitchAuthFlow, UserInfo};
use forge_storage::CredentialsRepo;
use forge_types::OAuthToken;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, div, prelude::*, px};

use crate::async_bridge;
use crate::integration_detail::IntegrationDetail;

pub type TwitchFlowHandle = Arc<tokio::sync::Mutex<Option<TwitchAuthFlow>>>;

#[derive(Debug, Clone, Default)]
pub enum TwitchPanelState {
    #[default]
    Disconnected,
    Requesting,
    AwaitingAuthorization {
        auth_url: String,
        expires_at: SystemTime,
    },
    Authorizing,
    Error(String),
    MissingClientId,
}

struct TwitchLoopbackData {
    auth_url: String,
    expires_at: SystemTime,
}

struct TwitchAuthOutcome {
    token: OAuthToken,
    user_info: UserInfo,
    client_id: String,
}

const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

async fn request_code(flow: TwitchFlowHandle) -> Result<TwitchLoopbackData, String> {
    let mut guard = flow.lock().await;
    let inner = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    let code = inner.start().await.map_err(|e| e.to_string())?;
    Ok(TwitchLoopbackData {
        auth_url: code.auth_url,
        expires_at: SystemTime::now() + AUTH_TIMEOUT,
    })
}

async fn wait_for_auth(
    flow: TwitchFlowHandle,
    credentials: Arc<dyn CredentialsRepo>,
) -> Result<TwitchAuthOutcome, String> {
    let mut inner = {
        let mut guard = flow.lock().await;
        guard
            .take()
            .ok_or_else(|| "OAuth flow already consumed".to_owned())?
    };
    let bundle = inner
        .wait_for_authorization(AUTH_TIMEOUT)
        .await
        .map_err(|e| e.to_string())?;
    forge_platform_twitch::credentials::store(&*credentials, &bundle)
        .await
        .map_err(|e| e.to_string())?;
    Ok(TwitchAuthOutcome {
        token: bundle.access_token,
        user_info: bundle.user_info,
        client_id: bundle.client_id,
    })
}

impl IntegrationDetail {
    pub(crate) fn twitch_start_connect(&mut self, cx: &mut Context<Self>) {
        let Some(cid) = forge_platform_twitch::client_id() else {
            self.twitch_state = TwitchPanelState::MissingClientId;
            cx.notify();
            return;
        };
        self.twitch_state = TwitchPanelState::Requesting;
        let flow: TwitchFlowHandle =
            Arc::new(tokio::sync::Mutex::new(Some(TwitchAuthFlow::new(cid))));
        self.twitch_flow = Some(Arc::clone(&flow));

        async_bridge::run_async(
            &self.rt_handle,
            request_code(flow),
            |this, result, cx| this.apply_twitch_device_code(result, cx),
            cx,
        );
        cx.notify();
    }

    pub(crate) fn twitch_cancel(&mut self, cx: &mut Context<Self>) {
        self.twitch_state = TwitchPanelState::Disconnected;
        cx.notify();
    }

    pub(crate) fn twitch_open_auth_url(&mut self, cx: &mut Context<Self>) {
        if let TwitchPanelState::AwaitingAuthorization { auth_url, .. } = &self.twitch_state {
            self.open_url(auth_url.clone(), cx);
        }
    }

    fn apply_twitch_device_code(
        &mut self,
        result: Result<TwitchLoopbackData, String>,
        cx: &mut Context<Self>,
    ) {
        let data = match result {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!(error = %e, "twitch authorization start failed");
                self.twitch_state = TwitchPanelState::Error(e);
                cx.notify();
                return;
            }
        };
        let auth_url = data.auth_url.clone();
        self.twitch_state = TwitchPanelState::AwaitingAuthorization {
            auth_url: data.auth_url,
            expires_at: data.expires_at,
        };
        self.open_url(auth_url, cx);

        let Some(flow) = self.twitch_flow.clone() else {
            self.twitch_state = TwitchPanelState::Error("no active flow handle".to_owned());
            cx.notify();
            return;
        };
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            wait_for_auth(flow, credentials),
            |this, result, cx| this.apply_twitch_auth(result, cx),
            cx,
        );
        self.start_awaiting_tick(cx);
        cx.notify();
    }

    fn apply_twitch_auth(
        &mut self,
        result: Result<TwitchAuthOutcome, String>,
        cx: &mut Context<Self>,
    ) {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::warn!(error = %e, "twitch authorization failed");
                self.twitch_state = TwitchPanelState::Error(e);
                cx.notify();
                return;
            }
        };
        tracing::info!(
            login = %outcome.user_info.login,
            id = %outcome.user_info.id,
            "twitch authorization complete",
        );
        self.twitch_state = TwitchPanelState::Authorizing;

        let bus = Arc::clone(&self.bus);
        let credentials = Arc::clone(&self.credentials);
        let live_viewers = self.live_viewers.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let login = Some(outcome.user_info.login.clone());
                let tracker = forge_platform_twitch::SubscriptionTracker::default();
                let config = forge_platform_twitch::ChatSessionConfig {
                    client_id: outcome.client_id,
                    broadcaster_id: outcome.user_info.id.clone(),
                    user_id: outcome.user_info.id,
                };
                let chat = forge_platform_twitch::TwitchChat::new(
                    outcome.token,
                    config.client_id.clone(),
                    config.broadcaster_id.clone(),
                    config.user_id.clone(),
                    Arc::clone(&bus),
                    Arc::clone(&tracker),
                );
                let handle = chat.start();
                let bundle = forge_platform_twitch::TwitchIntegrationBundle::new(
                    login,
                    config,
                    bus,
                    credentials,
                    tracker,
                    handle,
                );
                live_viewers.register(bundle.viewer_source());
                bundle
            },
            |this, bundle, cx| this.install_twitch_bundle(bundle, cx),
            cx,
        );
        cx.notify();
    }

    fn start_awaiting_tick(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let still_awaiting = this
                    .update(cx, |this, cx| {
                        if matches!(
                            this.twitch_state,
                            TwitchPanelState::AwaitingAuthorization { .. }
                        ) {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !still_awaiting {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn twitch_connect_view(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let flow_card = match &self.twitch_state {
            TwitchPanelState::Disconnected => self.twitch_idle_card(palette, density, cx),
            TwitchPanelState::Requesting => self.twitch_requesting_card(palette, density),
            TwitchPanelState::AwaitingAuthorization {
                auth_url,
                expires_at,
            } => self.twitch_awaiting_card(auth_url, *expires_at, palette, density, cx),
            TwitchPanelState::Authorizing => self.twitch_authorizing_card(palette, density),
            TwitchPanelState::Error(msg) => self.twitch_error_card(msg, palette, density, cx),
            TwitchPanelState::MissingClientId => {
                self.twitch_missing_client_id_card(palette, density)
            }
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.twitch_header_card(palette, density))
            .child(flow_card)
            .child(self.twitch_scopes_card(palette, density))
            .into_any_element()
    }

    fn twitch_header_card(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let tile = div()
            .flex_none()
            .size(px(48.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(11.0))
            .bg(palette.brand)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(24.0))
                    .text_color(palette.shell)
                    .child("T"),
            );
        let title_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Twitch"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_header_subtitle")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(tile)
            .child(title_col)
    }

    fn twitch_flow_intro(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::Lock, px(14.0), palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_auth_title")),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_auth_subtitle")),
            )
    }

    fn twitch_flow_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        body: AnyElement,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(self.twitch_flow_intro(palette, density))
            .child(body)
            .into_any_element()
    }

    fn twitch_idle_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let start_btn = div()
            .id("twitch-connect-start")
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.twitch_start_connect(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child(tr!("twitch_btn_start")),
            );
        let body = div()
            .w_full()
            .flex()
            .justify_center()
            .p(spacing(Spacing::Md, density))
            .child(start_btn)
            .into_any_element();
        self.twitch_flow_card(palette, density, body)
    }

    fn twitch_requesting_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let body = div()
            .w_full()
            .flex()
            .justify_center()
            .p(spacing(Spacing::Md, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_requesting")),
            )
            .into_any_element();
        self.twitch_flow_card(palette, density, body)
    }

    fn twitch_authorizing_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let body = div()
            .w_full()
            .flex()
            .justify_center()
            .p(spacing(Spacing::Md, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_authorizing")),
            )
            .into_any_element();
        self.twitch_flow_card(palette, density, body)
    }

    fn twitch_missing_client_id_card(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let body = div()
            .w_full()
            .p(spacing(Spacing::Md, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_missing_client_id")),
            )
            .into_any_element();
        self.twitch_flow_card(palette, density, body)
    }

    fn twitch_error_card(
        &self,
        msg: &str,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let retry = div()
            .id("twitch-retry")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.twitch_start_connect(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child(tr!("twitch_btn_try_again")),
            );
        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(msg.to_owned()),
            )
            .child(retry)
            .into_any_element();
        self.twitch_flow_card(palette, density, body)
    }

    fn twitch_awaiting_card(
        &self,
        auth_url: &str,
        expires_at: SystemTime,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url_box = div()
            .flex_1()
            .min_w(px(0.0))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.info)
            .child(auth_url.to_owned());
        let open_btn = div()
            .id("twitch-open-url")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.border_regular, 0.06)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.twitch_open_auth_url(cx)))
            .child(icon(Icon::ExternalLink, px(13.0), palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.brand)
                    .child(tr!("twitch_btn_open")),
            );
        let step1_content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("twitch_step1_title")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(url_box)
                    .child(open_btn),
            )
            .into_any_element();
        let step1 = self.twitch_step("1", false, step1_content, palette, density);

        let remaining = expires_at
            .duration_since(SystemTime::now())
            .unwrap_or_default();
        let restart_btn = div()
            .id("twitch-restart")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.twitch_start_connect(cx)))
            .child(icon(Icon::Refresh, px(12.0), palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child(tr!("twitch_btn_restart")),
            );
        let timer_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Clock, px(13.0), palette.text_muted))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(format!("{} ", tr!("twitch_timer_prefix"))),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(fmt_clock(remaining.as_secs())),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("\u{00b7}"),
            )
            .child(restart_btn);
        let step2_content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("twitch_step2_title")),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_step2_detail")),
            )
            .child(timer_row)
            .into_any_element();
        let step2 = self.twitch_step("2", true, step2_content, palette, density);

        let polling = self.twitch_polling_banner(palette, density, cx);

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .child(step1)
            .child(step2)
            .child(polling)
            .into_any_element();
        self.twitch_flow_card(palette, density, body)
    }

    fn twitch_step(
        &self,
        n: &str,
        active: bool,
        content: AnyElement,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement {
        let (bg, fg) = if active {
            (palette.brand, palette.shell)
        } else {
            (palette.surface_overlay, palette.text_primary)
        };
        let circle = div()
            .flex_none()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.0))
            .bg(bg)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(fg)
                    .child(n.to_owned()),
            );
        div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Sm, density))
            .child(circle)
            .child(div().flex_1().min_w(px(0.0)).child(content))
    }

    fn twitch_polling_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cancel = div()
            .id("twitch-cancel")
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.border_regular, 0.06)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.twitch_cancel(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(tr!("twitch_btn_cancel")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(
                div()
                    .flex_none()
                    .size(px(8.0))
                    .rounded(px(4.0))
                    .bg(palette.brand),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_polling_primary")),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_faint)
                            .child(tr!("twitch_polling_secondary")),
                    ),
            )
            .child(cancel)
    }

    fn twitch_scopes_card(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::CircleCheck, px(13.0), palette.success))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_scopes_header")),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "twitch_scopes_count",
                        count = TWITCH_BROADCASTER_SCOPES.len() as i64
                    )),
            );

        let mut pills_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density));
        for chunk in TWITCH_BROADCASTER_SCOPES.chunks(3) {
            let mut row = div().flex().gap(spacing(Spacing::Xxs, density));
            for scope in chunk {
                row = row.child(twitch_scope_pill(scope, palette, density));
            }
            pills_col = pills_col.child(row);
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(header)
            .child(pills_col)
    }

    pub(crate) fn twitch_reauth_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cta = div()
            .id("twitch-reauth")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.warning)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.warning, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reset_twitch_to_connect(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("twitch_reauth_btn")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.shell)
            .child(icon(Icon::AlertTriangle, px(14.0), palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_reauth_title")),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_reauth_detail")),
                    ),
            )
            .child(cta)
            .into_any_element()
    }
}

fn twitch_scope_pill(scope: &str, palette: &ForgePalette, density: Density) -> impl IntoElement {
    badge(
        palette.surface_overlay,
        palette.success,
        scope.to_owned(),
        true,
        FONT_XS,
    )
    .weight(FontWeight::NORMAL)
    .padding_xy(
        spacing(Spacing::Xxs, density),
        spacing(Spacing::Xs, density),
    )
    .radius(px(8.0))
    .flex_none()
}
