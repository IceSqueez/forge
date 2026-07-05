use forge_platform_core::BuiltinId;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_MD, FONT_SM, FONT_XS, Radius, Spacing, radius, spf};
use iced::Element;

use crate::app::App;
use crate::connectivity::{Connectivity, Integration};
use crate::{Message, Screen};
use forge_widgets::{BreadcrumbCrumb, breadcrumb};

/// Shared overview card for the Platforms and Stream-apps grids: a `leading`
/// visual (letter tile or icon box), a name with a live connection badge, a
/// description, and an optional feature-chip row, wrapped in an interactive
/// [`forge_widgets::card`] that navigates to the builtin detail on press.
pub(crate) fn overview_card<'a>(
    leading: Element<'a, Message>,
    name: &'a str,
    desc: impl Into<String>,
    features: &'static [&'static str],
    connected: bool,
    target: BuiltinId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{Row, column, container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let p = *palette;

    let badge = forge_widgets::connection_status_badge(connected, palette);

    let title_row = row![
        text(name.to_owned()).size(FONT_SM).color(p.text_primary),
        badge,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let desc_text = text(desc.into()).size(FONT_SM).color(p.text_muted);

    let mut info_col = column![title_row, desc_text].spacing(spf(Spacing::Xs));
    if !features.is_empty() {
        let mut chip_row = Row::new().spacing(spf(Spacing::Xxs));
        for f in features {
            let chip = container(text(*f).size(FONT_XS).color(p.text_secondary))
                .padding([2_u16, 7_u16])
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(Background::Color(p.shell)),
                    border: Border {
                        radius: radius(Radius::Sm).into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });
            chip_row = chip_row.push(chip);
        }
        info_col = info_col.push(chip_row.wrap());
    }

    let inner = row![
        leading,
        container(info_col).width(Length::Fill),
        tabler_icon(Icon::ChevronRight, 16.0, p.text_faint),
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(Alignment::Start);

    forge_widgets::card(inner, palette)
        .on_press(Message::Navigate(Screen::BuiltinDetail(target)))
        .padding([16_u16, 18_u16])
        .width(Length::Fill)
        .into()
}

pub(crate) fn platforms_overview_view<'a>(
    app: &'a App,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};
    use iced::{Length, Padding};

    let p = *palette;

    let title = text(forge_widgets::tr!("platforms.title"))
        .size(FONT_MD)
        .color(p.text_primary);
    let subtitle = text(forge_widgets::tr!("platforms.subtitle"))
        .size(FONT_SM)
        .color(p.text_muted);
    let header = column![title, subtitle].spacing(spf(Spacing::Xxs));

    let connectivity = Connectivity::resolve(&app.rt);

    let twitch_card = overview_card(
        forge_widgets::platform_identity_tile(
            "T",
            Integration::Twitch.brand_color(palette),
            p.shell,
            44.0,
        ),
        "Twitch",
        forge_widgets::tr!("platforms.twitch.desc"),
        &["IRC chat", "EventSub", "Channel points", "Bits & subs"],
        connectivity.is_connected(Integration::Twitch),
        BuiltinId::new("twitch"),
        palette,
    );
    let youtube_card = overview_card(
        forge_widgets::platform_identity_tile(
            "Y",
            Integration::YouTube.brand_color(palette),
            p.shell,
            44.0,
        ),
        "YouTube",
        forge_widgets::tr!("platforms.youtube.desc"),
        &["Live chat", "Super chat", "Memberships"],
        connectivity.is_connected(Integration::YouTube),
        BuiltinId::new("youtube"),
        palette,
    );
    let kick_card = overview_card(
        forge_widgets::platform_identity_tile(
            "K",
            Integration::Kick.brand_color(palette),
            p.shell,
            44.0,
        ),
        "Kick",
        forge_widgets::tr!("platforms.kick.desc"),
        &["Chat", "Subs", "Channel events"],
        connectivity.is_connected(Integration::Kick),
        BuiltinId::new("kick"),
        palette,
    );
    let grid_row_1 = row![twitch_card, youtube_card]
        .spacing(spf(Spacing::Sm))
        .width(Length::Fill);
    let grid_row_2 = row![kick_card, iced::widget::Space::new().width(Length::Fill)]
        .spacing(spf(Spacing::Sm))
        .width(Length::Fill);
    let grid = column![grid_row_1, grid_row_2].spacing(spf(Spacing::Sm));

    let body = column![header, grid].spacing(spf(Spacing::Md));
    let page_header = breadcrumb(
        vec![BreadcrumbCrumb::leaf(forge_widgets::tr!(
            "platforms.breadcrumb"
        ))],
        None,
        palette,
    );
    let body_container = container(scrollable(body).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 22.0,
            right: 28.0,
            bottom: 22.0,
            left: 28.0,
        });

    column![page_header, body_container]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
