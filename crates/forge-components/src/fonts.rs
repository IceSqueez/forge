use std::borrow::Cow;

/// Embedded typeface bytes the shell registers via `text_system().add_fonts(..)`
/// before opening its window. Covers the Inter body family (Regular / Medium /
/// SemiBold) and JetBrains Mono, whose in-file family names resolve to
/// [`crate::tokens::DEFAULT_BODY_FAMILY`] and [`crate::tokens::DEFAULT_MONO_FAMILY`]
/// respectively. Without this registration gpui paints every glyph in its
/// built-in fallback face.
pub fn embedded_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice()),
    ]
}
