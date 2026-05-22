use std::sync::Arc;

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Command, CommandPermission};
use forge_widgets::{
    ForgePalette, Icon, filter_chip,
    icons::tabler_icon,
    primary_button, search_input,
    tokens::{FONT_LG, FONT_SM, FONT_XS, FontRole, font},
};
use iced::{
    Alignment, Background, Border, Element, Length, Task,
    widget::{Space, column, container, row, scrollable, text},
};

use crate::{Message, Screen};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandsFilter {
    #[default]
    All,
    Enabled,
    Disabled,
}

#[derive(Debug, Default)]
pub struct CommandsState {
    pub commands: Vec<Command>,
    pub search: String,
    pub filter: CommandsFilter,
    pub loading: bool,
}

impl CommandsState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub enum CommandsMsg {
    LoadRequested,
    Loaded(Result<Vec<Command>, String>),
    SearchChanged(String),
    FilterChanged(CommandsFilter),
}

pub async fn load_commands(dp: Arc<SqliteBackend>) -> Result<Vec<Command>, String> {
    dp.command_repo().list().await.map_err(|e| e.to_string())
}

pub fn handle_msg(
    state: &mut CommandsState,
    msg: CommandsMsg,
    backend: &Arc<SqliteBackend>,
) -> Task<Message> {
    match msg {
        CommandsMsg::LoadRequested => {
            state.loading = true;
            let dp = Arc::clone(backend);
            Task::perform(load_commands(dp), |r| {
                Message::Commands(CommandsMsg::Loaded(r))
            })
        }
        CommandsMsg::Loaded(Ok(v)) => {
            state.loading = false;
            state.commands = v;
            Task::none()
        }
        CommandsMsg::Loaded(Err(_)) => {
            state.loading = false;
            Task::none()
        }
        CommandsMsg::SearchChanged(s) => {
            state.search = s;
            Task::none()
        }
        CommandsMsg::FilterChanged(f) => {
            state.filter = f;
            Task::none()
        }
    }
}

fn perm_label(p: &CommandPermission) -> &'static str {
    match p {
        CommandPermission::Everyone => "Everyone",
        CommandPermission::Subscriber => "Subs+",
        CommandPermission::Vip => "VIP+",
        CommandPermission::Moderator => "Mods+",
        CommandPermission::Broadcaster => "Broadcaster",
    }
}

fn format_cooldown(secs: u64) -> String {
    if secs == 0 {
        "\u{2014}".to_owned()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

pub fn commands_view<'a>(
    state: &'a CommandsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);

    let total = state.commands.len();
    let active_count = state.commands.len();

    let header = container(
        row![
            tabler_icon(Icon::Terminal, 18.0, p.brand),
            text("Commands").size(FONT_LG).color(p.text_primary),
            Space::new().width(Length::Fill),
            text(format!("{total} commands"))
                .size(FONT_XS)
                .color(p.text_muted),
            text("\u{00b7}").size(FONT_XS).color(p.text_faint),
            text(format!("{active_count} active"))
                .size(FONT_XS)
                .color(p.success),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([14_u16, 20_u16])
    .width(Length::Fill);

    let search_box = search_input(
        "Search commands...",
        &state.search,
        |s| Message::Commands(CommandsMsg::SearchChanged(s)),
        palette,
    );

    let chips = row![
        filter_chip(
            palette,
            "All",
            p.brand,
            state.filter == CommandsFilter::All,
            Message::Commands(CommandsMsg::FilterChanged(CommandsFilter::All)),
        ),
        filter_chip(
            palette,
            "Enabled",
            p.success,
            state.filter == CommandsFilter::Enabled,
            Message::Commands(CommandsMsg::FilterChanged(CommandsFilter::Enabled)),
        ),
        filter_chip(
            palette,
            "Disabled",
            p.text_faint,
            state.filter == CommandsFilter::Disabled,
            Message::Commands(CommandsMsg::FilterChanged(CommandsFilter::Disabled)),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let new_btn = primary_button("New command", Message::Navigate(Screen::Actions), palette);

    let toolbar = container(
        row![
            row![search_box, chips]
                .spacing(10)
                .align_y(Alignment::Center),
            Space::new().width(Length::Fill),
            new_btn,
        ]
        .align_y(Alignment::Center),
    )
    .padding([8_u16, 16_u16])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.base)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let column_header = container(
        row![
            text("STATE")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono)
                .width(Length::Fixed(50.0)),
            text("COMMAND")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono)
                .width(Length::FillPortion(3)),
            text("PERMISSIONS")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono)
                .width(Length::FillPortion(2)),
            text("COOLDOWN")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono)
                .width(Length::Fixed(110.0)),
            text("ACTION")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono)
                .width(Length::FillPortion(2)),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([7_u16, 16_u16])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.shell)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let search_lower = state.search.to_ascii_lowercase();
    let filtered: Vec<&Command> = state
        .commands
        .iter()
        .filter(|c| search_lower.is_empty() || c.name.to_ascii_lowercase().contains(&search_lower))
        .collect();

    let rows_body: Element<'a, Message> = if state.loading {
        container(
            text("Loading commands\u{2026}")
                .size(FONT_SM)
                .color(p.text_muted),
        )
        .padding(24)
        .width(Length::Fill)
        .into()
    } else if filtered.is_empty() {
        container(forge_widgets::empty_state(
            "No commands yet",
            "Commands are created by adding a chat trigger to an action. Open Actions and add a !command trigger.",
            Some(("Open Actions", Message::Navigate(Screen::Actions))),
            palette,
        ))
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let mut col = column![].spacing(0);
        for cmd in filtered {
            col = col.push(command_row(cmd, palette));
        }
        scrollable(col).height(Length::Fill).into()
    };

    container(
        column![header, toolbar, column_header, rows_body]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn command_row<'a>(cmd: &'a Command, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);

    let state_dot = container(Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.success)),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let state_cell = container(state_dot)
        .width(Length::Fixed(50.0))
        .align_x(Alignment::Start);

    let name_cell = text(cmd.name.clone())
        .size(FONT_SM)
        .color(p.text_primary)
        .font(mono)
        .width(Length::FillPortion(3));

    let perm_cell = text(perm_label(&cmd.permission))
        .size(FONT_SM)
        .color(p.text_secondary)
        .width(Length::FillPortion(2));

    let cooldown_cell = text(format_cooldown(cmd.cooldown_secs))
        .size(FONT_SM)
        .color(p.text_muted)
        .font(mono)
        .width(Length::Fixed(110.0));

    let action_cell = text(format!("{}", cmd.action_id))
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono)
        .width(Length::FillPortion(2));

    let inner = row![state_cell, name_cell, perm_cell, cooldown_cell, action_cell,]
        .spacing(12)
        .align_y(Alignment::Center);

    container(inner)
        .padding([8_u16, 16_u16])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
