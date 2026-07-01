use std::collections::{HashSet, VecDeque};

use forge_types::{UnifiedChatRow, UserBadge};
use forge_widgets::{
    BadgeKind, ForgePalette, search_input,
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

fn unified_badge_to_kind(badge: &UserBadge) -> BadgeKind {
    match badge {
        UserBadge::Broadcaster => BadgeKind::Broadcaster,
        UserBadge::Moderator => BadgeKind::Moderator,
        UserBadge::Vip => BadgeKind::Vip,
        UserBadge::Subscriber { .. } => BadgeKind::Subscriber,
        UserBadge::Member { .. } => BadgeKind::Subscriber,
        UserBadge::Bot => BadgeKind::Bot,
    }
}

fn synthesize_from_chat(
    username: &str,
    rows: &VecDeque<UnifiedChatRow>,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let count = rows.iter().filter(|r| r.author == username).count();
    if count == 0 {
        return None;
    }
    let last_entry = rows.iter().rev().find(|r| r.author == username)?;
    let role = last_entry.badges.first().map(unified_badge_to_kind);
    let avatar_letter = username
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?');
    let avatar_color = viewer_hash_color(username, palette);

    let sub = if role == Some(BadgeKind::Broadcaster) {
        "\u{221e}".to_owned()
    } else if role == Some(BadgeKind::Subscriber) {
        "Yes".to_owned()
    } else {
        "\u{2014}".to_owned()
    };

    Some(ViewerSummary {
        username: username.to_owned(),
        role,
        message_count: count as u64,
        last_seen_label: "now".to_owned(),
        avatar_letter,
        avatar_color,
        watch_time: "\u{2014}".to_owned(),
        sub,
        follow: "\u{2014}".to_owned(),
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

        let is_broadcaster = summary.role == Some(BadgeKind::Broadcaster);
        let is_subscriber = summary.role == Some(BadgeKind::Subscriber);
        summary.sub = if is_broadcaster {
            "\u{221e}".into()
        } else if is_subscriber {
            "Yes".into()
        } else {
            "\u{2014}".into()
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
        && let Some(s) = synthesize_from_chat(sel, &state.rows, palette)
    {
        return Some(enrich_with_storage(s, viewers));
    }
    let last = state.rows.back()?.author.as_str();
    synthesize_from_chat(last, &state.rows, palette).map(|s| enrich_with_storage(s, viewers))
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
        .rows
        .iter()
        .rev()
        .filter_map(|r| {
            if seen.insert(r.author.as_str()) {
                Some(r.author.as_str())
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

    let count_label = forge_widgets::tr!(
        "chat_drawer_active_count",
        total = total_count as i64,
        shown = shown_count as i64
    );

    let count_row = text(count_label)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_faint);

    let search_box = search_input(
        forge_widgets::tr!("chat_drawer_search_placeholder"),
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
            text(forge_widgets::tr!("chat_drawer_click_hint"))
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

    let last_label = forge_widgets::tr!(
        "chat_drawer_last_seen",
        when = summary.last_seen_label.as_str()
    );
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

    let sub_color = if summary.sub == "\u{2014}" {
        p.text_faint
    } else {
        p.text_primary
    };
    let follow_color = if summary.follow == "\u{2014}" {
        p.text_faint
    } else {
        p.text_primary
    };
    let watch_color = if summary.watch_time == "\u{2014}" {
        p.text_faint
    } else {
        p.text_primary
    };

    let stat_grid = column![
        row![
            viewer_stat(
                &forge_widgets::tr!("chat_stat_watch_time"),
                &summary.watch_time,
                watch_color,
                palette
            ),
            viewer_stat(
                &forge_widgets::tr!("chat_stat_messages"),
                &msg_str,
                p.text_primary,
                palette
            ),
        ]
        .spacing(spf(Spacing::Xs)),
        row![
            viewer_stat(
                &forge_widgets::tr!("chat_stat_sub"),
                &summary.sub,
                sub_color,
                palette
            ),
            viewer_stat(
                &forge_widgets::tr!("chat_stat_follow"),
                &summary.follow,
                follow_color,
                palette
            ),
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
                text(forge_widgets::tr!("chat_drawer_shoutout"))
                    .font(font(FontRole::Body))
                    .size(FONT_XS),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(iced::Alignment::Center),
    )
    .on_press(Message::LiveChat(LiveChatMsg::ShoutoutViewer))
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .style(btn_style);

    let whisper_btn = button(
        container(
            row![
                tabler_icon(Icon::MessageCircle, 11.0, p.text_muted),
                text(forge_widgets::tr!("chat_drawer_whisper"))
                    .font(font(FontRole::Body))
                    .size(FONT_XS),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(iced::Alignment::Center),
    )
    .on_press(Message::LiveChat(LiveChatMsg::WhisperOpen))
    .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .style(btn_style);

    let menu_items: Vec<MenuItem<Message>> = vec![
        MenuItem::Item {
            label: forge_widgets::tr!("chat_drawer_shoutout"),
            on_press: Message::LiveChat(LiveChatMsg::ShoutoutViewer),
            icon: Some(Icon::Flag),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("chat_drawer_whisper"),
            on_press: Message::LiveChat(LiveChatMsg::WhisperOpen),
            icon: Some(Icon::MessageCircle),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("chat_drawer_set_tts_voice"),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: None,
            disabled: true,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: forge_widgets::tr!("chat_drawer_block_tts"),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.warning),
            disabled: true,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("chat_drawer_timeout"),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.warning),
            disabled: true,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("chat_drawer_ban"),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.random),
            disabled: true,
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
        .rows
        .iter()
        .rev()
        .filter_map(|r| {
            if seen.insert(r.author.as_str()) {
                Some(r.author.as_str())
            } else {
                None
            }
        })
        .filter(|u| drawer_matches(u, &search_lower))
        .collect();

    let section_label = forge_widgets::tr!(
        "chat_drawer_section_active",
        count = unique_usernames.len() as i64
    );

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
            synthesize_from_chat(username, &state.rows, palette)
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
                text(forge_widgets::tr!("chat_drawer_no_matches"))
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

fn whisper_modal<'a>(
    form: &'a crate::live_chat::WhisperForm,
    recipient: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, font,
        tokens::{BORDER_THIN, FONT_SM, Radius, radius},
    };
    use iced::widget::{Space, button, column, container, row, text, text_input};
    use iced::{Background, Border};

    let p = *palette;

    let title_str = forge_widgets::tr!("chat_drawer_whisper_title", recipient = recipient);
    let title_el = text(title_str)
        .font(font(FontRole::Body))
        .size(FONT_SM)
        .color(p.text_primary);

    let input_el = text_input(
        &forge_widgets::tr!("chat_drawer_whisper_placeholder"),
        &form.message,
    )
    .on_input(|s| Message::LiveChat(LiveChatMsg::WhisperMessageChanged(s)))
    .on_submit(Message::LiveChat(LiveChatMsg::WhisperSend))
    .font(font(FontRole::Body))
    .size(FONT_SM)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_t: &iced::Theme, s: text_input::Status| {
        let focused = matches!(s, text_input::Status::Focused { .. });
        text_input::Style {
            background: Background::Color(p.elevated),
            border: Border {
                color: if focused { p.brand } else { p.border_input },
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            icon: p.text_faint,
            placeholder: p.text_faint,
            value: p.text_primary,
            selection: p.brand,
        }
    });

    let is_empty = form.message.trim().is_empty();
    let send_bg = if is_empty { p.surface_overlay } else { p.brand };
    let send_text_color = if is_empty { p.text_faint } else { p.base };

    let send_btn = button(
        text(forge_widgets::tr!("chat_drawer_whisper_send"))
            .font(font(FontRole::Body))
            .size(FONT_SM)
            .color(send_text_color),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_t: &iced::Theme, _s: button::Status| button::Style {
        background: Some(Background::Color(send_bg)),
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: send_text_color,
        shadow: iced::Shadow::default(),
        snap: false,
    });
    let send_btn = if is_empty {
        send_btn
    } else {
        send_btn.on_press(Message::LiveChat(LiveChatMsg::WhisperSend))
    };

    let cancel_btn = button(
        text(forge_widgets::tr!("common_cancel"))
            .font(font(FontRole::Body))
            .size(FONT_SM)
            .color(p.text_secondary),
    )
    .on_press(Message::LiveChat(LiveChatMsg::WhisperCancel))
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_t: &iced::Theme, _s: button::Status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: p.text_secondary,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let btn_row = row![Space::new().width(Length::Fill), cancel_btn, send_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    let card = container(column![title_el, input_el, btn_row].spacing(spf(Spacing::Sm)))
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        });

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::LiveChat(LiveChatMsg::WhisperCancel))
        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
            background: Some(Background::Color(Color {
                a: 0.5,
                ..Color::BLACK
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let modal_layer = container(
        container(card)
            .width(Length::Fill)
            .padding([sp(Spacing::Lg), sp(Spacing::Md)]),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(iced::Alignment::Start);

    iced::widget::stack![backdrop, modal_layer]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(crate) fn drawer_panel<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let base = iced::widget::column![
        drawer_header(state, palette),
        selected_viewer_detail(state, viewers, palette),
        viewer_list(state, viewers, palette),
    ]
    .height(Length::Fill);

    if let Some(form) = &state.whisper_form {
        let recipient = state.selected_viewer.as_deref().unwrap_or("");
        iced::widget::stack![base, whisper_modal(form, recipient, palette)]
            .height(Length::Fill)
            .into()
    } else {
        base.into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::VecDeque;

    use forge_storage::{Viewer, ViewerPlatform};
    use forge_types::{
        ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow, UserBadge,
    };
    use time::OffsetDateTime;

    use super::*;

    fn make_unified_row(
        id: &str,
        source: ChatSource,
        author: &str,
        badges: Vec<UserBadge>,
    ) -> UnifiedChatRow {
        UnifiedChatRow {
            id: id.to_owned(),
            event_id: EventId::new(),
            source,
            received_at: OffsetDateTime::now_utc(),
            author: author.to_owned(),
            author_color: None,
            body_segments: vec![ChatSegment::Text {
                text: "test".to_owned(),
            }],
            badges,
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

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
        let mut rows = VecDeque::new();
        rows.push_back(make_unified_row(
            "r1",
            ChatSource::Twitch,
            "danylo_ua",
            vec![UserBadge::Moderator],
        ));
        rows.push_back(make_unified_row(
            "r2",
            ChatSource::Twitch,
            "danylo_ua",
            vec![UserBadge::Vip],
        ));
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("danylo_ua", &rows, &palette).unwrap();
        assert_eq!(summary.role, Some(BadgeKind::Vip));
    }

    #[test]
    fn summary_none_when_username_absent_from_chat() {
        let rows = VecDeque::new();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        assert!(synthesize_from_chat("ghost_user", &rows, &palette).is_none());
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
        let mut rows = VecDeque::new();
        rows.push_back(make_unified_row(
            "a1",
            ChatSource::Twitch,
            "alice",
            vec![UserBadge::Moderator],
        ));
        rows.push_back(make_unified_row(
            "a2",
            ChatSource::Twitch,
            "alice",
            vec![UserBadge::Vip],
        ));
        rows.push_back(make_unified_row("b1", ChatSource::Twitch, "bob", vec![]));
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("alice", &rows, &palette).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.role, Some(BadgeKind::Vip));
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn synthesize_from_chat_returns_none_when_username_absent() {
        let rows = VecDeque::new();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        assert!(synthesize_from_chat("nobody", &rows, &palette).is_none());
    }

    #[test]
    fn enrich_with_storage_overwrites_message_count_when_viewer_present() {
        let mut rows = VecDeque::new();
        rows.push_back(make_unified_row("x1", ChatSource::Twitch, "alice", vec![]));
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("alice", &rows, &palette).unwrap();
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

    #[test]
    fn unified_badge_to_kind_maps_all_variants() {
        assert_eq!(
            unified_badge_to_kind(&UserBadge::Broadcaster),
            BadgeKind::Broadcaster
        );
        assert_eq!(
            unified_badge_to_kind(&UserBadge::Moderator),
            BadgeKind::Moderator
        );
        assert_eq!(unified_badge_to_kind(&UserBadge::Vip), BadgeKind::Vip);
        assert_eq!(
            unified_badge_to_kind(&UserBadge::Subscriber { months: 3 }),
            BadgeKind::Subscriber
        );
        assert_eq!(
            unified_badge_to_kind(&UserBadge::Member {
                level: "Level 1".into()
            }),
            BadgeKind::Subscriber
        );
        assert_eq!(unified_badge_to_kind(&UserBadge::Bot), BadgeKind::Bot);
    }
}
