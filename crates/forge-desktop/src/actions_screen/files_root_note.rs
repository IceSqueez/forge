use crate::toasts::PushToast;
use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Spacing, ToastKind, body_family,
    ghost_button_with_icon, mono_family, spacing, tr,
};
use gpui::{AnyElement, App, ClickEvent, Window, div, prelude::*};

pub(super) fn files_root_note(palette: &ForgePalette) -> AnyElement {
    let root = forge_platform_core::paths::assets_dir();
    let caption = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(tr!("actions_sub_files_root_label")),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(root.display().to_string()),
        )
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("actions_sub_files_root_hint")),
        );
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Sm, Density::Cozy))
        .child(div().flex_1().overflow_hidden().child(caption))
        .child(
            ghost_button_with_icon(
                Icon::FolderOpen,
                tr!("actions_sub_files_root_open"),
                palette,
            )
            .on_click(
                "actions-sub-files-root-open",
                |_: &ClickEvent, _: &mut Window, cx: &mut App| open_files_root(cx),
            ),
        )
        .into_any_element()
}

fn open_files_root(cx: &mut App) {
    let root = forge_platform_core::paths::assets_dir();
    if let Err(e) = std::fs::create_dir_all(&root) {
        cx.push_toast(
            ToastKind::Error,
            tr!(
                "actions_sub_files_root_create_failed",
                reason = e.to_string()
            ),
        );
        return;
    }
    cx.reveal_path(&root);
}
