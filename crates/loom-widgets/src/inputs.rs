use std::borrow::Cow;

use iced::Element;
use iced::widget::{pick_list, row, text, text_input};

use crate::palette::LoomPalette;

pub fn text_input_field<'a, Msg: 'a + Clone>(
    placeholder: impl Into<Cow<'a, str>>,
    value: &'a str,
    on_change: impl Fn(String) -> Msg + 'a,
    _palette: &LoomPalette,
) -> Element<'a, Msg> {
    let ph: Cow<'a, str> = placeholder.into();
    text_input(ph.as_ref(), value)
        .on_input(on_change)
        .padding(8)
        .width(iced::Length::Fill)
        .into()
}

pub fn search_input<'a, Msg: 'a + Clone>(
    placeholder: impl Into<Cow<'a, str>>,
    value: &'a str,
    on_change: impl Fn(String) -> Msg + 'a,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let icon = text("\u{1F50D}").color(palette.text_muted);
    let input = text_input_field(placeholder, value, on_change, palette);
    row![icon, input]
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .into()
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
    _palette: &LoomPalette,
) -> Element<'a, Msg> {
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
    .padding(8)
    .width(iced::Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    fn assert_element_compiles<Msg: Clone>(_e: Element<'_, Msg>) {}

    #[test]
    fn text_input_field_produces_element() {
        let e = text_input_field("placeholder", "value", |s: String| s, &CATPPUCCIN_MOCHA);
        assert_element_compiles(e);
    }

    #[test]
    fn search_input_produces_element() {
        let e = search_input("search...", "", |s: String| s, &CATPPUCCIN_MOCHA);
        assert_element_compiles(e);
    }

    #[test]
    fn select_produces_element_with_options() {
        let opts = [("Option A", "a"), ("Option B", "b")];
        let e = select(&opts, Some("a"), |v: &str| v.to_string(), &CATPPUCCIN_MOCHA);
        assert_element_compiles(e);
    }

    #[test]
    fn select_produces_element_with_no_selection() {
        let opts = [("Option A", "a")];
        let e = select(&opts, None, |v: &str| v.to_string(), &CATPPUCCIN_MOCHA);
        assert_element_compiles(e);
    }
}
