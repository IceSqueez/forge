use super::*;

const ICON_SMALL: Pixels = px(12.0);
const ICON_INLINE: Pixels = px(13.0);
const ICON_NOTICE: Pixels = px(16.0);
const SPINNER_SIZE: Pixels = px(11.0);
const PULSE_DOT: Pixels = px(8.0);
const BUTTON_RADIUS: Pixels = px(7.0);
const BODY_LINE_H: Pixels = px(20.0);
const PAGE_MAX_W: Pixels = px(640.0);
const STEP_BADGE: Pixels = px(24.0);
const OPEN_TITLE_MB: Pixels = px(8.0);
const CARD_STACK_MB: Pixels = px(14.0);
const INTRO_TEXT_MB: Pixels = px(22.0);
const OPEN_BUTTON_PAD_V: Pixels = px(5.0);
const URL_BOX_PAD_V: Pixels = px(8.0);
const NOTICE_PAD_V: Pixels = px(11.0);
const CODE_BOX_PAD_V: Pixels = px(14.0);
const OPEN_BUTTON_PAD_H: Pixels = px(11.0);
const URL_BOX_PAD_H: Pixels = px(12.0);
const NOTICE_PAD_H: Pixels = px(14.0);
const CODE_BOX_PAD_H: Pixels = px(20.0);
const NOTICE_TEXT_GAP: Pixels = px(1.0);
const COPY_BUTTON_GAP: Pixels = px(3.0);
const ICON_LABEL_GAP: Pixels = px(5.0);
const URL_ROW_GAP: Pixels = px(8.0);
const CODE_GLYPH_GAP: Pixels = px(8.0);
const META_ROW_GAP: Pixels = px(14.0);
const STEP_CARD_GAP: Pixels = px(14.0);
const COPY_BUTTON_PAD: Pixels = px(14.0);
const USER_CODE_FONT: Pixels = px(28.0);

impl ConnectFlow {
    pub(in super::super) fn twitch_device_status(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let accent =
            crate::integrations::platform_visuals::platform_color(PlatformId::Twitch, palette);
        let phase = self
            .device()
            .map_or(TwitchDevicePhase::Starting, |d| d.phase);
        let (indicator, label, color): (AnyElement, String, Rgba) = match phase {
            TwitchDevicePhase::Starting | TwitchDevicePhase::Waiting => (
                spinner(
                    "twitch-device-status-spin",
                    Icon::Loader2,
                    SPINNER_SIZE,
                    accent,
                )
                .into_any_element(),
                tr!("oauth_status_authorizing"),
                accent,
            ),
            TwitchDevicePhase::Authorized => (
                status_dot(palette.success).into_any_element(),
                tr!("oauth_status_authorized"),
                palette.success,
            ),
            TwitchDevicePhase::Denied | TwitchDevicePhase::Failed => (
                status_dot(palette.danger).into_any_element(),
                tr!("common_status_not_connected"),
                palette.danger,
            ),
            TwitchDevicePhase::Expired => (
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

    pub(in super::super) fn twitch_device_column(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let phase = self
            .device()
            .map_or(TwitchDevicePhase::Starting, |d| d.phase);
        let status_card = match phase {
            TwitchDevicePhase::Starting => {
                self.twitch_progress_card(true, accent, palette, density, cx)
            }
            TwitchDevicePhase::Waiting => {
                self.twitch_progress_card(false, accent, palette, density, cx)
            }
            TwitchDevicePhase::Authorized => self.twitch_done_card(palette, density, cx),
            TwitchDevicePhase::Expired => self.twitch_expired_card(palette, density, cx),
            TwitchDevicePhase::Denied => self.twitch_error_card(
                tr!("twitch_device_denied_title"),
                accent,
                palette,
                density,
                cx,
            ),
            TwitchDevicePhase::Failed => self.twitch_error_card(
                tr!("twitch_device_failed_title"),
                accent,
                palette,
                density,
                cx,
            ),
        };

        let has_code = self.device().is_some_and(|d| d.code.is_some());
        let mut column = div()
            .w_full()
            .max_w(PAGE_MAX_W)
            .flex()
            .flex_col()
            .child(self.twitch_explainer(palette, density, cx));
        if has_code {
            column = column
                .child(self.twitch_url_card(accent, palette, cx))
                .child(self.twitch_code_card(accent, palette, cx));
        }
        column
            .child(status_card)
            .child(self.twitch_device_footer(palette, density, cx))
            .into_any_element()
    }

    fn twitch_explainer(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        let prefix = tr!("twitch_device_explainer_prefix");
        let emphasis = tr!("twitch_device_explainer_emphasis");
        let suffix = tr!("twitch_device_explainer_suffix");
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
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .line_height(BODY_LINE_H)
            .mb(scaled(INTRO_TEXT_MB, density))
            .child(styled)
            .into_any_element()
    }

    fn twitch_url_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let url: SharedString = self.device().and_then(|d| d.code.as_ref()).map_or_else(
            || SharedString::from("-"),
            |c| SharedString::from(c.verification_uri.clone()),
        );
        let url_box = div()
            .flex_1()
            .min_w(px(0.0))
            .py(scaled(URL_BOX_PAD_V, density))
            .px(scaled(URL_BOX_PAD_H, density))
            .rounded(BUTTON_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(cx.mono_family())
            .text_size(FONT_XS)
            .text_color(palette.info)
            .child(url);
        let open_btn = div()
            .id("twitch-device-open")
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(ICON_LABEL_GAP, density))
            .py(scaled(OPEN_BUTTON_PAD_V, density))
            .px(scaled(OPEN_BUTTON_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_active))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _, _, cx| this.twitch_open_url(cx)))
            .child(icon(Icon::ExternalLink, ICON_INLINE, accent))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(accent)
                    .child(tr!("twitch_device_open_btn")),
            );
        step_card(
            1,
            palette.surface_overlay,
            palette.text_primary,
            palette,
            density,
            cx,
        )
        .border_color(palette.border_regular)
        .mb(spacing(Spacing::Sm, density))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .font_family(cx.body_family())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(FONT_XS)
                        .text_color(palette.text_primary)
                        .mb(scaled(OPEN_TITLE_MB, density))
                        .child(tr!("twitch_device_open_title")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(scaled(URL_ROW_GAP, density))
                        .child(url_box)
                        .child(open_btn),
                ),
        )
        .into_any_element()
    }

    fn twitch_code_card(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let code = self.device().and_then(|d| d.code.as_ref());
        let copied = self.device().is_some_and(|d| d.copied);

        let code_box = self.user_code_box(code, accent, palette, density, cx);
        let copy_btn = self.user_code_copy_button(copied, palette, density, cx);
        let meta_row = self.user_code_meta_row(code, accent, palette, density, cx);

        step_card(2, accent, palette.shell, palette, density, cx)
            .border_color(accent)
            .mb(scaled(CARD_STACK_MB, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .mb(spacing(Spacing::Sm, density))
                            .child(tr!("twitch_device_enter_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Sm, density))
                            .child(code_box)
                            .child(copy_btn),
                    )
                    .child(meta_row),
            )
            .into_any_element()
    }

    fn user_code_box(
        &self,
        code: Option<&DeviceCodeData>,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> gpui::Div {
        let glyphs: Vec<AnyElement> = code
            .map(|c| c.user_code.chars().collect::<Vec<_>>())
            .unwrap_or_else(|| vec!['-'])
            .into_iter()
            .map(|ch| {
                div()
                    .font_family(cx.mono_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(USER_CODE_FONT)
                    .text_color(accent)
                    .child(ch.to_string())
                    .into_any_element()
            })
            .collect();
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(scaled(CODE_GLYPH_GAP, density))
            .py(scaled(CODE_BOX_PAD_V, density))
            .px(scaled(CODE_BOX_PAD_H, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(accent)
            .bg(palette.shell)
            .children(glyphs)
    }

    fn user_code_copy_button(
        &self,
        copied: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let copy_color = if copied {
            palette.success
        } else {
            palette.text_secondary
        };
        div()
            .id("twitch-device-copy")
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(scaled(COPY_BUTTON_GAP, density))
            .p(scaled(COPY_BUTTON_PAD, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.border_color(palette.border_active))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _, _, cx| this.on_twitch_copy(cx)))
            .child(icon(
                if copied { Icon::Check } else { Icon::Copy },
                px(18.0),
                copy_color,
            ))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_MICRO)
                    .text_color(copy_color)
                    .child(if copied {
                        tr!("twitch_device_copied")
                    } else {
                        tr!("twitch_device_copy")
                    }),
            )
    }

    fn user_code_meta_row(
        &self,
        code: Option<&DeviceCodeData>,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let remaining = code.map_or(Duration::ZERO, DeviceCodeData::remaining);
        let total = remaining.as_secs();
        let clock: SharedString = SharedString::from(format!("{}:{:02}", total / 60, total % 60));
        div()
            .flex()
            .items_center()
            .gap(scaled(META_ROW_GAP, density))
            .mt(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(ICON_LABEL_GAP, density))
                    .child(icon(Icon::Clock, ICON_INLINE, palette.text_muted))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_LABEL)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_device_expires_in")),
                    )
                    .child(
                        div()
                            .font_family(cx.mono_family())
                            .text_size(FONT_LABEL)
                            .text_color(palette.text_primary)
                            .child(clock),
                    ),
            )
            .child(
                div()
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_faint)
                    .child("\u{00b7}"),
            )
            .child(
                div()
                    .id("twitch-device-new-code")
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .cursor_pointer()
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(|this, _, _, cx| this.begin_twitch_device(cx)))
                    .child(icon(Icon::Refresh, ICON_SMALL, accent))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_LABEL)
                            .text_color(accent)
                            .child(tr!("twitch_device_get_new_code")),
                    ),
            )
    }

    fn twitch_progress_card(
        &self,
        requesting: bool,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let name = self.display_name.clone();
        let scopes = scopes_preview();
        let headline = if requesting {
            tr!("twitch_device_requesting", name = name.as_str())
        } else {
            tr!("twitch_device_waiting", name = name.as_str())
        };
        let poll_interval = self
            .device()
            .and_then(|device| device.code.as_ref())
            .map(|code| code.poll_interval.as_secs().to_string());
        let subline = match poll_interval {
            Some(interval) => tr!(
                "twitch_device_polling_subline",
                interval = interval.as_str(),
                scopes = scopes.as_str()
            ),
            None => tr!("twitch_device_requesting_subline", scopes = scopes.as_str()),
        };
        let pulse = pulse_dot("twitch-device-pulse", accent, PULSE_DOT);
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
                            .child(headline),
                    )
                    .children((!requesting).then(|| {
                        div()
                            .font_family(cx.mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(subline)
                    })),
            )
            .into_any_element()
    }

    fn twitch_done_card(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
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
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("oauth_done_authorized")),
            )
            .into_any_element()
    }

    fn twitch_expired_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.elevated)
            .mb(scaled(CARD_STACK_MB, density))
            .child(icon(Icon::Clock, ICON_NOTICE, palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("twitch_device_expired_title")),
            )
            .into_any_element()
    }

    fn twitch_error_card(
        &self,
        title: String,
        accent: Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let detail = self
            .device()
            .and_then(|d| d.error.clone())
            .unwrap_or_else(|| tr!("twitch_device_denied_detail"));
        let retry = div()
            .id("twitch-denied-retry")
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Sm))
            .bg(accent)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(accent, CTA_HOVER_ALPHA)))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _, _, cx| this.begin_twitch_device(cx)))
            .child(icon(Icon::Refresh, ICON_SMALL, palette.shell))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("oauth_btn_retry")),
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
            .mb(scaled(CARD_STACK_MB, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, ICON_NOTICE, palette.danger))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(cx.body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(title),
                    )
                    .child(retry),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
                    .child(detail),
            )
            .into_any_element()
    }

    fn twitch_device_footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let back = div()
            .id("twitch-device-choose-different")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _, _, cx| this.cancel_twitch_and_leave(cx)))
            .child(icon(Icon::ChevronLeft, ICON_INLINE, palette.text_muted))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("oauth_footer_choose_different")),
            );
        let later = div()
            .id("twitch-device-later")
            .flex_none()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.border_regular, ROW_HOVER_WASH_ALPHA)))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _, _, cx| this.cancel_twitch_and_leave(cx)))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(tr!("twitch_device_do_later")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pt(spacing(Spacing::Xxs, density))
            .child(back)
            .child(later)
            .into_any_element()
    }
}

fn step_card(
    n: u8,
    circle_bg: Rgba,
    circle_fg: Rgba,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> gpui::Div {
    let circle = div()
        .flex_none()
        .size(STEP_BADGE)
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius(Radius::Lg))
        .bg(circle_bg)
        .child(
            div()
                .font_family(cx.body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_XXS)
                .text_color(circle_fg)
                .child(n.to_string()),
        );
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(scaled(STEP_CARD_GAP, density))
        .p(spacing(Spacing::Md, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .bg(palette.elevated)
        .child(circle)
}
