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
                    panel.update(cx, |panel, cx| panel.settle_icon_import(key, outcome, cx));
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_overlay::config::{DEFAULT_ICON, ICON};
    use forge_overlay::kinds::alert::AlertOverlayKind;
    use forge_overlay::{IconCategory, effective_overlay_config};
    use forge_storage::{MediaBlob, MediaBlobId, MediaFormat, MediaKind, OverlayConfig};
    use forge_types::Variant;
    use time::OffsetDateTime;

    use super::*;

    const BLOB_A: &str = "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8";
    const BLOB_B: &str = "sha256-9faccac8ea389a38814e46d03b2d4704bc2caf3bed368f3d6a694cfebcbf1d29";
    const GONE: &str = "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
    const GLYPH: &str = "heart";
    const NOT_A_GLYPH: &str = "forge-has-no-such-glyph";

    fn image(id: &str, label: &str, format: MediaFormat) -> IconImage {
        IconImage {
            id: id.to_owned(),
            label: label.to_owned(),
            format,
            path: Arc::from(Path::new("/library").join(format!("{id}.{}", format.as_str()))),
        }
    }

    fn library() -> Vec<IconImage> {
        vec![
            image(BLOB_A, "cat.png", MediaFormat::Png),
            image(BLOB_B, "wave.gif", MediaFormat::Gif),
        ]
    }

    fn blob(id: &str, label: &str, format: MediaFormat) -> MediaBlob {
        MediaBlob {
            id: MediaBlobId::from_stored(id),
            format,
            byte_size: 6,
            label: label.to_owned(),
            imported_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn choice_name(choice: &IconChoice<'_>) -> String {
        match choice {
            IconChoice::None => "none".to_owned(),
            IconChoice::Glyph(glyph) => format!("glyph:{}", glyph.name),
            IconChoice::Image(image) => format!("image:{}", image.id),
            IconChoice::Unresolved(value) => format!("unresolved:{value}"),
        }
    }

    fn art_name(art: Option<GlyphArt>) -> String {
        match art {
            None => "none".to_owned(),
            Some(GlyphArt::Icon(_)) => "kit-icon".to_owned(),
            Some(GlyphArt::Svg(bytes)) => format!("svg:{}", bytes.len()),
            Some(GlyphArt::Image(path)) => format!("file:{}", path.display()),
        }
    }

    fn pick_name(pick: &IconPick) -> String {
        match pick {
            IconPick::Import => "import".to_owned(),
            IconPick::Store(value) => format!("store:{value}"),
        }
    }

    fn refusal(result: &IconPickResult) -> Option<&str> {
        match result {
            IconPickResult::Chosen(_) => None,
            IconPickResult::Refused(message) => Some(message),
        }
    }

    fn chosen(result: &IconPickResult) -> Option<&str> {
        match result {
            IconPickResult::Chosen(value) => Some(value),
            IconPickResult::Refused(_) => None,
        }
    }

    #[test]
    fn a_stored_token_resolves_to_the_choice_it_names() {
        let images = library();
        for (stored, expected) in [
            ("", "none"),
            (GLYPH, "glyph:heart"),
            (NOT_A_GLYPH, "unresolved:forge-has-no-such-glyph"),
            (
                &image_reference(BLOB_A),
                "image:sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8",
            ),
            (
                &image_reference(GONE),
                "unresolved:sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8",
            ),
        ] {
            assert_eq!(
                choice_name(&icon_choice(stored, &images)),
                expected,
                "stored {stored:?}",
            );
        }
    }

    #[test]
    fn a_resolved_choice_draws_the_art_it_stands_for() {
        let images = library();
        let heart = curated_icon(GLYPH).expect("the curated roster carries a heart");

        assert_eq!(
            art_name(icon_art(&icon_choice(GLYPH, &images))),
            format!("svg:{}", heart.bytes().len()),
        );
        assert_eq!(
            art_name(icon_art(&icon_choice(&image_reference(BLOB_B), &images))),
            format!("file:/library/{BLOB_B}.gif"),
        );
    }

    #[test]
    fn nothing_is_drawn_for_no_icon_or_for_a_reference_this_library_cannot_resolve() {
        let images = library();
        for stored in ["", NOT_A_GLYPH, &image_reference(GONE)] {
            assert_eq!(
                art_name(icon_art(&icon_choice(stored, &images))),
                "none",
                "stored {stored:?}",
            );
        }
    }

    #[test]
    fn the_field_names_the_choice_by_what_the_user_recognises() {
        let images = library();
        for (stored, expected) in [
            ("", "overlays_icon_none"),
            (GLYPH, GLYPH),
            (&image_reference(BLOB_A), "cat.png"),
            (NOT_A_GLYPH, NOT_A_GLYPH),
        ] {
            assert_eq!(
                icon_label(&icon_choice(stored, &images)),
                expected,
                "stored {stored:?}",
            );
        }
    }

    #[test]
    fn a_picked_card_stores_its_own_id_unless_it_is_one_of_the_two_commands() {
        for (id, expected) in [
            (IMPORT_ICON_ID, "import"),
            (NO_ICON_ID, "store:"),
            (GLYPH, "store:heart"),
            (
                &image_reference(BLOB_A) as &str,
                "store:image:sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8",
            ),
        ] {
            assert_eq!(pick_name(&picked_icon(id)), expected, "card {id:?}");
        }
    }

    #[test]
    fn the_file_dialog_offers_every_accepted_image_format_and_no_audio_one() {
        let offered = image_dialog_extensions();

        for format in MediaFormat::ACCEPTED {
            assert_eq!(
                offered.contains(&format.as_str()),
                format.kind() == MediaKind::Image,
                "{} is offered by the icon dialog",
                format.as_str(),
            );
        }
    }

    #[test]
    fn an_imported_image_becomes_the_reference_the_overlay_stores() {
        let outcome = import_outcome(Ok(blob(BLOB_A, "cat.png", MediaFormat::Png)));

        assert_eq!(chosen(&outcome), Some(image_reference(BLOB_A).as_str()));
    }

    #[test]
    fn a_file_the_library_accepted_as_audio_is_refused_as_an_icon() {
        let outcome = import_outcome(Ok(blob(BLOB_A, "airhorn.wav", MediaFormat::Wav)));

        assert_eq!(refusal(&outcome), Some("overlays_icon_import_not_image"));
    }

    #[test]
    fn a_refused_import_reuses_the_library_wording() {
        let unsupported = import_outcome(Err(StorageError::MediaUnsupported {
            label: "notes.txt".to_owned(),
        }));
        let broken = import_outcome(Err(StorageError::Connection {
            reason: "disk gone".to_owned(),
        }));

        assert_eq!(refusal(&unsupported), Some("soundboard_import_unsupported"),);
        assert_eq!(refusal(&broken), Some("connection failed: disk gone"));
    }

    #[test]
    fn the_library_read_keeps_only_images_each_under_its_own_id() {
        let rows = image_rows(vec![
            blob(BLOB_A, "cat.png", MediaFormat::Png),
            blob(GONE, "airhorn.wav", MediaFormat::Wav),
            blob(BLOB_B, "wave.gif", MediaFormat::Gif),
        ]);

        assert_eq!(
            rows.iter()
                .map(|(id, blob)| (id.as_str().to_owned(), blob.label.clone()))
                .collect::<Vec<_>>(),
            vec![
                (BLOB_A.to_owned(), "cat.png".to_owned()),
                (BLOB_B.to_owned(), "wave.gif".to_owned()),
            ],
        );
    }

    #[test]
    fn the_grid_opens_on_no_icon_and_closes_on_the_imported_library() {
        let palette = forge_components::FORGE_DEFAULT;
        let groups = icon_groups(&library(), palette.brand, &palette);

        let mut expected: Vec<String> = vec![NONE_SCOPE.to_owned()];
        expected.extend(IconCategory::ALL.iter().map(|c| c.as_str().to_owned()));
        expected.push(IMPORTED_SCOPE.to_owned());
        assert_eq!(
            groups
                .iter()
                .map(|group| group.scope.to_string())
                .collect::<Vec<_>>(),
            expected,
        );
    }

    #[test]
    fn the_only_card_that_clears_the_field_leads_the_grid() {
        let palette = forge_components::FORGE_DEFAULT;
        let groups = icon_groups(&library(), palette.brand, &palette);

        let ids: Vec<String> = groups[0]
            .items
            .iter()
            .map(|item| item.id.to_string())
            .collect();
        assert_eq!(ids, vec![NO_ICON_ID.to_owned()]);
    }

    #[test]
    fn the_imported_band_offers_the_import_first_and_then_every_image_by_its_reference() {
        let palette = forge_components::FORGE_DEFAULT;
        let groups = icon_groups(&library(), palette.brand, &palette);
        let imported = groups.last().expect("the imported band closes the grid");

        assert_eq!(
            imported
                .items
                .iter()
                .map(|item| item.id.to_string())
                .collect::<Vec<_>>(),
            vec![
                IMPORT_ICON_ID.to_owned(),
                image_reference(BLOB_A),
                image_reference(BLOB_B),
            ],
        );
    }

    #[test]
    fn every_category_band_carries_a_label_and_a_dot_of_its_own() {
        let palette = forge_components::FORGE_DEFAULT;
        let groups = icon_groups(&library(), palette.brand, &palette);
        let bands = &groups[1..groups.len() - 1];

        let labels: std::collections::BTreeSet<String> =
            bands.iter().map(|g| g.label.to_string()).collect();
        let dots: std::collections::BTreeSet<(u32, u32, u32)> = bands
            .iter()
            .map(|g| {
                (
                    g.dot_color.r.to_bits(),
                    g.dot_color.g.to_bits(),
                    g.dot_color.b.to_bits(),
                )
            })
            .collect();
        assert_eq!(labels.len(), IconCategory::ALL.len());
        assert_eq!(dots.len(), IconCategory::ALL.len());
    }

    #[test]
    fn a_curated_glyph_is_searched_through_its_own_keywords_not_its_name() {
        let palette = forge_components::FORGE_DEFAULT;
        let groups = icon_groups(&library(), palette.brand, &palette);
        let gift = groups
            .iter()
            .flat_map(|group| group.items.iter())
            .find(|item| item.id.as_ref() == "gift")
            .expect("the curated roster carries a gift");
        let test = gift
            .matches
            .as_ref()
            .expect("a curated card searches itself");

        assert!(test("donation"), "a keyword the card name does not contain");
        assert!(!test("airhorn"));
    }

    #[test]
    fn an_alert_that_stored_no_icon_previews_the_curated_default() {
        let effective = effective_overlay_config(&AlertOverlayKind, &OverlayConfig::new());
        let stored = effective
            .get(ICON)
            .and_then(Variant::as_str)
            .expect("the alert declares an icon default");
        let default = curated_icon(DEFAULT_ICON).expect("the alert default is a curated glyph");

        assert_eq!(
            art_name(icon_art(&icon_choice(stored, &[]))),
            format!("svg:{}", default.bytes().len()),
        );
    }

    #[test]
    fn an_alert_whose_icon_was_cleared_previews_nothing() {
        let cleared: OverlayConfig = [(ICON.to_owned(), Variant::String(String::new()))]
            .into_iter()
            .collect();
        let effective = effective_overlay_config(&AlertOverlayKind, &cleared);
        let stored = effective
            .get(ICON)
            .and_then(Variant::as_str)
            .expect("the alert declares an icon default");

        assert_eq!(art_name(icon_art(&icon_choice(stored, &[]))), "none");
    }
}
