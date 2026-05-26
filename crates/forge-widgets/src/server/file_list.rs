use iced::{
    Alignment, Border, Color, Element, Length,
    widget::button::{Status, Style},
    widget::{Column, Space, button, column, container, row, scrollable, text},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMime {
    Html,
    Css,
    Js,
    Json,
    Image,
    Wasm,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    File { mime: FileMime },
    Dir,
}

pub struct OverlayEntry<'a> {
    pub name: &'a str,
    pub kind: OverlayKind,
    pub size_bytes: Option<u64>,
    pub child_count: Option<usize>,
}

pub struct OverlayFileListParams<'a> {
    pub root_path: &'a str,
    pub entries: &'a [OverlayEntry<'a>],
    pub bind_address: &'a str,
    pub selected_for_url: Option<&'a str>,
}

fn browser_url(bind_address: &str, file_name: &str) -> String {
    format!("http://{bind_address}/{file_name}")
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * 1_024;
    const GB: u64 = 1_024 * 1_024 * 1_024;
    if bytes == 0 {
        "0 B".to_owned()
    } else if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

fn entry_icon_and_color(kind: OverlayKind, p: ForgePalette) -> (Icon, Color) {
    match kind {
        OverlayKind::File {
            mime: FileMime::Html,
        } => (Icon::FileCode, p.bits),
        OverlayKind::File {
            mime: FileMime::Css,
        } => (Icon::FileCode, p.brand),
        OverlayKind::File { mime: FileMime::Js } => (Icon::FileCode, p.brand),
        OverlayKind::File {
            mime: FileMime::Json,
        } => (Icon::FileCode, p.success),
        OverlayKind::File {
            mime: FileMime::Image,
        } => (Icon::Photo, p.info),
        OverlayKind::File {
            mime: FileMime::Wasm,
        } => (Icon::FileCode, p.text_muted),
        OverlayKind::File {
            mime: FileMime::Other,
        } => (Icon::FileCode, p.text_muted),
        OverlayKind::Dir => (Icon::Folder, p.warning),
    }
}

fn right_label_for_entry(entry: &OverlayEntry<'_>) -> String {
    match entry.kind {
        OverlayKind::Dir => entry
            .child_count
            .map(|n| format!("{n} files"))
            .unwrap_or_default(),
        OverlayKind::File { .. } => entry.size_bytes.map(format_size).unwrap_or_default(),
    }
}

fn entry_row_style(is_selected: bool, p: ForgePalette) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| {
        let bg = if is_selected {
            Some(iced::Background::Color(p.surface_overlay))
        } else {
            match status {
                Status::Hovered => Some(iced::Background::Color(Color { a: 0.04, ..p.brand })),
                _ => None,
            }
        };
        Style {
            background: bg,
            text_color: p.text_primary,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

pub fn overlay_file_list<'a, Msg: Clone + 'a>(
    params: OverlayFileListParams<'a>,
    on_open_folder: Msg,
    on_copy_url: impl Fn(&'a str) -> Msg + 'a,
    on_select_file: impl Fn(usize) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;

    let effective_selected: Option<&'a str> = params.selected_for_url.or_else(|| {
        params
            .entries
            .iter()
            .find(|e| {
                matches!(
                    e.kind,
                    OverlayKind::File {
                        mime: FileMime::Html
                    }
                )
            })
            .map(|e| e.name)
    });

    let header_row = row![
        row![
            tabler_icon(Icon::Folder, 14.0, p.warning),
            text("Overlay host root")
                .size(FONT_SM)
                .font(iced::Font {
                    weight: iced::font::Weight::Medium,
                    ..font(FontRole::Body)
                })
                .color(p.text_primary),
        ]
        .spacing(7)
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill),
        tabler_icon(Icon::ExternalLink, 13.0, p.text_faint),
    ]
    .align_y(Alignment::Center)
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let path_label = text("PATH")
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let path_box = container(
        text(params.root_path)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_primary),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.shell)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let folder_open_btn = button(tabler_icon(Icon::FolderOpen, 13.0, p.text_secondary))
        .on_press(on_open_folder)
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
        .style(super::outline_btn_style(
            p.border_regular,
            p.text_secondary,
            p.text_primary,
        ));

    let path_row = row![path_box, folder_open_btn]
        .spacing(6)
        .align_y(Alignment::Center);

    let path_group: Element<'a, Msg> = column![path_label, path_row].spacing(5).into();

    let entry_count = params.entries.len();
    let files_label_row = row![
        text("FILES")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_muted),
        text(format!(" {entry_count}"))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_faint),
    ]
    .align_y(Alignment::Center);

    let last_idx = params.entries.len().saturating_sub(1);
    let mut file_row_els: Vec<Element<'a, Msg>> = Vec::with_capacity(params.entries.len());

    for (i, entry) in params.entries.iter().enumerate() {
        let (entry_icon, icon_color) = entry_icon_and_color(entry.kind, p);
        let is_selected = effective_selected == Some(entry.name);
        let is_last = i == last_idx;
        let right = right_label_for_entry(entry);

        let row_content = row![
            tabler_icon(entry_icon, 12.0, icon_color),
            text(entry.name)
                .font(font(FontRole::Monospace))
                .size(FONT_SM)
                .color(p.text_primary)
                .width(Length::Fill),
            text(right)
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(p.text_faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([sp(Spacing::Xxs), 0]);

        let row_el: Element<'a, Msg> = if matches!(entry.kind, OverlayKind::File { .. }) {
            let btn = button(row_content)
                .on_press(on_select_file(i))
                .padding(0)
                .width(Length::Fill)
                .style(entry_row_style(is_selected, p));
            if is_last {
                btn.into()
            } else {
                column![btn, super::section_divider::<Msg>(p.border_regular)]
                    .spacing(0)
                    .into()
            }
        } else {
            let plain = container(row_content).width(Length::Fill);
            if is_last {
                plain.into()
            } else {
                column![plain, super::section_divider::<Msg>(p.border_regular)]
                    .spacing(0)
                    .into()
            }
        };
        file_row_els.push(row_el);
    }

    let file_list_col = Column::with_children(file_row_els).spacing(0);

    let list_content: Element<'a, Msg> = if entry_count > 5 {
        scrollable(file_list_col)
            .height(Length::Fixed(110.0))
            .into()
    } else {
        file_list_col.into()
    };

    let files_group: Element<'a, Msg> = column![files_label_row, list_content].spacing(5).into();

    let url_display = effective_selected
        .map(|name| browser_url(params.bind_address, name))
        .unwrap_or_else(|| format!("http://{}/", params.bind_address));

    let copy_icon_el = tabler_icon(Icon::Copy, 12.0, p.text_faint);

    let copy_btn: Element<'a, Msg> = if let Some(name) = effective_selected {
        button(copy_icon_el)
            .on_press(on_copy_url(name))
            .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
            .style(super::ghost_icon_style(p.text_faint, p.text_secondary))
            .into()
    } else {
        container(copy_icon_el).padding([2u16, 4u16]).into()
    };

    let url_box = container(
        row![
            text(url_display)
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(p.info)
                .width(Length::Fill),
            copy_btn,
        ]
        .align_y(Alignment::Center)
        .spacing(6),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.shell)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let url_label = text("BROWSER SOURCE URL")
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let url_group: Element<'a, Msg> = column![
        super::section_divider::<Msg>(p.border_regular),
        column![url_label, url_box].spacing(5),
    ]
    .spacing(10)
    .into();

    let body_col: Element<'a, Msg> = column![path_group, files_group, url_group]
        .spacing(10)
        .into();

    let body = container(body_col)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .width(Length::Fill);

    let card_content = column![
        header_row,
        super::section_divider::<Msg>(p.border_regular),
        body
    ]
    .spacing(0);

    container(card_content)
        .width(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    fn make_entries() -> [OverlayEntry<'static>; 4] {
        [
            OverlayEntry {
                name: "alerts.html",
                kind: OverlayKind::File {
                    mime: FileMime::Html,
                },
                size_bytes: Some(4_301),
                child_count: None,
            },
            OverlayEntry {
                name: "chat.html",
                kind: OverlayKind::File {
                    mime: FileMime::Html,
                },
                size_bytes: Some(2_867),
                child_count: None,
            },
            OverlayEntry {
                name: "logo.png",
                kind: OverlayKind::File {
                    mime: FileMime::Image,
                },
                size_bytes: Some(18_432),
                child_count: None,
            },
            OverlayEntry {
                name: "assets",
                kind: OverlayKind::Dir,
                size_bytes: None,
                child_count: Some(12),
            },
        ]
    }

    #[test]
    fn smoke_overlay_file_list_three_files_one_dir() {
        let entries = make_entries();
        let params = OverlayFileListParams {
            root_path: "~/.local/share/forge/overlays",
            entries: &entries,
            bind_address: "127.0.0.1:8081",
            selected_for_url: Some("alerts.html"),
        };
        let _: Element<'_, usize> =
            overlay_file_list(params, 0usize, |_name| 1usize, |idx| idx, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_overlay_file_list_no_selection_falls_back_to_first_html() {
        let entries = make_entries();
        let params = OverlayFileListParams {
            root_path: "~/.local/share/forge/overlays",
            entries: &entries,
            bind_address: "127.0.0.1:8081",
            selected_for_url: None,
        };
        let _: Element<'_, usize> =
            overlay_file_list(params, 0usize, |_name| 1usize, |idx| idx, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn format_size_zero_is_zero_bytes() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn format_size_4288_is_4_point_2_kb() {
        assert_eq!(format_size(4_288), "4.2 KB");
    }

    #[test]
    fn format_size_1_500_000_is_1_point_4_mb() {
        assert_eq!(format_size(1_500_000), "1.4 MB");
    }

    #[test]
    fn browser_url_formats_correctly() {
        assert_eq!(
            browser_url("127.0.0.1:8081", "alerts.html"),
            "http://127.0.0.1:8081/alerts.html"
        );
    }
}
