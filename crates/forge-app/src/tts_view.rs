use crate::app::App;
use crate::message::Message;
use crate::page_chrome::simple_page_header;
use crate::screen::{Screen, TtsSection};
use crate::tts_dashboard::tts_dashboard_view;
use crate::tts_engines::tts_engines_view;
use crate::tts_filters::tts_filters_view;
use crate::tts_triggers::tts_triggers_view;
use crate::voice_aliases::voice_aliases_view;
use forge_widgets::ForgePalette;
use forge_widgets::tokens::{FONT_SM, Spacing, spf};
use iced::Element;

fn tts_tab_button<'a>(
    label: &'static str,
    section: TtsSection,
    active: &TtsSection,
    palette: &'a ForgePalette,
) -> iced::widget::Button<'a, Message> {
    use iced::widget::{button, column, container, text};
    let is_active = *active == section;
    let fg = if is_active {
        palette.text_primary
    } else {
        palette.text_muted
    };
    let indicator_color = if is_active {
        palette.brand
    } else {
        iced::Color::TRANSPARENT
    };
    let inner = column![
        text(label).size(FONT_SM).color(fg),
        container(iced::widget::Space::new())
            .width(iced::Length::Fill)
            .height(2)
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(indicator_color)),
                ..iced::widget::container::Style::default()
            }),
    ]
    .spacing(spf(Spacing::Xxs));
    button(inner)
        .on_press(Message::Navigate(Screen::Tts(section)))
        .padding([7_u16, 14_u16])
        .style(|_, _| iced::widget::button::Style {
            background: None,
            ..iced::widget::button::Style::default()
        })
}

pub(crate) fn tts_section_view<'a>(
    app: &'a App,
    section: &'a TtsSection,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row};
    let tab_bar = container(
        row![
            tts_tab_button("Dashboard", TtsSection::Dashboard, section, palette),
            tts_tab_button("Engines", TtsSection::Engines, section, palette),
            tts_tab_button("Voice aliases", TtsSection::Aliases, section, palette),
            tts_tab_button("Filters", TtsSection::Filters, section, palette),
            tts_tab_button("Triggers", TtsSection::Triggers, section, palette),
        ]
        .spacing(spf(Spacing::Xxs)),
    )
    .width(iced::Length::Fill)
    .style(move |_| iced::widget::container::Style {
        background: Some(iced::Background::Color(palette.shell)),
        border: iced::Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    });

    let content: Element<'a, Message> = match section {
        TtsSection::Dashboard => tts_dashboard_view(&app.ui.tts_dashboard, palette),
        TtsSection::Engines => tts_engines_view(&app.ui.tts_engines, palette),
        TtsSection::Aliases => voice_aliases_view(&app.ui.tts_aliases, palette),
        TtsSection::Filters => tts_filters_view(&app.ui.tts_filters, palette),
        TtsSection::Triggers => tts_triggers_view(&app.ui.tts_triggers, palette),
    };

    let section_label = match section {
        TtsSection::Dashboard => "Dashboard",
        TtsSection::Engines => "Engines",
        TtsSection::Aliases => "Voice aliases",
        TtsSection::Filters => "Filters",
        TtsSection::Triggers => "Triggers",
    };
    let page_header = simple_page_header(
        &[("Builtin", false), ("TTS", false), (section_label, true)],
        palette,
    );

    column![page_header, tab_bar, content]
        .spacing(0)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into()
}
