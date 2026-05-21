use std::sync::Arc;

use forge_storage::{Viewer, ViewerPlatform};
use forge_storage_sqlite::SqliteBackend;
use forge_widgets::{
    ForgePalette, Radius,
    icons::{Icon, tabler_icon},
    radius,
    tokens::{FONT_SM, FONT_XS, FontRole, font},
};
use iced::{
    Background, Border, Color, Element, Length, Task,
    widget::{Space, column, container, row, scrollable, text, text_input},
};
use time::OffsetDateTime;

use crate::Message;

#[derive(Debug, Clone, Default)]
pub struct ViewersState {
    pub viewers: Vec<Viewer>,
    pub search: String,
    pub platform_filter: Option<ViewerPlatform>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ViewersMsg {
    LoadRequested,
    Loaded(Result<Vec<Viewer>, String>),
    SearchChanged(String),
    PlatformFilterSelected(Option<ViewerPlatform>),
}

pub async fn load_viewers(dp: Arc<SqliteBackend>) -> Result<Vec<Viewer>, String> {
    use forge_storage::DataProvider;
    dp.viewer_repo().list().await.map_err(|e| e.to_string())
}

pub fn handle_msg(
    state: &mut ViewersState,
    msg: ViewersMsg,
    backend: &Arc<SqliteBackend>,
) -> Task<Message> {
    match msg {
        ViewersMsg::LoadRequested => {
            state.loading = true;
            state.error = None;
            let dp = Arc::clone(backend);
            Task::perform(load_viewers(dp), |r| {
                Message::Viewers(ViewersMsg::Loaded(r))
            })
        }
        ViewersMsg::Loaded(Ok(v)) => {
            state.loading = false;
            state.viewers = v;
            Task::none()
        }
        ViewersMsg::Loaded(Err(e)) => {
            state.loading = false;
            state.error = Some(e);
            Task::none()
        }
        ViewersMsg::SearchChanged(q) => {
            state.search = q;
            Task::none()
        }
        ViewersMsg::PlatformFilterSelected(p) => {
            state.platform_filter = p;
            Task::none()
        }
    }
}

fn matches_filter(v: &Viewer, search: &str, platform: Option<ViewerPlatform>) -> bool {
    if let Some(p) = platform.as_ref()
        && &v.platform != p
    {
        return false;
    }
    if search.is_empty() {
        return true;
    }
    v.username
        .to_ascii_lowercase()
        .contains(&search.to_ascii_lowercase())
}

fn last_seen_label(dt: OffsetDateTime) -> String {
    let now = OffsetDateTime::now_utc();
    let delta = now - dt;
    let secs = delta.whole_seconds().max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

fn platform_chip<'a>(
    label: &str,
    selected: bool,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let bg = if selected {
        palette.brand
    } else {
        palette.elevated
    };
    let fg = if selected {
        palette.shell
    } else {
        palette.text_secondary
    };
    iced::widget::button(
        text(label.to_owned())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(fg),
    )
    .padding([4u16, 10u16])
    .on_press(on_press)
    .style(move |_t, _s| iced::widget::button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Pill).into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .into()
}

fn viewer_row<'a>(v: &'a Viewer, palette: &'a ForgePalette) -> Element<'a, Message> {
    let avatar_letter = v
        .username
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    let avatar = container(
        text(avatar_letter)
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.shell),
    )
    .width(Length::Fixed(28.0))
    .height(Length::Fixed(28.0))
    .center_x(Length::Fixed(28.0))
    .center_y(Length::Fixed(28.0))
    .style(move |_t| container::Style {
        background: Some(Background::Color(palette.brand)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Pill).into(),
        },
        ..container::Style::default()
    });

    let name = text(v.username.clone())
        .font(font(FontRole::Body))
        .size(FONT_SM)
        .color(palette.text_primary);

    let platform_label = v.platform.as_str().to_ascii_uppercase();
    let platform_pill = container(
        text(platform_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_muted),
    )
    .padding([2u16, 8u16])
    .style(move |_t| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Pill).into(),
        },
        ..container::Style::default()
    });

    let msg_count = text(format!("{} msgs", v.message_count))
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .color(palette.text_muted);

    let last_seen = text(last_seen_label(v.last_seen_at))
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .color(palette.text_muted);

    let greeting_badge: Element<'_, Message> = if v.custom_greeting {
        text("greeting")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.success)
            .into()
    } else {
        Space::new().width(Length::Shrink).into()
    };

    let row_content = row![
        avatar,
        name,
        Space::new().width(Length::Fill),
        platform_pill,
        msg_count,
        last_seen,
        greeting_badge,
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    container(row_content)
        .padding([8u16, 14u16])
        .width(Length::Fill)
        .style(move |_t| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn viewers_view<'a>(
    state: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let title = row![
        tabler_icon(Icon::Users, 20.0, palette.text_primary),
        text("Viewers")
            .font(font(FontRole::Body))
            .size(20.0)
            .color(palette.text_primary),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let search_input = text_input("Search by username…", &state.search)
        .on_input(|q| Message::Viewers(ViewersMsg::SearchChanged(q)))
        .size(FONT_SM)
        .padding(6);

    let chips = row![
        platform_chip(
            "All",
            state.platform_filter.is_none(),
            Message::Viewers(ViewersMsg::PlatformFilterSelected(None)),
            palette,
        ),
        platform_chip(
            "Twitch",
            state.platform_filter == Some(ViewerPlatform::Twitch),
            Message::Viewers(ViewersMsg::PlatformFilterSelected(Some(
                ViewerPlatform::Twitch
            ))),
            palette,
        ),
        platform_chip(
            "YouTube",
            state.platform_filter == Some(ViewerPlatform::YouTube),
            Message::Viewers(ViewersMsg::PlatformFilterSelected(Some(
                ViewerPlatform::YouTube
            ))),
            palette,
        ),
        platform_chip(
            "Kick",
            state.platform_filter == Some(ViewerPlatform::Kick),
            Message::Viewers(ViewersMsg::PlatformFilterSelected(Some(
                ViewerPlatform::Kick
            ))),
            palette,
        ),
        platform_chip(
            "Trovo",
            state.platform_filter == Some(ViewerPlatform::Trovo),
            Message::Viewers(ViewersMsg::PlatformFilterSelected(Some(
                ViewerPlatform::Trovo
            ))),
            palette,
        ),
    ]
    .spacing(6);

    let filter_bar = row![search_input, chips]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    let filtered: Vec<&Viewer> = state
        .viewers
        .iter()
        .filter(|v| matches_filter(v, &state.search, state.platform_filter.clone()))
        .collect();

    let count_label = text(format!(
        "{} of {} viewers",
        filtered.len(),
        state.viewers.len()
    ))
    .font(font(FontRole::Monospace))
    .size(FONT_XS)
    .color(palette.text_muted);

    let rows: Vec<Element<'_, Message>> = filtered.iter().map(|v| viewer_row(v, palette)).collect();

    let list_body: Element<'_, Message> = if state.loading {
        text("Loading…")
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    } else if let Some(err) = state.error.as_deref() {
        text(format!("Error: {err}"))
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.random)
            .into()
    } else if rows.is_empty() {
        text("No viewers tracked yet — they appear on the first chat message.")
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_muted)
            .into()
    } else {
        scrollable(column(rows).spacing(6).width(Length::Fill))
            .height(Length::Fill)
            .into()
    };

    container(
        column![title, filter_bar, count_label, list_body]
            .spacing(14)
            .padding(20),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn make_v(name: &str, platform: ViewerPlatform) -> Viewer {
        Viewer {
            viewer_id: format!("id_{name}"),
            platform,
            username: name.to_owned(),
            first_seen_at: OffsetDateTime::UNIX_EPOCH,
            last_seen_at: OffsetDateTime::UNIX_EPOCH,
            message_count: 1,
            custom_greeting: false,
        }
    }

    #[test]
    fn search_is_case_insensitive() {
        let v = make_v("AliceWonderland", ViewerPlatform::Twitch);
        assert!(matches_filter(&v, "alice", None));
        assert!(matches_filter(&v, "WONDER", None));
        assert!(!matches_filter(&v, "bob", None));
    }

    #[test]
    fn platform_filter_excludes_others() {
        let v = make_v("alice", ViewerPlatform::Twitch);
        assert!(matches_filter(&v, "", Some(ViewerPlatform::Twitch)));
        assert!(!matches_filter(&v, "", Some(ViewerPlatform::YouTube)));
    }

    #[test]
    fn empty_search_matches_all() {
        let v = make_v("alice", ViewerPlatform::Twitch);
        assert!(matches_filter(&v, "", None));
    }
}
