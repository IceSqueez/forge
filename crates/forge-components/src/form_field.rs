use crate::FONT_FIELD;
use crate::FONT_LABEL;
use crate::FONT_META;
use crate::cards::field_label;
use crate::icons::{Icon, icon};
use crate::key_activation::key_activated;
use crate::palette::ForgePalette;
use crate::toggle::toggle;
use crate::tokens::Density;
use crate::tokens::Spacing;
use crate::tokens::spacing;
use crate::tokens::{
    BORDER_THIN, FONT_XS, FONT_XXS, Radius, body_family, mono_family, radius, scaled,
};
use crate::tr;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Pixels, SharedString, Window, div, prelude::*, px,
};

pub const NAME_FIELD_LIMIT: usize = 64;
const ROW_PAD_X: Pixels = px(12.0);
const ROW_PAD_Y: Pixels = px(8.0);
const HINT_FONT: Pixels = FONT_META;
const NOTE_FONT: Pixels = FONT_LABEL;
const RETRY_GAP: Pixels = px(5.0);
const CHOICE_GAP: Pixels = px(8.0);

pub fn field_column(
    palette: &ForgePalette,
    label: &str,
    hint: Option<&str>,
    error: Option<&str>,
    control: AnyElement,
    density: Density,
) -> AnyElement {
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(field_label(palette, label.to_uppercase(), control));
    if let Some(error) = error {
        column = column.child(failure_note(error, palette, density));
    } else if let Some(hint) = hint {
        column = column.child(
            div()
                .font_family(body_family())
                .text_size(HINT_FONT)
                .text_color(palette.text_faint)
                .child(hint.to_owned()),
        );
    }
    column.into_any_element()
}

pub fn toggle_row(
    id: impl Into<ElementId>,
    value: bool,
    palette: &ForgePalette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
) -> AnyElement {
    let (state_label, state_color) = if value {
        (tr!("widget_toggle_on"), palette.success)
    } else {
        (tr!("widget_toggle_off"), palette.danger)
    };
    field_frame(palette, density)
        .justify_between()
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_FIELD)
                .text_color(state_color)
                .child(state_label),
        )
        .child(
            toggle(value, palette)
                .on_color(palette.success)
                .on_click(id, on_click),
        )
        .into_any_element()
}

pub fn choice_trigger(
    id: impl Into<ElementId>,
    selected: SharedString,
    has_selection: bool,
    palette: &ForgePalette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
) -> AnyElement {
    let text_color = if has_selection {
        palette.text_primary
    } else {
        palette.text_muted
    };
    let border_active = palette.border_active;
    field_frame(palette, density)
        .id(id)
        .justify_between()
        .gap(scaled(CHOICE_GAP, density))
        .cursor_pointer()
        .hover(move |s| s.border_color(border_active))
        .on_click(on_click)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_FIELD)
                .text_color(text_color)
                .child(selected),
        )
        .child(icon(Icon::ChevronDown, FONT_FIELD, palette.text_faint))
        .into_any_element()
}

pub fn failure_note(reason: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(icon(Icon::AlertCircle, FONT_XS, palette.danger))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(NOTE_FONT)
                .text_color(palette.danger)
                .child(reason.to_owned()),
        )
        .into_any_element()
}

pub fn failure_with_retry(
    id: impl Into<ElementId>,
    reason: &str,
    palette: &ForgePalette,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(failure_note(reason, palette, density))
        .child(
            key_activated(div().id(id), palette.info)
                .flex()
                .items_center()
                .gap(scaled(RETRY_GAP, density))
                .cursor_pointer()
                .on_click(on_retry)
                .child(icon(Icon::Refresh, FONT_XXS, palette.info))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(NOTE_FONT)
                        .text_color(palette.info)
                        .child(tr!("widget_field_retry")),
                ),
        )
        .into_any_element()
}

pub fn field_frame(palette: &ForgePalette, density: Density) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .px(scaled(ROW_PAD_X, density))
        .py(scaled(ROW_PAD_Y, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
}

pub fn parse_int_in_range(raw: &str, min: i64, max: i64) -> Option<i64> {
    raw.trim()
        .parse::<i64>()
        .ok()
        .filter(|value| (min..=max).contains(value))
}

pub fn int_entry_invalid(raw: &str, min: i64, max: i64, required: bool) -> bool {
    if raw.trim().is_empty() {
        required
    } else {
        parse_int_in_range(raw, min, max).is_none()
    }
}

pub fn int_range_hint(min: i64, max: i64) -> String {
    if max == i64::MAX {
        tr!("widget_field_range_open", min = min.to_string())
    } else {
        tr!(
            "widget_field_range",
            min = min.to_string(),
            max = max.to_string()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_in_range_accepts_whole_numbers_within_the_bounds() {
        for (min, max, raw, expected) in [
            (1_i64, 10_i64, "5", 5_i64),
            (1, 10, "1", 1),
            (1, 10, "10", 10),
            (1, 10, "  7  ", 7),
            (1, 10, "+3", 3),
            (-10, -1, "-10", -10),
            (-10, -1, "-1", -1),
            (-5, 5, "0", 0),
            (0, 0, "0", 0),
        ] {
            assert_eq!(
                parse_int_in_range(raw, min, max),
                Some(expected),
                "expected {raw:?} to parse inside {min}..={max}",
            );
        }
    }

    #[test]
    fn parse_int_in_range_rejects_out_of_range_and_non_integer_entries() {
        for (min, max, raw) in [
            (1_i64, 10_i64, "0"),
            (1, 10, "11"),
            (-10, -1, "-11"),
            (-10, -1, "0"),
            (1, 10, ""),
            (1, 10, "   "),
            (1, 10, "abc"),
            (1, 5000, "5.5"),
            (1, 5000, "1e3"),
            (1, 5000, "5 5"),
            (1, i64::MAX, "9223372036854775808"),
        ] {
            assert_eq!(
                parse_int_in_range(raw, min, max),
                None,
                "expected {raw:?} to be rejected for {min}..={max}",
            );
        }
    }

    #[test]
    fn int_entry_invalid_flags_a_blank_entry_only_when_the_field_is_required() {
        for raw in ["", "   ", "\t\n"] {
            assert!(
                int_entry_invalid(raw, 1, 10, true),
                "expected {raw:?} to be invalid for a required field",
            );
            assert!(
                !int_entry_invalid(raw, 1, 10, false),
                "expected {raw:?} to be accepted for an optional field",
            );
        }
    }

    #[test]
    fn int_entry_invalid_judges_non_blank_entries_the_same_whether_or_not_required() {
        for required in [true, false] {
            for (raw, invalid) in [("abc", true), ("11", true), ("0", true), ("5", false)] {
                assert_eq!(
                    int_entry_invalid(raw, 1, 10, required),
                    invalid,
                    "expected {raw:?} invalid={invalid} with required={required}",
                );
            }
        }
    }
}
