use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, FONT_XXS, ForgePalette, Icon, PushToast, Spacing, ToastKind, icon,
    pad_tile, scaled, spacing, tr,
};
use forge_soundboard::{
    BuiltinSoundEntry, NewClip, SoundboardError, builtin_availability, resolve_builtin_path,
};
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, SharedString, Window, div, prelude::*, px,
};

use super::categories::{category_color, category_glyph};
use super::labels::section_label;
use super::pad::{PAD_GLYPH, pad_grid};
use super::{SMALL_GLYPH, SoundboardHandles};
use crate::app::async_bridge;
use crate::audio::clip_messages::failure_message;

const SUBLABEL_MT: gpui::Pixels = px(3.0);

pub(super) enum LibraryEvent {
    Imported,
}

pub(super) struct SoundboardLibrary {
    importable: Vec<BuiltinSoundEntry>,
    handles: SoundboardHandles,
}

impl EventEmitter<LibraryEvent> for SoundboardLibrary {}

impl SoundboardLibrary {
    pub(super) fn new(handles: SoundboardHandles) -> Self {
        Self {
            importable: Vec::new(),
            handles,
        }
    }

    pub(super) fn has_entries(&self) -> bool {
        !self.importable.is_empty()
    }

    pub(super) fn refresh_builtins(&self, imported_ids: HashSet<String>, cx: &mut Context<Self>) {
        async_bridge::run_blocking(
            &self.handles.rt_handle,
            move || {
                let data_dir = crate::app::app_paths::app_paths().data_dir();
                let available = builtin_availability(&data_dir);
                available
                    .into_iter()
                    .filter(|(entry, present)| *present && !imported_ids.contains(entry.builtin_id))
                    .map(|(entry, _)| entry)
                    .collect::<Vec<BuiltinSoundEntry>>()
            },
            |this, entries, cx| {
                this.importable = entries;
                cx.notify();
            },
            cx,
        );
    }

    fn import_builtin(&mut self, entry: BuiltinSoundEntry, cx: &mut Context<Self>) {
        let builtin_id = entry.builtin_id.to_owned();
        let name = entry.display_name.to_owned();
        let category = entry.category.as_str().to_owned();
        let library = Arc::clone(&self.handles.library);
        let player = Arc::clone(&self.handles.player);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                let data_dir = crate::app::app_paths::app_paths().data_dir();
                let Some(path) = resolve_builtin_path(&data_dir, &builtin_id) else {
                    return Err(SoundboardError::SourceMissing(name));
                };
                let clip_id = library
                    .create_clip(NewClip {
                        name,
                        file_path: path,
                        hotkey: None,
                        category,
                        loop_playback: false,
                        builtin_id: Some(builtin_id),
                    })
                    .await?;
                if let Err(error) = player.ensure_clip_duration(clip_id).await {
                    tracing::warn!(%error, "clip duration could not be measured");
                }
                Ok(())
            },
            |_this, result, cx| match result {
                Ok(()) => cx.emit(LibraryEvent::Imported),
                Err(error) => cx.push_toast(ToastKind::Error, failure_message(&error)),
            },
            cx,
        );
    }

    fn render_library_pad(
        &self,
        entry: BuiltinSoundEntry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let color = category_color(entry.category.as_str(), palette);
        let glyph = Icon::named(entry.icon_name).unwrap_or_else(|| category_glyph(entry.category));
        pad_tile(
            (
                gpui::ElementId::from("sb-lib"),
                SharedString::new_static(entry.builtin_id),
            ),
            icon(glyph, PAD_GLYPH, color),
            entry.display_name.to_owned(),
            palette,
        )
        .top_right(icon(Icon::Plus, SMALL_GLYPH, palette.text_faint))
        .sublabel(
            div()
                .mt(scaled(SUBLABEL_MT, density))
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("soundboard_library_import")),
        )
        .hover_border(color)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.import_builtin(entry, cx)))
        .into_any_element()
    }
}

impl Render for SoundboardLibrary {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let cards: Vec<AnyElement> = self
            .importable
            .iter()
            .map(|entry| self.render_library_pad(*entry, &palette, cx))
            .collect();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(
                tr!("soundboard_library_section"),
                &palette,
                cx,
            ))
            .child(pad_grid(cards))
    }
}
