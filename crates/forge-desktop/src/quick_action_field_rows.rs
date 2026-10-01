use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, body_family, field_label, icon,
    mono_family, radius, toggle, tr,
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Pixels, SharedString, Window, div, prelude::*, px,
};

pub(crate) const FIELD_FONT: Pixels = px(13.0);
pub(crate) const ROW_PAD_X: Pixels = px(12.0);
pub(crate) const ROW_PAD_Y: Pixels = px(8.0);
const FIELD_GAP: Pixels = px(4.0);
const HINT_FONT: Pixels = px(11.0);
const NOTE_FONT: Pixels = px(11.5);
const NOTE_GAP: Pixels = px(6.0);
const RETRY_GAP: Pixels = px(5.0);
const CHOICE_GAP: Pixels = px(8.0);

pub(crate) fn field_column(
    palette: &ForgePalette,
    label: &str,
    hint: Option<&str>,
    error: Option<&str>,
    control: AnyElement,
) -> AnyElement {
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(FIELD_GAP)
        .child(field_label(palette, label.to_uppercase(), control));
    if let Some(error) = error {
        column = column.child(failure_note(error, palette));
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

pub(crate) fn toggle_row(
    id: impl Into<ElementId>,
    value: bool,
    palette: &ForgePalette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let (state_label, state_color) = if value {
        (tr!("integration_qa_toggle_on"), palette.success)
    } else {
        (tr!("integration_qa_toggle_off"), palette.random)
    };
    field_frame(palette)
        .justify_between()
        .child(
            div()
                .font_family(mono_family())
                .text_size(FIELD_FONT)
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

pub(crate) fn choice_trigger(
    id: impl Into<ElementId>,
    selected: SharedString,
    has_selection: bool,
    palette: &ForgePalette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let text_color = if has_selection {
        palette.text_primary
    } else {
        palette.text_muted
    };
    let border_active = palette.border_active;
    field_frame(palette)
        .id(id)
        .justify_between()
        .gap(CHOICE_GAP)
        .cursor_pointer()
        .hover(move |s| s.border_color(border_active))
        .on_click(on_click)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FIELD_FONT)
                .text_color(text_color)
                .child(selected),
        )
        .child(icon(Icon::ChevronDown, FIELD_FONT, palette.text_faint))
        .into_any_element()
}

pub(crate) fn failure_note(reason: &str, palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(NOTE_GAP)
        .child(icon(Icon::AlertCircle, FONT_XS, palette.random))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(NOTE_FONT)
                .text_color(palette.random)
                .child(reason.to_owned()),
        )
        .into_any_element()
}

pub(crate) fn failure_with_retry(
    id: impl Into<ElementId>,
    reason: &str,
    palette: &ForgePalette,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(NOTE_GAP)
        .child(failure_note(reason, palette))
        .child(
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(RETRY_GAP)
                .cursor_pointer()
                .on_click(on_retry)
                .child(icon(Icon::Refresh, FONT_XXS, palette.info))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(NOTE_FONT)
                        .text_color(palette.info)
                        .child(tr!("integration_qa_field_retry")),
                ),
        )
        .into_any_element()
}

pub(crate) fn field_frame(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .px(ROW_PAD_X)
        .py(ROW_PAD_Y)
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
}

pub(crate) fn parse_int_in_range(raw: &str, min: i64, max: i64) -> Option<i64> {
    raw.trim()
        .parse::<i64>()
        .ok()
        .filter(|value| (min..=max).contains(value))
}

pub(crate) fn int_entry_invalid(raw: &str, min: i64, max: i64, required: bool) -> bool {
    if raw.trim().is_empty() {
        required
    } else {
        parse_int_in_range(raw, min, max).is_none()
    }
}

pub(crate) fn int_range_hint(min: i64, max: i64) -> String {
    if max == i64::MAX {
        tr!("integration_qa_field_range_open", min = min.to_string())
    } else {
        tr!(
            "integration_qa_field_range",
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
    fn int_entry_invalid_flags_unparsable_and_out_of_range_entries_even_when_optional() {
        for required in [true, false] {
            for raw in ["abc", "11"] {
                assert!(
                    int_entry_invalid(raw, 1, 10, required),
                    "expected {raw:?} to be invalid with required={required}",
                );
            }
        }
    }

    #[test]
    fn int_entry_invalid_accepts_an_in_range_entry_whether_or_not_it_is_required() {
        for required in [true, false] {
            assert!(!int_entry_invalid("5", 1, 10, required));
        }
    }
}
