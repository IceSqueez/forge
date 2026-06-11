use forge_script::{MethodDescriptor, catalog};
use forge_widgets::tokens::{FONT_SM, FONT_XS, FontRole, Spacing, font, spf};
use forge_widgets::{ForgePalette, filter_candidates};
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length};

use crate::Message;
use crate::script_editor::{ScriptEditorMsg, ScriptEditorState};

pub fn scripting_api_docs_view<'a>(
    state: &'a ScriptEditorState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let search_bar = text_input("Search by name or namespace…", &state.api_docs_search)
        .on_input(|q| Message::ScriptEditor(ScriptEditorMsg::ApiDocsSearchChanged(q)))
        .padding([spf(Spacing::Xs), spf(Spacing::Sm)])
        .size(FONT_SM)
        .style(
            move |_: &iced::Theme, _status| iced::widget::text_input::Style {
                background: Background::Color(palette.elevated),
                border: Border {
                    color: palette.border_input,
                    width: 0.5,
                    radius: 6.0.into(),
                },
                icon: palette.text_faint,
                placeholder: palette.text_extreme_faint,
                value: palette.text_primary,
                selection: iced::Color {
                    a: 0.2,
                    ..palette.brand
                },
            },
        );

    let candidates: Vec<&'static MethodDescriptor> =
        filter_candidates(catalog(), &state.api_docs_search);

    let mut list = column![].spacing(spf(Spacing::Xxs));
    let mut current_ns: Option<Option<&'static str>> = None;

    for entry in &candidates {
        let ns = entry.namespace;
        if current_ns != Some(ns) {
            current_ns = Some(ns);
            let ns_label = ns.unwrap_or("(root)");
            let header = text(ns_label.to_uppercase())
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(font(FontRole::Monospace));
            list = list.push(container(header).padding(iced::Padding {
                top: spf(Spacing::Sm),
                right: 0.0,
                bottom: spf(Spacing::Xxs),
                left: 0.0,
            }));
        }

        let kind_badge = {
            use iced::Shadow;
            use iced::widget::button;
            let shell_color = palette.shell;
            button(
                text("fn")
                    .size(FONT_XS)
                    .color(shell_color)
                    .font(font(FontRole::Monospace)),
            )
            .padding([spf(Spacing::Xxs), spf(Spacing::Xxs)])
            .style(move |_: &iced::Theme, _status| button::Style {
                background: Some(Background::Color(palette.brand)),
                border: Border {
                    radius: 2.0.into(),
                    ..Border::default()
                },
                text_color: shell_color,
                shadow: Shadow::default(),
                snap: false,
            })
        };

        let param_str = entry
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect::<Vec<_>>()
            .join(", ");
        let sig = format!("{}({}) → {}", entry.name, param_str, entry.return_type);

        let sig_text = text(sig)
            .size(FONT_XS)
            .color(palette.text_primary)
            .font(font(FontRole::Monospace));

        let mut entry_col = column![
            row![kind_badge, sig_text]
                .spacing(spf(Spacing::Xs))
                .align_y(Alignment::Center)
        ]
        .spacing(spf(Spacing::Xxs));

        if let Some(doc) = entry.doc {
            let doc_text = text(doc).size(FONT_XS).color(palette.text_muted);
            entry_col = entry_col.push(container(doc_text).padding([0.0, spf(Spacing::Lg)]));
        }

        list = list.push(container(entry_col).padding([spf(Spacing::Xxs), spf(Spacing::Xs)]));
    }

    if candidates.is_empty() {
        let empty = text(forge_widgets::tr!("script_editor_api_no_matches"))
            .size(FONT_SM)
            .color(palette.text_muted);
        list = list.push(container(empty).padding([spf(Spacing::Md), 0.0]));
    }

    let scroll = scrollable(
        container(list)
            .padding([0.0, spf(Spacing::Sm)])
            .width(Length::Fill),
    )
    .height(Length::Fill);

    column![
        container(search_bar)
            .padding([spf(Spacing::Sm), spf(Spacing::Md)])
            .width(Length::Fill),
        scroll,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
