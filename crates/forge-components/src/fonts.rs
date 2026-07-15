use std::borrow::Cow;

/// The shell must register these via `text_system().add_fonts(..)` before opening
/// its window, or gpui falls back off [`crate::tokens::DEFAULT_BODY_FAMILY`] /
/// [`crate::tokens::DEFAULT_MONO_FAMILY`] to its built-in face.
pub fn embedded_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice()),
    ]
}
