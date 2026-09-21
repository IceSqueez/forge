use std::path::Path;
use std::sync::Arc;

use gpui::{AnyElement, ElementId, Pixels, Rgba, img, prelude::*, svg};

use crate::icons::{Icon, icon};

#[derive(Clone)]
pub enum GlyphArt {
    Icon(Icon),
    Svg(&'static [u8]),
    Image(Arc<Path>),
}

pub fn glyph_art(art: &GlyphArt, size: Pixels, tint: Rgba, id: impl Into<ElementId>) -> AnyElement {
    match art {
        GlyphArt::Icon(glyph) => icon(*glyph, size, tint).into_any_element(),
        GlyphArt::Svg(bytes) => svg()
            .flex_none()
            .size(size)
            .data(bytes)
            .text_color(tint)
            .into_any_element(),
        GlyphArt::Image(path) => img(Arc::clone(path))
            .id(id)
            .flex_none()
            .size(size)
            .into_any_element(),
    }
}
