use iced::{
    Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, text, text_input},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Moderator,
    Vip,
    Subscriber,
    Bot,
    Broadcaster,
    Partner,
    Premium,
    Founder,
    Turbo,
    HypeTrain,
    Bits,
    BitsLeader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Twitch,
    YouTube,
    Kick,
}

impl Platform {
    pub fn color(self, palette: &ForgePalette) -> Color {
        match self {
            Platform::Twitch => palette.brand,
            Platform::YouTube => palette.random,
            Platform::Kick => palette.info,
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Platform::Twitch => "T",
            Platform::YouTube => "Y",
            Platform::Kick => "K",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    Message(String),
    Subscription {
        tier: u8,
        months: Option<u32>,
        message: Option<String>,
        triggered_action: Option<String>,
    },
    Cheer {
        bits: u64,
        text: String,
    },
    Raid {
        viewers: u64,
        triggered_action: Option<String>,
    },
    Command {
        command: String,
        action_name: Option<String>,
        action_duration_ms: Option<u64>,
    },
}

#[derive(Debug, Clone)]
pub struct ChatRow {
    pub seq: u64,
    pub timestamp: String,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: String,
    pub username_color: Color,
    pub body: ChatBody,
}

pub struct PlatformTarget<'a, Msg> {
    pub platform: Platform,
    pub active: bool,
    pub on_press: Option<Box<dyn Fn() -> Msg + 'a>>,
}

pub fn chip_bg(active: bool, palette: &ForgePalette) -> Color {
    if active {
        palette.surface_overlay
    } else {
        Color::TRANSPARENT
    }
}

pub fn filter_chip<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    label: &str,
    dot_color: Color,
    active: bool,
    on_press: Msg,
) -> Element<'a, Msg> {
    let bg = chip_bg(active, palette);
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let dot_size = 5.0_f32;
    let dot_radius = dot_size / 2.0;

    let dot = container(iced::widget::Space::new().width(dot_size).height(dot_size))
        .width(dot_size)
        .height(dot_size)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: dot_radius.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_text = text(label.to_owned())
        .size(FONT_XS)
        .color(text_color)
        .font(font(FontRole::Body));

    let content = row![dot, label_text]
        .spacing(5)
        .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: radius(Radius::Pill).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            text_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn platform_target_button<'a, Msg: Clone + 'a>(
    platform: Platform,
    active: bool,
    on_press: Option<Box<dyn Fn() -> Msg + 'a>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let platform_color = platform.color(palette);
    let letter = platform.letter();

    let (bg, text_color) = if active {
        (palette.surface_overlay, platform_color)
    } else {
        (Color::TRANSPARENT, palette.text_faint)
    };

    let content = text(letter)
        .size(FONT_XS)
        .color(text_color)
        .font(font(FontRole::Body));

    let btn = button(
        container(content)
            .width(20)
            .height(20)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center),
    )
    .padding(0)
    .style(move |_theme: &iced::Theme, _status| button::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        text_color,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    if let Some(handler) = on_press {
        btn.on_press_with(handler).into()
    } else {
        btn.into()
    }
}

fn hint_row<'a, Msg: 'a>(palette: &ForgePalette) -> Element<'a, Msg> {
    let color = palette.text_faint;
    let mono = font(FontRole::Monospace);

    let slash_hint = row![
        text("/").size(FONT_XS).color(color).font(mono),
        text(" commands").size(FONT_XS).color(color),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let at_hint = row![
        text("@").size(FONT_XS).color(color).font(mono),
        text(" mention").size(FONT_XS).color(color),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let bang_hint = row![
        text("!").size(FONT_XS).color(color).font(mono),
        text(" trigger action").size(FONT_XS).color(color),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    row![slash_hint, at_hint, bang_hint]
        .spacing(14)
        .align_y(iced::Alignment::Center)
        .into()
}

const EMOJIS: &[&str] = &[
    "😀",
    "😃",
    "😄",
    "😁",
    "😆",
    "😅",
    "😂",
    "🤣",
    "😊",
    "😇",
    "🙂",
    "🙃",
    "😉",
    "😌",
    "😍",
    "🥰",
    "😘",
    "😗",
    "😙",
    "😚",
    "😋",
    "😛",
    "😝",
    "😜",
    "🤪",
    "🤨",
    "🧐",
    "🤓",
    "😎",
    "🥸",
    "🤩",
    "🥳",
    "😏",
    "😒",
    "😞",
    "😔",
    "😟",
    "😕",
    "🙁",
    "☹️",
    "😣",
    "😖",
    "😫",
    "😩",
    "🥺",
    "😢",
    "😭",
    "😤",
    "😠",
    "😡",
    "🤬",
    "🤯",
    "😳",
    "🥵",
    "🥶",
    "😱",
    "😨",
    "😰",
    "😥",
    "😓",
    "🤗",
    "🤔",
    "🫣",
    "🤭",
    "🤫",
    "🤥",
    "😶",
    "😶‍🌫️",
    "😐",
    "😑",
];

#[allow(clippy::too_many_arguments)]
pub fn input_bar<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    value: &'a str,
    placeholder: impl Into<String>,
    platform_targets: Vec<PlatformTarget<'a, Msg>>,
    on_input: impl Fn(String) -> Msg + Clone + 'a,
    on_submit: Msg,
    emoji_picker_open: bool,
    on_toggle_emoji: Msg,
) -> Element<'a, Msg> {
    let p = *palette;

    let target_buttons: Vec<Element<'a, Msg>> = platform_targets
        .into_iter()
        .map(|t| platform_target_button(t.platform, t.active, t.on_press, palette))
        .collect();

    let divider = container(iced::widget::Space::new().width(0.5_f32).height(18))
        .width(0.5_f32)
        .height(18)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let send_msg = on_submit.clone();
    let input_widget = text_input(&placeholder.into(), value)
        .on_input(on_input.clone())
        .on_submit(on_submit)
        .padding([0, 0])
        .size(FONT_SM)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, _status| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            icon: p.text_muted,
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: Color { a: 0.25, ..p.brand },
        });

    let mood_icon = button(tabler_icon::<Msg>(Icon::MoodSmile, 15.0, p.text_faint))
        .on_press(on_toggle_emoji)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let send_button = button(tabler_icon::<Msg>(Icon::Send, 15.0, p.brand))
        .on_press(send_msg)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let mut composer_children: Vec<Element<'a, Msg>> = target_buttons;
    composer_children.push(divider.into());
    composer_children.push(input_widget.into());
    composer_children.push(mood_icon.into());
    composer_children.push(send_button.into());

    let composer = container(
        row(composer_children)
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(p.base)),
        border: Border {
            color: p.border_input,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let hints = container(hint_row(palette)).padding(Padding {
        top: spf(Spacing::Xs),
        right: spf(Spacing::Xxs),
        bottom: 0.0,
        left: spf(Spacing::Xxs),
    });

    let mut body_elements = Vec::new();
    if emoji_picker_open {
        let emoji_buttons: Vec<Element<'a, Msg>> = EMOJIS
            .iter()
            .map(|&emoji| {
                let emoji_str = emoji.to_owned();
                let new_val = format!("{value}{emoji_str}");
                let on_click_msg = on_input(new_val);
                button(text(emoji).size(FONT_SM).font(font(FontRole::Body)))
                    .on_press(on_click_msg)
                    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
                    .style(move |_theme: &iced::Theme, status| {
                        let hovered =
                            matches!(status, button::Status::Hovered | button::Status::Pressed);
                        button::Style {
                            background: if hovered {
                                Some(Background::Color(p.surface_overlay))
                            } else {
                                None
                            },
                            border: Border::default(),
                            text_color: p.text_primary,
                            shadow: iced::Shadow::default(),
                            snap: false,
                        }
                    })
                    .into()
            })
            .collect();

        let grid = row(emoji_buttons).spacing(4).wrap();

        let picker_box = container(iced::widget::scrollable(grid).height(Length::Fixed(120.0)))
            .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(Background::Color(p.shell)),
                border: Border {
                    color: p.border_regular,
                    width: 0.5,
                    radius: radius(Radius::Md).into(),
                },
                ..container::Style::default()
            });

        body_elements.push(picker_box.into());
        body_elements.push(
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fixed(8.0))
                .into(),
        );
    }

    body_elements.push(composer.into());
    body_elements.push(hints.into());

    let top_border = container(iced::widget::Space::new().width(Length::Fill).height(0.5))
        .width(Length::Fill)
        .height(0.5)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let body = container(column(body_elements).spacing(0))
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            ..container::Style::default()
        });

    column![top_border, body].spacing(0).into()
}
