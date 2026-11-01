use forge_components::PushToast;
use forge_components::{
    ActivePresentation, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Spacing, ToastKind,
    ghost_button_with_icon, spacing, tr,
};
use gpui::{AnyElement, App, ClickEvent, Window, div, prelude::*};

use crate::shared::repo_failure::RepoFailure;

pub(super) fn files_root_note(palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
    let root = crate::app::app_paths::app_paths().assets_dir();
    let caption = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(tr!("actions_sub_files_root_label")),
        )
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(root.display().to_string()),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("actions_sub_files_root_hint")),
        );
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Sm, density))
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
    let root = crate::app::app_paths::app_paths().assets_dir();
    cx.spawn(async move |cx| {
        let created = cx
            .background_spawn({
                let root = root.clone();
                async move { std::fs::create_dir_all(root) }
            })
            .await;
        cx.update(|cx| match created {
            Ok(()) => cx.reveal_path(&root),
            Err(e) => cx.push_toast(
                ToastKind::Error,
                tr!(
                    "actions_sub_files_root_create_failed",
                    reason = RepoFailure::from(e).text()
                ),
            ),
        })
    })
    .detach();
}
