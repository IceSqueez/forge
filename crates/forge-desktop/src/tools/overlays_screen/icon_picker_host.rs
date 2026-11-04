use forge_components::{
    ActivePresentation, ForgePalette, GridPicker, GridPickerArt, GridPickerConfig, GridPickerEvent,
    GridPickerSubtitle, Icon, OverlayPosition, focus_inside_modal, overlay, tr,
};
use forge_overlay::config::ICON;
use forge_overlay::effective_overlay_config;
use forge_storage::{MediaBlob, MediaRepo, OverlayDefinition, StorageError};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Rgba, Subscription, Window, prelude::*,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::OverlaysView;
use super::icon_choice::{
    IconChoice, IconImage, IconPick, PICK_ART, PICK_TILE, icon_art, icon_choice,
    image_dialog_extensions, image_rows, import_outcome, offers_library_images_only, picked_icon,
    picker_groups,
};
pub(crate) use super::property_panel::IconPickResult;
use super::property_panel::{IconPickRequested, OverlayPropertyPanel};
use crate::app::async_bridge::{self, DialogFilter};
use crate::shared::picker_favorites::{StoredFavorites, bind_picker, follow_picker_event};

pub(crate) enum IconHostEvent {
    ImagesChanged,
    Picked {
        key: String,
        value: String,
    },
    ImportStarted {
        key: String,
    },
    ImportCancelled {
        key: String,
    },
    ImportSettled {
        key: String,
        outcome: IconPickResult,
    },
}

struct OpenIconPicker {
    key: String,
    accent: Rgba,
    view: Entity<GridPicker>,
    focused: bool,
    _sub: Subscription,
    _favorites: Subscription,
}

pub(crate) struct IconPickerHost {
    images: Vec<IconImage>,
    picker: Option<OpenIconPicker>,
    images_gen: async_bridge::Generation,
    media: Arc<dyn MediaRepo>,
    rt_handle: tokio::runtime::Handle,
    favorites: Entity<StoredFavorites>,
}

impl EventEmitter<IconHostEvent> for IconPickerHost {}

impl IconPickerHost {
    pub(crate) fn new(
        media: Arc<dyn MediaRepo>,
        rt_handle: tokio::runtime::Handle,
        favorites: Entity<StoredFavorites>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut host = Self {
            images: Vec::new(),
            picker: None,
            images_gen: async_bridge::Generation::default(),
            media,
            rt_handle,
            favorites,
        };
        host.load_images(cx);
        host
    }

    pub(crate) fn images(&self) -> &[IconImage] {
        &self.images
    }

    pub(crate) fn load_images(&mut self, cx: &mut Context<Self>) {
        let ticket = self.images_gen.next();
        let media = Arc::clone(&self.media);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let blobs = media.list().await?;
                let mut resolved: Vec<IconImage> = Vec::new();
                for (id, blob) in image_rows(blobs) {
                    let path = match media.resolve(&id).await {
                        Ok(path) => path,
                        Err(error) => {
                            tracing::warn!(%error, "imported icon image unreadable; left out");
                            continue;
                        }
                    };
                    resolved.push(IconImage {
                        id: blob.id.as_str().to_owned(),
                        label: blob.label,
                        format: blob.format,
                        path: Arc::from(path),
                    });
                }
                Ok::<_, StorageError>(resolved)
            },
            move |this, result: Result<Vec<IconImage>, StorageError>, cx| {
                if !this.images_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(images) => this.apply_images(images, cx),
                    Err(error) => {
                        tracing::warn!(%error, "media library unavailable for the icon picker");
                    }
                }
            },
            cx,
        );
    }

    fn apply_images(&mut self, images: Vec<IconImage>, cx: &mut Context<Self>) {
        if self.images == images {
            return;
        }
        self.images = images;
        cx.emit(IconHostEvent::ImagesChanged);
        if let Some((key, picker, accent)) = self
            .picker
            .as_ref()
            .map(|open| (open.key.clone(), open.view.clone(), open.accent))
        {
            let palette = cx.palette();
            let groups = picker_groups(&key, &self.images, accent, &palette);
            picker.update(cx, |picker, cx| picker.set_groups(groups, cx));
        }
        cx.notify();
    }

    pub(crate) fn open_picker(&mut self, key: String, accent: Rgba, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let groups = picker_groups(&key, &self.images, accent, &palette);
        let (title, subtitle, search) = if offers_library_images_only(&key) {
            (
                tr!("overlays_image_picker_title"),
                tr!("overlays_image_picker_subtitle"),
                tr!("overlays_image_picker_search"),
            )
        } else {
            (
                tr!("overlays_icon_picker_title"),
                tr!("overlays_icon_picker_subtitle"),
                tr!("overlays_icon_picker_search"),
            )
        };
        let config = GridPickerConfig {
            accent,
            art: GridPickerArt {
                tile: PICK_TILE,
                glyph: PICK_ART,
            },
            header_icon: Icon::Photo,
            title: title.into(),
            subtitle: GridPickerSubtitle::Plain(subtitle.into()),
            footer_hint: tr!("overlays_icon_picker_hint").into(),
            search_placeholder: search.into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.favorites.read(cx).ids().cloned().unwrap_or_default();
        let view = cx.new(|cx| GridPicker::new(config, groups, favorites, cx));
        let sub = cx.subscribe(&view, Self::on_icon_picker_event);
        let favorites_sub = bind_picker(&self.favorites, &view, cx);
        self.picker = Some(OpenIconPicker {
            key,
            accent,
            view,
            focused: false,
            _sub: sub,
            _favorites: favorites_sub,
        });
        cx.notify();
    }

    fn on_icon_picker_event(
        &mut self,
        _view: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => self.apply_icon_pick(id.to_string(), cx),
            GridPickerEvent::FavoriteToggled(_) | GridPickerEvent::FavoritesRetry => {
                follow_picker_event(&self.favorites, event, cx);
            }
            GridPickerEvent::Dismissed => self.close_icon_picker(cx),
        }
    }

    fn apply_icon_pick(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(key) = self.picker.as_ref().map(|open| open.key.clone()) else {
            return;
        };
        self.picker = None;
        cx.notify();

        match picked_icon(&id) {
            IconPick::Store(value) => cx.emit(IconHostEvent::Picked { key, value }),
            IconPick::Import => {
                cx.emit(IconHostEvent::ImportStarted { key: key.clone() });
                self.browse_icon_image(key, cx);
            }
        }
    }

    fn browse_icon_image(&mut self, key: String, cx: &mut Context<Self>) {
        let filter = DialogFilter {
            name: tr!("overlays_icon_file_filter"),
            extensions: image_dialog_extensions(),
        };
        async_bridge::run_async(
            &self.rt_handle,
            async_bridge::pick_file(Some(filter)),
            move |this, picked: Option<PathBuf>, cx| match picked {
                Some(path) => this.import_icon_image(key, path, cx),
                None => cx.emit(IconHostEvent::ImportCancelled { key }),
            },
            cx,
        );
    }

    fn import_icon_image(&mut self, key: String, path: PathBuf, cx: &mut Context<Self>) {
        let media = Arc::clone(&self.media);
        async_bridge::run_async(
            &self.rt_handle,
            async move { media.import_file(&path).await },
            move |this, result: Result<MediaBlob, StorageError>, cx| {
                let outcome = import_outcome(result);
                let settled = matches!(outcome, IconPickResult::Chosen(_));
                cx.emit(IconHostEvent::ImportSettled { key, outcome });
                if settled {
                    this.load_images(cx);
                }
            },
            cx,
        );
    }

    fn close_icon_picker(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        cx.notify();
    }

    pub(crate) fn render_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.picker.as_ref()?;
        let view = cx.entity();
        Some(
            overlay(open.view.clone(), palette)
                .position(OverlayPosition::Center)
                .on_dismiss("overlays-icon-picker-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.close_icon_picker(cx));
                })
                .into_any_element(),
        )
    }

    pub(crate) fn focus_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.picker.as_mut().filter(|open| !open.focused) else {
            return;
        };
        open.focused = true;
        let picker = open.view.clone();
        focus_inside_modal(window, cx, |window, cx| {
            picker.update(cx, |picker, cx| picker.focus(window, cx))
        });
    }
}

impl OverlaysView {
    fn icon_accent(&self, palette: &ForgePalette) -> Rgba {
        self.selected_definition()
            .map(|definition| self.visuals(definition, palette).page_accent)
            .unwrap_or(palette.brand)
    }

    pub(super) fn on_icon_pick_requested(
        &mut self,
        _view: Entity<OverlayPropertyPanel>,
        event: &IconPickRequested,
        cx: &mut Context<Self>,
    ) {
        let key = event.key.clone();
        let palette = cx.palette();
        let accent = self.icon_accent(&palette);
        self.icon_host.update(cx, |host, cx| {
            host.open_picker(key, accent, cx);
            host.load_images(cx);
        });
    }

    pub(super) fn on_icon_host_event(
        &mut self,
        _host: Entity<IconPickerHost>,
        event: &IconHostEvent,
        cx: &mut Context<Self>,
    ) {
        if let IconHostEvent::ImagesChanged = event {
            self.sync_preview(cx);
            cx.notify();
        }
        let Some(panel) = self.editor.panel.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        match event {
            IconHostEvent::ImagesChanged => {
                let images = self.icon_host.read(cx).images().to_vec();
                panel.update(cx, |panel, cx| panel.set_icon_images(images, cx));
            }
            IconHostEvent::Picked { key, value } => panel.update(cx, |panel, cx| {
                panel.settle_icon_pick(key.clone(), IconPickResult::Chosen(value.clone()), cx);
            }),
            IconHostEvent::ImportStarted { key } => {
                panel.update(cx, |panel, cx| panel.begin_icon_import(key.clone(), cx));
            }
            IconHostEvent::ImportCancelled { key } => {
                panel.update(cx, |panel, cx| panel.cancel_icon_import(key.clone(), cx));
            }
            IconHostEvent::ImportSettled { key, outcome } => {
                panel.update(cx, |panel, cx| {
                    panel.settle_icon_import(key.clone(), outcome.clone(), cx);
                });
            }
        }
    }

    pub(super) fn preview_icon(
        &self,
        definition: &OverlayDefinition,
        cx: &App,
    ) -> Option<forge_components::GlyphArt> {
        let descriptor = self.handles.kinds.get(&definition.kind_id)?;
        let config = effective_overlay_config(descriptor, &definition.config);
        let stored = config.get(ICON).and_then(forge_types::Variant::as_str)?;
        icon_art(&icon_choice(stored, self.icon_host.read(cx).images()))
    }

    pub(super) fn preview_image(
        &self,
        definition: &OverlayDefinition,
        key: &str,
        cx: &App,
    ) -> Option<Arc<Path>> {
        let descriptor = self.handles.kinds.get(&definition.kind_id)?;
        let config = effective_overlay_config(descriptor, &definition.config);
        let stored = config.get(key).and_then(forge_types::Variant::as_str)?;
        match icon_choice(stored, self.icon_host.read(cx).images()) {
            IconChoice::Image(image) => Some(Arc::clone(&image.path)),
            IconChoice::None | IconChoice::Glyph(_) | IconChoice::Unresolved(_) => None,
        }
    }
}
