use super::*;
use crate::app::async_bridge;
use crate::automation::actions_screen::sub_action_modal::{EditSubActionForm, SubFormCommit};
use crate::shared::picker_favorites::StoredFavorites;
use crate::tools::overlays_screen::icon_choice::ICON_FAVORITES;
use crate::tools::overlays_screen::icon_picker_host::{
    IconHostEvent, IconPickResult, IconPickerHost,
};
use forge_overlay::config::IMAGE;
use forge_overlay::{MediaValue, SHOW_SOUND, read_media_value};
use forge_soundboard::SoundboardError;
use forge_storage::{MediaRepo, SettingsRepo};
use forge_types::ClipId;
use gpui::{Entity, Subscription};

const SHOW_SOUND_ADOPTION_CONTEXT: &str = "adopt the show sound into the media library";

pub(super) struct ShowMediaPicker {
    host: Entity<IconPickerHost>,
    _subs: [Subscription; 2],
}

impl ActionEditor {
    pub(in crate::automation::actions_screen) fn attach_media_library(
        &mut self,
        media: Arc<dyn MediaRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        cx: &mut Context<Self>,
    ) {
        let rt_handle = self.deps.rt_handle.clone();
        let favorites = StoredFavorites::shared(ICON_FAVORITES, settings_repo, &rt_handle, cx);
        let host = cx.new(|cx| IconPickerHost::new(media, rt_handle, favorites, cx));
        let subs = [
            cx.subscribe(&host, Self::on_show_media_event),
            cx.observe(&host, |_, _, cx| cx.notify()),
        ];
        self.show_media = Some(ShowMediaPicker { host, _subs: subs });
        cx.notify();
    }

    pub(super) fn offer_show_images(&self, form: &Entity<EditSubActionForm>, cx: &mut App) {
        let Some(picker) = &self.show_media else {
            return;
        };
        let images = picker.host.read(cx).images().to_vec();
        form.update(cx, |form, cx| form.offer_show_images(images, cx));
    }

    pub(super) fn open_show_image_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &self.show_media else {
            return;
        };
        let accent = cx.palette().brand;
        picker.host.update(cx, |host, cx| {
            host.open_picker(IMAGE.to_owned(), accent, cx);
            host.load_images(cx);
        });
    }

    fn on_show_media_event(
        &mut self,
        host: Entity<IconPickerHost>,
        event: &IconHostEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.modals.sub_form.clone() else {
            return;
        };
        match event {
            IconHostEvent::ImagesChanged => {
                let images = host.read(cx).images().to_vec();
                form.update(cx, |form, cx| form.offer_show_images(images, cx));
            }
            IconHostEvent::Picked { value, .. } => form.update(cx, |form, cx| {
                form.settle_show_image(IconPickResult::Chosen(value.clone()), cx);
            }),
            IconHostEvent::ImportStarted { .. } => {
                form.update(cx, |form, cx| form.begin_show_image_import(cx));
            }
            IconHostEvent::ImportCancelled { .. } => {
                form.update(cx, |form, cx| form.cancel_show_image_import(cx));
            }
            IconHostEvent::ImportSettled { outcome, .. } => {
                form.update(cx, |form, cx| form.settle_show_image(outcome.clone(), cx));
            }
        }
    }

    pub(super) fn render_show_media_layer(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.show_media.as_ref()?;
        picker
            .host
            .update(cx, |host, cx| host.render_overlay(palette, cx))
    }

    pub(super) fn focus_show_media_picker(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = &self.show_media {
            picker
                .host
                .update(cx, |host, cx| host.focus_picker(window, cx));
        }
    }

    pub(super) fn adopt_show_sound(&self, commit: &SubFormCommit) {
        let Some(clip) = commit
            .overrides
            .iter()
            .find(|(key, _)| key == SHOW_SOUND)
            .and_then(|(_, value)| value.as_str())
            .and_then(picked_show_clip)
        else {
            return;
        };
        let library = Arc::clone(&self.deps.clip_library);
        async_bridge::detached(
            &self.deps.rt_handle,
            SHOW_SOUND_ADOPTION_CONTEXT,
            async move {
                let Some(row) = library.get(clip).await? else {
                    return Ok(());
                };
                let verdict = library.adopt_now(&row).await?;
                tracing::debug!(?verdict, "show sound adoption settled");
                Ok::<(), SoundboardError>(())
            },
        );
    }
}

fn picked_show_clip(value: &str) -> Option<ClipId> {
    match read_media_value(value) {
        MediaValue::Clip(clip) => clip.parse().ok(),
        MediaValue::Empty | MediaValue::File(_) => None,
    }
}
