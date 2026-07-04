use std::borrow::Cow;

use iced::{
    Alignment, Background, Border, Color, Element, Font, Theme, font::Weight, widget::container,
    widget::text,
};

/// Corner radius as a fraction of tile size: 44px -> ~10, 48px -> ~11.
const CORNER_RATIO: f32 = 0.23;
/// Initial glyph size as a fraction of tile size: 44px -> 22, 48px -> 24.
const GLYPH_RATIO: f32 = 0.5;

/// Rounded, brand-filled square holding a platform's centered initial.
///
/// `size` drives both the corner radius and the glyph size, so the 44px
/// overview card and the 48px detail header share one shape. `brand_color`
/// fills the tile; `ink_color` paints the initial (the design's `--ink`).
pub fn platform_identity_tile<'a, Msg: 'a>(
    letter: impl Into<Cow<'a, str>>,
    brand_color: Color,
    ink_color: Color,
    size: f32,
) -> Element<'a, Msg> {
    let corner = size * CORNER_RATIO;
    let glyph = size * GLYPH_RATIO;
    let letter: Cow<'a, str> = letter.into();

    container(text(letter).size(glyph).color(ink_color).font(Font {
        weight: Weight::Semibold,
        ..Font::DEFAULT
    }))
    .width(size)
    .height(size)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(brand_color)),
        border: Border {
            radius: corner.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}
