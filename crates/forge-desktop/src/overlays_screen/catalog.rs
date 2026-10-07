use std::collections::HashSet;
use std::sync::Arc;

use forge_soundboard::SoundboardError;
use forge_storage::StoredClip;
use gpui::{Context, Entity, SharedString};

use super::OverlaysView;
use super::icon_choice::{IconImage, OpenIconPicker};
use super::property_panel::{AdoptClipRequested, OverlayPropertyPanel};
use super::sound_choice;
use crate::async_bridge;

#[derive(Default)]
pub(super) struct MediaCatalog {
    pub(super) clip_choices: Vec<(String, String)>,
    clips_gen: async_bridge::Generation,
    pub(super) icon_images: Vec<IconImage>,
    pub(super) icon_favorites: HashSet<SharedString>,
    pub(super) icon_picker: Option<OpenIconPicker>,
    pub(super) images_gen: async_bridge::Generation,
}

impl OverlaysView {
    pub(super) fn load_clips(&mut self, cx: &mut Context<Self>) {
        let ticket = self.catalog.clips_gen.next();
        let library = Arc::clone(&self.handles.library);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { library.list().await },
            move |this, result: Result<Vec<StoredClip>, SoundboardError>, cx| {
                if !this.catalog.clips_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(clips) => this.apply_clips(&clips, cx),
                    Err(error) => {
                        tracing::warn!(%error, "soundboard clips unavailable for the sound picker");
                    }
                }
            },
            cx,
        );
    }

    fn apply_clips(&mut self, clips: &[StoredClip], cx: &mut Context<Self>) {
        self.catalog.clip_choices = sound_choice::clip_choices(clips);
        let choices = self.catalog.clip_choices.clone();
        let Some(panel) = self.editor.panel.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        panel.update(cx, |panel, cx| panel.set_sound_choices(choices, cx));
    }

    pub(super) fn on_adopt_requested(
        &mut self,
        view: Entity<OverlayPropertyPanel>,
        event: &AdoptClipRequested,
        cx: &mut Context<Self>,
    ) {
        let library = Arc::clone(&self.handles.library);
        let clip = event.clip;
        let key = event.key.clone();
        let value = event.value.clone();
        async_bridge::run_async_entity(
            &self.handles.rt_handle,
            view,
            async move {
                let Some(row) = library.get(clip).await? else {
                    return Ok(None);
                };
                let verdict = library.adopt_now(&row).await?;
                Ok::<_, SoundboardError>(Some((row.name, verdict)))
            },
            move |panel, result, cx| {
                panel.settle_clip_pick(key, value, sound_choice::pick_outcome(result), cx);
            },
            cx,
        );
    }
}
