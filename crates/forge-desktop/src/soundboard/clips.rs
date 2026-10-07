use std::sync::Arc;

use forge_components::Icon;
use forge_soundboard::builtin_library::BUILTIN_SOUNDS;
use forge_storage::StoredClip;
use gpui::Context;

use super::categories::category_glyph;
use super::library::glyph_for_name;
use super::{SoundClip, SoundboardView};
use crate::async_bridge;
use crate::clip_messages::failure_message;

impl SoundboardView {
    pub(super) fn reload(&self, cx: &mut Context<Self>) {
        let ticket = self.reload_gen.next();
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move { library.list().await },
            move |this, result, cx| {
                if !this.reload_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(clips) => this.apply_clips(clips, cx),
                    Err(error) => this.on_load_error(failure_message(&error), cx),
                }
            },
            cx,
        );
    }

    fn apply_clips(&mut self, clips: Vec<StoredClip>, cx: &mut Context<Self>) {
        self.keys.set_clips(&clips);
        self.keys.refresh_live();
        self.refresh_availability(clips.clone(), cx);
        self.clips = clips.into_iter().map(stored_to_clip).collect();
        self.loading = false;
        self.error = None;
        self.adopt_summary = None;
        if self
            .category_filter
            .as_ref()
            .is_some_and(|c| !self.clips.iter().any(|clip| &clip.category == c))
        {
            self.category_filter = None;
        }
        self.refresh_library_size(cx);
        self.refresh_builtins(cx);
        cx.notify();
    }

    pub(super) fn on_load_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.loading = false;
        self.error = Some(message.into());
        cx.notify();
    }

    pub(super) fn refresh_library_size(&self, cx: &mut Context<Self>) {
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move { library.total_bytes().await },
            |this, result, cx| {
                this.total_size = match result {
                    Ok(total) => Some(total),
                    Err(error) => {
                        tracing::warn!(error = %error, "media library size unavailable");
                        None
                    }
                };
                cx.notify();
            },
            cx,
        );
    }
}

pub(super) fn stored_to_clip(c: StoredClip) -> SoundClip {
    let glyph = c
        .builtin_id
        .as_deref()
        .and_then(|id| BUILTIN_SOUNDS.iter().find(|e| e.builtin_id == id))
        .and_then(|e| glyph_for_name(e.icon_name).or_else(|| Some(category_glyph(e.category))))
        .unwrap_or(Icon::Music);
    SoundClip {
        id: c.id,
        name: c.name,
        file_path: c.file_path,
        hotkey: c.hotkey,
        category: c.category,
        loop_playback: c.loop_playback,
        duration_secs: c.duration_secs,
        builtin_id: c.builtin_id,
        glyph,
    }
}
