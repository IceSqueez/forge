use std::borrow::Cow;

use iced::{
    Background, Border, Color, Element, Padding,
    widget::{container, pick_list, row, text_editor, text_input},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, Radius, Spacing, radius, sp};

pub fn input_padding() -> Padding {
    Padding::from([sp(Spacing::Xs), sp(Spacing::Sm)])
}

fn text_input_style(palette: ForgePalette, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => palette.border_active,
        text_input::Status::Disabled => palette.disabled,
        _ => palette.border_input,
    };
    let value_color = match status {
        text_input::Status::Disabled => palette.text_muted,
        _ => palette.text_primary,
    };
    text_input::Style {
        background: Background::Color(palette.shell),
        border: Border {
            color: border_color,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        icon: palette.text_muted,
        placeholder: palette.text_muted,
        value: value_color,
        selection: Color {
            a: 0.25,
            ..palette.brand
        },
    }
}

fn borderless_input_style(palette: ForgePalette, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        icon: palette.text_muted,
        placeholder: palette.text_muted,
        value: palette.text_primary,
        selection: Color {
            a: 0.25,
            ..palette.brand
        },
    }
}

pub fn text_input_field<'a, Msg: 'a + Clone>(
    placeholder: impl Into<Cow<'a, str>>,
    value: &'a str,
    on_change: impl Fn(String) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let ph: Cow<'a, str> = placeholder.into();
    text_input(ph.as_ref(), value)
        .on_input(on_change)
        .padding(input_padding())
        .width(iced::Length::Fill)
        .style(move |_theme, status| text_input_style(p, status))
        .into()
}

/// Like [`text_input_field`] but also emits `on_submit` when the user presses
/// Enter — used where an edit is buffered and committed explicitly.
pub fn text_input_field_submit<'a, Msg: 'a + Clone>(
    placeholder: impl Into<Cow<'a, str>>,
    value: &'a str,
    on_change: impl Fn(String) -> Msg + 'a,
    on_submit: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let ph: Cow<'a, str> = placeholder.into();
    text_input(ph.as_ref(), value)
        .on_input(on_change)
        .on_submit(on_submit)
        .padding(input_padding())
        .width(iced::Length::Fill)
        .style(move |_theme, status| text_input_style(p, status))
        .into()
}

fn inline_rename_style(palette: ForgePalette) -> text_input::Style {
    text_input::Style {
        background: Background::Color(palette.shell),
        border: Border {
            color: palette.brand,
            width: 0.5,
            radius: radius(Radius::Sm).into(),
        },
        icon: palette.text_muted,
        placeholder: palette.text_muted,
        value: palette.text_primary,
        selection: Color {
            a: 0.25,
            ..palette.brand
        },
    }
}

/// Brand-outlined "rename in place" `text_input` for tree/list rows. The caller
/// supplies the focus `id`, current buffer, and change/submit messages, then
/// chains row-specific `size`/`padding`/`font` before `.into()`; Escape and
/// click-away cancel stay the caller's job.
pub fn inline_rename<'a, Msg: 'a + Clone>(
    id: impl Into<iced::advanced::widget::Id>,
    value: &'a str,
    on_change: impl Fn(String) -> Msg + 'a,
    on_submit: Msg,
    palette: &ForgePalette,
) -> text_input::TextInput<'a, Msg> {
    let p = *palette;
    text_input("", value)
        .id(id)
        .on_input(on_change)
        .on_submit(on_submit)
        .width(iced::Length::Fill)
        .style(move |_theme, _status| inline_rename_style(p))
}

fn text_editor_style(palette: ForgePalette, status: text_editor::Status) -> text_editor::Style {
    let border_color = match status {
        text_editor::Status::Focused { .. } => palette.border_active,
        text_editor::Status::Disabled => palette.disabled,
        _ => palette.border_input,
    };
    let value_color = match status {
        text_editor::Status::Disabled => palette.text_muted,
        _ => palette.text_primary,
    };
    text_editor::Style {
        background: Background::Color(palette.shell),
        border: Border {
            color: border_color,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
        placeholder: palette.text_muted,
        value: value_color,
        selection: Color {
            a: 0.25,
            ..palette.brand
        },
    }
}

/// Multi-line counterpart to [`text_input_field`] — a ~130px resizable area for
/// long-form or JSON-shaped values. Wraps the stateful `text_editor`: the caller
/// holds a `text_editor::Content` and forwards each `text_editor::Action` back
/// through `on_action`.
pub fn text_area_field<'a, Msg: 'a + Clone>(
    placeholder: impl Into<Cow<'a, str>>,
    content: &'a text_editor::Content,
    on_action: impl Fn(text_editor::Action) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let ph: Cow<'a, str> = placeholder.into();
    text_editor(content)
        .placeholder(ph.into_owned())
        .on_action(on_action)
        .padding(input_padding())
        .height(iced::Length::Fixed(130.0))
        .style(move |_theme, status| text_editor_style(p, status))
        .into()
}

pub fn search_input<'a, Msg: 'a + Clone>(
    placeholder: impl Into<Cow<'a, str>>,
    value: &'a str,
    on_change: impl Fn(String) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let ph: Cow<'a, str> = placeholder.into();
    let icon = tabler_icon(Icon::Search, 14.0, p.text_muted);
    let input = text_input(ph.as_ref(), value)
        .on_input(on_change)
        .padding(0)
        .width(iced::Length::Fill)
        .style(move |_theme, status| borderless_input_style(p, status));
    let inner = row![icon, input]
        .spacing(6)
        .align_y(iced::Alignment::Center);
    container(inner)
        .padding(input_padding())
        .style(move |_theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn pick_list_style(palette: ForgePalette, status: pick_list::Status) -> pick_list::Style {
    let border_color = match status {
        pick_list::Status::Opened { .. } => palette.border_active,
        pick_list::Status::Hovered => palette.border_input,
        pick_list::Status::Active => palette.border_regular,
    };
    pick_list::Style {
        text_color: palette.text_primary,
        placeholder_color: palette.text_muted,
        handle_color: palette.text_muted,
        background: Background::Color(palette.shell),
        border: Border {
            color: border_color,
            width: BORDER_THIN,
            radius: radius(Radius::Md).into(),
        },
    }
}

#[derive(Clone, PartialEq)]
struct SelectOption<'a> {
    label: &'a str,
    value: &'a str,
}

impl std::fmt::Display for SelectOption<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

pub fn select<'a, Msg: 'a + Clone>(
    options: &'a [(&'a str, &'a str)],
    selected: Option<&'a str>,
    on_select: impl Fn(&'a str) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let opt_vec: Vec<SelectOption<'a>> = options
        .iter()
        .map(|(label, value)| SelectOption { label, value })
        .collect();

    let selected_opt: Option<SelectOption<'a>> = selected.and_then(|sel| {
        options
            .iter()
            .find(|(_, v)| *v == sel)
            .map(|(label, value)| SelectOption { label, value })
    });

    pick_list(opt_vec, selected_opt, move |opt: SelectOption<'a>| {
        on_select(opt.value)
    })
    .padding(input_padding())
    .width(iced::Length::Fill)
    .style(move |_theme, status| pick_list_style(p, status))
    .into()
}

pub fn select_owned<'a, Msg: 'a + Clone>(
    options: Vec<String>,
    selected: Option<String>,
    placeholder: String,
    on_select: impl Fn(String) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    pick_list(options, selected, on_select)
        .placeholder(placeholder)
        .padding(input_padding())
        .width(iced::Length::Fill)
        .style(move |_theme, status| pick_list_style(p, status))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use crate::tokens::BORDER_THIN;

    #[test]
    fn text_input_active_uses_shell_bg_and_input_border() {
        let style = text_input_style(CATPPUCCIN_MOCHA, text_input::Status::Active);
        assert_eq!(style.background, Background::Color(CATPPUCCIN_MOCHA.shell));
        assert!((style.border.width - BORDER_THIN).abs() < f32::EPSILON);
        assert_eq!(style.border.color, CATPPUCCIN_MOCHA.border_input);
        let expected_r = radius(Radius::Md);
        assert!((style.border.radius.top_left - expected_r).abs() < f32::EPSILON);
        assert!((style.border.radius.top_right - expected_r).abs() < f32::EPSILON);
    }

    #[test]
    fn text_input_disabled_uses_muted_value_color() {
        let style = text_input_style(CATPPUCCIN_MOCHA, text_input::Status::Disabled);
        assert_eq!(style.value, CATPPUCCIN_MOCHA.text_muted);
    }

    #[test]
    fn pick_list_active_uses_shell_bg_and_regular_border() {
        let style = pick_list_style(CATPPUCCIN_MOCHA, pick_list::Status::Active);
        assert_eq!(style.background, Background::Color(CATPPUCCIN_MOCHA.shell));
        assert!((style.border.width - BORDER_THIN).abs() < f32::EPSILON);
        assert_eq!(style.border.color, CATPPUCCIN_MOCHA.border_regular);
        let expected_r = radius(Radius::Md);
        assert!((style.border.radius.top_left - expected_r).abs() < f32::EPSILON);
        assert!((style.border.radius.top_right - expected_r).abs() < f32::EPSILON);
    }

    #[test]
    fn pick_list_opened_uses_active_border_color() {
        let style = pick_list_style(
            CATPPUCCIN_MOCHA,
            pick_list::Status::Opened { is_hovered: false },
        );
        assert_eq!(style.border.color, CATPPUCCIN_MOCHA.border_active);
    }
}
