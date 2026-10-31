use super::{
    ClientLiveness, HEADER_GLYPH, OwnedClientRow, OwnedSubscriptionChip, ServerConsoleView,
    SubscriptionChipKind,
};
use crate::async_bridge;
use crate::platform_visuals::platform_color;
use forge_components::{
    BORDER_THIN, ColumnWidth, ConfirmTone, DataRow, Density, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, KeyActivated, OverlayPosition, PushToast, Radius, Spacing, ToastKind,
    body_family, card, column, confirm_modal, data_table, empty_state, fmt_number, icon,
    mono_family, overlay, spacing, status_dot, tooltip_builder, tr, virtual_table,
};
use forge_events::EventSource;
use forge_server::ClientId;
use forge_types::PlatformId;
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, FontWeight, Pixels, Rgba, SharedString, div,
    prelude::*, px,
};

const MAX_VISIBLE_CHIPS: usize = 6;
const CLIENT_DOT: Pixels = px(6.0);
const DOT_CELL_W: Pixels = px(12.0);
const EVS_CELL_W: Pixels = px(60.0);
const UPTIME_CELL_W: Pixels = px(70.0);
const X_CELL_W: Pixels = px(20.0);
const SUB_CHIP: Pixels = px(8.0);
const SUB_CHIP_RADIUS: Pixels = px(2.0);
const X_GLYPH: Pixels = px(12.0);
const CLIENT_GROW: f32 = 1.4;
const SUBS_GROW: f32 = 1.6;

fn subscriptions_cell(
    row: &OwnedClientRow,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut cell = div()
        .flex()
        .items_center()
        .min_w(px(0.0))
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .mr(spacing(Spacing::Xxs, density))
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(row.origin_label.clone()),
        );

    if row.subscriptions.is_empty() {
        return cell
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("-"),
            )
            .into_any_element();
    }

    let visible = row.subscriptions.len().min(MAX_VISIBLE_CHIPS);
    for chip in &row.subscriptions[..visible] {
        cell = cell.child(subscription_chip_element(&row.key, chip, palette));
    }

    let overflow = row.subscriptions.len().saturating_sub(MAX_VISIBLE_CHIPS);
    if overflow > 0 {
        cell = cell.child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(format!("+{overflow}")),
        );
    }

    cell.into_any_element()
}

fn subscription_chip_element(
    row_key: &str,
    chip: &OwnedSubscriptionChip,
    palette: &ForgePalette,
) -> AnyElement {
    if chip.kind == SubscriptionChipKind::All {
        return div()
            .flex_none()
            .italic()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SUBSCRIBE_ALL_LABEL)
            .into_any_element();
    }

    let color = match chip.kind {
        SubscriptionChipKind::Source(source) => color_for_source(source, palette),
        _ => palette.text_faint,
    };

    div()
        .id((
            ElementId::from("srv-sub-chip"),
            SharedString::from(format!("{row_key}:{}", chip.label)),
        ))
        .flex_none()
        .size(SUB_CHIP)
        .rounded(SUB_CHIP_RADIUS)
        .bg(color)
        .tooltip(tooltip_builder(chip.label.clone(), palette))
        .into_any_element()
}

pub(super) const SUBSCRIBE_ALL_LABEL: &str = "*all";

fn color_for_source(source: EventSource, palette: &ForgePalette) -> Rgba {
    match source {
        EventSource::Twitch => platform_color(PlatformId::Twitch, palette),
        EventSource::YouTube => platform_color(PlatformId::YouTube, palette),
        EventSource::Kick => platform_color(PlatformId::Kick, palette),
        EventSource::Core => palette.warning,
        EventSource::Rhai => palette.warning,
        EventSource::Http => palette.danger,
        EventSource::Obs => palette.success,
        EventSource::VTube => palette.accent_teal,
        EventSource::Discord => palette.theme_brand,
        EventSource::Midi => palette.accent_peach,
        EventSource::Hotkey => palette.accent_peach,
        EventSource::Timer => palette.warning,
        EventSource::Server => palette.info,
        EventSource::Audio => palette.accent_peach,
        EventSource::Donation => palette.success,
    }
}

impl ServerConsoleView {
    pub(super) fn request_disconnect(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.connected_clients.get(index) {
            self.pending_disconnect.request(row.id);
            cx.notify();
        }
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect.cancel();
        cx.notify();
    }

    pub(super) fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        let target = self.pending_disconnect.take();
        cx.notify();
        let (Some(id), Some(handle)) = (target, self.server.clone()) else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { (id, handle.kick_client(id).await) },
            |this, (id, kicked): (ClientId, bool), cx| {
                this.connected_clients.retain(|c| c.id != id);
                if !kicked {
                    cx.push_toast(ToastKind::Info, tr!("server_client_already_gone"));
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(super) fn clients_panel(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Browser, HEADER_GLYPH, palette.info))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("server_clients_header")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_faint)
                    .child(fmt_number(self.connected_clients.len() as f64, 0)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("server_clients_live")),
            );

        let columns = vec![
            column("", ColumnWidth::Fixed(DOT_CELL_W)),
            column(tr!("server_col_client"), ColumnWidth::Flex(CLIENT_GROW)),
            column(
                tr!("server_col_subscriptions"),
                ColumnWidth::Flex(SUBS_GROW),
            ),
            column(tr!("server_col_evs"), ColumnWidth::Fixed(EVS_CELL_W)),
            column(tr!("server_col_uptime"), ColumnWidth::Fixed(UPTIME_CELL_W)),
            column("", ColumnWidth::Fixed(X_CELL_W)),
        ];

        let table: AnyElement = if self.connected_clients.is_empty() {
            let header_only = data_table(palette, columns, Vec::new())
                .density(density)
                .header_bg(palette.elevated)
                .separator(palette.elevated)
                .header_rule(palette.border_regular)
                .header_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
                .cell_gap(spacing(Spacing::Xs, density));
            div()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(header_only)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(empty_state(tr!("server_clients_empty"), palette).density(density)),
                )
                .into_any_element()
        } else {
            let pal = *palette;
            virtual_table(
                "srv-clients-scroll",
                palette,
                columns,
                self.connected_clients.len(),
                &self.clients_scroll,
                density,
            )
            .header_bg(palette.elevated)
            .separator(palette.elevated)
            .header_rule(palette.border_regular)
            .header_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
            .row_padding(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
            .cell_gap(spacing(Spacing::Xs, density))
            .build(
                move |this, ix, _window, cx| match this.connected_clients.get(ix) {
                    Some(row) => this.client_row(ix, row, &pal, density, cx),
                    None => DataRow::new(Vec::new()),
                },
                cx,
            )
        };

        let inner = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(table);

        card(inner, palette)
            .padding(px(0.0))
            .radius(Radius::Md)
            .full_width()
            .into_any_element()
    }

    fn client_row(
        &self,
        index: usize,
        row: &OwnedClientRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> DataRow {
        let dot_color = match row.liveness {
            ClientLiveness::Active => palette.success,
            ClientLiveness::Idle => palette.warning,
        };

        let name = div()
            .truncate()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(row.identification.clone());

        let x_button = div()
            .id((ElementId::from("srv-disconnect"), row.key.clone()))
            .flex()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("server_disconnect_tooltip"), palette))
            .key_activated(palette.text_primary)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_disconnect(index, cx)),
            )
            .child(icon(Icon::X, X_GLYPH, palette.text_faint));

        let evs = div()
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(fmt_number(row.events_per_second as f64, 1));
        let uptime = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(row.uptime_short.clone());

        DataRow::new(vec![
            status_dot(dot_color, CLIENT_DOT).into_any_element(),
            name.into_any_element(),
            subscriptions_cell(row, palette, density),
            evs.into_any_element(),
            uptime.into_any_element(),
            x_button.into_any_element(),
        ])
    }

    pub(super) fn disconnect_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = *self.pending_disconnect.get()?;
        let row = self.connected_clients.iter().find(|c| c.id == id)?;

        let message = tr!(
            "server_disconnect_confirm_hint",
            info = row.origin_label.as_str()
        );
        let confirm = confirm_modal(
            tr!("server_disconnect_confirm_title"),
            message,
            ConfirmTone::Warning,
            palette,
        )
        .item_name(row.identification.clone())
        .esc_hint(tr!("server_disconnect_esc_hint"))
        .on_cancel(
            "srv-disc-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "srv-disc-confirm",
            tr!("server_btn_disconnect"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(confirm, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("srv-disc-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_disconnect(cx));
                })
                .into_any_element(),
        )
    }
}
