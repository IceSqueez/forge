use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_components::{
    BORDER_THIN, Density, ForgePalette, GlyphArt, GridPicker, GridPickerConfig, GridPickerEvent,
    GridPickerGroup, GridPickerItem, GridPickerItemState, GridPickerSubtitle, Icon,
    OverlayPosition, Radius, Spacing, body_family, glyph_art, icon, overlay, radius, spacing, tr,
};
use forge_overlay::config::ICON;
use forge_overlay::{
    CURATED_ICONS, CuratedIcon, IconCategory, IconValue, curated_icon, effective_overlay_config,
    image_reference, read_icon_value,
};
use forge_storage::{
    MediaBlob, MediaBlobId, MediaFormat, MediaKind, OverlayDefinition, StorageError,
};
use gpui::{
    AnyElement, Context, Div, Entity, Pixels, Rgba, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use super::OverlaysView;
use super::property_panel::{IconPickRequested, IconPickResult, OverlayPropertyPanel};
use crate::async_bridge::{self, DialogFilter};
use crate::clip_messages::refusal_message;
use crate::config_form::{CHOICE_GLYPH, CHOICE_PAD_H, CHOICE_PAD_V, FILL_VAL_FS};
use crate::presentation::ActivePresentation;

pub(super) const NO_ICON_ID: &str = "forge-icon-none";
pub(super) const IMPORT_ICON_ID: &str = "forge-icon-import";

const NONE_SCOPE: &str = "all";
const IMPORTED_SCOPE: &str = "imported";
const WORD_SEPARATOR: &str = ", ";

const FIELD_TILE: Pixels = px(22.0);
const FIELD_TILE_RADIUS: Pixels = px(6.0);
const FIELD_ART: Pixels = px(13.0);

#[derive(Clone, PartialEq, Eq)]
pub(super) struct IconImage {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) format: MediaFormat,
    pub(super) path: Arc<Path>,
}

pub(super) enum IconChoice<'a> {
    None,
    Glyph(&'static CuratedIcon),
    Image(&'a IconImage),
    Unresolved(&'a str),
}

pub(super) fn icon_choice<'a>(stored: &'a str, images: &'a [IconImage]) -> IconChoice<'a> {
    match read_icon_value(stored) {
        IconValue::Empty => IconChoice::None,
        IconValue::Glyph(name) => {
            curated_icon(name).map_or(IconChoice::Unresolved(name), IconChoice::Glyph)
        }
        IconValue::Image(id) => images
            .iter()
            .find(|image| image.id == id)
            .map_or(IconChoice::Unresolved(id), IconChoice::Image),
    }
}

pub(super) fn icon_art(choice: &IconChoice<'_>) -> Option<GlyphArt> {
    match choice {
        IconChoice::None | IconChoice::Unresolved(_) => None,
        IconChoice::Glyph(glyph) => Some(GlyphArt::Svg(glyph.bytes())),
        IconChoice::Image(image) => Some(GlyphArt::Image(Arc::clone(&image.path))),
    }
}

pub(super) fn icon_label(choice: &IconChoice<'_>) -> String {
    match choice {
        IconChoice::None => tr!("overlays_icon_none"),
        IconChoice::Glyph(glyph) => glyph.name.to_owned(),
        IconChoice::Image(image) => image.label.clone(),
        IconChoice::Unresolved(value) => (*value).to_owned(),
    }
}

pub(super) enum IconPick {
    Import,
    Store(String),
}

pub(super) fn picked_icon(id: &str) -> IconPick {
    match id {
        IMPORT_ICON_ID => IconPick::Import,
        NO_ICON_ID => IconPick::Store(String::new()),
        chosen => IconPick::Store(chosen.to_owned()),
    }
}

pub(super) fn image_dialog_extensions() -> Vec<&'static str> {
    MediaFormat::ACCEPTED
        .iter()
        .copied()
        .filter(|format| format.kind() == MediaKind::Image)
        .map(MediaFormat::as_str)
        .collect()
}

pub(super) fn import_outcome(result: Result<MediaBlob, StorageError>) -> IconPickResult {
    match result {
        Ok(blob) if blob.format.kind() == MediaKind::Image => {
            IconPickResult::Chosen(image_reference(blob.id.as_str()))
        }
        Ok(blob) => IconPickResult::Refused(tr!(
            "overlays_icon_import_not_image",
            file = blob.label.as_str(),
            format = blob.format.as_str()
        )),
        Err(error) => IconPickResult::Refused(refusal_message(&error)),
    }
}

pub(super) fn image_rows(blobs: Vec<MediaBlob>) -> Vec<(MediaBlobId, MediaBlob)> {
    blobs
        .into_iter()
        .filter(|blob| blob.format.kind() == MediaKind::Image)
        .map(|blob| (blob.id.clone(), blob))
        .collect()
}

fn category_label(category: IconCategory) -> String {
    match category {
        IconCategory::Support => tr!("overlays_icon_category_support"),
        IconCategory::Celebration => tr!("overlays_icon_category_celebration"),
        IconCategory::Community => tr!("overlays_icon_category_community"),
        IconCategory::Stream => tr!("overlays_icon_category_stream"),
        IconCategory::Play => tr!("overlays_icon_category_play"),
        IconCategory::Goals => tr!("overlays_icon_category_goals"),
        IconCategory::Nature => tr!("overlays_icon_category_nature"),
    }
}

fn category_dot(category: IconCategory, palette: &ForgePalette) -> Rgba {
    match category {
        IconCategory::Support => palette.random,
        IconCategory::Celebration => palette.warning,
        IconCategory::Community => palette.info,
        IconCategory::Stream => palette.brand,
        IconCategory::Play => palette.success,
        IconCategory::Goals => palette.bits,
        IconCategory::Nature => palette.accent_teal,
    }
}

pub(super) fn icon_groups(
    images: &[IconImage],
    accent: Rgba,
    palette: &ForgePalette,
) -> Vec<GridPickerGroup> {
    let mut groups = vec![GridPickerGroup {
        label: tr!("overlays_icon_group_none").into(),
        dot_color: palette.text_muted,
        scope: SharedString::from(NONE_SCOPE),
        items: vec![GridPickerItem {
            id: SharedString::from(NO_ICON_ID),
            glyph: GlyphArt::Icon(Icon::Ban),
            tint: palette.text_faint,
            name: tr!("overlays_icon_none").into(),
            desc: tr!("overlays_icon_none_desc").into(),
            state: GridPickerItemState::Normal,
            matches: None,
        }],
    }];

    for category in IconCategory::ALL {
        let items: Vec<GridPickerItem> = CURATED_ICONS
            .iter()
            .filter(|glyph| glyph.category == *category)
            .map(|glyph| GridPickerItem {
                id: SharedString::from(glyph.name),
                glyph: GlyphArt::Svg(glyph.bytes()),
                tint: accent,
                name: SharedString::from(glyph.name),
                desc: SharedString::from(glyph.words.join(WORD_SEPARATOR)),
                state: GridPickerItemState::Normal,
                matches: Some(Box::new(move |query: &str| glyph.matches(query))),
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        groups.push(GridPickerGroup {
            label: category_label(*category).into(),
            dot_color: category_dot(*category, palette),
            scope: SharedString::from(category.as_str()),
            items,
        });
    }

    let formats = image_dialog_extensions().join(WORD_SEPARATOR);
    let mut imported = vec![GridPickerItem {
        id: SharedString::from(IMPORT_ICON_ID),
        glyph: GlyphArt::Icon(Icon::FolderOpen),
        tint: accent,
        name: tr!("overlays_icon_import").into(),
        desc: tr!("overlays_icon_import_desc", formats = formats.as_str()).into(),
        state: GridPickerItemState::Normal,
        matches: None,
    }];
    imported.extend(images.iter().map(|image| GridPickerItem {
        id: SharedString::from(image_reference(&image.id)),
        glyph: GlyphArt::Image(Arc::clone(&image.path)),
        tint: accent,
        name: SharedString::from(image.label.clone()),
        desc: SharedString::from(image.format.as_str().to_uppercase()),
        state: GridPickerItemState::Normal,
        matches: None,
    }));
    groups.push(GridPickerGroup {
        label: tr!("overlays_icon_group_imported").into(),
        dot_color: palette.info,
        scope: SharedString::from(IMPORTED_SCOPE),
        items: imported,
    });

    groups
}

pub(super) fn icon_field_row(
    selected: &str,
    images: &[IconImage],
    accent: Rgba,
    palette: &ForgePalette,
) -> Div {
    let choice = icon_choice(selected, images);
    let art = icon_art(&choice);
    let label = icon_label(&choice);
    let tone = match choice {
        IconChoice::None => palette.text_faint,
        _ => palette.text_primary,
    };

    let tile = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(FIELD_TILE)
        .rounded(FIELD_TILE_RADIUS)
        .bg(palette.surface_overlay)
        .children(art.map(|art| glyph_art(&art, FIELD_ART, accent, "overlays-icon-field-art")));

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(CHOICE_PAD_V)
        .px(CHOICE_PAD_H)
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .child(tile)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(body_family())
                .text_size(FILL_VAL_FS)
                .text_color(tone)
                .child(label),
        )
        .child(icon(Icon::ChevronDown, CHOICE_GLYPH, palette.text_faint))
}

pub(super) struct OpenIconPicker {
    key: String,
    view: Entity<GridPicker>,
    focused: bool,
    _sub: Subscription,
}

impl OverlaysView {
    pub(super) fn load_images(&mut self, cx: &mut Context<Self>) {
        let ticket = self.images_gen.next();
        let media = Arc::clone(&self.media);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let blobs = media.list().await?;
                let mut resolved: Vec<IconImage> = Vec::new();
                for (id, blob) in image_rows(blobs) {
                    let Ok(path) = media.resolve(&id).await else {
                        continue;
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
        if self.icon_images == images {
            return;
        }
        self.icon_images = images;
        let pushed = self.icon_images.clone();
        if let Some(panel) = self.panel.as_ref().map(|open| open.view.clone()) {
            panel.update(cx, |panel, cx| panel.set_icon_images(pushed, cx));
        }
        let palette = cx.palette();
        let accent = self.icon_accent(&palette);
        let groups = icon_groups(&self.icon_images, accent, &palette);
        if let Some(picker) = self.icon_picker.as_ref().map(|open| open.view.clone()) {
            picker.update(cx, |picker, cx| picker.set_groups(groups, cx));
        }
        self.sync_preview();
        cx.notify();
    }

    fn icon_accent(&self, palette: &ForgePalette) -> Rgba {
        self.selected_definition()
            .map(|definition| self.visuals(definition, palette).accent)
            .unwrap_or(palette.brand)
    }

    pub(super) fn on_icon_pick_requested(
        &mut self,
        _view: Entity<OverlayPropertyPanel>,
        event: &IconPickRequested,
        cx: &mut Context<Self>,
    ) {
        let key = event.key.clone();
        self.open_icon_picker(key, cx);
        self.load_images(cx);
    }

    fn open_icon_picker(&mut self, key: String, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let accent = self.icon_accent(&palette);
        let groups = icon_groups(&self.icon_images, accent, &palette);
        let config = GridPickerConfig {
            accent,
            header_icon: Icon::Photo,
            title: tr!("overlays_icon_picker_title").into(),
            subtitle: GridPickerSubtitle::Plain(tr!("overlays_icon_picker_subtitle").into()),
            footer_hint: tr!("overlays_icon_picker_hint").into(),
            search_placeholder: tr!("overlays_icon_picker_search").into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.icon_favorites.clone();
        let view = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
        let sub = cx.subscribe(&view, Self::on_icon_picker_event);
        self.icon_picker = Some(OpenIconPicker {
            key,
            view,
            focused: false,
            _sub: sub,
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
            GridPickerEvent::FavoriteToggled(id) => {
                if !self.icon_favorites.remove(id) {
                    self.icon_favorites.insert(id.clone());
                }
                cx.notify();
            }
            GridPickerEvent::Dismissed => self.close_icon_picker(cx),
        }
    }

    fn apply_icon_pick(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(key) = self.icon_picker.as_ref().map(|open| open.key.clone()) else {
            return;
        };
        self.icon_picker = None;
        cx.notify();

        let Some(panel) = self.panel.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        match picked_icon(&id) {
            IconPick::Store(value) => panel.update(cx, |panel, cx| {
                panel.settle_icon_pick(key, IconPickResult::Chosen(value), cx);
            }),
            IconPick::Import => {
                panel.update(cx, |panel, cx| panel.begin_icon_import(key.clone(), cx));
                self.browse_icon_image(key, cx);
            }
        }
    }

    fn browse_icon_image(&mut self, key: String, cx: &mut Context<Self>) {
        let filter = DialogFilter {
            name: tr!("overlays_icon_file_filter"),
            extensions: image_dialog_extensions(),
        };
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file(Some(filter)),
            move |this, result: Result<PathBuf, String>, cx| match result {
                Ok(path) => this.import_icon_image(key, path, cx),
                Err(_) => this.cancel_icon_import(key, cx),
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
                if let Some(panel) = this.panel.as_ref().map(|open| open.view.clone()) {
                    panel.update(cx, |panel, cx| panel.settle_icon_pick(key, outcome, cx));
                }
                if settled {
                    this.load_images(cx);
                }
            },
            cx,
        );
    }

    fn cancel_icon_import(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        panel.update(cx, |panel, cx| panel.cancel_icon_import(key, cx));
    }

    fn close_icon_picker(&mut self, cx: &mut Context<Self>) {
        self.icon_picker = None;
        cx.notify();
    }

    pub(super) fn render_icon_picker(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.icon_picker.as_ref()?;
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

    pub(super) fn preview_icon(&self, definition: &OverlayDefinition) -> Option<GlyphArt> {
        let descriptor = self.kinds.get(&definition.kind_id)?;
        let config = effective_overlay_config(descriptor, &definition.config);
        let stored = config.get(ICON).and_then(forge_types::Variant::as_str)?;
        icon_art(&icon_choice(stored, &self.icon_images))
    }

    pub(super) fn focus_icon_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.icon_picker.as_mut().filter(|open| !open.focused) else {
            return;
        };
        open.focused = true;
        let picker = open.view.clone();
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    }
}
