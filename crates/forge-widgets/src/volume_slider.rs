use iced::{
    Alignment, Element, Length,
    widget::{row, text},
};

use crate::palette::ForgePalette;
use crate::slider::slider;
use crate::tokens::{FONT_SM, FontRole, Spacing, font, spf};

pub fn volume_slider<'a, Msg: 'a + Clone>(
    value: f32,
    on_change: impl Fn(f32) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let pct = (value * 100.0).round() as u32;
    let pct_color = if value > 1.0 {
        palette.warning
    } else {
        palette.text_secondary
    };
    let pct_label = format!("{pct}%");
    let label_w = 36.0_f32;
    let gap = spf(Spacing::Sm);

    row![
        text(crate::tr!("widget.volume.label"))
            .size(FONT_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace))
            .width(label_w),
        slider(0.0..=1.5, value, on_change, palette).width(Length::Fill),
        text(pct_label)
            .size(FONT_SM)
            .color(pct_color)
            .font(font(FontRole::Monospace))
            .width(label_w),
    ]
    .spacing(gap)
    .align_y(Alignment::Center)
    .into()
}
