use forge_script::{MethodDescriptor, SymbolKind};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow,
    widget::{Space, button, column, container, row, scrollable, text},
};

use crate::palette::ForgePalette;
use crate::tokens::{BORDER_ACCENT, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf};

#[derive(Debug, Clone, Default)]
pub struct AutocompletePopupState {
    pub filter: String,
    pub selected_idx: usize,
}

#[derive(Debug, Clone)]
pub enum AutocompletePopupMessage {
    FilterChanged(String),
    SelectionUp,
    SelectionDown,
    Insert(MethodDescriptor),
}

/// Filters and sorts `catalog` by `prefix`: when `prefix` contains `::` the left part is matched
/// as a namespace prefix and the right as a name prefix; otherwise each entry matches if its name
/// or namespace starts with `prefix`. Case-insensitive. `Fn` before `Property`; alphabetical within kind.
pub fn filter_candidates<'a>(
    catalog: &'a [MethodDescriptor],
    prefix: &str,
) -> Vec<&'a MethodDescriptor> {
    let lower = prefix.to_lowercase();
    let mut matched: Vec<&'a MethodDescriptor> =
        if let Some((ns_part, name_part)) = lower.split_once("::") {
            catalog
                .iter()
                .filter(|d| {
                    d.namespace
                        .map(|ns| ns.to_lowercase().starts_with(ns_part))
                        .unwrap_or(false)
                        && d.name.to_lowercase().starts_with(name_part)
                })
                .collect()
        } else {
            catalog
                .iter()
                .filter(|d| {
                    d.name.to_lowercase().starts_with(lower.as_str())
                        || d.namespace
                            .map(|ns| ns.to_lowercase().starts_with(lower.as_str()))
                            .unwrap_or(false)
                })
                .collect()
        };

    matched.sort_by(|a, b| {
        let ko = |k: &SymbolKind| match k {
            SymbolKind::Fn => 0u8,
            SymbolKind::Property => 1u8,
        };
        ko(&a.kind)
            .cmp(&ko(&b.kind))
            .then_with(|| a.name.cmp(b.name))
    });

    matched
}

pub fn autocomplete_popup<'a, Msg: 'a + Clone>(
    state: &'a AutocompletePopupState,
    candidates: &'a [&'static MethodDescriptor],
    on_message: impl Fn(AutocompletePopupMessage) -> Msg + 'static + Copy,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let border_regular = palette.border_regular;
    let selected_bg = palette.surface_overlay;
    let elevated = palette.elevated;

    let mut rows: Vec<Element<'a, Msg>> = Vec::with_capacity(candidates.len());

    for (idx, d) in candidates.iter().enumerate() {
        let desc = **d;
        let is_selected = idx == state.selected_idx;

        let qualified = match d.namespace {
            Some(ns) => format!("{}::{}", ns, d.name),
            None => d.name.to_string(),
        };

        let row_content = row![
            kind_badge(d.kind, palette),
            Space::new().width(spf(Spacing::Xs)),
            text(qualified).size(FONT_XS).color(text_primary),
            Space::new().width(Length::Fill),
            text(d.return_type).size(FONT_XS).color(text_muted),
        ]
        .align_y(Alignment::Center);

        let row_bg: Option<Background> = if is_selected {
            Some(Background::Color(selected_bg))
        } else {
            None
        };

        let btn = button(row_content)
            .on_press(on_message(AutocompletePopupMessage::Insert(desc)))
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
            .width(Length::Fill)
            .style(move |_theme, status| {
                use iced::widget::button::{Status, Style};
                let bg = if is_selected {
                    row_bg
                } else if matches!(status, Status::Hovered | Status::Pressed) {
                    Some(Background::Color(Color {
                        a: 0.06,
                        ..selected_bg
                    }))
                } else {
                    None
                };
                Style {
                    background: bg,
                    text_color: text_primary,
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: false,
                }
            });

        rows.push(btn.into());
    }

    const ROW_HEIGHT: f32 = 28.0;
    const MAX_VISIBLE_ROWS: usize = 8;

    let list_height = (candidates.len().min(MAX_VISIBLE_ROWS) as f32 * ROW_HEIGHT).max(ROW_HEIGHT);
    let candidate_list = scrollable(column(rows)).height(list_height);

    let match_count = candidates.len();
    let match_label = if match_count == 1 {
        "1 match".to_string()
    } else {
        format!("{} matches", match_count)
    };

    let footer = container(
        row![
            text(match_label).size(FONT_XS).color(text_muted),
            Space::new().width(Length::Fill),
            text("\u{2191}\u{2193}  Tab  Enter")
                .size(FONT_XS)
                .color(text_muted),
        ]
        .align_y(Alignment::Center),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)]);

    let separator = container(Space::new())
        .width(Length::Fill)
        .height(1.0)
        .style(move |_| container::Style {
            background: Some(Background::Color(border_regular)),
            ..container::Style::default()
        });

    container(column![candidate_list, separator, footer])
        .width(Length::Fixed(360.0))
        .style(move |_| container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: BORDER_ACCENT,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub(crate) fn kind_badge<'a, Msg: 'a>(
    kind: SymbolKind,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let (label, base_color) = match kind {
        SymbolKind::Fn => ("fn", palette.brand),
        SymbolKind::Property => ("prp", palette.info),
    };
    let bg = Color {
        a: 0.30,
        ..base_color
    };
    container(
        text(label)
            .size(FONT_XS)
            .color(Color::WHITE)
            .font(font(FontRole::Monospace)),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
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
    use forge_script::catalog;

    const KIND_TEST_CATALOG: [MethodDescriptor; 3] = [
        MethodDescriptor {
            namespace: Some("demo"),
            name: "beta_fn",
            kind: SymbolKind::Fn,
            params: &[],
            return_type: "()",
            doc: None,
        },
        MethodDescriptor {
            namespace: Some("demo"),
            name: "alpha_fn",
            kind: SymbolKind::Fn,
            params: &[],
            return_type: "()",
            doc: None,
        },
        MethodDescriptor {
            namespace: Some("demo"),
            name: "alpha_prop",
            kind: SymbolKind::Property,
            params: &[],
            return_type: "String",
            doc: None,
        },
    ];

    static SMOKE_D1: MethodDescriptor = MethodDescriptor {
        namespace: Some("globals"),
        name: "get",
        kind: SymbolKind::Fn,
        params: &[],
        return_type: "Variant",
        doc: None,
    };
    static SMOKE_D2: MethodDescriptor = MethodDescriptor {
        namespace: None,
        name: "log",
        kind: SymbolKind::Fn,
        params: &[],
        return_type: "()",
        doc: None,
    };
    static SMOKE_D3: MethodDescriptor = MethodDescriptor {
        namespace: Some("demo"),
        name: "alpha_prop",
        kind: SymbolKind::Property,
        params: &[],
        return_type: "String",
        doc: None,
    };

    #[test]
    fn filter_empty_prefix_returns_all() {
        let result = filter_candidates(catalog(), "");
        assert_eq!(result.len(), catalog().len());
    }

    #[test]
    fn filter_globals_prefix_matches_globals_get_set_etc() {
        let result = filter_candidates(catalog(), "globals");
        assert!(!result.is_empty());
        assert!(result.iter().all(|d| d.namespace == Some("globals")));
    }

    #[test]
    fn filter_namespace_qualified_prefix() {
        let result = filter_candidates(catalog(), "globals::g");
        assert!(!result.is_empty());
        assert!(
            result
                .iter()
                .all(|d| d.namespace == Some("globals") && d.name.starts_with('g'))
        );
        assert!(result.iter().any(|d| d.name == "get"));
    }

    #[test]
    fn filter_case_insensitive() {
        let result = filter_candidates(catalog(), "GLOBALS");
        assert!(!result.is_empty());
        assert!(result.iter().all(|d| d.namespace == Some("globals")));
    }

    #[test]
    fn filter_unknown_prefix_returns_empty() {
        let result = filter_candidates(catalog(), "xyzzy_unknown_prefix_9f3k2");
        assert!(result.is_empty());
    }

    #[test]
    fn filter_sorts_fn_before_property() {
        let result = filter_candidates(&KIND_TEST_CATALOG, "");
        assert_eq!(result.len(), 3);
        let first_prop = result
            .iter()
            .position(|d| matches!(d.kind, SymbolKind::Property));
        let last_fn = result
            .iter()
            .rposition(|d| matches!(d.kind, SymbolKind::Fn));
        if let (Some(prop_i), Some(fn_i)) = (first_prop, last_fn) {
            assert!(
                fn_i < prop_i,
                "all Fn entries must precede Property entries"
            );
        }
    }

    #[test]
    fn filter_alphabetical_within_kind() {
        let result = filter_candidates(&KIND_TEST_CATALOG, "");
        let fn_names: Vec<_> = result
            .iter()
            .filter(|d| matches!(d.kind, SymbolKind::Fn))
            .map(|d| d.name)
            .collect();
        for w in fn_names.windows(2) {
            assert!(
                w[0] <= w[1],
                "expected alphabetical order: {} <= {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn popup_state_default_selected_idx_is_zero() {
        assert_eq!(AutocompletePopupState::default().selected_idx, 0);
    }

    #[test]
    fn autocomplete_popup_smoke_no_panic() {
        let state = AutocompletePopupState::default();
        let candidates: &[&'static MethodDescriptor] = &[&SMOKE_D1, &SMOKE_D2, &SMOKE_D3];
        let _: Element<'_, u32> = autocomplete_popup(
            &state,
            candidates,
            |msg| match msg {
                AutocompletePopupMessage::Insert(_) => 1u32,
                _ => 0u32,
            },
            &CATPPUCCIN_MOCHA,
        );
    }
}
