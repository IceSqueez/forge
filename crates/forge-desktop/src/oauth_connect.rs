use std::sync::Arc;
use std::time::Duration;

use crate::async_bridge;
use forge_components::{
    BORDER_THIN, Density, FONT_MD, FONT_SM, FONT_XS, ForgePalette, Icon, PlatformKind, Radius,
    Spacing, avatar_tile, body_family, icon, mono_family, platform_color, radius, spacing, tr,
    with_alpha,
};
use forge_events::EventPublisher;
use forge_platform_core::ChatPlatform;
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Rgba, div, prelude::*, px};

use crate::integration_detail::IntegrationDetail;

pub(crate) type YoutubeFlowHandle =
    Arc<tokio::sync::Mutex<Option<forge_platform_youtube::GoogleAuthFlow>>>;
pub(crate) type KickFlowHandle = Arc<tokio::sync::Mutex<Option<forge_platform_kick::KickAuthFlow>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalCallbackFlowPhase {
    Idle,
    Starting,
    Waiting,
    Authorized,
    Failed,
}

struct LocalCallbackData {
    auth_url: String,
}

impl IntegrationDetail {
    pub(crate) fn start_connect(&mut self, cx: &mut Context<Self>) {
        let Some(platform) = self.connect_platform else {
            return;
        };
        self.flow_phase = LocalCallbackFlowPhase::Starting;
        self.flow_error = None;
        match platform {
            PlatformId::YouTube => {
                let Some((cid, csec)) = forge_platform_youtube::client_credentials() else {
                    self.flow_phase = LocalCallbackFlowPhase::Failed;
                    self.flow_error = Some(tr!("auth_error_credentials_missing_youtube"));
                    cx.notify();
                    return;
                };
                let handle = Arc::new(tokio::sync::Mutex::new(Some(
                    forge_platform_youtube::GoogleAuthFlow::new(cid, csec),
                )));
                self.youtube_flow = Some(Arc::clone(&handle));
                self.spawn_start(async move { start_youtube_oauth(handle).await }, cx);
            }
            PlatformId::Kick => {
                let Some(cid) = forge_platform_kick::client_credentials() else {
                    self.flow_phase = LocalCallbackFlowPhase::Failed;
                    self.flow_error = Some(tr!("auth_error_credentials_missing_kick"));
                    cx.notify();
                    return;
                };
                let handle = Arc::new(tokio::sync::Mutex::new(Some(
                    forge_platform_kick::KickAuthFlow::new(cid),
                )));
                self.kick_flow = Some(Arc::clone(&handle));
                self.spawn_start(async move { start_kick_oauth(handle).await }, cx);
            }
            PlatformId::Twitch => {
                self.flow_phase = LocalCallbackFlowPhase::Failed;
                self.flow_error = Some("Twitch is not wired through this flow".to_owned());
            }
        }
        cx.notify();
    }

    fn spawn_start(
        &self,
        fut: impl std::future::Future<Output = Result<LocalCallbackData, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            |this, result, cx| this.apply_start_result(result, cx),
            cx,
        );
    }

    fn apply_start_result(
        &mut self,
        result: Result<LocalCallbackData, String>,
        cx: &mut Context<Self>,
    ) {
        let data = match result {
            Ok(data) => data,
            Err(e) => {
                self.flow_phase = LocalCallbackFlowPhase::Failed;
                self.flow_error = Some(e);
                cx.notify();
                return;
            }
        };
        let auth_url = data.auth_url.clone();
        self.flow_auth_url = Some(data.auth_url);
        self.flow_phase = LocalCallbackFlowPhase::Waiting;
        self.open_url(auth_url, cx);

        let credentials = Arc::clone(&self.credentials);
        match self.connect_platform {
            Some(PlatformId::YouTube) => {
                let Some(flow) = self.youtube_flow.clone() else {
                    self.flow_phase = LocalCallbackFlowPhase::Failed;
                    self.flow_error = Some("no active YouTube flow handle".to_owned());
                    cx.notify();
                    return;
                };
                self.spawn_wait(
                    async move { wait_for_youtube_authorization(flow, credentials).await },
                    cx,
                );
            }
            Some(PlatformId::Kick) => {
                let Some(flow) = self.kick_flow.clone() else {
                    self.flow_phase = LocalCallbackFlowPhase::Failed;
                    self.flow_error = Some("no active Kick flow handle".to_owned());
                    cx.notify();
                    return;
                };
                self.spawn_wait(
                    async move { wait_for_kick_authorization(flow, credentials).await },
                    cx,
                );
            }
            _ => {}
        }
        cx.notify();
    }

    fn spawn_wait(
        &self,
        fut: impl std::future::Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            fut,
            |this, result, cx| this.apply_wait_result(result, cx),
            cx,
        );
    }

    fn apply_wait_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.flow_phase = LocalCallbackFlowPhase::Authorized;
                if matches!(self.connect_platform, Some(PlatformId::YouTube)) {
                    let credentials = Arc::clone(&self.credentials);
                    let bus = Arc::clone(&self.bus);
                    self.rt_handle.spawn(async move {
                        if let Err(e) = connect_youtube_after_oauth(credentials, bus).await {
                            eprintln!("forge-desktop: youtube in-session connect failed: {e}");
                        }
                    });
                }
            }
            Err(e) => {
                self.flow_phase = LocalCallbackFlowPhase::Failed;
                self.flow_error = Some(e);
            }
        }
        cx.notify();
    }

    fn retry_flow(&mut self, cx: &mut Context<Self>) {
        self.flow_phase = LocalCallbackFlowPhase::Idle;
        self.flow_auth_url = None;
        self.flow_error = None;
        cx.notify();
    }

    fn cancel_flow(&mut self, cx: &mut Context<Self>) {
        self.flow_phase = LocalCallbackFlowPhase::Idle;
        self.flow_auth_url = None;
        self.flow_error = None;
        cx.notify();
    }

    fn open_current_url(&mut self, cx: &mut Context<Self>) {
        if let Some(url) = self.flow_auth_url.clone() {
            self.open_url(url, cx);
        }
    }

    pub(crate) fn connect_body(
        &self,
        platform: PlatformId,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = platform_accent(platform, palette);
        let (letter, desc, features) = connect_copy(platform);

        let tile = avatar_tile(letter, accent, palette)
            .size(px(48.0))
            .corner(px(11.0))
            .font(px(24.0));

        let status_badge = div()
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.info)
                    .child(tr!("common_status_not_connected")),
            );

        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(self.display_name.clone()),
            )
            .child(status_badge);

        let info = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(desc),
            );

        let hero = div()
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
            .child(info);

        let mut features_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("platform_generic_features_available")),
            );
        for feature in features {
            features_col = features_col.child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::CircleCheck, px(14.0), palette.text_faint))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_secondary)
                            .child(feature),
                    ),
            );
        }

        let connect_btn = div()
            .id("integration-connect")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_connect(cx)))
            .child(icon(Icon::Lock, px(14.0), palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child(tr!("platform_generic_connect_btn")),
            );
        let connect_row = div().w_full().flex().justify_center().child(connect_btn);

        let footer = div()
            .w_full()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!(
                        "{} \u{00b7} {}",
                        tr!("platform_generic_kind_platform"),
                        tr!("platform_generic_status_available"),
                    )),
            );

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(hero);
        if matches!(platform, PlatformId::Kick) {
            body = body.child(self.connect_disclaimer(palette, density));
        }
        body.child(features_col)
            .child(connect_row)
            .child(footer)
            .into_any_element()
    }

    fn connect_disclaimer(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let text_col = div()
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
                    .child(tr!("iseed_kick_banner_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(forge_platform_kick::capabilities::KICK_COMMUNITY_NOTE),
            );
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.elevated)
            .child(icon(Icon::AlertTriangle, px(16.0), palette.warning))
            .child(text_col)
    }

    pub(crate) fn flow_body(
        &self,
        platform: PlatformId,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let phase_card = match self.flow_phase {
            LocalCallbackFlowPhase::Starting => self.flow_starting_card(palette, density),
            LocalCallbackFlowPhase::Waiting => self.flow_polling_card(palette, density, cx),
            LocalCallbackFlowPhase::Authorized => self.flow_authorized_card(palette, density, cx),
            LocalCallbackFlowPhase::Failed => self.flow_failed_card(palette, density, cx),
            LocalCallbackFlowPhase::Idle => div().into_any_element(),
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.flow_header_card(platform, palette, density))
            .child(phase_card)
            .into_any_element()
    }

    fn flow_header_card(
        &self,
        platform: PlatformId,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement {
        let dot = div()
            .flex_none()
            .size(px(40.0))
            .rounded(px(10.0))
            .bg(platform_accent(platform, palette));
        let title_col = div()
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
                    .child(self.display_name.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_header_subtitle")),
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
            .child(dot)
            .child(title_col)
    }

    fn flow_intro(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
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
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("oauth_auth_title", name = self.display_name.as_str())),
                    ),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_auth_subtitle")),
            )
    }

    fn flow_starting_card(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(self.flow_intro(palette, density))
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .p(spacing(Spacing::Md, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_muted)
                            .child(tr!("oauth_requesting")),
                    ),
            )
            .into_any_element()
    }

    fn flow_polling_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = self.flow_auth_url.clone().unwrap_or_default();

        let url_box = div()
            .flex_1()
            .min_w(px(0.0))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(FONT_SM)
            .text_color(palette.info)
            .child(url);
        let open_btn = div()
            .id("integration-oauth-open")
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_current_url(cx)))
            .child(icon(Icon::ExternalLink, px(13.0), palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.brand)
                    .child(tr!("oauth_step1_open")),
            );
        let step1 = self.flow_step(
            "1",
            false,
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(tr!("oauth_step1_title")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(spacing(Spacing::Xs, density))
                        .child(url_box)
                        .child(open_btn),
                )
                .into_any_element(),
            palette,
            density,
        );
        let step2 = self.flow_step(
            "2",
            true,
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(tr!("oauth_step2_title")),
                )
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("oauth_step2_detail")),
                )
                .into_any_element(),
            palette,
            density,
        );

        let cancel = div()
            .id("integration-oauth-cancel")
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.border_regular, 0.06)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_flow(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(tr!("oauth_btn_cancel")),
            );
        let banner = div()
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
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("oauth_polling_primary")),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_faint)
                            .child(tr!("oauth_polling_secondary")),
                    ),
            )
            .child(cancel);

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(self.flow_intro(palette, density))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Sm, density))
                    .p(spacing(Spacing::Md, density))
                    .child(step1)
                    .child(step2)
                    .child(banner),
            )
            .into_any_element()
    }

    fn flow_step(
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
                    .font_family(body_family())
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

    fn flow_authorized_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let return_btn = div()
            .id("integration-oauth-return")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_flow(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child(tr!("oauth_btn_return")),
            );
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Lg, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(icon(Icon::CircleCheck, px(28.0), palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!(
                        "oauth_authorized_title",
                        name = self.display_name.as_str()
                    )),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_authorized_subtitle")),
            )
            .child(return_btn)
            .into_any_element()
    }

    fn flow_failed_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let error = self
            .flow_error
            .clone()
            .unwrap_or_else(|| tr!("oauth_failed_title"));
        let retry = div()
            .id("integration-oauth-retry")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.retry_flow(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child(tr!("oauth_btn_retry")),
            );
        let cancel = div()
            .id("integration-oauth-failed-cancel")
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.border_regular, 0.06)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_flow(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(tr!("oauth_btn_cancel")),
            );
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, px(20.0), palette.random))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("oauth_failed_title")),
                    ),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(error),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(retry)
                    .child(cancel),
            )
            .into_any_element()
    }
}

fn platform_accent(platform: PlatformId, palette: &ForgePalette) -> Rgba {
    match platform {
        PlatformId::Twitch => platform_color(PlatformKind::Twitch, palette),
        PlatformId::YouTube => platform_color(PlatformKind::YouTube, palette),
        PlatformId::Kick => platform_color(PlatformKind::Kick, palette),
    }
}

fn connect_copy(platform: PlatformId) -> (&'static str, String, Vec<String>) {
    match platform {
        PlatformId::Kick => (
            "K",
            tr!("kick_description"),
            vec![
                tr!("kick_feature_live_chat"),
                tr!("kick_feature_subs"),
                tr!("kick_feature_hosts_bans"),
                tr!("kick_feature_deleted_replies"),
            ],
        ),
        _ => (
            "Y",
            tr!("youtube_description"),
            vec![
                tr!("youtube_feature_live_chat"),
                tr!("youtube_feature_super_chat"),
                tr!("youtube_feature_memberships"),
                tr!("youtube_feature_subscribers"),
            ],
        ),
    }
}

async fn start_youtube_oauth(flow_handle: YoutubeFlowHandle) -> Result<LocalCallbackData, String> {
    let mut guard = flow_handle.lock().await;
    let flow = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    let code = flow.start().await.map_err(|e| e.to_string())?;
    Ok(LocalCallbackData {
        auth_url: code.auth_url,
    })
}

async fn start_kick_oauth(flow_handle: KickFlowHandle) -> Result<LocalCallbackData, String> {
    let mut guard = flow_handle.lock().await;
    let flow = guard
        .as_mut()
        .ok_or_else(|| "OAuth flow already consumed".to_owned())?;
    let code = flow.start().await.map_err(|e| e.to_string())?;
    Ok(LocalCallbackData {
        auth_url: code.auth_url,
    })
}

async fn wait_for_youtube_authorization(
    flow_handle: YoutubeFlowHandle,
    credentials_repo: Arc<dyn CredentialsRepo>,
) -> Result<(), String> {
    let mut flow = {
        let mut guard = flow_handle.lock().await;
        guard
            .take()
            .ok_or_else(|| "OAuth flow already consumed".to_owned())?
    };
    let bundle = flow
        .wait_for_authorization(Duration::from_secs(300))
        .await
        .map_err(|e| e.to_string())?;
    let manager = forge_platform_youtube::YoutubeCredentialsManager::new(credentials_repo, flow);
    manager
        .save_from_bundle(bundle)
        .await
        .map_err(|e| e.to_string())
}

async fn wait_for_kick_authorization(
    flow_handle: KickFlowHandle,
    credentials_repo: Arc<dyn CredentialsRepo>,
) -> Result<(), String> {
    let mut flow = {
        let mut guard = flow_handle.lock().await;
        guard
            .take()
            .ok_or_else(|| "OAuth flow already consumed".to_owned())?
    };
    let bundle = flow
        .wait_for_authorization(Duration::from_secs(300))
        .await
        .map_err(|e| e.to_string())?;
    let Some(cid) = forge_platform_kick::client_credentials() else {
        return Err("Kick OAuth client credentials are not configured".to_owned());
    };
    let manager = forge_platform_kick::KickCredentialsManager::new(credentials_repo, cid);
    manager
        .save_from_bundle(bundle)
        .await
        .map_err(|e| e.to_string())
}

async fn connect_youtube_after_oauth(
    credentials_repo: Arc<dyn CredentialsRepo>,
    bus: Arc<dyn EventPublisher>,
) -> Result<(), String> {
    let (client_id, client_secret) = forge_platform_youtube::client_credentials()
        .ok_or_else(|| "YouTube OAuth client credentials are not configured".to_owned())?;
    let google = forge_platform_youtube::GoogleAuthFlow::new(client_id, client_secret);
    let manager = Arc::new(forge_platform_youtube::YoutubeCredentialsManager::new(
        credentials_repo,
        google,
    ));
    let creds = manager
        .load()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no YouTube credentials found right after authorization".to_owned())?;
    let channel_id = creds.channel_id;

    let quota = Arc::new(tokio::sync::Mutex::new(
        forge_platform_youtube::QuotaState::default(),
    ));
    let platform = Arc::new(forge_platform_youtube::YoutubePlatform::new(
        channel_id.clone(),
        Arc::clone(&manager),
        forge_platform_youtube::LiveChatIdHandle::new(),
        forge_platform_youtube::ActiveBroadcastIdHandle::new(),
        Arc::clone(&quota),
    ));

    let mut platform_events = platform.events();
    tokio::spawn(async move {
        loop {
            match platform_events.recv().await {
                Ok(event) => bus.publish(event),
                Err(forge_events::EventsError::BusClosed) => break,
                Err(forge_events::EventsError::LaggingReceiver) => {
                    tracing::warn!("youtube platform event bridge: lagging receiver");
                    continue;
                }
                Err(_) => continue,
            }
        }
    });

    platform.connect().await.map_err(|e| e.to_string())?;
    Ok(())
}
