use std::collections::{HashMap, HashSet};

use forge_runtime::LiveViewerCount;
use forge_types::{ChatEventDetail, ChatSource, EventId, PlatformId, UnifiedChatRow, UserBadge};
use forge_widgets::{
    BadgeKind, ChatBody, ChatRow, ForgePalette, Icon, Platform, PlatformTarget, tabler_icon,
    tokens::{Radius, Spacing, radius, sp, spf},
};
use iced::{Color, Element, Length};
use time::OffsetDateTime;

use crate::Message;
use crate::live_chat::{EventsFilter, LiveChatState, PlatformFilter, chat_scroll_id};
use crate::message::LiveChatMsg;
use crate::runtime_view::RuntimeView;
use crate::viewers::ViewersState;

/// How many recent bus events are scanned to correlate a chat event with the
/// automation it triggered. An `action.start` is published in the same instant
/// as its triggering event, so both sit adjacent at the head of the ring; older
/// events beyond this window degrade to no badge rather than a false positive.
const TRIGGER_SCAN_WINDOW: usize = 2_048;

pub fn row_match_opacity(row: &UnifiedChatRow, query: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }
    let lower = query.to_ascii_lowercase();
    let body = row.body_text().to_ascii_lowercase();
    let author = row.author.to_ascii_lowercase();
    if body.contains(&lower) || author.contains(&lower) {
        1.0
    } else {
        0.3
    }
}

fn row_matches_platform_filter(row: &UnifiedChatRow, filter: &PlatformFilter) -> bool {
    match filter {
        PlatformFilter::All => true,
        PlatformFilter::Single(id) => chat_source_to_platform_id(row.source) == *id,
        PlatformFilter::Custom(ids) => ids.contains(&chat_source_to_platform_id(row.source)),
    }
}

fn row_matches_events_filter(row: &UnifiedChatRow, filter: EventsFilter) -> bool {
    match filter {
        EventsFilter::All => true,
        EventsFilter::OnlyMessages => !row.is_event,
        EventsFilter::OnlyEvents => row.is_event,
    }
}

fn row_is_bot(row: &UnifiedChatRow) -> bool {
    row.badges.iter().any(|b| matches!(b, UserBadge::Bot))
}

pub fn chat_source_to_platform_id(source: ChatSource) -> PlatformId {
    match source {
        ChatSource::Twitch => PlatformId::Twitch,
        ChatSource::YouTube => PlatformId::YouTube,
        ChatSource::Kick => PlatformId::Kick,
    }
}

fn platform_id_to_widget(id: PlatformId) -> Platform {
    match id {
        PlatformId::Twitch => Platform::Twitch,
        PlatformId::YouTube => Platform::YouTube,
        PlatformId::Kick => Platform::Kick,
    }
}

pub(crate) fn platform_id_to_key(id: PlatformId) -> &'static str {
    match id {
        PlatformId::Twitch => "twitch",
        PlatformId::YouTube => "youtube",
        PlatformId::Kick => "kick",
    }
}

fn unified_badge_to_kind(badge: &UserBadge) -> BadgeKind {
    match badge {
        UserBadge::Broadcaster => BadgeKind::Broadcaster,
        UserBadge::Moderator => BadgeKind::Moderator,
        UserBadge::Vip => BadgeKind::Vip,
        UserBadge::Subscriber { .. } => BadgeKind::Subscriber,
        UserBadge::Member { .. } => BadgeKind::Subscriber,
        UserBadge::Bot => BadgeKind::Bot,
        UserBadge::Partner => BadgeKind::Partner,
        UserBadge::Premium => BadgeKind::Premium,
        UserBadge::Founder => BadgeKind::Founder,
        UserBadge::Turbo => BadgeKind::Turbo,
        UserBadge::HypeTrain => BadgeKind::HypeTrain,
        UserBadge::Bits { .. } => BadgeKind::Bits,
        UserBadge::BitsLeader { .. } => BadgeKind::BitsLeader,
    }
}

fn author_color_to_iced(color: Option<[u8; 3]>, fallback: Color) -> Color {
    match color {
        Some([r, g, b]) => Color::from_rgb8(r, g, b),
        None => fallback,
    }
}

fn format_row_timestamp(dt: OffsetDateTime) -> String {
    let secs = dt.unix_timestamp();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// YouTube membership level names are free text set by the channel owner (e.g.
/// "Level 3", "Superfan"); this extracts a digit run as a best-effort tier
/// number and falls back to 1 when the name carries no number, since `ChatBody`
/// has no string-level field of its own.
fn member_level_to_tier(level: &str) -> u8 {
    level
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(1)
}

fn unified_to_chat_row(
    row: &UnifiedChatRow,
    seq: u64,
    triggered_action: Option<String>,
) -> ChatRow {
    let platform = match row.source {
        ChatSource::Twitch => Platform::Twitch,
        ChatSource::YouTube => Platform::YouTube,
        ChatSource::Kick => Platform::Kick,
    };
    let badges: Vec<BadgeKind> = row.badges.iter().map(unified_badge_to_kind).collect();
    let username_color = author_color_to_iced(row.author_color, Color::from_rgb(0.4, 0.7, 1.0));
    let timestamp = format_row_timestamp(row.received_at);
    let body = match &row.event_detail {
        Some(ChatEventDetail::Subscription {
            tier,
            months,
            message,
        }) => ChatBody::Subscription {
            tier: *tier,
            months: *months,
            message: message.clone(),
            triggered_action,
        },
        Some(ChatEventDetail::Raid { viewer_count }) => ChatBody::Raid {
            viewers: *viewer_count,
            triggered_action,
        },
        Some(ChatEventDetail::SuperChat {
            amount_micros,
            message,
            ..
        }) => ChatBody::Cheer {
            bits: amount_micros / 10_000,
            text: message.clone().unwrap_or_default(),
        },
        Some(ChatEventDetail::NewMember { level }) => ChatBody::Subscription {
            tier: member_level_to_tier(level),
            months: None,
            message: Some(level.clone()).filter(|s| !s.is_empty()),
            triggered_action,
        },
        Some(ChatEventDetail::MemberMilestone { months, message }) => ChatBody::Subscription {
            tier: 1,
            months: Some(*months),
            message: message.clone(),
            triggered_action,
        },
        None => ChatBody::Message(row.display_text()),
    };
    ChatRow {
        seq,
        timestamp,
        platform,
        badges,
        username: row.author.clone(),
        username_color,
        body,
    }
}

fn detail_supports_trigger_badge(detail: &ChatEventDetail) -> bool {
    matches!(
        detail,
        ChatEventDetail::Subscription { .. }
            | ChatEventDetail::Raid { .. }
            | ChatEventDetail::NewMember { .. }
            | ChatEventDetail::MemberMilestone { .. }
    )
}

/// Reverse of the `caused_by` edge: given the chat event ids in `wanted`, finds
/// the automation that fired for each by scanning recent `action.start`
/// observability events whose `caused_by` points back at the chat event, then
/// reads its `action_name`. The bus exposes only a forward `lookup`, so this
/// scans the recent ring once; newest match wins when several actions fired.
fn resolve_triggered_actions(
    rt: &RuntimeView,
    wanted: &HashSet<EventId>,
) -> HashMap<EventId, String> {
    let mut resolved = HashMap::new();
    for ev in rt.bus.recent(TRIGGER_SCAN_WINDOW) {
        if ev.kind != "action.start" {
            continue;
        }
        let Some(cause) = ev.caused_by else {
            continue;
        };
        if !wanted.contains(&cause) || resolved.contains_key(&cause) {
            continue;
        }
        if let Some(name) = ev.payload["action_name"].as_str() {
            resolved.insert(cause, name.to_owned());
        }
    }
    resolved
}

fn select_viewer_msg(name: String) -> Message {
    Message::LiveChat(LiveChatMsg::DrawerSelectViewer(name))
}

pub fn live_chat_view<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    rt: &RuntimeView,
    live_viewers: LiveViewerCount,
    uptime: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let page_header = live_chat_page_header(state, live_viewers, uptime, palette);
    let chat_area = build_chat_area(state, rt, palette);

    let targets: Vec<PlatformTarget<'a, Message>> = state
        .connected_platforms
        .iter()
        .map(|&p| {
            let is_primary = state.primary_send_target == Some(p);
            PlatformTarget {
                platform: platform_id_to_widget(p),
                active: is_primary,
                on_press: Some(Box::new(move || {
                    Message::LiveChat(LiveChatMsg::PrimarySendTargetChanged(p))
                })),
            }
        })
        .collect();

    let send_placeholder = if state.connected_platforms.is_empty() {
        forge_widgets::tr!("chat_send_placeholder_disconnected")
    } else {
        forge_widgets::tr!("chat_send_placeholder_connected")
    };

    let bar = forge_widgets::input_bar(
        palette,
        &state.input_buffer,
        send_placeholder,
        targets,
        |s| Message::LiveChat(LiveChatMsg::InputChanged(s)),
        Message::LiveChat(LiveChatMsg::SendPressed),
        state.emoji_picker_open,
        Message::LiveChat(LiveChatMsg::ToggleEmoji),
    );

    let chat_column = iced::widget::column![chat_area, bar]
        .width(Length::Fill)
        .height(Length::Fill);

    let panel_content = crate::live_chat_drawer::drawer_panel(state, viewers, palette);
    let sheet = forge_widgets::SideSheet::new(panel_content)
        .open(state.drawer_open)
        .palette(palette)
        .width(forge_widgets::SheetWidth::new(
            state.drawer_width.unwrap_or(360.0).clamp(280.0, 560.0),
            280.0,
            560.0,
        ))
        .resizable(true)
        .sheet_key("viewers_drawer")
        .header_icon(Icon::Users, palette.brand)
        .header(forge_widgets::SheetHeader {
            title: std::borrow::Cow::Owned(forge_widgets::tr!("chat_viewers_title")),
            subtitle: None,
            on_close: None,
        })
        .on_close(Message::LiveChat(LiveChatMsg::ToggleDrawer))
        .on_resize(|w| Message::LiveChat(LiveChatMsg::SheetResized(w)));

    let body: Element<'a, Message> = iced::widget::stack![chat_column, sheet].into();

    iced::widget::column![page_header, body]
        .height(Length::Fill)
        .into()
}

fn platform_filter_chip_color(id: PlatformId, palette: &ForgePalette) -> Color {
    platform_id_to_widget(id).color(palette)
}

fn filter_icon_button_style(
    p: ForgePalette,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        iced::widget::button::Style {
            background: None,
            text_color: if hovered {
                p.text_primary
            } else {
                p.text_secondary
            },
            border: iced::Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Sm).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

fn live_chat_page_header<'a>(
    state: &'a LiveChatState,
    live_viewers: LiveViewerCount,
    uptime: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::tokens::FONT_XS;
    use iced::widget::{button, column, container, row, text};
    use iced::{Background, Border};

    let p = *palette;

    let label_all = forge_widgets::tr!("chat_filter_all");
    let chip_all = forge_widgets::filter_chip(
        palette,
        &label_all,
        p.brand,
        state.platform_filter == PlatformFilter::All,
        Message::LiveChat(LiveChatMsg::PlatformFilterChanged(PlatformFilter::All)),
    );

    let mut filter_chips: Vec<Element<'a, Message>> = vec![chip_all];
    for &pid in &state.connected_platforms {
        let color = platform_filter_chip_color(pid, palette);
        let label = match pid {
            PlatformId::Twitch => "Twitch",
            PlatformId::YouTube => "YouTube",
            PlatformId::Kick => "Kick",
        };
        let is_active = state.platform_filter == PlatformFilter::Single(pid);
        filter_chips.push(forge_widgets::filter_chip(
            palette,
            label,
            color,
            is_active,
            Message::LiveChat(LiveChatMsg::PlatformFilterChanged(PlatformFilter::Single(
                pid,
            ))),
        ));
    }

    let chip_divider = container(iced::widget::Space::new().width(0.5).height(14.0))
        .width(0.5)
        .height(14.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });
    filter_chips.push(chip_divider.into());

    let label_events = forge_widgets::tr!("chat_filter_events");
    let events_chip = forge_widgets::chip(
        label_events,
        forge_widgets::ChipGlyph::None,
        state.events_filter == EventsFilter::OnlyEvents,
        Some(Message::LiveChat(LiveChatMsg::EventsFilterToggled(
            if state.events_filter == EventsFilter::OnlyEvents {
                EventsFilter::All
            } else {
                EventsFilter::OnlyEvents
            },
        ))),
        palette,
    );
    filter_chips.push(events_chip);

    let label_hide_bots = forge_widgets::tr!("chat_filter_hide_bots");
    let hide_bots_chip = forge_widgets::chip(
        label_hide_bots,
        forge_widgets::ChipGlyph::Icon(Icon::EyeOff, p.text_faint),
        state.hide_bots,
        Some(Message::LiveChat(LiveChatMsg::HideBotsToggled)),
        palette,
    );
    filter_chips.push(hide_bots_chip);

    let chips = iced::widget::row(filter_chips)
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::alignment::Vertical::Center);

    let drawer_label = if state.drawer_open {
        forge_widgets::tr!("chat_hide_viewers")
    } else {
        forge_widgets::tr!("chat_show_viewers")
    };
    let drawer_btn = button(
        row![
            tabler_icon(Icon::Users, 11.0, p.text_secondary),
            text(drawer_label).size(FONT_XS).color(p.text_secondary),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::LiveChat(LiveChatMsg::ToggleDrawer))
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_: &iced::Theme, status| {
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        iced::widget::button::Style {
            background: Some(Background::Color(iced::Color::TRANSPARENT)),
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
    });

    let viewers_dot_color = match live_viewers {
        LiveViewerCount::Reporting(_) => p.success,
        LiveViewerCount::Empty => p.text_faint,
    };
    let viewers_label = match live_viewers {
        LiveViewerCount::Reporting(n) => forge_widgets::tr!(
            "chat_header_viewers",
            count = n as i64,
            formatted = forge_widgets::fmt_number(n as f64, 0)
        ),
        LiveViewerCount::Empty => "\u{2014}".to_owned(),
    };
    let viewers_span = row![
        forge_widgets::status_dot(viewers_dot_color, 6.0),
        text(viewers_label).size(FONT_XS).color(p.text_secondary),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(iced::alignment::Vertical::Center);

    let uptime_span = row![
        tabler_icon(Icon::Clock, 12.0, p.text_muted),
        text(uptime.to_owned()).size(FONT_XS).color(p.text_muted),
    ]
    .spacing(spf(Spacing::Xxs))
    .align_y(iced::alignment::Vertical::Center);

    let header_right = row![
        viewers_span,
        text("\u{00b7}").size(FONT_XS).color(p.text_faint),
        uptime_span,
        drawer_btn,
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(iced::alignment::Vertical::Center);

    let crumb_bar = forge_widgets::breadcrumb(
        vec![forge_widgets::BreadcrumbCrumb::leaf(forge_widgets::tr!(
            "chat_breadcrumb_chat"
        ))],
        Some(header_right.into()),
        palette,
    );

    let toggle_icon = if state.search_open {
        Icon::X
    } else {
        Icon::Search
    };
    let search_toggle = button(tabler_icon(toggle_icon, 15.0, p.text_faint))
        .on_press(Message::LiveChat(LiveChatMsg::SearchToggled))
        .padding(sp(Spacing::Xxs))
        .style(filter_icon_button_style(p));

    let search_control: Element<'a, Message> = if state.search_open {
        row![
            container(forge_widgets::search_input(
                forge_widgets::tr!("chat_search_placeholder"),
                &state.search_query,
                |s| Message::LiveChat(LiveChatMsg::SearchChanged(s)),
                palette,
            ))
            .width(Length::Fixed(220.0)),
            search_toggle,
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::alignment::Vertical::Center)
        .into()
    } else {
        search_toggle.into()
    };

    let filter_row = row![
        chips,
        iced::widget::Space::new().width(Length::Fill),
        search_control,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::alignment::Vertical::Center);

    let filter_bar = container(filter_row)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    column![crumb_bar, filter_bar].into()
}

fn build_chat_area<'a>(
    state: &'a LiveChatState,
    rt: &RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_XS, Radius, radius},
    };
    use iced::widget::{button, container, scrollable, text};
    use iced::{Background, Border, Padding};

    let palette_copy = *palette;
    let query = state.search_query.clone();

    let filtered: Vec<(usize, &UnifiedChatRow)> = state
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row_matches_platform_filter(row, &state.platform_filter)
                && row_matches_events_filter(row, state.events_filter)
                && !(state.hide_bots && row_is_bot(row))
        })
        .collect();

    let wanted: HashSet<EventId> = filtered
        .iter()
        .filter(|(_, row)| {
            row.event_detail
                .as_ref()
                .is_some_and(detail_supports_trigger_badge)
        })
        .map(|(_, row)| row.event_id)
        .collect();

    let triggered = if wanted.is_empty() {
        HashMap::new()
    } else {
        resolve_triggered_actions(rt, &wanted)
    };

    let visible: Vec<Element<'a, Message>> = filtered
        .into_iter()
        .map(|(idx, row)| {
            let opacity = row_match_opacity(row, &query);
            let triggered_action = triggered.get(&row.event_id).cloned();
            let chat_row = unified_to_chat_row(
                row,
                state.next_chat_seq.wrapping_add(idx as u64),
                triggered_action,
            );
            let seq = chat_row.seq;
            let row_el: Element<'a, Message> = iced::widget::lazy(seq, move |_: &u64| {
                forge_widgets::ChatRowWidget::new(
                    palette_copy,
                    chat_row.clone(),
                    Some(select_viewer_msg),
                )
            })
            .into();
            if opacity < 1.0 {
                let p = palette_copy;
                container(row_el)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(Background::Color(iced::Color { a: 0.7, ..p.base })),
                        ..container::Style::default()
                    })
                    .into()
            } else {
                row_el
            }
        })
        .collect();

    let p = *palette;

    let empty_msg = if !state.search_query.is_empty() {
        forge_widgets::tr!("chat_no_search_matches")
    } else {
        match state.events_filter {
            EventsFilter::OnlyEvents => forge_widgets::tr!("chat_no_events_yet"),
            _ => forge_widgets::tr!("chat_no_messages_empty"),
        }
    };

    let content: Element<'a, Message> = if visible.is_empty() {
        container(forge_widgets::empty_state(
            forge_widgets::tr!("chat_no_messages_title"),
            empty_msg,
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let col = iced::widget::column(visible)
            .spacing(spf(Spacing::Xxs))
            .width(Length::Fill)
            .padding([sp(Spacing::Xs), sp(Spacing::Md)]);

        let scrollable_chat = scrollable(col)
            .id(chat_scroll_id())
            .on_scroll(|vp| Message::LiveChat(LiveChatMsg::Scrolled(vp)))
            .height(Length::Fill);

        if state.unread_count > 0 {
            let label = if state.unread_count == 1 {
                forge_widgets::tr!("chat_new_message")
            } else {
                let count = state.unread_count as i64;
                forge_widgets::tr!("chat_new_messages", count = count)
            };

            let bubble_content = iced::widget::row![
                tabler_icon(Icon::ArrowDown, FONT_XS, p.shell),
                text(label)
                    .size(FONT_XS)
                    .color(p.shell)
                    .font(font(FontRole::Body)),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::alignment::Vertical::Center);

            let bubble = button(bubble_content)
                .on_press(Message::LiveChat(LiveChatMsg::ScrollToBottom))
                .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
                .style(move |_theme: &iced::Theme, _status| button::Style {
                    background: Some(Background::Color(p.brand)),
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: radius(Radius::Pill).into(),
                    },
                    text_color: p.shell,
                    shadow: iced::Shadow {
                        color: Color {
                            a: 0.4,
                            ..Color::BLACK
                        },
                        offset: iced::Vector::new(0.0, 4.0),
                        blur_radius: 14.0,
                    },
                    snap: false,
                });

            let floating_overlay = container(bubble)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::alignment::Vertical::Bottom)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 16.0,
                    left: 0.0,
                });

            iced::widget::stack![scrollable_chat, floating_overlay].into()
        } else {
            scrollable_chat.into()
        }
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border::default(),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use forge_types::{
        ChatEventDetail, ChatSegment, ChatSource, EventId, ModerationMarks, PlatformId,
        UnifiedChatRow, UserBadge,
    };
    use time::OffsetDateTime;

    use super::*;
    use crate::live_chat::{EventsFilter, PlatformFilter};

    fn make_row(id: &str, source: ChatSource, author: &str, is_event: bool) -> UnifiedChatRow {
        UnifiedChatRow {
            id: id.to_owned(),
            event_id: EventId::new(),
            source,
            received_at: OffsetDateTime::now_utc(),
            author: author.to_owned(),
            author_color: None,
            body_segments: vec![ChatSegment::Text {
                text: "test msg".to_owned(),
            }],
            badges: vec![],
            is_event,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn make_event_row(id: &str) -> UnifiedChatRow {
        let mut row = make_row(id, ChatSource::Twitch, "raider", true);
        row.event_detail = Some(ChatEventDetail::Raid { viewer_count: 10 });
        row
    }

    fn make_bot_row(id: &str) -> UnifiedChatRow {
        let mut row = make_row(id, ChatSource::Twitch, "nightbot", false);
        row.badges = vec![UserBadge::Bot];
        row
    }

    #[test]
    fn platform_filter_single_keeps_only_matching() {
        let rows = [
            make_row("t1", ChatSource::Twitch, "a", false),
            make_row("y1", ChatSource::YouTube, "b", false),
            make_row("k1", ChatSource::Kick, "c", false),
        ];
        let filter = PlatformFilter::Single(PlatformId::Twitch);
        let matching: Vec<_> = rows
            .iter()
            .filter(|r| row_matches_platform_filter(r, &filter))
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, "t1");
    }

    #[test]
    fn events_filter_only_messages_skips_event_rows() {
        let rows = [
            make_row("msg-1", ChatSource::Twitch, "a", false),
            make_event_row("ev-1"),
            make_row("msg-2", ChatSource::YouTube, "b", false),
        ];
        let messages_only: Vec<_> = rows
            .iter()
            .filter(|r| row_matches_events_filter(r, EventsFilter::OnlyMessages))
            .collect();
        assert_eq!(messages_only.len(), 2);
        assert!(messages_only.iter().all(|r| !r.is_event));
    }

    #[test]
    fn search_query_dims_non_matching() {
        let mut row = make_row("s1", ChatSource::Twitch, "alice", false);
        row.body_segments = vec![ChatSegment::Text {
            text: "hello world".to_owned(),
        }];
        assert_eq!(row_match_opacity(&row, "hello"), 1.0);
        assert_eq!(row_match_opacity(&row, "alice"), 1.0);
        assert_eq!(row_match_opacity(&row, "xyz"), 0.3);
        assert_eq!(row_match_opacity(&row, ""), 1.0);
    }

    #[test]
    fn hide_bots_filter_removes_bot_rows() {
        let rows = [
            make_bot_row("b1"),
            make_row("m1", ChatSource::Twitch, "viewer", false),
        ];
        let visible: Vec<_> = rows.iter().filter(|r| !row_is_bot(r)).collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "m1");
    }

    /// Conflict trace CH-05-F14: a Twitch `channel.cheer` reaches the render path
    /// as `ChatEventDetail::SuperChat` (the variant repurposed for bits). This pins
    /// that `unified_to_chat_row` surfaces it inline as `ChatBody::Cheer` and does
    /// NOT fall through the `None` arm to a plain `ChatBody::Message` (which would be
    /// the silent-drop the second audit pass suspected). `amount_micros / 10_000`
    /// decodes back to raw bits; a `None` cheer note yields empty body text but is
    /// still a Cheer, never dropped.
    #[test]
    fn cheer_super_chat_detail_renders_as_cheer_body_not_dropped() {
        for (message, expected_text) in [(Some("PogChamp".to_owned()), "PogChamp"), (None, "")] {
            let mut row = make_row("cheer-1", ChatSource::Twitch, "cheerer", true);
            row.event_detail = Some(ChatEventDetail::SuperChat {
                amount_micros: 1_000_000,
                currency: "BITS".to_owned(),
                message,
            });
            let chat_row = unified_to_chat_row(&row, 0, None);
            assert_eq!(
                chat_row.body,
                ChatBody::Cheer {
                    bits: 100,
                    text: expected_text.to_owned(),
                }
            );
        }
    }
}
