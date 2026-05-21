use iced::widget::button::Status;
use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Shadow, font as iced_font,
    widget::{Column, Row, Space, button, column, container, rule, text},
};

use forge_types::Variant;
pub use forge_types::VariantKind;

use crate::palette::ForgePalette;
use crate::tokens::{FONT_SM, FONT_XS, FontRole, Radius, font, radius};

pub fn variant_kind_color(kind: VariantKind, palette: &ForgePalette) -> Color {
    match kind {
        VariantKind::Int => palette.info,
        VariantKind::Float => palette.bits,
        VariantKind::Bool => palette.random,
        VariantKind::String => palette.success,
        VariantKind::Datetime => palette.accent_teal,
        VariantKind::Array => palette.brand,
        VariantKind::Object => palette.accent_pink_light,
    }
}

#[derive(Debug, Clone)]
pub struct FooterProps<'a> {
    pub position_info: &'a str,
    pub storage_info: Option<&'a str>,
    pub save_info: Option<&'a str>,
    pub live_indicator: bool,
}

pub fn type_pill<'a, Msg: 'a>(palette: &'a ForgePalette, kind: VariantKind) -> Element<'a, Msg> {
    let bg = palette.surface_overlay;
    let fg = variant_kind_color(kind, palette);
    let pill_font = Font {
        family: iced_font::Family::Name("JetBrains Mono"),
        weight: iced_font::Weight::Medium,
        stretch: iced_font::Stretch::Normal,
        style: iced_font::Style::Normal,
    };
    let r = radius(Radius::Xxl);

    container(text(kind.label()).size(FONT_XS).font(pill_font).color(fg))
        .padding([2.0_f32, 7.0_f32])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: r.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        })
        .into()
}

pub fn data_table<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    headers: Vec<&'a str>,
    widths: &[Length],
    rows: Vec<Vec<Element<'a, Msg>>>,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let shell_bg = palette.shell;
    let header_fg = palette.text_faint;

    let rule_style = move |_: &iced::Theme| rule::Style {
        color: border_color,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let mut header_row = Row::new().spacing(0).align_y(Alignment::Center);
    for (label, &width) in headers.iter().zip(widths.iter()) {
        header_row = header_row.push(
            container(
                text(*label)
                    .size(FONT_XS)
                    .font(font(FontRole::Monospace))
                    .color(header_fg),
            )
            .width(width),
        );
    }

    let header_container = container(header_row)
        .padding([7.0_f32, 14.0_f32])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(shell_bg)),
            ..container::Style::default()
        });

    let mut col = Column::new();
    col = col.push(header_container);
    col = col.push(rule::horizontal(1.0_f32).style(rule_style));

    for row_cells in rows {
        let mut data_row = Row::new().spacing(0).align_y(Alignment::Center);
        for (cell, &width) in row_cells.into_iter().zip(widths.iter()) {
            data_row = data_row.push(container(cell).width(width));
        }
        let data_container = container(data_row)
            .padding([8.0_f32, 14.0_f32])
            .width(Length::Fill);
        col = col.push(data_container);
        col = col.push(rule::horizontal(1.0_f32).style(rule_style));
    }

    col.width(Length::Fill).into()
}

pub fn persistence_toggle_inline<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    persisted: bool,
    on_toggle: Msg,
) -> Element<'a, Msg> {
    let pill_bg = if persisted {
        palette.success
    } else {
        palette.surface_overlay
    };
    let dot_color = if persisted {
        palette.shell
    } else {
        palette.disabled
    };
    let dot_left = if persisted {
        Length::Fill
    } else {
        Length::Fixed(2.0)
    };
    let dot_right = if persisted {
        Length::Fixed(2.0)
    } else {
        Length::Fill
    };

    let dot = container(Space::new())
        .width(10.0)
        .height(10.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 5.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let pill_inner = Row::new()
        .push(Space::new().width(dot_left))
        .push(dot)
        .push(Space::new().width(dot_right))
        .align_y(Alignment::Center);

    let pill = container(pill_inner)
        .width(24.0)
        .height(14.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(pill_bg)),
            border: Border {
                radius: 7.0.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    button(pill)
        .on_press(on_toggle)
        .padding(0)
        .style(|_: &iced::Theme, _: Status| button::Style {
            background: None,
            text_color: Color::TRANSPARENT,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

pub fn value_preview<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    variant: &Variant,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let (content, color) = match variant {
        Variant::Int(n) => (n.to_string(), palette.text_primary),
        Variant::Float(f) => (f.to_string(), palette.text_primary),
        Variant::Bool(true) => ("true".to_owned(), palette.success),
        Variant::Bool(false) => ("false".to_owned(), palette.random),
        Variant::String(s) => (format!("\"{}\"", s), palette.text_primary),
        Variant::Array(v) => (format!("[{} items]", v.len()), palette.text_primary),
        Variant::Object(m) => (format!("{{{} keys}}", m.len()), palette.text_primary),
        Variant::Datetime(_) => (variant.to_string(), palette.text_primary),
    };

    text(content).size(FONT_SM).font(mono).color(color).into()
}

pub fn data_screen_footer<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    props: FooterProps<'a>,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let shell_bg = palette.shell;
    let faint = palette.text_faint;
    let mono = font(FontRole::Monospace);

    let left = text(props.position_info)
        .size(FONT_XS)
        .font(mono)
        .color(faint);

    let mut right_row = Row::new().spacing(14).align_y(Alignment::Center);

    if let Some(storage) = props.storage_info {
        right_row = right_row.push(text(storage).size(FONT_XS).font(mono).color(faint));
    }

    if let Some(save) = props.save_info {
        let mut save_row = Row::new().spacing(5).align_y(Alignment::Center);

        if props.live_indicator {
            let dot_color = palette.success;
            let dot =
                container(Space::new())
                    .width(6.0)
                    .height(6.0)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(Background::Color(dot_color)),
                        border: Border {
                            radius: 3.0.into(),
                            color: Color::TRANSPARENT,
                            width: 0.0,
                        },
                        ..container::Style::default()
                    });
            save_row = save_row.push(dot);
        }

        save_row = save_row.push(text(save).size(FONT_XS).font(mono).color(faint));

        right_row = right_row.push(save_row);
    }

    let content = Row::new()
        .push(left)
        .push(Space::new().width(Length::Fill))
        .push(right_row)
        .align_y(Alignment::Center);

    column![
        rule::horizontal(1.0_f32).style(move |_: &iced::Theme| rule::Style {
            color: border_color,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        }),
        container(content)
            .padding([8.0_f32, 14.0_f32])
            .width(Length::Fill)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(shell_bg)),
                ..container::Style::default()
            }),
    ]
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use std::collections::BTreeMap;

    #[test]
    fn variant_kind_from_variant_int() {
        assert_eq!(
            VariantKind::from_variant(&Variant::Int(0)),
            VariantKind::Int
        );
    }

    #[test]
    fn variant_kind_from_variant_float() {
        assert_eq!(
            VariantKind::from_variant(&Variant::float(1.0).unwrap()),
            VariantKind::Float,
        );
    }

    #[test]
    fn variant_kind_from_variant_bool() {
        assert_eq!(
            VariantKind::from_variant(&Variant::Bool(true)),
            VariantKind::Bool,
        );
    }

    #[test]
    fn variant_kind_from_variant_string() {
        assert_eq!(
            VariantKind::from_variant(&Variant::String("hi".into())),
            VariantKind::String,
        );
    }

    #[test]
    fn variant_kind_from_variant_datetime() {
        assert_eq!(
            VariantKind::from_variant(&Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH)),
            VariantKind::Datetime,
        );
    }

    #[test]
    fn variant_kind_from_variant_array() {
        assert_eq!(
            VariantKind::from_variant(&Variant::Array(vec![])),
            VariantKind::Array,
        );
    }

    #[test]
    fn variant_kind_from_variant_object() {
        assert_eq!(
            VariantKind::from_variant(&Variant::Object(BTreeMap::new())),
            VariantKind::Object,
        );
    }

    #[test]
    fn variant_kind_labels_are_caps_abbreviations() {
        assert_eq!(VariantKind::Int.label(), "INT");
        assert_eq!(VariantKind::Float.label(), "FLOAT");
        assert_eq!(VariantKind::Bool.label(), "BOOL");
        assert_eq!(VariantKind::String.label(), "STR");
        assert_eq!(VariantKind::Datetime.label(), "TIME");
        assert_eq!(VariantKind::Array.label(), "ARR");
        assert_eq!(VariantKind::Object.label(), "OBJ");
    }

    #[test]
    fn variant_kind_colors_are_distinct() {
        let p = CATPPUCCIN_MOCHA;
        let kinds = [
            VariantKind::Int,
            VariantKind::Float,
            VariantKind::Bool,
            VariantKind::String,
            VariantKind::Datetime,
            VariantKind::Array,
            VariantKind::Object,
        ];
        let colors: Vec<Color> = kinds.iter().map(|k| variant_kind_color(*k, &p)).collect();
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i].r, colors[j].r,
                    "VariantKind index {i} and {j} share identical red channel"
                );
            }
        }
    }

    #[test]
    fn type_pill_compiles_for_unit_msg() {
        let p = CATPPUCCIN_MOCHA;
        let _: Element<'_, ()> = type_pill(&p, VariantKind::Int);
        let _: Element<'_, ()> = type_pill(&p, VariantKind::Float);
        let _: Element<'_, ()> = type_pill(&p, VariantKind::Bool);
        let _: Element<'_, ()> = type_pill(&p, VariantKind::String);
        let _: Element<'_, ()> = type_pill(&p, VariantKind::Datetime);
        let _: Element<'_, ()> = type_pill(&p, VariantKind::Array);
        let _: Element<'_, ()> = type_pill(&p, VariantKind::Object);
    }

    #[test]
    fn data_table_compiles_empty_rows() {
        let p = CATPPUCCIN_MOCHA;
        let _: Element<'_, ()> = data_table(
            &p,
            vec!["NAME", "TYPE", "VALUE"],
            &[Length::FillPortion(2), Length::Fixed(80.0), Length::Fill],
            vec![],
        );
    }

    #[test]
    fn data_table_compiles_with_rows() {
        let p = CATPPUCCIN_MOCHA;
        let rows: Vec<Vec<Element<'_, ()>>> = vec![vec![
            text("quoteCounter").into(),
            type_pill(&p, VariantKind::Int),
            text("47").into(),
        ]];
        let _: Element<'_, ()> = data_table(
            &p,
            vec!["NAME", "TYPE", "VALUE"],
            &[Length::FillPortion(2), Length::Fixed(80.0), Length::Fill],
            rows,
        );
    }

    #[test]
    fn persistence_toggle_compiles() {
        let p = CATPPUCCIN_MOCHA;
        let _: Element<'_, ()> = persistence_toggle_inline(&p, true, ());
        let _: Element<'_, ()> = persistence_toggle_inline(&p, false, ());
    }

    #[test]
    fn value_preview_compiles_all_variants() {
        let p = CATPPUCCIN_MOCHA;
        let _: Element<'_, ()> = value_preview(&p, &Variant::Int(42));
        let _: Element<'_, ()> = value_preview(&p, &Variant::float(2.5).unwrap());
        let _: Element<'_, ()> = value_preview(&p, &Variant::Bool(true));
        let _: Element<'_, ()> = value_preview(&p, &Variant::Bool(false));
        let _: Element<'_, ()> = value_preview(&p, &Variant::String("hello".into()));
        let _: Element<'_, ()> = value_preview(&p, &Variant::Array(vec![Variant::Int(1)]));
        let _: Element<'_, ()> = {
            let mut m = BTreeMap::new();
            m.insert("k".to_owned(), Variant::Int(1));
            value_preview(&p, &Variant::Object(m))
        };
        let _: Element<'_, ()> =
            value_preview(&p, &Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH));
    }

    #[test]
    fn data_screen_footer_compiles_full() {
        let p = CATPPUCCIN_MOCHA;
        let props = FooterProps {
            position_info: "Showing 47 of 47",
            storage_info: Some("Storage: 4.2 KB"),
            save_info: Some("Auto-saved 3s ago"),
            live_indicator: true,
        };
        let _: Element<'_, ()> = data_screen_footer(&p, props);
    }

    #[test]
    fn data_screen_footer_compiles_minimal() {
        let p = CATPPUCCIN_MOCHA;
        let props = FooterProps {
            position_info: "Showing 0 of 0",
            storage_info: None,
            save_info: None,
            live_indicator: false,
        };
        let _: Element<'_, ()> = data_screen_footer(&p, props);
    }
}
