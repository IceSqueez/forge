use std::borrow::Cow;

use iced::{
    Alignment, Border, Color, Element, Length,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, stack, text},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{
        BORDER_THIN, FONT_MD, FONT_SM, FONT_XS, FontRole, ModalSize, Radius, Spacing, font,
        modal_width, radius, sp, spf,
    },
};

/// Semantic subject of a destructive confirmation. Drives the noun in the
/// heading ("Delete action?", "Delete step?") so callers stay declarative
/// instead of hand-formatting per-screen title strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    Action,
    Step,
    TriggerLink,
    Global,
    Script,
    Client,
}

impl ConfirmKind {
    fn noun_key(self) -> &'static str {
        match self {
            ConfirmKind::Action => "widget.confirm_delete.kind_action",
            ConfirmKind::Step => "widget.confirm_delete.kind_step",
            ConfirmKind::TriggerLink => "widget.confirm_delete.kind_trigger_link",
            ConfirmKind::Global => "widget.confirm_delete.kind_global",
            ConfirmKind::Script => "widget.confirm_delete.kind_script",
            ConfirmKind::Client => "widget.confirm_delete.kind_client",
        }
    }
}

/// Confirm-button severity. `Destructive` paints the danger-red accent for
/// irreversible removals; `Warning` paints the caution-yellow accent for
/// reversible-but-disruptive actions (e.g. disable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmTone {
    Destructive,
    Warning,
}

impl ConfirmTone {
    fn accent(self, palette: &ForgePalette) -> Color {
        match self {
            ConfirmTone::Destructive => palette.random,
            ConfirmTone::Warning => palette.warning,
        }
    }
}

/// Inputs for [`confirm_modal`]. The consuming screen owns the two-phase gate
/// (a `pending_delete: Option<_>` field); this primitive is a pure view that
/// renders only while that field is `Some`.
pub struct ConfirmModalParams<'a> {
    pub kind: ConfirmKind,
    /// Monospace sub-heading naming the specific target (id, label, path).
    pub item_name: Cow<'a, str>,
    /// Optional cascade warning ("3 sub-actions, 2 trigger links"). Falls back
    /// to a generic irreversible-removal notice when `None`.
    pub cascade_hint: Option<Cow<'a, str>>,
    pub tone: ConfirmTone,
}

fn accent_btn_style(bg: Color, fg: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Sm);
    move |_theme, status| {
        let adjusted = match status {
            Status::Hovered => Color { a: 0.9, ..bg },
            Status::Pressed => Color { a: 0.75, ..bg },
            _ => bg,
        };
        Style {
            background: Some(iced::Background::Color(adjusted)),
            text_color: fg,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

/// Shared destructive-confirm dialog: alert-triangle icon tile, "Delete
/// {kind}?" heading, monospace target name, cascade hint, and an accent-toned
/// confirm button beside a ghost cancel. `on_confirm` / `on_cancel` are emitted
/// on the respective button (and `on_cancel` also on scrim click); the caller
/// wires Esc-to-cancel via its keyboard subscription.
pub fn confirm_modal<'a, Msg: Clone + 'a>(
    params: ConfirmModalParams<'a>,
    on_confirm: Msg,
    on_cancel: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let accent = params.tone.accent(&p);
    let cancel_for_backdrop = on_cancel.clone();

    let icon_bg = Color { a: 0.12, ..accent };
    let icon_box = container(tabler_icon(Icon::AlertTriangle, 18.0, accent))
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(36.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(icon_bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        });

    let noun = crate::tr!(params.kind.noun_key());
    let title = text(crate::tr!("widget.confirm_delete.title", kind = noun))
        .size(FONT_MD)
        .color(p.text_primary)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..font(FontRole::Body)
        });
    let name = text(params.item_name)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let header = row![icon_box, column![title, name].spacing(spf(Spacing::Xxs))]
        .spacing(12)
        .align_y(Alignment::Center);

    let hint_text = params
        .cascade_hint
        .unwrap_or_else(|| Cow::Owned(crate::tr!("widget.confirm_delete.hint")));
    let hint = text(hint_text).size(FONT_SM).color(p.text_muted);

    let body = container(column![header, hint].spacing(spf(Spacing::Sm)))
        .width(Length::Fill)
        .padding(sp(Spacing::Lg));

    let esc_hint = row![
        tabler_icon(Icon::Keyboard, 12.0, p.text_faint),
        text("Esc")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_faint),
        text(format!(" {}", crate::tr!("widget.confirm.esc_to_cancel")))
            .size(FONT_XS)
            .color(p.text_faint),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    let cancel_btn = button(
        text(crate::tr!("common.cancel"))
            .size(FONT_SM)
            .color(p.text_secondary),
    )
    .on_press(on_cancel)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(crate::buttons::outline_btn_style(
        p.border_regular,
        p.text_secondary,
        p.text_primary,
    ));

    let confirm_btn = button(
        text(crate::tr!("common.delete"))
            .size(FONT_SM)
            .color(p.shell)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
    )
    .on_press(on_confirm)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(accent_btn_style(accent, p.shell));

    let footer = container(
        row![
            esc_hint,
            Space::new().width(Length::Fill),
            row![cancel_btn, confirm_btn].spacing(8),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Lg)]);

    let card_content = column![
        body,
        crate::sections::divider(&p, crate::sections::DividerAxis::Horizontal),
        footer,
    ]
    .spacing(0);

    let card = container(card_content)
        .width(Length::Fixed(modal_width(ModalSize::Sm)))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(cancel_for_backdrop)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| Style {
            background: Some(iced::Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.55,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    stack![backdrop, centered].into()
}
