use super::{BindState, CONTROL_GLYPH, LINK_GLYPH, ServerConsoleView, caption};
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use crate::server_bearer;
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, KeyActivated, Radius,
    Spacing, body_family, card, icon, mono_family, radius, spacing, tooltip_builder, tr,
};
use forge_types::Secret;
use gpui::{AnyElement, ClickEvent, Context, Div, div, prelude::*, px};

pub(super) fn mono_field(palette: &ForgePalette, density: Density) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .font_family(mono_family())
        .text_size(FONT_SM)
}

impl ServerConsoleView {
    fn toggle_token_reveal(&mut self, cx: &mut Context<Self>) {
        self.token_revealed = !self.token_revealed;
        cx.notify();
    }

    fn copy_bind_address(&mut self, cx: &mut Context<Self>) {
        if let Some(address) = self.bind_address.clone() {
            crate::toasts::copy_to_clipboard(address, cx);
        }
    }

    fn copy_token(&mut self, cx: &mut Context<Self>) {
        crate::toasts::copy_to_clipboard(self.bearer_token.expose().to_owned(), cx);
    }

    pub(super) fn request_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = true;
        cx.notify();
    }

    pub(super) fn cancel_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = false;
        cx.notify();
    }

    pub(super) fn confirm_regenerate(&mut self, cx: &mut Context<Self>) {
        self.regenerate_pending = false;
        self.regenerate_token(cx);
        cx.notify();
    }

    fn regenerate_token(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                handle
                    .regenerate_bearer_token()
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result: Result<Secret, RepoFailure>, cx| match result {
                Ok(token) => {
                    this.bearer_token = token;
                    cx.notify();
                }
                Err(reason) => server_bearer::token_regenerate_failed(&reason.text(), cx),
            },
            cx,
        );
    }

    pub(super) fn credentials_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.bind_column(palette, density, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.token_column(palette, density, cx)),
            );

        card(row, palette)
            .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Md, density))
            .radius(Radius::Md)
            .full_width()
            .into_any_element()
    }

    pub(super) fn bind_state(&self) -> BindState {
        match (self.running, self.bind_address.clone()) {
            (true, Some(address)) => BindState::Bound(address),
            (true, None) => BindState::AwaitingAddress,
            (false, _) => BindState::Stopped,
        }
    }

    fn bind_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = match self.bind_state() {
            BindState::Bound(address) => mono_field(palette, density)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_color(palette.text_primary)
                        .child(address),
                )
                .child(
                    div()
                        .id("srv-copy-addr")
                        .flex()
                        .flex_none()
                        .cursor_pointer()
                        .tooltip(tooltip_builder(tr!("common_copy"), palette))
                        .key_activated(palette.text_primary)
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.copy_bind_address(cx)),
                        )
                        .child(icon(Icon::Copy, CONTROL_GLYPH, palette.text_faint)),
                ),
            BindState::AwaitingAddress => mono_field(palette, density).child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_color(palette.text_faint)
                    .child(tr!("server_bind_address_loading")),
            ),
            BindState::Stopped => mono_field(palette, density).child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_color(palette.text_faint)
                    .child(tr!("server_not_running")),
            ),
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(caption(tr!("server_bind_address"), palette))
            .child(field)
            .into_any_element()
    }

    fn token_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(caption(tr!("server_bearer_token"), palette))
            .child(self.bearer_token_row(palette, density, cx))
            .child(self.regen_warning(palette, density))
            .into_any_element()
    }

    fn bearer_token_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = if self.token_revealed {
            self.bearer_token.expose().to_owned()
        } else {
            forge_server::masked_token(self.bearer_token.expose())
        };
        let reveal_glyph = if self.token_revealed {
            Icon::EyeOff
        } else {
            Icon::Eye
        };

        let field = mono_field(palette, density)
            .flex_1()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(palette.text_primary)
                    .child(shown),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(
                        div()
                            .id("srv-token-reveal")
                            .flex()
                            .cursor_pointer()
                            .key_activated(palette.text_primary)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.toggle_token_reveal(cx)
                            }))
                            .child(icon(reveal_glyph, CONTROL_GLYPH, palette.text_faint)),
                    )
                    .child(
                        div()
                            .id("srv-token-copy")
                            .flex()
                            .cursor_pointer()
                            .tooltip(tooltip_builder(tr!("common_copy"), palette))
                            .key_activated(palette.text_primary)
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.copy_token(cx)),
                            )
                            .child(icon(Icon::Copy, CONTROL_GLYPH, palette.text_faint)),
                    ),
            );

        let regenerate = div()
            .id("srv-token-regen")
            .flex()
            .flex_none()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.border_color(palette.warning))
            .child(icon(Icon::Refresh, CONTROL_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(tr!("server_btn_regenerate")),
            )
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.request_regenerate(cx)));

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(field)
            .child(regenerate)
            .into_any_element()
    }

    fn regen_warning(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(icon(Icon::AlertTriangle, LINK_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.warning)
                    .child(tr!("server_regen_warning_title")),
            )
            .into_any_element()
    }
}
