use std::sync::Arc;
use std::time::Duration;

use crate::async_bridge;
use crate::screen::Screen;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, PlatformKind, Radius, Spacing,
    body_family, icon, mono_family, platform_color, platform_hero, radius, spacing, spinner, tr,
    with_alpha,
};
use forge_events::EventPublisher;
use forge_platform_core::ChatPlatform;
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;
use gpui::{
    Animation, AnimationExt, AnyElement, ClickEvent, Context, FontWeight, HighlightStyle, Hsla,
    Rgba, SharedString, StyledText, div, prelude::*, px,
};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepState {
    Pending,
    Active,
    Done,
}

pub(crate) struct LocalCallbackData {
    pub(crate) auth_url: String,
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
                    self.flow_error =
                        Some(tr!("auth_error_credentials_missing_youtube").to_string());
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
                let Some((cid, csec)) = forge_platform_kick::client_credentials() else {
                    self.flow_phase = LocalCallbackFlowPhase::Failed;
                    self.flow_error = Some(tr!("auth_error_credentials_missing_kick").to_string());
                    cx.notify();
                    return;
                };
                let handle = Arc::new(tokio::sync::Mutex::new(Some(
                    forge_platform_kick::KickAuthFlow::new(cid, csec),
                )));
                self.kick_flow = Some(Arc::clone(&handle));
                self.spawn_start(async move { start_kick_oauth(handle).await }, cx);
            }
            PlatformId::Twitch => {
                self.begin_twitch_device(cx);
                return;
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
            Some(PlatformId::Twitch) => {}
            None => {}
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
                match self.connect_platform {
                    Some(PlatformId::YouTube) => {
                        let credentials = Arc::clone(&self.credentials);
                        let bus = Arc::clone(&self.bus);
                        self.rt_handle.spawn(async move {
                            if let Err(e) = connect_youtube_after_oauth(credentials, bus).await {
                                eprintln!("forge-desktop: youtube in-session connect failed: {e}");
                            }
                        });
                    }
                    Some(PlatformId::Kick) => {
                        let credentials = Arc::clone(&self.credentials);
                        let bus = Arc::clone(&self.bus);
                        self.rt_handle.spawn(async move {
                            if let Err(e) = connect_kick_after_oauth(credentials, bus).await {
                                eprintln!("forge-desktop: kick in-session connect failed: {e}");
                            }
                        });
                    }
                    Some(PlatformId::Twitch) | None => {}
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

    pub(crate) fn connect_status(
        &self,
        platform: PlatformId,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let accent = platform_accent(platform, palette);
        let (indicator, label, color): (AnyElement, String, Rgba) = match self.flow_phase {
            LocalCallbackFlowPhase::Starting | LocalCallbackFlowPhase::Waiting => (
                spinner("oauth-status-spin", Icon::Loader2, px(11.0), accent).into_any_element(),
                tr!("oauth_status_authorizing"),
                accent,
            ),
            LocalCallbackFlowPhase::Authorized => (
                status_dot(palette.success).into_any_element(),
                tr!("oauth_status_authorized"),
                palette.success,
            ),
            _ => (
                status_dot(palette.text_faint).into_any_element(),
                tr!("common_status_not_connected"),
                palette.text_faint,
            ),
        };
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(indicator)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(label),
            )
            .into_any_element()
    }

    pub(crate) fn oauth_screen(
        &self,
        platform: PlatformId,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = platform_accent(platform, palette);
        let (letter, desc) = connect_copy(platform);

        if matches!(platform, PlatformId::Twitch) {
            let hero = platform_hero(letter, accent, self.display_name.clone(), desc, palette)
                .density(density);
            let column = self.twitch_device_column(accent, palette, density, cx);
            return div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Md, density))
                .child(hero)
                .child(div().w_full().flex().justify_center().child(column))
                .into_any_element();
        }
        let _ = desc;

        let eyebrow = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .mb(px(6.0))
            .child(tr!("oauth_connect_eyebrow"));

        let disclaimer = matches!(platform, PlatformId::Kick)
            .then(|| self.connect_disclaimer(palette, density).into_any_element());
        let progress = matches!(
            self.flow_phase,
            LocalCallbackFlowPhase::Starting | LocalCallbackFlowPhase::Waiting
        )
        .then(|| self.oauth_progress_card(accent, palette));
        let done = matches!(self.flow_phase, LocalCallbackFlowPhase::Authorized)
            .then(|| self.oauth_done_card(palette));
        let error = matches!(self.flow_phase, LocalCallbackFlowPhase::Failed)
            .then(|| self.oauth_error_card(palette, density, cx));

        let column = div()
            .w_full()
            .max_w(px(640.0))
            .flex()
            .flex_col()
            .child(eyebrow)
            .child(self.oauth_title(letter, accent, palette))
            .child(self.oauth_explainer(palette))
            .children(disclaimer)
            .child(self.oauth_steps_card(accent, palette))
            .children(progress)
            .children(done)
            .children(error)
            .child(self.oauth_footer(palette, cx));

        div()
            .w_full()
            .flex()
            .justify_center()
            .pt(px(14.0))
            .child(column)
            .into_any_element()
    }

    fn oauth_title(
        &self,
        letter: &'static str,
        accent: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        let tile = div()
            .flex_none()
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .bg(accent)
            .font_family(body_family())
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(16.0))
            .text_color(palette.shell)
            .child(letter);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.0))
            .mb(px(4.0))
            .child(tile)
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(22.0))
                    .text_color(palette.text_primary)
                    .child(tr!(
                        "oauth_footer_signin",
                        name = self.display_name.as_str()
                    )),
            )
            .into_any_element()
    }

    fn oauth_explainer(&self, palette: &ForgePalette) -> AnyElement {
        let prefix = tr!("oauth_connect_explainer_prefix");
        let emphasis = tr!("oauth_connect_explainer_emphasis");
        let suffix = tr!(
            "oauth_connect_explainer_suffix",
            name = self.display_name.as_str()
        );
        let start = prefix.len();
        let end = start + emphasis.len();
        let full = format!("{prefix}{emphasis}{suffix}");
        let styled = StyledText::new(SharedString::from(full)).with_highlights(vec![(
            start..end,
            HighlightStyle::from(Hsla::from(palette.text_primary)),
        )]);
        div()
            .w_full()
            .font_family(body_family())
            .text_size(px(13.0))
            .text_color(palette.text_muted)
            .line_height(px(20.0))
            .mb(px(22.0))
            .child(styled)
            .into_any_element()
    }

    fn oauth_steps_card(&self, accent: Rgba, palette: &ForgePalette) -> AnyElement {
        let phase = self.flow_phase;
        let platform_name = self.display_name.clone();

        let url_text: SharedString = self
            .flow_auth_url
            .as_deref()
            .map(elide_code_challenge)
            .or_else(|| self.connect_platform.map(idle_auth_url_template))
            .map_or_else(SharedString::default, SharedString::from);
        let url_box = div()
            .w_full()
            .py(px(7.0))
            .px(px(11.0))
            .rounded(px(7.0))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(px(11.0))
            .text_color(palette.info)
            .child(url_text);
        let s1 = self.oauth_step_row(
            "1",
            step_state(phase, 0),
            tr!("oauth_step_open_title", name = platform_name.as_str()),
            Some(url_box.into_any_element()),
            false,
            accent,
            palette,
        );

        let s2_active = matches!(step_state(phase, 1), StepState::Active);
        let loopback: SharedString = self
            .flow_auth_url
            .as_deref()
            .and_then(loopback_display)
            .map_or_else(
                || SharedString::from("http://127.0.0.1:\u{2026}/callback"),
                SharedString::from,
            );
        let approve = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(px(11.5))
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_step_approve_caption")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .mt(px(6.0))
                    .child(icon(
                        Icon::Server2,
                        px(12.0),
                        if s2_active {
                            accent
                        } else {
                            palette.text_faint
                        },
                    ))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(px(10.5))
                            .text_color(palette.text_faint)
                            .child(loopback),
                    ),
            );
        let s2 = self.oauth_step_row(
            "2",
            step_state(phase, 1),
            tr!("oauth_step_approve_title"),
            Some(approve.into_any_element()),
            false,
            accent,
            palette,
        );

        let s3 = self.oauth_step_row(
            "3",
            step_state(phase, 2),
            tr!("oauth_step_exchange_title"),
            Some(
                div()
                    .font_family(body_family())
                    .text_size(px(11.5))
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_step_exchange_caption"))
                    .into_any_element(),
            ),
            false,
            accent,
            palette,
        );

        let s4 = self.oauth_step_row(
            "4",
            step_state(phase, 3),
            tr!("oauth_step_connected_title"),
            None,
            true,
            accent,
            palette,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .py(px(4.0))
            .px(px(16.0))
            .mb(px(14.0))
            .child(s1)
            .child(s2)
            .child(s3)
            .child(s4)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn oauth_step_row(
        &self,
        n: &'static str,
        state: StepState,
        title: String,
        children: Option<AnyElement>,
        is_last: bool,
        accent: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        let circle_bg = match state {
            StepState::Active => accent,
            _ => palette.surface_overlay,
        };
        let inner: AnyElement = match state {
            StepState::Done => icon(Icon::Check, px(13.0), palette.success).into_any_element(),
            StepState::Active => spinner(
                SharedString::from(format!("oauth-step-spin-{n}")),
                Icon::Loader2,
                px(13.0),
                palette.shell,
            )
            .into_any_element(),
            StepState::Pending => div()
                .font_family(body_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(11.0))
                .text_color(palette.text_faint)
                .child(n)
                .into_any_element(),
        };
        let mut circle = div()
            .flex_none()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.0))
            .bg(circle_bg);
        if matches!(state, StepState::Done) {
            circle = circle.border(px(1.5)).border_color(palette.success);
        }
        let circle = circle.child(inner);

        let title_color = if matches!(state, StepState::Pending) {
            palette.text_faint
        } else {
            palette.text_primary
        };
        let mut content = div().flex_1().min_w(px(0.0)).flex().flex_col().child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(13.0))
                .text_color(title_color)
                .child(title),
        );
        if let Some(children) = children {
            content = content.gap(px(6.0)).child(children);
        }

        let mut row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(px(13.0))
            .py(px(11.0))
            .child(circle)
            .child(content);
        if !is_last {
            row = row
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        if matches!(state, StepState::Pending) {
            row = row.opacity(0.6);
        }
        row.into_any_element()
    }

    fn oauth_progress_card(&self, accent: Rgba, palette: &ForgePalette) -> AnyElement {
        let name = self.display_name.clone();
        let line: String = match self.flow_phase {
            LocalCallbackFlowPhase::Starting => tr!("oauth_progress_launching"),
            _ => tr!("oauth_progress_waiting", name = name.as_str()),
        };
        let port = self
            .flow_auth_url
            .as_deref()
            .and_then(loopback_port)
            .unwrap_or_default();
        let scopes = self
            .flow_auth_url
            .as_deref()
            .and_then(scopes_display)
            .unwrap_or_default();
        let subline = tr!(
            "oauth_progress_subline",
            port = port.as_str(),
            scopes = scopes.as_str()
        );

        let pulse = div()
            .flex_none()
            .size(px(8.0))
            .rounded(px(4.0))
            .bg(accent)
            .with_animation(
                SharedString::from("oauth-progress-pulse"),
                Animation::new(Duration::from_millis(1400)).repeat(),
                |el, delta| el.opacity(1.0 - (delta * 2.0 - 1.0).abs() * 0.6),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .py(px(11.0))
            .px(px(14.0))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .mb(px(14.0))
            .child(pulse)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(px(12.0))
                            .text_color(palette.text_primary)
                            .child(line),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(px(10.5))
                            .text_color(palette.text_faint)
                            .child(subline),
                    ),
            )
            .into_any_element()
    }

    fn oauth_done_card(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .py(px(11.0))
            .px(px(14.0))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.elevated)
            .mb(px(14.0))
            .child(icon(Icon::CircleCheckFilled, px(16.0), palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(12.5))
                    .text_color(palette.text_primary)
                    .child(tr!("oauth_done_authorized")),
            )
            .into_any_element()
    }

    fn oauth_error_card(
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
            .id("oauth-error-retry")
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
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("oauth_btn_retry")),
            );
        let cancel = div()
            .id("oauth-error-cancel")
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
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.random)
            .bg(palette.elevated)
            .mb(spacing(Spacing::Md, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, px(16.0), palette.random))
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
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

    fn oauth_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let back = div()
            .id("oauth-choose-different")
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| this.navigate_to(Screen::Platforms, cx)),
            )
            .child(icon(Icon::ArrowLeft, px(13.0), palette.text_muted))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(px(12.5))
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_footer_choose_different")),
            );

        let right: Option<AnyElement> = match self.flow_phase {
            LocalCallbackFlowPhase::Idle => {
                let name = self.display_name.clone();
                Some(
                    div()
                        .id("oauth-signin")
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .py(px(9.0))
                        .px(px(20.0))
                        .rounded(radius(Radius::Sm))
                        .bg(palette.brand)
                        .cursor_pointer()
                        .hover(|s| s.bg(with_alpha(palette.brand, 0.85)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_connect(cx)))
                        .child(icon(Icon::ExternalLink, px(13.0), palette.shell))
                        .child(
                            div()
                                .font_family(body_family())
                                .font_weight(FontWeight::MEDIUM)
                                .text_size(px(12.0))
                                .text_color(palette.shell)
                                .child(tr!("oauth_footer_signin", name = name.as_str())),
                        )
                        .into_any_element(),
                )
            }
            LocalCallbackFlowPhase::Starting | LocalCallbackFlowPhase::Waiting => Some(
                div()
                    .id("oauth-footer-cancel")
                    .flex_none()
                    .py(px(5.0))
                    .px(px(11.0))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .cursor_pointer()
                    .hover(|s| s.border_color(palette.border_input))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_flow(cx)))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(px(12.0))
                            .text_color(palette.text_secondary)
                            .child(tr!("oauth_btn_cancel")),
                    )
                    .into_any_element(),
            ),
            _ => None,
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pt(px(4.0))
            .child(back)
            .children(right)
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
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("iseed_kick_banner_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
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
            .mb(spacing(Spacing::Md, density))
            .child(icon(Icon::AlertTriangle, px(16.0), palette.warning))
            .child(text_col)
    }
}

fn status_dot(color: Rgba) -> impl IntoElement {
    div().flex_none().size(px(8.0)).rounded(px(4.0)).bg(color)
}

fn step_state(phase: LocalCallbackFlowPhase, index: usize) -> StepState {
    match phase {
        LocalCallbackFlowPhase::Idle | LocalCallbackFlowPhase::Failed => StepState::Pending,
        LocalCallbackFlowPhase::Starting => {
            if index == 0 {
                StepState::Active
            } else {
                StepState::Pending
            }
        }
        LocalCallbackFlowPhase::Waiting => match index {
            0 => StepState::Done,
            1 => StepState::Active,
            _ => StepState::Pending,
        },
        LocalCallbackFlowPhase::Authorized => StepState::Done,
    }
}

fn platform_accent(platform: PlatformId, palette: &ForgePalette) -> Rgba {
    match platform {
        PlatformId::Twitch => platform_color(PlatformKind::Twitch, palette),
        PlatformId::YouTube => platform_color(PlatformKind::YouTube, palette),
        PlatformId::Kick => platform_color(PlatformKind::Kick, palette),
    }
}

pub(crate) fn twitch_accent(palette: &ForgePalette) -> Rgba {
    platform_color(PlatformKind::Twitch, palette)
}

fn connect_copy(platform: PlatformId) -> (&'static str, String) {
    match platform {
        PlatformId::Twitch => ("T", tr!("twitch_description")),
        PlatformId::Kick => ("K", tr!("kick_description")),
        PlatformId::YouTube => ("Y", tr!("youtube_description")),
    }
}

fn authorize_endpoint(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::YouTube => forge_platform_youtube::GOOGLE_AUTHORIZE_ENDPOINT,
        PlatformId::Kick => forge_platform_kick::auth::KICK_AUTHORIZE_ENDPOINT,
        PlatformId::Twitch => "",
    }
}

fn idle_auth_url_template(platform: PlatformId) -> String {
    format!(
        "{}?response_type=code&code_challenge=\u{2026}&redirect_uri=http%3A%2F%2F127.0.0.1%3A\u{2026}%2Fcallback",
        authorize_endpoint(platform)
    )
}

fn param_value<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=");
    let start = url.find(&needle)? + needle.len();
    let rest = &url[start..];
    let end = rest.find('&').unwrap_or(rest.len());
    Some(&rest[..end])
}

fn elide_code_challenge(url: &str) -> String {
    let needle = "code_challenge=";
    let Some(i) = url.find(needle) else {
        return url.to_owned();
    };
    let start = i + needle.len();
    let end = url[start..].find('&').map_or(url.len(), |j| start + j);
    format!("{}\u{2026}{}", &url[..start], &url[end..])
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn loopback_display(url: &str) -> Option<String> {
    param_value(url, "redirect_uri").map(percent_decode)
}

fn loopback_port(url: &str) -> Option<String> {
    let display = loopback_display(url)?;
    let after = display.rsplit_once(':')?.1;
    Some(after.split('/').next()?.to_owned())
}

fn scopes_display(url: &str) -> Option<String> {
    let raw = param_value(url, "scope")?;
    let joined = raw
        .split('+')
        .take(3)
        .map(percent_decode)
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
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
    let Some((cid, csec)) = forge_platform_kick::client_credentials() else {
        return Err("Kick OAuth client credentials are not configured".to_owned());
    };
    let manager = forge_platform_kick::KickCredentialsManager::new(credentials_repo, cid, csec);
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

async fn connect_kick_after_oauth(
    credentials_repo: Arc<dyn CredentialsRepo>,
    bus: Arc<dyn EventPublisher>,
) -> Result<(), String> {
    let (client_id, client_secret) = forge_platform_kick::client_credentials()
        .ok_or_else(|| "Kick OAuth client credentials are not configured".to_owned())?;
    let manager = Arc::new(forge_platform_kick::KickCredentialsManager::new(
        credentials_repo,
        client_id,
        client_secret,
    ));
    manager
        .load()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no Kick credentials found right after authorization".to_owned())?;

    let rate_limiter: Arc<dyn forge_platform_core::RateLimiter> = Arc::new(
        forge_platform_core::TokenBucketRateLimiter::new(60, Duration::from_secs(60)),
    );
    let platform = Arc::new(forge_platform_kick::KickPlatform::new(
        manager,
        rate_limiter,
    ));

    let mut platform_events = platform.events();
    let forward_bus = Arc::clone(&bus);
    tokio::spawn(async move {
        loop {
            match platform_events.recv().await {
                Ok(event) => forward_bus.publish(event),
                Err(forge_events::EventsError::BusClosed) => break,
                Err(forge_events::EventsError::LaggingReceiver) => {
                    tracing::warn!("kick platform event bridge: lagging receiver");
                    continue;
                }
                Err(_) => continue,
            }
        }
    });

    platform.connect().await.map_err(|e| e.to_string())?;
    Ok(())
}
