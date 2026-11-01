use super::*;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, Density, FONT_XS, ForgePalette, Icon, SaveState, Spacing, field_title,
    ghost_button_with_icon, readonly_mono_field, spacing, tr,
};
use forge_server::ServerSettings;
use gpui::{ClickEvent, Context, div};

use crate::app::async_bridge::{self};
use crate::shared::repo_failure::RepoFailure;
use crate::tools::overlay_build_failure::OverlayBuildFailure;

impl SettingsWebSocketView {
    pub(super) fn toggle_cors(&mut self, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let prev = self.cors_any_origin;
        let value = !prev;
        self.cors_any_origin = value;
        let repo = self.backend.settings_repo();
        self.persist_field(
            prev,
            |this, prev, _| this.cors_any_origin = prev,
            AfterSave::ReloadServer,
            async move {
                ServerSettings::save_overlay_cors_any_origin(repo.as_ref(), value)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn browse_overlay_folder(&mut self, cx: &mut Context<Self>) {
        async_bridge::run_async(
            &self.rt_handle,
            async_bridge::pick_folder(),
            |this, picked, cx| {
                if let Some(path) = picked {
                    this.apply_overlay_root(path, cx);
                }
            },
            cx,
        );
    }

    fn apply_overlay_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.settings_loaded() {
            return;
        }
        let path_str = path.to_string_lossy().into_owned();
        let prev_root = self.overlay_root.replace(path_str.clone());
        let repo = self.backend.settings_repo();
        let overlays = self.overlays.clone();
        let samples = Arc::clone(&self.overlay_samples);
        self.save_state = SaveState::Saving;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                ServerSettings::save_overlay_root(repo.as_ref(), &path_str)
                    .await
                    .map_err(RepoFailure::from)?;
                let pass = overlays.materialize_all(samples.as_ref()).await;
                Ok(OverlayBuildFailure::from_outcome(&pass))
            },
            |this, result: Result<Option<OverlayBuildFailure>, RepoFailure>, cx| {
                let build_failure = result.as_ref().ok().and_then(Option::as_ref);
                if let Some(failure) = build_failure {
                    failure.raise(cx);
                }
                if result.is_err() {
                    this.overlay_root = prev_root;
                }
                this.apply_persist_outcome(result.map(|_| ()), AfterSave::ReloadServer, cx);
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn overlay_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let desc = div()
            .flex()
            .items_center()
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_ws_overlay_folder_prefix")),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(" http://<bind>/"),
            );

        let path_label = self
            .overlay_root
            .clone()
            .unwrap_or_else(|| self.default_overlay_root.clone());
        let path_box = readonly_mono_field(path_label, palette, density, cx);

        let browse =
            ghost_button_with_icon(Icon::FolderOpen, tr!("settings_ws_browse_btn"), palette)
                .on_click(
                    "settings-ws-browse",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.browse_overlay_folder(cx)),
                );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(
                tr!("settings_ws_overlay_section_title"),
                palette,
                cx,
            ))
            .child(desc)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(path_box)
                    .child(browse),
            )
    }
}
