use forge_components::ActivePresentation;
use forge_components::CTA_HOVER_ALPHA;
use forge_components::Density;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_ROW;
use forge_components::Spacing;
use forge_components::spacing;
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, KeyActivated, Radius, TextInput,
    body_family, grow_cell, icon, mono_family, radius, scaled, spinner, toggle, tr, with_alpha,
};
use forge_storage::{Flag, Setting, SettingsRepo, read_setting_or_default, write_setting};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString, div,
    prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};

const BOX_TEXT_SIZE: Pixels = FONT_XS;
const BOX_PAD_V: Pixels = px(7.0);
pub(crate) const BOX_PAD_H: Pixels = px(11.0);
const BOX_RADIUS: Pixels = px(7.0);

const FIELD_LABEL_SIZE: Pixels = FONT_META;
pub(crate) const FIELD_BLOCK_GAP: Pixels = px(12.0);

const TOGGLE_GLYPH: Pixels = px(14.0);
const TOGGLE_ROW_PAD_V: Pixels = px(8.0);
const TOGGLE_LABEL_SIZE: Pixels = FONT_ROW;
const TOGGLE_HINT_SIZE: Pixels = FONT_META;

const STRIP_MARGIN: Pixels = px(12.0);
const STRIP_GLYPH: Pixels = px(13.0);
const STRIP_TITLE_SIZE: Pixels = FONT_LABEL;

const BUTTON_ROW_GAP: Pixels = px(8.0);
const BUTTON_PAD_V: Pixels = px(8.0);
const GHOST_PAD_H: Pixels = px(14.0);
const BUTTON_GAP: Pixels = px(5.0);
const BUTTON_GLYPH: Pixels = px(13.0);
const BUSY_OPACITY: f32 = 0.6;

const HOST_CELL_GROW: f32 = 16.0;
const PORT_CELL_GROW: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectFlag {
    AutoReconnect,
    ConnectOnLaunch,
}

pub(crate) struct ConnectFlagSpec {
    pub auto_reconnect: &'static Setting<Flag>,
    pub connect_on_launch: &'static Setting<Flag>,
    pub auto_reconnect_id: &'static str,
    pub connect_on_launch_id: &'static str,
    pub auto_reconnect_label_key: &'static str,
    pub auto_reconnect_hint_key: &'static str,
    pub connect_on_launch_label_key: &'static str,
    pub connect_on_launch_hint_key: &'static str,
    pub save_failed_key: &'static str,
}

impl ConnectFlagSpec {
    pub(crate) fn declaration(&self, flag: ConnectFlag) -> &'static Setting<Flag> {
        match flag {
            ConnectFlag::AutoReconnect => self.auto_reconnect,
            ConnectFlag::ConnectOnLaunch => self.connect_on_launch,
        }
    }

    fn element_id(&self, flag: ConnectFlag) -> &'static str {
        match flag {
            ConnectFlag::AutoReconnect => self.auto_reconnect_id,
            ConnectFlag::ConnectOnLaunch => self.connect_on_launch_id,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ConnectFlags {
    pub auto_reconnect: bool,
    pub connect_on_launch: bool,
}

impl Default for ConnectFlags {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            connect_on_launch: true,
        }
    }
}

impl ConnectFlags {
    pub(crate) async fn load(settings: &dyn SettingsRepo, spec: &ConnectFlagSpec) -> Self {
        Self {
            auto_reconnect: read_setting_or_default(settings, spec.auto_reconnect).await,
            connect_on_launch: read_setting_or_default(settings, spec.connect_on_launch).await,
        }
    }

    pub(crate) fn flip(&mut self, flag: ConnectFlag) -> bool {
        let slot = match flag {
            ConnectFlag::AutoReconnect => &mut self.auto_reconnect,
            ConnectFlag::ConnectOnLaunch => &mut self.connect_on_launch,
        };
        *slot = !*slot;
        *slot
    }

    fn get(self, flag: ConnectFlag) -> bool {
        match flag {
            ConnectFlag::AutoReconnect => self.auto_reconnect,
            ConnectFlag::ConnectOnLaunch => self.connect_on_launch,
        }
    }
}

pub(crate) fn persist_flag<V: 'static>(
    rt_handle: &tokio::runtime::Handle,
    settings: &Arc<dyn SettingsRepo>,
    spec: &ConnectFlagSpec,
    flag: ConnectFlag,
    value: bool,
    cx: &mut Context<V>,
) {
    let settings = Arc::clone(settings);
    let setting = spec.declaration(flag);
    async_bridge::report_failure(
        rt_handle,
        async move { write_setting(&*settings, setting, &value).await },
        ErrorSink::Toast,
        tr!(spec.save_failed_key),
        cx,
    );
}

pub(crate) fn endpoint_input(
    placeholder: impl Into<SharedString>,
    palette: ForgePalette,
    cx: &mut Context<TextInput>,
) -> TextInput {
    TextInput::new(placeholder, cx)
        .plain()
        .mono()
        .with_palette(palette)
        .with_font_size(BOX_TEXT_SIZE)
}

pub(crate) fn endpoint_host_is_valid(host: &str) -> bool {
    let bracketed_ipv6 = host.starts_with('[') && host.ends_with(']');
    !host.is_empty()
        && !host
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '@'))
        && (bracketed_ipv6 || !host.contains(':'))
}

pub(crate) fn read_endpoint(
    host: &str,
    port: &str,
    default_host: &str,
    default_port: u16,
) -> (String, Option<u16>) {
    let host = host.trim();
    let host = if host.is_empty() {
        default_host.to_owned()
    } else {
        host.to_owned()
    };
    let port = port.trim();
    let port = if port.is_empty() {
        Some(default_port)
    } else {
        port.parse::<u16>().ok().filter(|p| *p != 0)
    };
    (host, port)
}

pub(crate) struct EndpointFields<'a> {
    pub host_label: String,
    pub port_label: String,
    pub host: &'a Entity<TextInput>,
    pub port: &'a Entity<TextInput>,
    pub host_focused: bool,
    pub port_focused: bool,
}

pub(crate) fn endpoint_row(
    fields: EndpointFields<'_>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let host_field = div()
        .flex()
        .flex_col()
        .child(field_label(fields.host_label, None, palette, density))
        .child(input_box(fields.host_focused, palette, density).child(fields.host.clone()));
    let port_field = div()
        .flex()
        .flex_col()
        .child(field_label(fields.port_label, None, palette, density))
        .child(input_box(fields.port_focused, palette, density).child(fields.port.clone()));

    div()
        .w_full()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Sm, density))
        .mb(scaled(FIELD_BLOCK_GAP, density))
        .child(grow_cell(host_field, HOST_CELL_GROW))
        .child(grow_cell(port_field, PORT_CELL_GROW))
        .into_any_element()
}

pub(crate) fn field_label(
    label: String,
    note: Option<String>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .mb(spacing(Spacing::Xs, density))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FIELD_LABEL_SIZE)
                .text_color(palette.text_muted)
                .child(label),
        );
    if let Some(note) = note {
        row = row.child(
            div()
                .font_family(body_family())
                .text_size(FIELD_LABEL_SIZE)
                .text_color(palette.text_faint)
                .child(note),
        );
    }
    row
}

pub(crate) fn box_frame(palette: &ForgePalette, density: Density) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .py(scaled(BOX_PAD_V, density))
        .px(scaled(BOX_PAD_H, density))
        .rounded(BOX_RADIUS)
        .border(BORDER_THIN)
        .bg(palette.shell)
}

pub(crate) fn input_box(focused: bool, palette: &ForgePalette, density: Density) -> gpui::Div {
    let border = if focused {
        palette.border_active
    } else {
        palette.border_input
    };
    box_frame(palette, density).border_color(border)
}

pub(crate) fn mono_box_text(text: impl Into<SharedString>, palette: &ForgePalette) -> gpui::Div {
    div()
        .font_family(mono_family())
        .text_size(BOX_TEXT_SIZE)
        .text_color(palette.text_primary)
        .child(text.into())
}

pub(crate) fn flag_rows<V: 'static>(
    spec: &ConnectFlagSpec,
    flags: ConnectFlags,
    palette: &ForgePalette,
    on_toggle: fn(&mut V, ConnectFlag, &mut Context<V>),
    cx: &mut Context<V>,
) -> [AnyElement; 2] {
    [
        flag_row(
            FlagRow {
                glyph: Icon::Refresh,
                tint: palette.info,
                label: tr!(spec.auto_reconnect_label_key),
                hint: tr!(spec.auto_reconnect_hint_key),
                flag: ConnectFlag::AutoReconnect,
                last: false,
            },
            spec,
            flags,
            palette,
            on_toggle,
            cx,
        ),
        flag_row(
            FlagRow {
                glyph: Icon::Bolt,
                tint: palette.warning,
                label: tr!(spec.connect_on_launch_label_key),
                hint: tr!(spec.connect_on_launch_hint_key),
                flag: ConnectFlag::ConnectOnLaunch,
                last: true,
            },
            spec,
            flags,
            palette,
            on_toggle,
            cx,
        ),
    ]
}

struct FlagRow {
    glyph: Icon,
    tint: Rgba,
    label: String,
    hint: String,
    flag: ConnectFlag,
    last: bool,
}

fn flag_row<V: 'static>(
    row: FlagRow,
    spec: &ConnectFlagSpec,
    flags: ConnectFlags,
    palette: &ForgePalette,
    on_toggle: fn(&mut V, ConnectFlag, &mut Context<V>),
    cx: &mut Context<V>,
) -> AnyElement {
    let density = cx.density();
    let flag = row.flag;
    let control = toggle(flags.get(flag), palette).on_click(
        spec.element_id(flag),
        cx.listener(move |this, _: &ClickEvent, _, cx| on_toggle(this, flag, cx)),
    );

    let labels = div()
        .flex()
        .flex_col()
        .child(
            div()
                .font_family(body_family())
                .text_size(TOGGLE_LABEL_SIZE)
                .text_color(palette.text_primary)
                .child(row.label),
        )
        .child(
            div()
                .font_family(body_family())
                .text_size(TOGGLE_HINT_SIZE)
                .text_color(palette.text_faint)
                .child(row.hint),
        );

    let mut element = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .py(scaled(TOGGLE_ROW_PAD_V, density))
        .child(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Sm, density))
                .child(icon(row.glyph, TOGGLE_GLYPH, row.tint))
                .child(labels),
        )
        .child(control);
    if !row.last {
        element = element
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
    }
    element.into_any_element()
}

pub(crate) enum AttemptStatus {
    Busy(String),
    Success { title: String, detail: String },
    Failure { title: String, detail: String },
}

#[derive(Clone, Copy)]
pub(crate) struct StripLook {
    pub gap: Pixels,
    pub pad_v: Pixels,
    pub pad_h: Pixels,
}

pub(crate) struct StripContent {
    pub accent: Rgba,
    pub indicator: AnyElement,
    pub title: String,
    pub detail: Option<(String, Rgba)>,
}

impl AttemptStatus {
    pub(crate) fn strip(
        &self,
        spinner_id: &'static str,
        look: StripLook,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let content = match self {
            AttemptStatus::Busy(label) => StripContent {
                accent: palette.border_regular,
                indicator: spinner(
                    SharedString::from(spinner_id),
                    Icon::Loader2,
                    STRIP_GLYPH,
                    palette.text_muted,
                )
                .into_any_element(),
                title: label.clone(),
                detail: None,
            },
            AttemptStatus::Success { title, detail } => StripContent {
                accent: palette.success,
                indicator: strip_icon(Icon::Check, palette.success),
                title: title.clone(),
                detail: Some((detail.clone(), palette.text_muted)),
            },
            AttemptStatus::Failure { title, detail } => StripContent {
                accent: palette.danger,
                indicator: strip_icon(Icon::AlertTriangle, palette.danger),
                title: title.clone(),
                detail: Some((detail.clone(), palette.text_muted)),
            },
        };
        outcome_strip(content, look, palette, density)
    }
}

pub(crate) fn strip_icon(glyph: Icon, tint: Rgba) -> AnyElement {
    icon(glyph, STRIP_GLYPH, tint).into_any_element()
}

pub(crate) fn strip_frame(look: StripLook, palette: &ForgePalette, density: Density) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(look.gap)
        .py(look.pad_v)
        .px(look.pad_h)
        .my(scaled(STRIP_MARGIN, density))
        .rounded(BOX_RADIUS)
        .border(BORDER_THIN)
        .bg(palette.shell)
}

pub(crate) fn outcome_strip(
    content: StripContent,
    look: StripLook,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut text = div().flex_1().min_w(px(0.0)).flex().flex_col().child(
        div()
            .font_family(body_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(STRIP_TITLE_SIZE)
            .text_color(palette.text_primary)
            .child(content.title),
    );
    if let Some((detail, tint)) = content.detail {
        text = text.child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(tint)
                .child(detail),
        );
    }

    strip_frame(look, palette, density)
        .border_color(content.accent)
        .child(content.indicator)
        .child(text)
        .into_any_element()
}

pub(crate) struct FormButtons {
    pub test_id: &'static str,
    pub submit_id: &'static str,
    pub test_label: String,
    pub submit_glyph: Icon,
    pub submit_label: String,
    pub test_disabled: bool,
    pub submit_disabled: bool,
}

pub(crate) fn form_buttons<V: 'static>(
    buttons: FormButtons,
    palette: &ForgePalette,
    on_test: fn(&mut V, &mut Context<V>),
    on_submit: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> AnyElement {
    let density = cx.density();
    let test_disabled = buttons.test_disabled;
    let submit_disabled = buttons.submit_disabled;

    let test = div()
        .id(buttons.test_id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(scaled(BUTTON_GAP, density))
        .py(scaled(BUTTON_PAD_V, density))
        .px(scaled(GHOST_PAD_H, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .when(!test_disabled, |el| {
            el.cursor_pointer()
                .hover(|s| s.border_color(palette.border_input))
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_test(this, cx)))
        })
        .when(test_disabled, |el| el.opacity(BUSY_OPACITY))
        .child(icon(
            Icon::PlugConnected,
            BUTTON_GLYPH,
            palette.text_secondary,
        ))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(buttons.test_label),
        );

    let submit = div()
        .id(buttons.submit_id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(scaled(BUTTON_GAP, density))
        .py(scaled(BUTTON_PAD_V, density))
        .px(spacing(Spacing::Md, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.brand)
        .when(!submit_disabled, |el| {
            el.cursor_pointer()
                .hover(|s| s.bg(with_alpha(palette.brand, CTA_HOVER_ALPHA)))
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_submit(this, cx)))
        })
        .when(submit_disabled, |el| el.opacity(BUSY_OPACITY))
        .child(icon(buttons.submit_glyph, BUTTON_GLYPH, palette.shell))
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_XS)
                .text_color(palette.shell)
                .child(buttons.submit_label),
        );

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(BUTTON_ROW_GAP, density))
        .child(test)
        .child(submit)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::endpoint_host_is_valid;

    #[test]
    fn endpoint_host_accepts_names_and_addresses_and_refuses_urls_and_stray_characters() {
        for (host, valid) in [
            ("localhost", true),
            ("studio.lan", true),
            ("192.0.2.10", true),
            ("[::1]", true),
            ("[fe80::1%eth0]", true),
            ("", false),
            ("::1", false),
            ("host:8001", false),
            ("ws://host", false),
            ("host/path", false),
            ("host\\path", false),
            ("host?query", false),
            ("host#frag", false),
            ("user@host", false),
            ("my host", false),
            ("host\t", false),
        ] {
            assert_eq!(endpoint_host_is_valid(host), valid, "{host:?}");
        }
    }
}
