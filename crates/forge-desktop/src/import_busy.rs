use forge_components::{
    ForgePalette, OverlayPosition, PushToast, ToastKind, busy_modal, overlay, tr,
};
use gpui::{AnyElement, Context, div, prelude::*};

pub(crate) fn busy_overlay(
    id: &'static str,
    title: String,
    detail: String,
    palette: &ForgePalette,
) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            overlay(busy_modal(id, title, detail, palette), palette)
                .position(OverlayPosition::Center),
        )
        .into_any_element()
}

pub(crate) fn refuse_while_busy<V: 'static>(cx: &mut Context<V>) {
    tracing::info!("import ignored while another import runs");
    cx.push_toast(ToastKind::Warn, tr!("import_busy_already"));
}
