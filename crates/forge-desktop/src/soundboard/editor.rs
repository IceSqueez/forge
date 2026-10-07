use std::sync::Arc;

use forge_components::{ForgePalette, Icon, ToastKind, icon, pad_tile, tr};
use forge_soundboard::SoundboardError;
use forge_storage::{MediaFormat, MediaKind, StoredClip};
use forge_types::{ClipId, OutputDevice};
use gpui::{AnyElement, ClickEvent, Context, Entity, Pixels, Window, prelude::*, px};
use time::OffsetDateTime;

use super::SoundboardView;
use super::categories::CATEGORY_ORDER;
use super::keys::unregistered_combo;
use crate::async_bridge;
use crate::clip_editor::{ClipDraft, ClipEditor, ClipEditorEvent, ClipEditorLaunch};
use crate::clip_key_state::canonical_combo;
use crate::clip_messages::failure_message;
use crate::combo_conflict::release_holder;
use crate::toasts::PushToast;

const ADD_ICON: Pixels = px(13.0);

impl SoundboardView {
    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_editor(
            ClipEditorLaunch {
                edit_id: None,
                name: String::new(),
                category: CATEGORY_ORDER[0].to_owned(),
                file_path: None,
                loop_playback: false,
                hotkey: None,
            },
            window,
            cx,
        );
    }

    pub(super) fn open_edit(&mut self, id: ClipId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clip) = self.clips.iter().find(|c| c.id == id) else {
            return;
        };
        let launch = ClipEditorLaunch {
            edit_id: Some(id),
            name: clip.name.clone(),
            category: clip.category.clone(),
            file_path: Some(clip.file_path.clone()),
            loop_playback: clip.loop_playback,
            hotkey: clip.hotkey.as_deref().map(canonical_combo),
        };
        self.open_editor(launch, window, cx);
    }

    fn open_editor(
        &mut self,
        launch: ClipEditorLaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rt_handle = self.rt_handle.clone();
        let holders = self.keys.available().then(|| self.keys.holders());
        let switch = self.integration_switch();
        let modal = cx.new(|cx| {
            let editor = ClipEditor::new(launch, holders, rt_handle, cx);
            match switch {
                Some(switch) => editor.with_integration_switch(switch, cx),
                None => editor,
            }
        });
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        self.reload_key_holders(cx);
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _modal: Entity<ClipEditor>,
        event: &ClipEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ClipEditorEvent::Submit(draft) => self.persist(draft, cx),
            ClipEditorEvent::Cancel => self.close_modal(cx),
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self._modal_sub = None;
        cx.notify();
    }

    fn persist(&mut self, draft: &ClipDraft, cx: &mut Context<Self>) {
        let name = draft.name.clone();
        let file_path = draft.file_path.clone();
        let category = draft.category.clone();
        let loop_playback = draft.loop_playback;
        let edit_id = draft.edit_id;
        let hotkey = draft.hotkey.clone();
        let release = match (draft.release.clone(), draft.hotkey.clone()) {
            (Some(holder), Some(combo)) => self
                .keys
                .reconciler()
                .map(|reconciler| (holder.label(), holder, combo, Arc::clone(reconciler))),
            _ => None,
        };
        let reconciler = self.keys.reconciler().cloned();
        let backend = Arc::clone(self.keys.backend());

        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| m.set_saving(cx));
        }
        let library = Arc::clone(&self.library);
        let player = Arc::clone(&self.player);
        let saved_name = name.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let (clip, target_id) = match edit_id {
                    Some(id) => {
                        let Some(mut clip) = library.get(id).await? else {
                            return Err(SoundboardError::ClipNotFound(id.to_string()));
                        };
                        clip.name = name;
                        if clip.file_path != file_path {
                            clip.file_path = file_path;
                            clip.duration_secs = None;
                        }
                        clip.category = category;
                        clip.loop_playback = loop_playback;
                        clip.hotkey = hotkey.clone();
                        (clip, id)
                    }
                    None => {
                        let clip_id = ClipId::new();
                        let clip = StoredClip {
                            id: clip_id,
                            name,
                            file_path,
                            volume: 1.0,
                            output_device: OutputDevice::Default,
                            hotkey: hotkey.clone(),
                            created_at: OffsetDateTime::now_utc(),
                            category,
                            loop_playback,
                            duration_secs: None,
                            builtin_id: None,
                        };
                        (clip, clip_id)
                    }
                };
                library.save_clip(&clip).await?;
                let shared = match release {
                    Some((label, holder, combo, reconciler)) => {
                        release_holder(holder, combo.clone(), reconciler, backend)
                            .await
                            .err()
                            .map(|_| SharedCombo {
                                combo: canonical_combo(&combo),
                                holder: label,
                            })
                    }
                    None => None,
                };
                let _ = player.ensure_clip_duration(target_id).await;
                Ok::<_, SoundboardError>(ClipSaved {
                    unregistered: unregistered_combo(hotkey.as_deref(), reconciler.as_deref()),
                    shared,
                })
            },
            move |this, result, cx| match result {
                Ok(saved) => this.on_saved(&saved_name, saved, cx),
                Err(error) => this.on_save_error(failure_message(&error), cx),
            },
            cx,
        );
        cx.notify();
    }

    fn on_saved(&mut self, name: &str, saved: ClipSaved, cx: &mut Context<Self>) {
        if let Some(shared) = saved.shared {
            cx.push_toast(
                ToastKind::Warn,
                tr!(
                    "soundboard_toast_key_still_shared",
                    name = name,
                    combo = shared.combo.as_str(),
                    holder = shared.holder.as_str()
                ),
            );
        }
        if let Some(combo) = saved.unregistered {
            cx.push_toast(
                ToastKind::Warn,
                tr!(
                    "soundboard_toast_key_not_registered",
                    name = name,
                    combo = combo.as_str()
                ),
            );
        }
        self.close_modal(cx);
        self.reload(cx);
        self.reload_key_holders(cx);
    }

    fn on_save_error(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| m.fail_save(message, cx));
        }
        cx.notify();
    }

    pub(super) fn render_add_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        pad_tile(
            "sb-add-bar",
            icon(Icon::Plus, ADD_ICON, palette.bits),
            tr!("soundboard_add_sound"),
            palette,
        )
        .bar(palette)
        .title_color(palette.bits)
        .hover_border(palette.bits)
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_add(window, cx)))
        .into_any_element()
    }
}

struct ClipSaved {
    unregistered: Option<String>,
    shared: Option<SharedCombo>,
}

struct SharedCombo {
    combo: String,
    holder: String,
}

pub(crate) fn audio_dialog_extensions() -> Vec<&'static str> {
    MediaFormat::ACCEPTED
        .iter()
        .copied()
        .filter(|format| format.kind() == MediaKind::Audio)
        .map(MediaFormat::as_str)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_import_dialog_offers_every_accepted_audio_format_and_nothing_else() {
        let mut offered = audio_dialog_extensions();
        offered.sort_unstable();
        assert_eq!(offered, ["flac", "m4a", "mp3", "ogg", "wav"]);
    }
}
