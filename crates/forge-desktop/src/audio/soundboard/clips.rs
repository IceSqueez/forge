use std::sync::Arc;

use forge_components::{Icon, PushToast, ToastKind};
use forge_soundboard::BUILTIN_SOUNDS;
use forge_storage::StoredClip;
use gpui::Context;

use super::categories::category_glyph;
use super::pads::{PadsEvent, SoundClip, SoundboardPads};
use crate::app::async_bridge::{self, Loadable};
use crate::shared::repo_failure::RepoFailure;

impl SoundboardPads {
    pub(super) fn reload(&self, cx: &mut Context<Self>) {
        let ticket = self.reload_gen.next();
        let library = Arc::clone(&self.handles.library);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { library.list().await },
            move |this, result, cx| {
                if !this.reload_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(clips) => this.apply_clips(clips, cx),
                    Err(error) => this.on_load_error(RepoFailure::operation(error), cx),
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
        self.list_load = Loadable::Loaded(());
        self.error = None;
        if self
            .category_filter
            .as_ref()
            .is_some_and(|c| !self.clips.iter().any(|clip| &clip.category == c))
        {
            self.category_filter = None;
        }
        cx.emit(PadsEvent::ClipsLoaded);
        cx.notify();
    }

    pub(super) fn on_load_error(&mut self, failure: RepoFailure, cx: &mut Context<Self>) {
        self.error = Some(failure.text().into());
        self.list_load = Loadable::Failed(failure);
        cx.notify();
    }

    pub(super) fn retry_list_load(&mut self, cx: &mut Context<Self>) {
        self.list_load = Loadable::Loading;
        self.reload(cx);
        cx.notify();
    }

    pub(super) fn on_action_failure(&mut self, message: String, cx: &mut Context<Self>) {
        cx.push_toast(ToastKind::Error, message);
        cx.notify();
    }
}

pub(super) fn stored_to_clip(c: StoredClip) -> SoundClip {
    let glyph = c
        .builtin_id
        .as_deref()
        .and_then(|id| BUILTIN_SOUNDS.iter().find(|e| e.builtin_id == id))
        .and_then(|e| Icon::named(e.icon_name).or_else(|| Some(category_glyph(e.category))))
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
