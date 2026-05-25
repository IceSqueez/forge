use std::collections::{HashSet, VecDeque};

use forge_widgets::{
    BadgeKind, ChatRow, ForgePalette, search_input,
    tokens::{Spacing, sp, spf},
};
use iced::{Color, Element, Length};
use time::OffsetDateTime;

use crate::Message;
use crate::live_chat::LiveChatState;
use crate::message::LiveChatMsg;
use crate::viewers::ViewersState;

struct ViewerSummary {
    username: String,
    role: Option<BadgeKind>,
    message_count: u64,
    last_seen_label: String,
    avatar_letter: char,
    avatar_color: Color,
    watch_time: String,
    sub: String,
    follow: String,
}

fn drawer_matches(username: &str, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    username.to_ascii_lowercase().contains(search)
}

fn synthesize_from_chat(
    username: &str,
    chat_log: &VecDeque<ChatRow>,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let count = chat_log.iter().filter(|r| r.username == username).count();
    if count == 0 {
        return None;
    }
    let last_entry = chat_log.iter().rev().find(|r| r.username == username)?;
    let role = last_entry.badges.first().copied();
    let avatar_letter = username
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?');
    let avatar_color = viewer_hash_color(username, palette);

    let sub = if role == Some(BadgeKind::Broadcaster) {
        "\u{221e}".into()
    } else if role == Some(BadgeKind::Subscriber) {
        "Yes".into()
    } else {
        "No".into()
    };

    Some(ViewerSummary {
        username: username.to_owned(),
        role,
        message_count: count as u64,
        last_seen_label: "now".into(),
        avatar_letter,
        avatar_color,
        watch_time: "0m".into(),
        sub,
        follow: "Yes".into(),
    })
}

fn enrich_with_storage(mut summary: ViewerSummary, viewers: &ViewersState) -> ViewerSummary {
    if let Some(v) = viewers
        .viewers
        .iter()
        .find(|v| v.username == summary.username)
    {
        summary.message_count = v.message_count;
        summary.last_seen_label = viewer_last_seen(v.last_seen_at);

        let duration = v.last_seen_at - v.first_seen_at;
        let sec_count_based = v.message_count * 90;
        let seconds = duration.whole_seconds().max(sec_count_based as i64).max(0);

        summary.watch_time = if seconds < 60 {
            format!("{}s", seconds)
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
        };

        let age = OffsetDateTime::now_utc() - v.first_seen_at;
        let days = age.whole_days().max(0);
        summary.follow = if v.message_count == 0 {
            "No".into()
        } else if days >= 365 {
            format!("{}y", days / 365)
        } else if days >= 30 {
            format!("{}mo", days / 30)
        } else if days >= 1 {
            format!("{}d", days)
        } else {
            let hours = age.whole_hours().max(0);
            if hours >= 1 {
                format!("{}h", hours)
            } else {
                let mins = age.whole_minutes().max(1);
                format!("{}m", mins)
            }
        };

        let is_broadcaster = summary.role == Some(BadgeKind::Broadcaster);
        let is_subscriber = summary.role == Some(BadgeKind::Subscriber);
        summary.sub = if is_broadcaster {
            "\u{221e}".into()
        } else if is_subscriber {
            let age_months = (days / 30).max(1);
            format!("{}mo", age_months)
        } else {
            "No".into()
        };
    }
    summary
}

fn effective_summary(
    state: &LiveChatState,
    viewers: &ViewersState,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    if let Some(sel) = state.selected_viewer.as_deref()
        && let Some(s) = synthesize_from_chat(sel, &state.chat_log, palette)
    {
        return Some(enrich_with_storage(s, viewers));
    }
    let last = state.chat_log.back()?.username.as_str();
    synthesize_from_chat(last, &state.chat_log, palette).map(|s| enrich_with_storage(s, viewers))
}

fn viewer_hash_color(username: &str, palette: &ForgePalette) -> Color {
    let idx = username
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(u32::from(b))) as usize
        % 8;
    [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
        palette.accent_pink_light,
        palette.accent_teal,
    ][idx]
}

fn viewer_last_seen(dt: OffsetDateTime) -> String {
    let secs = (OffsetDateTime::now_utc() - dt).whole_seconds().max(0);
    if secs < 5 {
        "now".into()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

fn viewer_avatar<'a, Msg: 'a>(
    letter: char,
    color: Color,
    size: f32,
    border_radius: forge_widgets::tokens::Radius,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::tokens::radius;
    use forge_widgets::{FontRole, font};
    use iced::widget::{container, text};
    use iced::{Background, Border};

    let p = *palette;
    let r = radius(border_radius);
    container(
        text(letter.to_string())
            .font(font(FontRole::Body))
            .size(size * 0.45)
            .color(p.shell),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Length::Fixed(size))
    .center_y(Length::Fixed(size))
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: r.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn viewer_stat<'a, Msg: 'a>(
    label: &str,
    value: &str,
    color: Color,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_XS, Radius, radius},
    };
    use iced::widget::{column, container, text};
    use iced::{Background, Border};

    let l = label.to_owned();
    let v = value.to_owned();
    let p = *palette;

    let label_el = text(l)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let value_el = text(v)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(color);

    container(column![label_el, value_el].spacing(spf(Spacing::Xxs)))
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn drawer_role_badge<'a>(kind: BadgeKind, palette: &ForgePalette) -> Element<'a, Message> {
    use forge_widgets::tokens::FONT_XS;
    use forge_widgets::{FontRole, font};
    use iced::widget::{container, text};
    use iced::{Background, Border};

    let (label, text_color) = match kind {
        BadgeKind::Broadcaster => ("LIVE", palette.warning),
        BadgeKind::Moderator => ("MOD", palette.success),
        BadgeKind::Vip => ("VIP", palette.brand),
        BadgeKind::Subscriber => ("SUB", palette.info),
        BadgeKind::Bot => ("BOT", palette.text_muted),
    };
    let p = *palette;
    container(
        text(label)
            .size(FONT_XS)
            .color(text_color)
            .font(font(FontRole::Body)),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 3.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn section_sep<'a, Msg: 'a>(palette: &'a ForgePalette) -> Element<'a, Msg> {
    use iced::Background;
    use iced::widget::{Space, container};

    let p = *palette;
    container(Space::new())
        .height(0.5_f32)
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        })
        .into()
}

fn drawer_header<'a>(state: &'a LiveChatState, palette: &'a ForgePalette) -> Element<'a, Message> {
    use forge_widgets::{FontRole, font, tokens::FONT_XS};
    use iced::widget::{column, container, text};
    use iced::{Background, Border, Length};

    let p = *palette;

    let mut seen = HashSet::new();
    let unique: Vec<&str> = state
        .chat_log
        .iter()
        .rev()
        .filter_map(|r| {
            if seen.insert(r.username.as_str()) {
                Some(r.username.as_str())
            } else {
                None
            }
        })
        .collect();
    let total_count = unique.len();
    let search_lower = state.drawer_search.to_ascii_lowercase();
    let shown_count = unique
        .iter()
        .filter(|u| drawer_matches(u, &search_lower))
        .count();

    let count_label = format!("{total_count} active \u{b7} {shown_count} shown");

    let count_row = text(count_label)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_faint);

    let search_box = search_input(
        "Search viewers...",
        &state.drawer_search,
        |s| Message::LiveChat(LiveChatMsg::DrawerSearchChanged(s)),
        palette,
    );

    let header_content = column![count_row, search_box].spacing(spf(Spacing::Xs));

    let body = container(header_content)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    iced::widget::column![body, section_sep::<Message>(palette)]
        .spacing(0)
        .into()
}

fn selected_viewer_detail<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, MenuItem, MenuPlacement, font,
        icons::Icon,
        menu_button, tabler_icon,
        tokens::{FONT_SM, FONT_XS, Radius, radius},
    };
    use iced::widget::{Space, button, column, container, row, text};
    use iced::{Background, Border};

    let p = *palette;

    let sep = section_sep::<Message>(palette);

    let Some(summary) = effective_summary(state, viewers, palette) else {
        let placeholder = container(
            text("Click a username in chat to see details")
                .font(font(FontRole::Body))
                .size(FONT_XS)
                .color(p.text_faint),
        )
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .width(Length::Fill);
        return column![placeholder, sep].spacing(0).into();
    };

    let avatar_el = viewer_avatar::<Message>(
        summary.avatar_letter,
        summary.avatar_color,
        38.0,
        Radius::Md,
        palette,
    );

    let name_el = text(summary.username.clone())
        .font(font(FontRole::Body))
        .size(FONT_SM)
        .color(p.text_primary);

    let last_label = format!("Last seen {}", summary.last_seen_label);
    let last_el = text(last_label)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let mut name_row_items: Vec<Element<'a, Message>> = vec![name_el.into()];
    if let Some(kind) = summary.role {
        name_row_items.push(drawer_role_badge(kind, palette));
    }
    let name_row = row(name_row_items)
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    let name_col = column![name_row, last_el].spacing(spf(Spacing::Xxs));

    let info_row = row![avatar_el, name_col]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    let msg_str = format!("{}", summary.message_count);

    let sub_color = if summary.sub == "No" {
        p.text_faint
    } else {
        p.text_primary
    };
    let follow_color = if summary.follow == "No" {
        p.text_faint
    } else {
        p.text_primary
    };
    let watch_color = if summary.watch_time == "0m" || summary.watch_time == "\u{2014}" {
        p.text_faint
    } else {
        p.text_primary
    };

    let stat_grid = column![
        row![
            viewer_stat("WATCH TIME", &summary.watch_time, watch_color, palette),
            viewer_stat("MESSAGES", &msg_str, p.text_primary, palette),
        ]
        .spacing(spf(Spacing::Xs)),
        row![
            viewer_stat("SUB", &summary.sub, sub_color, palette),
            viewer_stat("FOLLOW", &summary.follow, follow_color, palette),
        ]
        .spacing(spf(Spacing::Xs)),
    ]
    .spacing(spf(Spacing::Xs));

    let btn_style = move |_t: &iced::Theme, s: button::Status| {
        let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: if hovered {
                    p.border_input
                } else {
                    p.border_regular
                },
                width: 0.5,
                radius: radius(Radius::Sm).into(),
            },
            text_color: if hovered {
                p.text_primary
            } else {
                p.text_secondary
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    };

    let shoutout_btn = button(
        container(
            row![
                tabler_icon(Icon::Bolt, 11.0, p.text_muted),
                text("Shoutout").font(font(FontRole::Body)).size(FONT_XS),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(iced::Alignment::Center),
    )
    .on_press(Message::Noop)
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .style(btn_style);

    let whisper_btn = button(
        container(
            row![
                tabler_icon(Icon::MessageCircle, 11.0, p.text_muted),
                text("Whisper").font(font(FontRole::Body)).size(FONT_XS),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(iced::Alignment::Center),
    )
    .on_press(Message::Noop)
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .style(btn_style);

    let menu_items: Vec<MenuItem<Message>> = vec![
        MenuItem::Item {
            label: "Shoutout".into(),
            on_press: Message::Noop,
            icon: Some(Icon::Flag),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: "Whisper".into(),
            on_press: Message::Noop,
            icon: Some(Icon::MessageCircle),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: "Set TTS voice\u{2026}".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: "Block from TTS".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.warning),
            disabled: false,
        },
        MenuItem::Item {
            label: "Timeout 10 min".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.warning),
            disabled: false,
        },
        MenuItem::Item {
            label: "Ban from channel".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.random),
            disabled: false,
        },
    ];

    let more_btn = menu_button(
        Icon::DotsVertical,
        state.drawer_menu_open,
        Message::LiveChat(LiveChatMsg::DrawerMenuToggle),
        Message::LiveChat(LiveChatMsg::DrawerMenuDismiss),
        menu_items,
        MenuPlacement::TopRight,
        palette,
    );

    let actions_row = row![shoutout_btn, whisper_btn, more_btn].spacing(spf(Spacing::Xs));

    let detail_content = column![info_row, stat_grid, actions_row].spacing(spf(Spacing::Xs));

    let detail_box = container(detail_content)
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    column![detail_box, Space::new().height(0.0_f32), sep,]
        .spacing(0)
        .into()
}

fn drawer_viewer_row<'a>(
    summary: ViewerSummary,
    is_sel: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_SM, FONT_XS, Radius},
    };
    use iced::widget::{Space, button, column, container, row, text};
    use iced::{Background, Border};

    let p = *palette;
    let stripe_color = if is_sel { p.brand } else { Color::TRANSPARENT };
    let selected_bg = p.elevated;
    let hover_bg = p.base;

    let stripe = container(Space::new().width(2).height(Length::Fill))
        .width(2)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(stripe_color)),
            ..container::Style::default()
        });

    let avatar_el = viewer_avatar::<Message>(
        summary.avatar_letter,
        summary.avatar_color,
        22.0,
        Radius::Sm,
        palette,
    );

    let name_el = text(summary.username.clone())
        .font(font(FontRole::Body))
        .size(FONT_SM)
        .color(p.text_primary);

    let mut name_row_items: Vec<Element<'a, Message>> = vec![name_el.into()];
    if let Some(kind) = summary.role {
        name_row_items.push(drawer_role_badge(kind, palette));
    }
    let name_row = row(name_row_items)
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::Alignment::Center);

    let meta_el = text(format!(
        "{} \u{b7} {} msg",
        summary.watch_time, summary.message_count
    ))
    .font(font(FontRole::Monospace))
    .size(FONT_XS)
    .color(p.text_muted);

    let name_col = column![name_row, meta_el].spacing(spf(Spacing::Xxs));

    let last_el = text(summary.last_seen_label.clone())
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_faint);

    let row_content = row![
        avatar_el,
        name_col,
        Space::new().width(Length::Fill),
        last_el,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::Alignment::Center);

    let username = summary.username.clone();
    let row_btn = button(row_content)
        .on_press(Message::LiveChat(LiveChatMsg::DrawerSelectViewer(username)))
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme, s: button::Status| {
            let bg = if is_sel {
                selected_bg
            } else if matches!(s, button::Status::Hovered | button::Status::Pressed) {
                hover_bg
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                text_color: p.text_primary,
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    row![stripe, row_btn].spacing(0).into()
}

fn viewer_list<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::tokens::FONT_XS;
    use forge_widgets::{FontRole, font};
    use iced::Background;
    use iced::widget::{Space, column, container, scrollable, text};

    let p = *palette;

    let search_lower = state.drawer_search.to_ascii_lowercase();

    let mut seen: HashSet<&str> = HashSet::new();
    let unique_usernames: Vec<&str> = state
        .chat_log
        .iter()
        .rev()
        .filter_map(|r| {
            if seen.insert(r.username.as_str()) {
                Some(r.username.as_str())
            } else {
                None
            }
        })
        .filter(|u| drawer_matches(u, &search_lower))
        .collect();

    let section_label = format!("ACTIVE NOW \u{b7} {}", unique_usernames.len());

    let section_header = container(
        text(section_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_faint),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        ..container::Style::default()
    });

    let selected_name = state.selected_viewer.as_deref();

    let list_items: Vec<Element<'a, Message>> = unique_usernames
        .iter()
        .filter_map(|username| {
            synthesize_from_chat(username, &state.chat_log, palette)
                .map(|s| enrich_with_storage(s, viewers))
        })
        .map(|summary| {
            let is_sel = selected_name == Some(summary.username.as_str());
            drawer_viewer_row(summary, is_sel, palette)
        })
        .collect();

    let list_col = if list_items.is_empty() {
        column![
            Space::new().height(8.0_f32),
            container(
                text("No chat participants match the search")
                    .font(font(FontRole::Body))
                    .size(FONT_XS)
                    .color(p.text_faint),
            )
            .padding([0, sp(Spacing::Sm)]),
        ]
        .spacing(0)
    } else {
        iced::widget::column(list_items).spacing(0)
    };

    column![section_header, scrollable(list_col).height(Length::Fill)]
        .height(Length::Fill)
        .into()
}

pub(crate) fn drawer_panel<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    iced::widget::column![
        drawer_header(state, palette),
        selected_viewer_detail(state, viewers, palette),
        viewer_list(state, viewers, palette),
    ]
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_storage::{Viewer, ViewerPlatform};
    use forge_widgets::{ChatBody, Platform};

    fn make_viewer(username: &str, msg_count: u64) -> Viewer {
        Viewer {
            viewer_id: username.to_owned(),
            platform: ViewerPlatform::Twitch,
            username: username.to_owned(),
            first_seen_at: OffsetDateTime::now_utc(),
            last_seen_at: OffsetDateTime::now_utc(),
            message_count: msg_count,
            custom_greeting: false,
        }
    }

    fn make_viewers_state(usernames: &[&str]) -> ViewersState {
        ViewersState {
            viewers: usernames.iter().map(|u| make_viewer(u, 0)).collect(),
        }
    }

    #[test]
    fn summary_role_is_latest_badge_from_chat() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            seq: 0,
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "danylo_ua".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("first".into()),
        });
        log.push_back(ChatRow {
            seq: 0,
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Vip],
            username: "danylo_ua".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("second".into()),
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("danylo_ua", &log, &palette).unwrap();
        assert_eq!(summary.role, Some(BadgeKind::Vip));
    }

    #[test]
    fn summary_none_when_username_absent_from_chat() {
        let log = VecDeque::new();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        assert!(synthesize_from_chat("ghost_user", &log, &palette).is_none());
    }

    #[test]
    fn viewer_hash_color_is_deterministic() {
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let c1 = viewer_hash_color("danylo_ua", &palette);
        let c2 = viewer_hash_color("danylo_ua", &palette);
        assert_eq!(c1, c2);
    }

    #[test]
    fn effective_summary_returns_selected_when_in_chat() {
        let mut state = LiveChatState::new();
        state.selected_viewer = Some("haash_".to_owned());
        let vs = make_viewers_state(&[]);
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let result = effective_summary(&state, &vs, &palette);
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "haash_");
    }

    #[test]
    fn effective_summary_falls_back_to_last_chat_entry_when_selected_missing() {
        let mut state = LiveChatState::new();
        state.selected_viewer = Some("ghost_not_in_chat".to_owned());
        let vs = make_viewers_state(&["alice", "bob"]);
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let result = effective_summary(&state, &vs, &palette);
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "factorio_streamer");
    }

    #[test]
    fn synthesize_from_chat_counts_occurrences_and_uses_latest_badge() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            seq: 0,
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "alice".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("first".into()),
        });
        log.push_back(ChatRow {
            seq: 0,
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Vip],
            username: "alice".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("second".into()),
        });
        log.push_back(ChatRow {
            seq: 0,
            timestamp: "00:00:02".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "bob".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("hi".into()),
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("alice", &log, &palette).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.role, Some(BadgeKind::Vip));
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn synthesize_from_chat_returns_none_when_username_absent() {
        let log = VecDeque::new();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        assert!(synthesize_from_chat("nobody", &log, &palette).is_none());
    }

    #[test]
    fn enrich_with_storage_overwrites_message_count_when_viewer_present() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            seq: 0,
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "alice".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("hi".into()),
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("alice", &log, &palette).unwrap();
        assert_eq!(summary.message_count, 1);
        let vs = make_viewers_state(&["alice"]);
        let enriched = enrich_with_storage(summary, &vs);
        assert_eq!(enriched.message_count, 0);
    }

    #[test]
    fn drawer_search_filter_case_insensitive() {
        let usernames = ["Alice", "Bob", "alicetv"];
        let matches: Vec<&&str> = usernames
            .iter()
            .filter(|u| drawer_matches(u, "alice"))
            .collect();
        assert_eq!(matches.len(), 2);
    }
}
