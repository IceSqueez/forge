use forge_components::{ForgePalette, error_row, tr};
use gpui::{AnyElement, App, Global, IntoElement, SharedString};

pub(crate) struct ServerUnavailable(pub SharedString);

impl Global for ServerUnavailable {}

pub(crate) fn unavailable_banner(cx: &App, palette: &ForgePalette) -> Option<AnyElement> {
    let reason = cx.try_global::<ServerUnavailable>()?;
    Some(
        error_row(
            tr!("server_unavailable_banner", reason = reason.0.as_ref()),
            palette,
        )
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::presentation::ActivePresentation;
    use crate::test_support::install_presentation;

    #[gpui::test]
    fn the_banner_is_shown_only_once_a_startup_failure_reason_is_recorded(cx: &mut TestAppContext) {
        install_presentation(cx);
        let shown = |cx: &mut TestAppContext| {
            cx.update(|cx| unavailable_banner(cx, &cx.palette()).is_some())
        };

        let before = shown(cx);
        cx.update(|cx| cx.set_global(ServerUnavailable("port in use".into())));
        let after = shown(cx);

        assert_eq!((before, after), (false, true));
    }
}
