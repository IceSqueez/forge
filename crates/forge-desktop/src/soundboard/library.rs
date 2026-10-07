use std::collections::HashSet;
use std::sync::Arc;

use forge_components::{
    Density, FONT_XXS, ForgePalette, Icon, Spacing, icon, mono_family, pad_tile, spacing, tr,
};
use forge_soundboard::SoundboardError;
use forge_soundboard::builtin_library::{
    BuiltinSoundEntry, builtin_availability, resolve_builtin_path,
};
use forge_storage::StoredClip;
use forge_types::{ClipId, OutputDevice};
use gpui::{AnyElement, ClickEvent, Context, SharedString, div, prelude::*, px};
use time::OffsetDateTime;

use super::categories::{category_color, category_glyph};
use super::labels::section_label;
use super::pad::PAD_GLYPH;
use super::{HOTKEY_FS, SoundboardView};
use crate::async_bridge;
use crate::clip_messages::failure_message;

impl SoundboardView {
    pub(super) fn refresh_builtins(&self, cx: &mut Context<Self>) {
        let imported_ids: HashSet<String> = self.imported_builtin_ids();
        async_bridge::run_blocking(
            &self.rt_handle,
            move || {
                let data_dir = forge_platform_core::paths::data_dir();
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

    fn imported_builtin_ids(&self) -> HashSet<String> {
        self.clips
            .iter()
            .filter_map(|clip| clip.builtin_id.clone())
            .collect()
    }

    pub(super) fn import_builtin(&mut self, entry: BuiltinSoundEntry, cx: &mut Context<Self>) {
        let builtin_id = entry.builtin_id.to_owned();
        let name = entry.display_name.to_owned();
        let category = entry.category.to_owned();
        let loop_playback = entry.loop_playback;
        let library = Arc::clone(&self.library);
        let player = Arc::clone(&self.player);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let data_dir = forge_platform_core::paths::data_dir();
                let Some(path) = resolve_builtin_path(&data_dir, &builtin_id) else {
                    return Err(SoundboardError::SourceMissing(name));
                };
                let clip_id = ClipId::new();
                let clip = StoredClip {
                    id: clip_id,
                    name,
                    file_path: path,
                    volume: 1.0,
                    output_device: OutputDevice::Default,
                    hotkey: None,
                    created_at: OffsetDateTime::now_utc(),
                    category,
                    loop_playback,
                    duration_secs: None,
                    builtin_id: Some(builtin_id),
                };
                library.save_clip(&clip).await?;
                let _ = player.ensure_clip_duration(clip_id).await;
                Ok(())
            },
            |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(error) => this.on_load_error(failure_message(&error), cx),
            },
            cx,
        );
    }

    pub(super) fn render_library(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.importable.is_empty() {
            return None;
        }
        let cards: Vec<AnyElement> = self
            .importable
            .iter()
            .map(|entry| self.render_library_pad(*entry, palette, cx))
            .collect();
        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(section_label(tr!("soundboard_library_section"), palette))
                .child(self.render_grid(cards))
                .into_any_element(),
        )
    }

    fn render_library_pad(
        &self,
        entry: BuiltinSoundEntry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = category_color(entry.category, palette);
        let glyph =
            glyph_for_name(entry.icon_name).unwrap_or_else(|| category_glyph(entry.category));
        pad_tile(
            (
                gpui::ElementId::from("sb-lib"),
                SharedString::new_static(entry.builtin_id),
            ),
            icon(glyph, PAD_GLYPH, color),
            entry.display_name.to_owned(),
            palette,
        )
        .top_right(icon(Icon::Plus, HOTKEY_FS, palette.text_faint))
        .sublabel(
            div()
                .mt(px(3.0))
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("soundboard_library_import")),
        )
        .hover_border(color)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.import_builtin(entry, cx)))
        .into_any_element()
    }
}

pub(super) fn glyph_for_name(name: &str) -> Option<Icon> {
    Some(match name {
        "music" => Icon::Music,
        "repeat" => Icon::Repeat,
        "star" => Icon::Star,
        "flag" => Icon::Flag,
        "speakerphone" => Icon::Speakerphone,
        "sparkles" => Icon::Sparkles,
        "wave-sine" => Icon::WaveSine,
        "wave-saw-tool" => Icon::WaveSawTool,
        "ripple" => Icon::Ripple,
        "hand-click" => Icon::HandClick,
        "user-plus" => Icon::UserPlus,
        "mood-crazy-happy" => Icon::MoodCrazyHappy,
        "mood-sad" => Icon::MoodSmile,
        "alert-triangle" => Icon::AlertTriangle,
        "player-skip-forward" => Icon::PlayerSkipForward,
        "bolt" => Icon::Bolt,
        "volume" => Icon::Volume,
        "eye" => Icon::Eye,
        "x" => Icon::X,
        "message-circle" => Icon::MessageCircle,
        _ => return None,
    })
}
