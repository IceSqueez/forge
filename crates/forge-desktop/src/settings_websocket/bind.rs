use super::*;
use std::sync::Arc;

use forge_components::focus_inside_modal;
use forge_components::{
    ActivePresentation, BORDER_THIN, BulletItem, BulletKind, Density, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, OverlayPosition, Radius, Spacing, TypeToConfirm, TypeToConfirmEvent, icon,
    overlay, radio_row, radius, scaled, setting_block, spacing, tr, type_to_confirm,
};
use forge_server::{LAN_BIND_ADDRESS, LOCALHOST_BIND_ADDRESS, ServerSettings};
use forge_storage::SettingsRepo;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, SharedString, Window, div, px,
};

use crate::repo_failure::RepoFailure;

const TITLE_SUBTITLE_GAP: Pixels = px(2.0);
const BADGE_GAP: Pixels = px(3.0);
const BADGE_PAD_V: Pixels = px(1.0);

const MIN_PORT: u16 = 1024;
const BADGE_GLYPH: Pixels = px(10.0);

impl SettingsWebSocketView {
    fn select_localhost(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let prev = self.bind_choice;
        self.bind_choice = BindChoice::Localhost;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_field(
            prev,
            |this, prev, _| this.bind_choice = prev,
            AfterSave::ReloadServer,
            async move {
                ServerSettings::save_bind_address(repo.as_ref(), LOCALHOST_BIND_ADDRESS)
                    .await
                    .map_err(RepoFailure::from)?;
                ServerSettings::save_lan_bind_enabled(repo.as_ref(), false)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn open_lan_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let palette = cx.palette();
        let modal = cx.new(|cx| {
            type_to_confirm(tr!("settings_ws_lan_phrase"), &palette, cx)
                .title(tr!("settings_ws_lan_modal_title"))
                .explanation(tr!("settings_ws_lan_modal_explanation"))
                .instruction(
                    tr!("widget_confirm_type_prefix"),
                    tr!("widget_confirm_type_suffix"),
                )
                .bullets(lan_bind_bullets())
                .confirm_label(tr!("settings_ws_lan_modal_confirm_label"))
                .cancel_label(tr!("common_cancel"))
                .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        });
        let sub = cx.subscribe(
            &modal,
            |this, _modal, event: &TypeToConfirmEvent, cx| match event {
                TypeToConfirmEvent::Confirmed => this.confirm_lan(cx),
                TypeToConfirmEvent::Cancelled => this.cancel_lan(cx),
            },
        );
        focus_inside_modal(window, cx, |window, cx| {
            modal.update(cx, |m, cx| m.focus_input(window, cx))
        });
        self.lan_modal = Some(modal);
        self.lan_sub = Some(sub);
        cx.notify();
    }

    fn close_lan_modal(&mut self) {
        self.lan_modal = None;
        self.lan_sub = None;
    }

    pub(super) fn confirm_lan(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        self.bind_choice = BindChoice::Lan;
        self.close_lan_modal();
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.persist_field(
            BindChoice::Localhost,
            |this, prev, _| this.bind_choice = prev,
            AfterSave::ReloadServer,
            async move {
                ServerSettings::save_bind_address(repo.as_ref(), LAN_BIND_ADDRESS)
                    .await
                    .map_err(RepoFailure::from)?;
                ServerSettings::save_lan_bind_enabled(repo.as_ref(), true)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn cancel_lan(&mut self, cx: &mut Context<Self>) {
        self.bind_choice = BindChoice::Localhost;
        self.close_lan_modal();
        cx.notify();
    }

    pub(super) fn commit_port(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let parsed = self
            .port_input
            .read(cx)
            .content()
            .parse::<u16>()
            .ok()
            .filter(|p| *p >= MIN_PORT);
        match parsed {
            Some(port) if port == self.port => {}
            Some(port) => {
                let prev = self.port;
                self.port = port;
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.persist_field(
                    prev,
                    |this, prev, cx| {
                        this.port = prev;
                        this.port_input
                            .update(cx, |i, cx| i.set_content(prev.to_string(), cx));
                    },
                    AfterSave::ReloadServer,
                    async move {
                        ServerSettings::save_port(repo.as_ref(), port)
                            .await
                            .map_err(RepoFailure::from)
                    },
                    cx,
                );
            }
            None => {
                let restore = self.port.to_string();
                self.port_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
                cx.notify();
            }
        }
    }

    pub(super) fn bind_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        setting_block(
            tr!("settings_ws_bind_section_title"),
            Some(tr!("settings_ws_bind_section_subtitle").into()),
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(self.bind_card(BindChoice::Localhost, palette, density, cx))
                .child(self.bind_card(BindChoice::Lan, palette, density, cx)),
            palette,
            density,
            cx,
        )
    }

    fn bind_card(
        &self,
        choice: BindChoice,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.bind_choice == choice;
        let (title, tech, body, accent, badge_glyph, badge_label, badge_color) = match choice {
            BindChoice::Localhost => (
                tr!("settings_ws_bind_localhost_title"),
                LOCALHOST_BIND_ADDRESS.to_owned(),
                tr!("settings_ws_bind_localhost_description"),
                palette.brand,
                Icon::Lock,
                tr!("settings_ws_badge_recommended"),
                palette.success,
            ),
            BindChoice::Lan => (
                tr!("settings_ws_bind_lan_title"),
                LAN_BIND_ADDRESS.to_owned(),
                tr!("settings_ws_bind_lan_description"),
                palette.warning,
                Icon::AlertTriangle,
                tr!("settings_ws_badge_requires_confirmation"),
                palette.warning,
            ),
        };

        let badge = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(BADGE_GAP, density))
            .py(scaled(BADGE_PAD_V, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .child(icon(badge_glyph, BADGE_GLYPH, badge_color))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(badge_color)
                    .child(badge_label),
            );

        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .min_w(px(0.0))
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(badge)
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tech),
            );

        let info = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(scaled(TITLE_SUBTITLE_GAP, density))
            .child(title_row)
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(body),
            );

        let id: SharedString = match choice {
            BindChoice::Localhost => "settings-ws-bind-localhost".into(),
            BindChoice::Lan => "settings-ws-bind-lan".into(),
        };

        let pad = spacing(Spacing::Sm, density);
        radio_row(id, selected, accent, info, palette)
            .dot_metrics(px(16.0), px(7.0), px(2.0))
            .dot_unselected(palette.border_input)
            .align_start()
            .gap(spacing(Spacing::Sm, density))
            .padding(pad, pad)
            .corner_radius(radius(Radius::Md))
            .row_border(BORDER_THIN, BORDER_THIN)
            .row_border_color(palette.border_regular)
            .background(palette.base, palette.base)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| match choice {
                    BindChoice::Localhost => this.select_localhost(cx),
                    BindChoice::Lan => this.open_lan_modal(window, cx),
                }),
            )
    }

    pub(super) fn port_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> impl IntoElement {
        setting_block(
            tr!("settings_ws_port_section_title"),
            Some(tr!("settings_ws_port_subtitle").into()),
            self.port_input.clone(),
            palette,
            density,
            cx,
        )
    }

    pub(super) fn lan_overlay(
        &self,
        modal: Entity<TypeToConfirm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        overlay(modal, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("settings-ws-lan-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_lan(cx));
            })
            .into_any_element()
    }
}

fn lan_bind_bullets() -> Vec<BulletItem> {
    vec![
        BulletItem::new(BulletKind::Check, tr!("settings_ws_lan_bullet_phone")),
        BulletItem::new(
            BulletKind::Warning,
            tr!("settings_ws_lan_bullet_token_warning"),
        ),
        BulletItem::new(
            BulletKind::Warning,
            tr!("settings_ws_lan_bullet_public_wifi"),
        ),
        BulletItem::new(BulletKind::Info, tr!("settings_ws_lan_bullet_firewall")),
    ]
}
