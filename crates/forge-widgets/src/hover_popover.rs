use forge_script::{MethodDescriptor, SymbolKind};
use iced::{
    Alignment, Background, Border, Element, Length,
    widget::{Space, column, container, row, text},
};

use crate::autocomplete_popup::kind_badge;
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_ACCENT, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf};

/// `Property` kind emits `name -> return_type` (no parentheses); `Fn` emits `name(p: ty, …) -> return_type`.
pub fn format_signature(descriptor: &MethodDescriptor) -> String {
    match descriptor.kind {
        SymbolKind::Property => {
            format!("{} -> {}", descriptor.name, descriptor.return_type)
        }
        SymbolKind::Fn => {
            let params: String = descriptor
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.ty))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}({}) -> {}",
                descriptor.name, params, descriptor.return_type
            )
        }
    }
}

pub fn hover_popover<'a, Msg: 'a>(
    descriptor: &'a MethodDescriptor,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let border_regular = palette.border_regular;
    let elevated = palette.elevated;

    let qualified = match descriptor.namespace {
        Some(ns) => format!("{ns}::{}", descriptor.name),
        None => descriptor.name.to_string(),
    };

    let doc_raw = descriptor.doc.unwrap_or("(no docs)");
    let first_line = doc_raw.lines().next().unwrap_or("(no docs)");
    let doc_display: String = if first_line.chars().count() > 80 {
        let truncated: String = first_line.chars().take(80).collect();
        format!("{truncated}\u{2026}")
    } else {
        first_line.to_owned()
    };

    let header = row![
        kind_badge(descriptor.kind, palette),
        Space::new().width(spf(Spacing::Xs)),
        text(qualified).size(FONT_XS).color(text_primary),
        Space::new().width(Length::Fill),
        text(descriptor.return_type).size(FONT_XS).color(text_muted),
    ]
    .align_y(Alignment::Center);

    let sig = text(format_signature(descriptor))
        .size(FONT_XS)
        .color(text_primary)
        .font(font(FontRole::Monospace));

    let doc = text(doc_display).size(FONT_XS).color(text_muted);

    container(column![header, sig, doc].spacing(spf(Spacing::Xxs)))
        .max_width(400.0)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use forge_script::{ParamDescriptor, catalog};

    #[test]
    fn format_signature_function_with_params() {
        let d = MethodDescriptor {
            namespace: Some("globals"),
            name: "get",
            kind: SymbolKind::Fn,
            params: &[ParamDescriptor {
                name: "key",
                ty: "string",
            }],
            return_type: "Variant",
            doc: None,
        };
        assert_eq!(format_signature(&d), "get(key: string) -> Variant");
    }

    #[test]
    fn format_signature_function_no_params() {
        let d = MethodDescriptor {
            namespace: Some("time"),
            name: "now",
            kind: SymbolKind::Fn,
            params: &[],
            return_type: "Datetime",
            doc: None,
        };
        assert_eq!(format_signature(&d), "now() -> Datetime");
    }

    #[test]
    fn format_signature_property_no_parens() {
        let d = MethodDescriptor {
            namespace: None,
            name: "len",
            kind: SymbolKind::Property,
            params: &[],
            return_type: "Int",
            doc: None,
        };
        assert_eq!(format_signature(&d), "len -> Int");
    }

    #[test]
    fn hover_popover_smoke_no_panic() {
        let entries = catalog();
        let d = entries
            .iter()
            .find(|d| d.namespace == Some("globals") && d.name == "get")
            .unwrap();
        let _: iced::Element<'_, u32> = hover_popover(d, &CATPPUCCIN_MOCHA);
    }
}
