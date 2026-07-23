use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, ForgePalette, Icon, Radius, Spacing, body_family, icon,
    radius, spacing, tr, with_alpha,
};
use forge_platform_twitch::{TwitchAuthFlow, UserInfo};
use forge_storage::CredentialsRepo;
use forge_types::OAuthToken;
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*, px};

use crate::async_bridge;
use crate::integration_detail::IntegrationDetail;
use crate::oauth_connect::{LocalCallbackData, LocalCallbackFlowPhase};

pub type TwitchFlowHandle = Arc<tokio::sync::Mutex<Option<TwitchAuthFlow>>>;

struct TwitchAuthOutcome {
    token: OAuthToken,
    user_info: UserInfo,
    client_id: String,
}

const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) async fn request_code(flow: TwitchFlowHandle) -> Result<LocalCallbackData, String> {
    let mut guard = flow.lock().await;
    let inner = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    let code = inner.start().await.map_err(|e| e.to_string())?;
    Ok(LocalCallbackData {
        auth_url: code.auth_url,
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
    pub(crate) fn spawn_twitch_wait(&self, flow: TwitchFlowHandle, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        async_bridge::run_async(
            &self.rt_handle,
            wait_for_auth(flow, credentials),
            |this, result, cx| this.apply_twitch_auth(result, cx),
            cx,
        );
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
                self.flow_phase = LocalCallbackFlowPhase::Failed;
                self.flow_error = Some(e);
                cx.notify();
                return;
            }
        };
        tracing::info!(
            login = %outcome.user_info.login,
            id = %outcome.user_info.id,
            "twitch authorization complete",
        );
        self.flow_phase = LocalCallbackFlowPhase::Authorized;

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
                    .font_family(body_family())
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
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_reauth_title")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_reauth_detail")),
                    ),
            )
            .child(cta)
            .into_any_element()
    }
}
