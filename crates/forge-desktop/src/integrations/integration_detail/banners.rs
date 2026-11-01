use super::IntegrationDetail;
use crate::shared::user_text::resolve_message;
use forge_components::{
    ActivePresentation, BORDER_THIN, CTA_HOVER_ALPHA, ConfirmTone, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, KeyActivated, OverlayPosition, Radius, Spacing, confirm_modal, icon,
    overlay, radius, spacing, tr, with_alpha,
};
use forge_host::integration_catalog::declaration_of;
use forge_platform_core::{ConnectionState, CredentialState, HeaderAction, SignInRoute};
use gpui::{AnyElement, App, ClickEvent, Context, Rgba, Window, div, prelude::*, px};

pub(super) const CREDENTIAL_RECOVERY_SELECTOR: &str = "credential-recovery";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CredentialRecovery {
    SignInAgain,
    ReplaceCredentials,
}

pub(super) fn credential_recovery(
    route: SignInRoute,
    state: CredentialState,
) -> Option<CredentialRecovery> {
    if state == CredentialState::Valid {
        return None;
    }
    match route {
        SignInRoute::ChatPlatform(_) => Some(CredentialRecovery::SignInAgain),
        SignInRoute::CredentialForm => Some(CredentialRecovery::ReplaceCredentials),
        SignInRoute::NotNeeded => None,
    }
}

impl IntegrationDetail {
    pub(super) fn disconnect_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("integration_disconnect_title"),
            tr!("builtin_disconnect_confirm_hint"),
            ConfirmTone::Warning,
            palette,
        )
        .item_name(self.shown.display_name.clone())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "integration-disconnect-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "integration-disconnect-confirm",
            tr!("widget_header_action_disconnect"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("integration-disconnect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disconnect(cx));
            })
    }

    pub(super) fn needed_credential_recovery(&self) -> Option<CredentialRecovery> {
        let route = declaration_of(self.ports.status.id())?.sign_in;
        credential_recovery(route, self.shown.credential)
    }

    fn recover_credentials(
        &mut self,
        recovery: CredentialRecovery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match recovery {
            CredentialRecovery::ReplaceCredentials
                if self.shown.header_actions.contains(&HeaderAction::Settings) =>
            {
                self.on_header_action(HeaderAction::Settings, window, cx)
            }
            CredentialRecovery::ReplaceCredentials | CredentialRecovery::SignInAgain => {
                self.sign_out(cx)
            }
        }
    }

    pub(super) fn credential_banner(
        &self,
        recovery: CredentialRecovery,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.shown.display_name.clone();
        let title = match self.shown.credential {
            CredentialState::Rejected => tr!("integration_credential_rejected_title", name = name),
            CredentialState::ReauthRequired | CredentialState::Valid => {
                tr!("integration_reauth_title", name = name)
            }
        };
        let detail = self.ports.status.connect_failure().map_or_else(
            || tr!("integration_reauth_detail"),
            |failure| resolve_message(&failure).inline(),
        );
        let cta_label = match recovery {
            CredentialRecovery::SignInAgain => tr!("integration_reauth_btn"),
            CredentialRecovery::ReplaceCredentials => tr!("integration_credential_update_btn"),
        };
        let cta = div()
            .id(CREDENTIAL_RECOVERY_SELECTOR)
            .debug_selector(|| CREDENTIAL_RECOVERY_SELECTOR.to_owned())
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.warning)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.warning, CTA_HOVER_ALPHA)))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.recover_credentials(recovery, window, cx)
            }))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(cta_label),
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
            .child(icon(Icon::AlertTriangle, FONT_SM, palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(title),
                    )
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(detail),
                    ),
            )
            .child(cta)
            .into_any_element()
    }

    pub(super) fn state_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> Option<AnyElement> {
        let recovery_pending = self.needed_credential_recovery().is_some();
        let (accent, glyph, title, detail): (Rgba, Icon, String, String) =
            match self.shown.connection {
                ConnectionState::Connected => return None,
                ConnectionState::Disconnected
                    if self.sign_in.donation_settings.is_some() || recovery_pending =>
                {
                    return None;
                }
                ConnectionState::Connecting => (
                    palette.info,
                    Icon::Refresh,
                    tr!("integration_state_connecting_title"),
                    tr!("integration_state_connecting_detail"),
                ),
                ConnectionState::Reconnecting => (
                    palette.warning,
                    Icon::Refresh,
                    tr!("integration_state_reconnecting_title"),
                    tr!("integration_state_reconnecting_detail"),
                ),
                ConnectionState::Disconnected => (
                    palette.text_muted,
                    Icon::PlugConnected,
                    tr!("common_status_not_connected"),
                    tr!("integration_state_disconnected_detail"),
                ),
            };

        let failure_reason = if recovery_pending {
            None
        } else {
            self.ports
                .status
                .connect_failure()
                .map(|failure| resolve_message(&failure).inline())
        };

        let text_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(detail),
            )
            .children(failure_reason.map(|reason| {
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(reason)
            }));

        Some(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(accent)
                .bg(palette.elevated)
                .child(icon(glyph, FONT_SM, accent))
                .child(text_col)
                .into_any_element(),
        )
    }
}
