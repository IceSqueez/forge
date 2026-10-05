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
