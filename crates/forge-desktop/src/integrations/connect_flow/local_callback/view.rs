use super::*;
use crate::integrations::youtube_consent::{PolicyLink, youtube_consent_line};

const ICON_INLINE: gpui::Pixels = px(13.0);
const ICON_NOTICE: gpui::Pixels = px(16.0);
const SPINNER_SIZE: gpui::Pixels = px(11.0);
const PULSE_DOT: gpui::Pixels = px(8.0);
const BUTTON_RADIUS: gpui::Pixels = px(7.0);
const BODY_LINE_H: gpui::Pixels = px(20.0);
const PAGE_MAX_W: gpui::Pixels = px(640.0);
const STEP_BADGE: gpui::Pixels = px(24.0);
const STEP_BORDER: gpui::Pixels = px(1.5);
const CARD_STACK_MB: gpui::Pixels = px(14.0);
const INTRO_TEXT_MB: gpui::Pixels = px(22.0);
const CANCEL_BUTTON_PAD_V: gpui::Pixels = px(5.0);
const URL_BOX_PAD_V: gpui::Pixels = px(7.0);
const SIGN_IN_BUTTON_PAD_V: gpui::Pixels = px(9.0);
const NOTICE_PAD_V: gpui::Pixels = px(11.0);
const STEP_ROW_PAD_V: gpui::Pixels = px(11.0);
const URL_BOX_PAD_H: gpui::Pixels = px(11.0);
const CANCEL_BUTTON_PAD_H: gpui::Pixels = px(11.0);
const NOTICE_PAD_H: gpui::Pixels = px(14.0);
const SIGN_IN_BUTTON_PAD_H: gpui::Pixels = px(20.0);
const NOTICE_TEXT_GAP: gpui::Pixels = px(1.0);
const BUTTON_ICON_GAP: gpui::Pixels = px(5.0);
const HINT_ICON_GAP: gpui::Pixels = px(7.0);
const STEP_ROW_GAP: gpui::Pixels = px(13.0);

impl ConnectFlow {
    pub(in super::super) fn connect_status(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let accent = crate::integrations::platform_visuals::platform_color(self.platform, palette);
        let (indicator, label, color): (AnyElement, String, Rgba) = match self.phase() {
            LocalCallbackFlowPhase::Starting | LocalCallbackFlowPhase::Waiting => (
                spinner("oauth-status-spin", Icon::Loader2, SPINNER_SIZE, accent)
                    .into_any_element(),
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
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(label),
            )
            .into_any_element()
    }

    pub(in super::super) fn local_callback_column(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disclaimer = matches!(self.platform, PlatformId::Kick).then(|| {
            self.connect_disclaimer(palette, density, cx)
                .into_any_element()
        });
        let consent = matches!(self.platform, PlatformId::YouTube)
            .then(|| self.youtube_consent(palette, density, cx));
        let progress = matches!(
            self.phase(),
            LocalCallbackFlowPhase::Starting | LocalCallbackFlowPhase::Waiting
        )
        .then(|| self.oauth_progress_card(accent, palette, density, cx));
        let done = matches!(self.phase(), LocalCallbackFlowPhase::Authorized)
            .then(|| self.oauth_done_card(palette, density, cx));
        let error = matches!(self.phase(), LocalCallbackFlowPhase::Failed)
            .then(|| self.oauth_error_card(palette, density, cx));

        div()
            .w_full()
            .max_w(PAGE_MAX_W)
            .flex()
            .flex_col()
            .child(self.oauth_explainer(palette, density, cx))
            .children(disclaimer)
            .children(consent)
            .child(self.oauth_steps_card(accent, palette, density, cx))
            .children(progress)
            .children(done)
            .children(error)
            .child(self.oauth_footer(palette, cx))
            .into_any_element()
    }

    fn youtube_consent(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on_open = cx.listener(|this, link: &PolicyLink, _, cx| {
            this.open_url(link.url().to_owned(), cx);
        });
        div()
            .w_full()
            .mb(spacing(Spacing::Md, density))
            .child(youtube_consent_line(
                "oauth-youtube-consent",
                palette,
                cx,
                on_open,
            ))
            .into_any_element()
    }

    fn oauth_explainer(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
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
            .font_family(cx.body_family())
            .text_size(FONT_FIELD)
            .text_color(palette.text_muted)
            .line_height(BODY_LINE_H)
            .mb(scaled(INTRO_TEXT_MB, density))
            .child(styled)
            .into_any_element()
    }

    fn oauth_steps_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let phase = self.phase();
        let s1 = self.oauth_open_step(phase, accent, palette, density, cx);
        let s2 = self.oauth_approve_step(phase, accent, palette, density, cx);

        let s3 = self.oauth_step_row(
            "3",
            step_state(phase, 2),
            tr!("oauth_step_exchange_title"),
            Some(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_step_exchange_caption"))
                    .into_any_element(),
            ),
            false,
            accent,
            palette,
            density,
            cx,
        );

        let s4 = self.oauth_step_row(
            "4",
            step_state(phase, 3),
            tr!("oauth_step_connected_title"),
            None,
            true,
            accent,
            palette,
            density,
            cx,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Md, density))
            .mb(scaled(CARD_STACK_MB, density))
            .child(s1)
            .child(s2)
            .child(s3)
            .child(s4)
            .into_any_element()
    }

    fn oauth_open_step(
        &self,
        phase: LocalCallbackFlowPhase,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let platform_name = self.display_name.clone();

        let url_text: SharedString = SharedString::from(
            self.auth_url()
                .map(elide_code_challenge)
                .unwrap_or_else(|| idle_auth_url_template(self.platform)),
        );
        let url_box = div()
            .w_full()
            .py(scaled(URL_BOX_PAD_V, density))
            .px(scaled(URL_BOX_PAD_H, density))
            .rounded(BUTTON_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(cx.mono_family())
            .text_size(FONT_META)
            .text_color(palette.info)
            .child(url_text);
        self.oauth_step_row(
            "1",
            step_state(phase, 0),
            tr!("oauth_step_open_title", name = platform_name.as_str()),
            Some(url_box.into_any_element()),
            false,
            accent,
            palette,
            density,
            cx,
        )
    }

    fn oauth_approve_step(
        &self,
        phase: LocalCallbackFlowPhase,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let s2_active = matches!(step_state(phase, 1), StepState::Active);
        let loopback: SharedString = self.auth_url().and_then(loopback_display).map_or_else(
            || SharedString::from("http://127.0.0.1:\u{2026}/callback"),
            SharedString::from,
        );
        let approve = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_step_approve_caption")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(HINT_ICON_GAP, density))
                    .mt(spacing(Spacing::Xs, density))
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
                            .font_family(cx.mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(loopback),
                    ),
            );
        self.oauth_step_row(
            "2",
            step_state(phase, 1),
            tr!("oauth_step_approve_title"),
            Some(approve.into_any_element()),
            false,
            accent,
            palette,
            density,
            cx,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    fn oauth_step_row(
        &self,
        n: &'static str,
        state: StepState,
        title: String,
        children: Option<AnyElement>,
        is_last: bool,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let circle_bg = match state {
            StepState::Active => accent,
            _ => palette.surface_overlay,
        };
        let inner: AnyElement = match state {
            StepState::Done => icon(Icon::Check, ICON_INLINE, palette.success).into_any_element(),
            StepState::Active => spinner(
                SharedString::from(format!("oauth-step-spin-{n}")),
                Icon::Loader2,
                px(13.0),
                palette.shell,
            )
            .into_any_element(),
            StepState::Pending => div()
                .font_family(cx.body_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(FONT_META)
                .text_color(palette.text_faint)
                .child(n)
                .into_any_element(),
        };
        let mut circle = div()
            .flex_none()
            .size(STEP_BADGE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Lg))
            .bg(circle_bg);
        if matches!(state, StepState::Done) {
            circle = circle.border(STEP_BORDER).border_color(palette.success);
        }
        let circle = circle.child(inner);

        let title_color = if matches!(state, StepState::Pending) {
            palette.text_faint
        } else {
            palette.text_primary
        };
        let mut content = div().flex_1().min_w(px(0.0)).flex().flex_col().child(
            div()
                .font_family(cx.body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_FIELD)
                .text_color(title_color)
                .child(title),
        );
        if let Some(children) = children {
            content = content.gap(spacing(Spacing::Xs, density)).child(children);
        }

        let mut row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(scaled(STEP_ROW_GAP, density))
            .py(scaled(STEP_ROW_PAD_V, density))
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

    fn oauth_progress_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let name = self.display_name.clone();
        let line: String = match self.phase() {
            LocalCallbackFlowPhase::Starting => tr!("oauth_progress_launching"),
            _ => tr!("oauth_progress_waiting", name = name.as_str()),
        };
        let port = self.auth_url().and_then(loopback_port).unwrap_or_default();
        let scopes = self.auth_url().and_then(scopes_display).unwrap_or_default();
        let subline = tr!(
            "oauth_progress_subline",
            port = port.as_str(),
            scopes = scopes.as_str()
        );

        let pulse = pulse_dot("oauth-progress-pulse", accent, PULSE_DOT);

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(NOTICE_PAD_V, density))
            .px(scaled(NOTICE_PAD_H, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .mb(scaled(CARD_STACK_MB, density))
            .child(pulse)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(scaled(NOTICE_TEXT_GAP, density))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(line),
                    )
                    .child(
                        div()
                            .font_family(cx.mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(subline),
                    ),
            )
            .into_any_element()
    }

    fn oauth_done_card(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(NOTICE_PAD_V, density))
            .px(scaled(NOTICE_PAD_H, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.elevated)
            .mb(scaled(CARD_STACK_MB, density))
            .child(icon(Icon::CircleCheckFilled, ICON_NOTICE, palette.success))
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_ROW)
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
            .local()
            .and_then(|local| local.error.clone())
            .unwrap_or_else(|| tr!("oauth_failed_title"));
        let retry = div()
            .id("oauth-error-retry")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.brand, CTA_HOVER_ALPHA)))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_connect(cx)))
            .child(
                div()
                    .font_family(cx.body_family())
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
            .hover(|s| s.bg(with_alpha(palette.border_regular, ROW_HOVER_WASH_ALPHA)))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reset_flow(cx)))
            .child(
                div()
                    .font_family(cx.body_family())
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
            .border_color(palette.danger)
            .bg(palette.elevated)
            .mb(spacing(Spacing::Md, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, ICON_NOTICE, palette.danger))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(tr!("oauth_failed_title")),
                    ),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
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
        let density = cx.density();
        let back = div()
            .id("oauth-choose-different")
            .flex()
            .items_center()
            .gap(scaled(BUTTON_ICON_GAP, density))
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.leave(cx)))
            .child(icon(Icon::ArrowLeft, ICON_INLINE, palette.text_muted))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_ROW)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_footer_choose_different")),
            );

        let right: Option<AnyElement> = match self.phase() {
            LocalCallbackFlowPhase::Idle => {
                let name = self.display_name.clone();
                Some(
                    div()
                        .id("oauth-signin")
                        .flex()
                        .items_center()
                        .gap(scaled(BUTTON_ICON_GAP, density))
                        .py(scaled(SIGN_IN_BUTTON_PAD_V, density))
                        .px(scaled(SIGN_IN_BUTTON_PAD_H, density))
                        .rounded(radius(Radius::Sm))
                        .bg(palette.brand)
                        .cursor_pointer()
                        .hover(|s| s.bg(with_alpha(palette.brand, CTA_HOVER_ALPHA)))
                        .key_activated(palette.text_primary)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_connect(cx)))
                        .child(icon(Icon::ExternalLink, ICON_INLINE, palette.shell))
                        .child(
                            div()
                                .font_family(cx.body_family())
                                .font_weight(FontWeight::MEDIUM)
                                .text_size(FONT_XS)
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
                    .py(scaled(CANCEL_BUTTON_PAD_V, density))
                    .px(scaled(CANCEL_BUTTON_PAD_H, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .cursor_pointer()
                    .hover(|s| s.border_color(palette.border_input))
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reset_flow(cx)))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
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
            .pt(spacing(Spacing::Xxs, density))
            .child(back)
            .children(right)
            .into_any_element()
    }

    fn connect_disclaimer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> impl IntoElement {
        let text_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("oauth_kick_hybrid_title")),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(
                        resolve_message(&forge_platform_kick::KICK_COMMUNITY_NOTE.into()).primary,
                    ),
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
            .child(icon(Icon::AlertTriangle, ICON_NOTICE, palette.warning))
            .child(text_col)
    }
}
