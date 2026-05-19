use std::time::Duration;

use forge_platform_core::{CapabilityFlags, ConnectionState, HeaderAction, SectionIcon};
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Row, container},
};

use crate::{
    BOOTSTRAP_FONT,
    palette::ForgePalette,
    tokens::{
        BORDER_THIN, Density, FONT_BODY, FONT_BODY_SM, FONT_CAPS_SM, FONT_CAPS_XS, FONT_HEADING_SM,
        FONT_VALUE, FontRole, Radius, Spacing, font, radius, spacing,
    },
};

pub struct HeaderCardParams<'a> {
    pub display_name: &'a str,
    pub version: Option<&'a str>,
    pub endpoint: Option<&'a str>,
    pub uptime: Option<Duration>,
    pub capability_flags: &'a CapabilityFlags,
    pub header_actions: &'a [HeaderAction],
    pub connection: ConnectionState,
    pub icon: SectionIcon,
}

pub fn integration_header_card<'a, Msg: Clone + 'a>(
    params: HeaderCardParams<'a>,
    on_action: impl Fn(HeaderAction) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_str = params.icon.as_str().to_owned();
    let icon_elem = icon_box(icon_str, params.connection, palette);
    let info_elem = info_column(&params, palette);
    let actions_elem = action_buttons(params.header_actions, on_action, palette);

    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxxl);
    let v_pad = spacing(Spacing::Xl, Density::Cozy);
    let h_pad = spacing(Spacing::Xxxl, Density::Cozy);

    let inner = iced::widget::row![
        icon_elem,
        container(info_elem).width(Length::Fill),
        actions_elem,
    ]
    .spacing(spacing(Spacing::Xxl, Density::Cozy) as f32)
    .align_y(Alignment::Center);

    container(inner)
        .padding([v_pad, h_pad])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn icon_box<'a, Msg: 'a>(
    icon_str: String,
    connection: ConnectionState,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_color = match connection {
        ConnectionState::Connected => palette.success,
        ConnectionState::Connecting | ConnectionState::Reconnecting => palette.warning,
        ConnectionState::Disconnected => palette.disabled,
    };
    let box_bg = palette.surface_overlay;
    let r = radius(Radius::Xxxl);

    let icon_text = iced::widget::text(icon_str)
        .font(BOOTSTRAP_FONT)
        .size(24.0)
        .color(icon_color);

    container(icon_text)
        .width(48.0)
        .height(48.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(box_bg)),
            border: Border {
                radius: r.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn info_column<'a, Msg: 'a>(
    params: &HeaderCardParams<'a>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let surface_overlay = palette.surface_overlay;
    let warning = palette.warning;

    let mut name_row: Row<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Md, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(
            iced::widget::text(params.display_name)
                .size(FONT_HEADING_SM)
                .color(text_primary),
        );

    if let Some(version) = params.version {
        name_row = name_row.push(version_pill(version, surface_overlay, text_muted));
    }

    if params.capability_flags.limited {
        let label = params
            .capability_flags
            .label
            .as_deref()
            .unwrap_or("Limited");
        name_row = name_row.push(limited_badge(label, surface_overlay, warning));
    }

    let sub = sub_line(params.endpoint, params.uptime);
    let sub_text = iced::widget::text(sub)
        .font(font(FontRole::Monospace))
        .size(FONT_BODY)
        .color(text_muted);

    iced::widget::column![name_row, sub_text]
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .into()
}

fn version_pill<'a, Msg: 'a>(version: &'a str, bg: Color, text_color: Color) -> Element<'a, Msg> {
    let r = radius(Radius::Xxl);
    container(
        iced::widget::text(version)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_XS)
            .color(text_color),
    )
    .padding([1, 7])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            radius: r.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn limited_badge<'a, Msg: 'a>(label: &'a str, bg: Color, text_color: Color) -> Element<'a, Msg> {
    let r = radius(Radius::Xxl);
    container(
        iced::widget::text(label.to_uppercase())
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_XS)
            .color(text_color),
    )
    .padding([1, 7])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            radius: r.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn action_buttons<'a, Msg: Clone + 'a>(
    actions: &'a [HeaderAction],
    on_action: impl Fn(HeaderAction) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let text_secondary = palette.text_secondary;
    let text_disconnect = palette.random;
    let r = radius(Radius::Sm);
    let v_pad = spacing(Spacing::Sm, Density::Cozy);
    let h_pad = spacing(Spacing::Xl, Density::Cozy);

    let mut row: Row<'a, Msg> = Row::new().spacing(spacing(Spacing::Sm, Density::Cozy) as f32);

    for action in actions {
        let label = action_label(action);
        let text_color = match action {
            HeaderAction::Disconnect => text_disconnect,
            _ => text_secondary,
        };
        let msg = on_action(action.clone());

        let btn = iced::widget::button(
            iced::widget::text(label)
                .size(FONT_BODY_SM)
                .color(text_color),
        )
        .on_press(msg)
        .padding([v_pad, h_pad])
        .style(move |_theme: &iced::Theme, status| {
            use iced::widget::button::Status;
            let bg_color = match status {
                Status::Hovered => Some(iced::Background::Color(Color {
                    a: 0.06,
                    ..border_color
                })),
                Status::Active | Status::Pressed | Status::Disabled => None,
            };
            iced::widget::button::Style {
                background: bg_color,
                text_color,
                border: Border {
                    color: border_color,
                    width: BORDER_THIN,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

        row = row.push(btn);
    }

    row.into()
}

fn action_label(action: &HeaderAction) -> &'static str {
    match action {
        HeaderAction::Reconnect => "Reconnect",
        HeaderAction::RefreshToken => "Refresh Token",
        HeaderAction::Disconnect => "Disconnect",
        HeaderAction::Settings => "Settings",
    }
}

fn sub_line(endpoint: Option<&str>, uptime: Option<Duration>) -> String {
    match (endpoint, uptime) {
        (Some(ep), Some(d)) => format!("{} · uptime {}", ep, format_uptime(d)),
        (Some(ep), None) => ep.to_owned(),
        (None, Some(d)) => format!("uptime {}", format_uptime(d)),
        (None, None) => String::new(),
    }
}

fn format_uptime(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

pub fn integration_health_grid<'a, Msg: 'a>(
    metrics: &[forge_platform_core::HealthMetric; 4],
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let gap = spacing(Spacing::Md, Density::Cozy) as f32;

    iced::widget::Row::new()
        .spacing(gap)
        .push(health_metric_card(&metrics[0], palette))
        .push(health_metric_card(&metrics[1], palette))
        .push(health_metric_card(&metrics[2], palette))
        .push(health_metric_card(&metrics[3], palette))
        .into()
}

fn health_metric_card<'a, Msg: 'a>(
    metric: &forge_platform_core::HealthMetric,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_platform_core::HealthValue;

    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xl);
    let v_pad = spacing(Spacing::Lg, Density::Cozy);
    let h_pad = spacing(Spacing::Xl, Density::Cozy);

    let cap_label = iced::widget::text(metric.label.to_uppercase())
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS_SM)
        .color(palette.text_muted);

    let value_col: Element<'a, Msg> = match &metric.value {
        HealthValue::Status {
            label: val_label,
            active,
        } => {
            let color = if *active {
                palette.success
            } else {
                palette.disabled
            };
            let dot = crate::status::status_dot(color, 7.0);
            let val_text = iced::widget::text(val_label.clone())
                .size(FONT_VALUE)
                .color(color);
            iced::widget::row![dot, val_text]
                .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
                .align_y(Alignment::Center)
                .into()
        }
        HealthValue::Text { primary, secondary } => {
            let primary_text = iced::widget::text(primary.clone())
                .size(FONT_VALUE)
                .color(palette.text_primary);
            if let Some(sec) = secondary {
                let sub = iced::widget::text(sec.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_CAPS_SM)
                    .color(palette.text_faint);
                iced::widget::column![primary_text, sub]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                iced::widget::column![primary_text].into()
            }
        }
        HealthValue::Pair { left, right } => iced::widget::text(format!("{left} · {right}"))
            .font(font(FontRole::Monospace))
            .size(FONT_VALUE)
            .color(palette.text_primary)
            .into(),
        HealthValue::Ratio {
            used,
            total,
            reset_hint,
        } => {
            let ratio_text = iced::widget::text(format!("{used} / {total}"))
                .size(FONT_VALUE)
                .color(palette.text_primary);
            if let Some(hint) = reset_hint {
                let hint_text = iced::widget::text(hint.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_CAPS_SM)
                    .color(palette.text_faint);
                iced::widget::column![ratio_text, hint_text]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                iced::widget::column![ratio_text].into()
            }
        }
    };

    let inner = iced::widget::column![cap_label, value_col]
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32);

    container(inner)
        .padding([v_pad, h_pad])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use forge_platform_core::{CapabilityFlags, ConnectionState, HeaderAction, SectionIcon};

    fn sample_flags() -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn sample_actions() -> Vec<HeaderAction> {
        vec![HeaderAction::Reconnect, HeaderAction::Settings]
    }

    #[test]
    fn integration_header_card_compiles_with_unit_msg() {
        let flags = sample_flags();
        let actions = sample_actions();
        let params = HeaderCardParams {
            display_name: "OBS Studio",
            version: Some("v31.0.2"),
            endpoint: Some("obs-websocket v5.5.0"),
            uptime: Some(Duration::from_secs(8040)),
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Connected,
            icon: SectionIcon::new("\u{F1D6}"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_header_card_with_limited_flag() {
        let flags = CapabilityFlags {
            limited: true,
            label: Some("Chat only".to_owned()),
        };
        let actions = vec![HeaderAction::Reconnect, HeaderAction::Disconnect];
        let params = HeaderCardParams {
            display_name: "Kick",
            version: Some("channel 1247813"),
            endpoint: Some("pusher.kick.com"),
            uptime: Some(Duration::from_secs(8040)),
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Connected,
            icon: SectionIcon::new("K"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_header_card_disconnected_no_optional_fields() {
        let flags = sample_flags();
        let actions = vec![HeaderAction::Reconnect];
        let params = HeaderCardParams {
            display_name: "OBS Studio",
            version: None,
            endpoint: None,
            uptime: None,
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Disconnected,
            icon: SectionIcon::new("\u{F1D6}"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_header_card_empty_actions() {
        let flags = sample_flags();
        let params = HeaderCardParams {
            display_name: "OBS Studio",
            version: None,
            endpoint: None,
            uptime: None,
            capability_flags: &flags,
            header_actions: &[],
            connection: ConnectionState::Connecting,
            icon: SectionIcon::new("\u{F1D6}"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn format_uptime_seconds_only() {
        assert_eq!(format_uptime(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_uptime_minutes_and_seconds() {
        assert_eq!(format_uptime(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn format_uptime_hours_and_minutes() {
        assert_eq!(format_uptime(Duration::from_secs(8040)), "2h 14m");
    }

    #[test]
    fn format_uptime_exactly_one_hour() {
        assert_eq!(format_uptime(Duration::from_secs(3600)), "1h 0m");
    }

    #[test]
    fn sub_line_both_present() {
        let result = sub_line(Some("obs-websocket v5"), Some(Duration::from_secs(3600)));
        assert_eq!(result, "obs-websocket v5 · uptime 1h 0m");
    }

    #[test]
    fn sub_line_endpoint_only() {
        let result = sub_line(Some("pusher.kick.com"), None);
        assert_eq!(result, "pusher.kick.com");
    }

    #[test]
    fn sub_line_uptime_only() {
        let result = sub_line(None, Some(Duration::from_secs(60)));
        assert_eq!(result, "uptime 1m 0s");
    }

    #[test]
    fn sub_line_neither() {
        let result = sub_line(None, None);
        assert_eq!(result, "");
    }

    #[test]
    fn action_label_maps_all_variants() {
        assert_eq!(action_label(&HeaderAction::Reconnect), "Reconnect");
        assert_eq!(action_label(&HeaderAction::RefreshToken), "Refresh Token");
        assert_eq!(action_label(&HeaderAction::Disconnect), "Disconnect");
        assert_eq!(action_label(&HeaderAction::Settings), "Settings");
    }

    #[test]
    fn limited_badge_defaults_to_limited_when_no_label() {
        let flags = CapabilityFlags {
            limited: true,
            label: None,
        };
        let actions = sample_actions();
        let params = HeaderCardParams {
            display_name: "Kick",
            version: None,
            endpoint: None,
            uptime: None,
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Connected,
            icon: SectionIcon::new("K"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_health_grid_renders_all_value_variants() {
        use forge_platform_core::{HealthMetric, HealthValue};

        let metrics: [HealthMetric; 4] = [
            HealthMetric {
                label: "Stream".to_owned(),
                value: HealthValue::Status {
                    label: "Live".to_owned(),
                    active: true,
                },
            },
            HealthMetric {
                label: "CPU / FPS".to_owned(),
                value: HealthValue::Pair {
                    left: "8.2%".to_owned(),
                    right: "60.0".to_owned(),
                },
            },
            HealthMetric {
                label: "Dropped".to_owned(),
                value: HealthValue::Text {
                    primary: "0 frames".to_owned(),
                    secondary: Some("0.00%".to_owned()),
                },
            },
            HealthMetric {
                label: "API Calls".to_owned(),
                value: HealthValue::Ratio {
                    used: 142,
                    total: 800,
                    reset_hint: Some("resets in 47s".to_owned()),
                },
            },
        ];

        let _: Element<'_, ()> = integration_health_grid(&metrics, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_health_grid_renders_inactive_status_and_bare_text() {
        use forge_platform_core::{HealthMetric, HealthValue};

        let metrics: [HealthMetric; 4] = [
            HealthMetric {
                label: "Recording".to_owned(),
                value: HealthValue::Status {
                    label: "Off".to_owned(),
                    active: false,
                },
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                value: HealthValue::Text {
                    primary: "1,284".to_owned(),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "EventSub".to_owned(),
                value: HealthValue::Status {
                    label: "11 subs".to_owned(),
                    active: true,
                },
            },
            HealthMetric {
                label: "Quota".to_owned(),
                value: HealthValue::Ratio {
                    used: 142,
                    total: 10_000,
                    reset_hint: None,
                },
            },
        ];

        let _: Element<'_, ()> = integration_health_grid(&metrics, &CATPPUCCIN_MOCHA);
    }
}
