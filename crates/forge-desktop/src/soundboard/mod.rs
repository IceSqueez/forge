use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use forge_audio::DeviceInfo;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Confirm, FONT_XS, Icon, Radius, SearchState, Spacing,
    body_family, empty_state, icon, page_frame, radius, spacing, tr, with_alpha,
};
use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_soundboard::builtin_library::BuiltinSoundEntry;
use forge_soundboard::{ClipAvailability, ClipLibrary, SoundboardPlayer, SoundboardSettings};
use forge_storage::SettingsRepo;
use forge_types::ClipId;
use gpui::{
    Context, Entity, EventEmitter, Pixels, SharedString, Subscription, Task, Window, div,
    prelude::*, px,
};

use crate::async_bridge::{self, BridgeFlow, drain_events};
use crate::clip_editor::ClipEditor;
use crate::clip_key_state::{ClipKeyState, changes_registrations};
use crate::clip_playback::clip_id_of;
use crate::hub_crumb::{core_category, hub_crumb};
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

mod adoption;
mod categories;
mod clips;
mod delete;
mod editor;
mod filter;
mod footer;
mod hero;
pub(crate) mod hotkeys_notice;
mod keys;
mod labels;
mod library;
mod pad;
mod playback;
mod routing;

use adoption::settled_adoption;

pub(crate) use categories::{CATEGORY_ORDER, category_color, category_label};
pub(crate) use editor::audio_dialog_extensions;
pub(crate) use labels::field_lite_label;

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);
const SECTION_GAP: Pixels = px(14.0);
const LABEL_FS: Pixels = px(11.5);
const HOTKEY_FS: Pixels = px(10.0);

struct SoundClip {
    id: ClipId,
    name: String,
    file_path: PathBuf,
    hotkey: Option<String>,
    category: String,
    loop_playback: bool,
    duration_secs: Option<f32>,
    builtin_id: Option<String>,
    glyph: Icon,
}

struct PlaybackProgress {
    started_at: Instant,
    duration_secs: Option<f64>,
    looped: bool,
}

pub struct SoundboardView {
    clips: Vec<SoundClip>,
    loading: bool,
    error: Option<SharedString>,
    devices: Vec<DeviceInfo>,
    importable: Vec<BuiltinSoundEntry>,
    total_size: Option<u64>,
    availability: HashMap<ClipId, ClipAvailability>,
    refusal_reads: HashSet<ClipId>,
    adopting_all: bool,
    adopt_summary: Option<SharedString>,
    playing: HashMap<ClipId, PlaybackProgress>,
    ticking: bool,
    settings: Arc<SoundboardSettings>,
    device_menu_open: bool,
    search: SearchState,
    category_filter: Option<String>,
    modal: Option<Entity<ClipEditor>>,
    keys: ClipKeyState,
    integrations: Option<crate::integration_switch::SwitchWatch>,
    _modal_sub: Option<Subscription>,
    pending_delete: Confirm<ClipId>,
    _search_sub: Subscription,
    player: Arc<SoundboardPlayer>,
    library: Arc<ClipLibrary>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    master_volume_debounce: async_bridge::Debounced,
    reload_gen: async_bridge::Generation,
    availability_gen: async_bridge::Generation,
    _event_bridge: Task<()>,
}

impl SoundboardView {
    pub fn new(
        player: Arc<SoundboardPlayer>,
        settings_repo: Arc<dyn SettingsRepo>,
        rt_handle: tokio::runtime::Handle,
        bus: Arc<EventBus>,
        keys: ClipKeyState,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("soundboard_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let settings = player.settings_handle().load();

        let event_bridge = cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                match this.update(cx, |this, cx| {
                    for event in batch {
                        this.on_bus_event(event, cx);
                    }
                }) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        });

        let library = Arc::clone(player.library());
        let view = Self {
            clips: Vec::new(),
            loading: true,
            error: None,
            devices: Vec::new(),
            importable: Vec::new(),
            total_size: None,
            availability: HashMap::new(),
            refusal_reads: HashSet::new(),
            adopting_all: false,
            adopt_summary: None,
            playing: HashMap::new(),
            ticking: false,
            settings,
            device_menu_open: false,
            search,
            category_filter: None,
            modal: None,
            keys,
            integrations: None,
            _modal_sub: None,
            pending_delete: Confirm::default(),
            _search_sub: search_sub,
            player,
            library,
            settings_repo,
            rt_handle,
            master_volume_debounce: async_bridge::Debounced::new(
                async_bridge::SLIDER_PERSIST_DEBOUNCE,
            ),
            reload_gen: async_bridge::Generation::default(),
            availability_gen: async_bridge::Generation::default(),
            _event_bridge: event_bridge,
        };
        view.reload(cx);
        view.reload_devices(cx);
        view.reload_key_holders(cx);
        view
    }

    fn on_bus_event(&mut self, event: &Event, cx: &mut Context<Self>) {
        if changes_registrations(event) {
            self.keys.refresh_live();
            cx.notify();
            return;
        }
        if event.source != EventSource::Audio {
            return;
        }
        let Some(clip_id) = clip_id_of(&event.payload) else {
            return;
        };
        match event.kind.as_str() {
            "playback.started" => {
                let duration_secs = event.payload.get("duration_secs").and_then(|v| v.as_f64());
                let looped = event
                    .payload
                    .get("looped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.playing.insert(
                    clip_id,
                    PlaybackProgress {
                        started_at: Instant::now(),
                        duration_secs,
                        looped,
                    },
                );
                self.ensure_ticker(cx);
                cx.notify();
            }
            "playback.finished" | "playback.failed" => {
                self.clear_playing(clip_id, cx);
            }
            "soundboard.clip.adopted" => {
                if let Some(settled) = settled_adoption(&event.payload) {
                    self.apply_settled_adoption(clip_id, settled, cx);
                }
            }
            _ => {}
        }
    }
}

impl EventEmitter<NavRequested> for SoundboardView {}

impl Render for SoundboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.settings = self.player.settings_handle().load();
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.render_header_right(&palette);
        let subheader_left = self.render_subheader_left(&palette, density, cx);
        let subheader_right = self.render_subheader_right(&palette, density, cx);

        let inner = if self.loading {
            empty_state(tr!("soundboard_loading"), &palette)
                .loading("soundboard-loading")
                .density(density)
                .into_any_element()
        } else {
            let error_banner = self.error.clone().map(|message| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.random, 0.10))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.random, 0.30))
                    .child(icon(Icon::AlertCircle, FONT_XS, palette.random))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(message),
                    )
            });

            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(SECTION_GAP)
                .children(error_banner)
                .child(self.render_hero(&palette, density, cx))
                .children(self.render_hotkeys_notice(&palette, cx))
                .child(self.render_pads(&palette, density, cx))
                .children(self.render_library(&palette, density, cx))
                .child(self.render_add_bar(&palette, cx))
                .child(self.render_routing(&palette, density, cx))
                .child(self.render_footer(&palette, density, cx))
                .into_any_element()
        };

        let body = div()
            .id("sb-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(SCROLL_PAD_Y)
                    .px(SCROLL_PAD_X)
                    .child(inner),
            );

        let frame = page_frame(
            vec![
                hub_crumb(core_category(&Screen::Soundboard), cx),
                BreadcrumbCrumb::leaf(tr!("soundboard_breadcrumb_soundboard")),
            ],
            &palette,
        )
        .header_right(header_right)
        .subheader_left(subheader_left)
        .subheader_right(subheader_right)
        .density(density)
        .body(body);

        let active_overlay = if let Some(modal) = self.modal.as_ref() {
            Some(modal.clone().into_any_element())
        } else {
            self.pending_delete
                .get()
                .copied()
                .map(|id| self.render_delete_confirm(id, &palette, cx))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(active_overlay)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_soundboard::SoundboardError;
    use forge_storage::{MediaFormat, MediaKind, StorageError};

    use crate::clip_messages::failure_message;

    fn catalog_entry(locale: &str, key: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("locales")
            .join(locale)
            .join("main.ftl");
        let catalog = std::fs::read_to_string(&path).unwrap();
        let head = format!("{key} =");
        let mut lines = catalog.lines().skip_while(|line| !line.starts_with(&head));
        let mut entry = lines
            .next()
            .unwrap_or_else(|| panic!("{locale}/main.ftl is missing {key}"))
            .to_owned();
        for line in lines {
            if line.trim().is_empty() || !line.starts_with(char::is_whitespace) {
                break;
            }
            entry.push('\n');
            entry.push_str(line);
        }
        entry
    }

    #[test]
    fn failure_message_routes_each_typed_reason_to_its_own_catalog_key() {
        for (error, expected_key) in [
            (
                SoundboardError::SourceMissing("Fanfare".to_owned()),
                "soundboard_error_source_missing",
            ),
            (
                SoundboardError::ClipNotFound("01J0".to_owned()),
                "soundboard_error_clip_gone",
            ),
            (
                SoundboardError::ImportRefused(StorageError::MediaUnsupported {
                    label: "notes.txt".to_owned(),
                }),
                "soundboard_import_unsupported",
            ),
            (
                SoundboardError::ImportRefused(StorageError::MediaTypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                }),
                "soundboard_import_type_mismatch",
            ),
            (
                SoundboardError::ImportRefused(StorageError::MediaTooLarge {
                    label: "huge.wav".to_owned(),
                    size: 60 * 1024 * 1024,
                    limit: 50 * 1024 * 1024,
                    kind: MediaKind::Audio,
                }),
                "soundboard_import_too_large",
            ),
        ] {
            assert_eq!(failure_message(&error), expected_key, "{error}");
        }
    }

    #[test]
    fn failure_message_falls_back_to_the_error_text_when_no_key_covers_the_reason() {
        let error = SoundboardError::Storage("disk is busy".to_owned());
        assert_eq!(failure_message(&error), error.to_string());
    }

    #[test]
    fn a_refusal_from_outside_the_admission_gate_falls_back_to_its_own_text() {
        let refusal = StorageError::NotFound {
            key: "sha256-abc".to_owned(),
        };
        let expected = refusal.to_string();

        assert_eq!(
            failure_message(&SoundboardError::ImportRefused(refusal)),
            expected
        );
    }

    #[test]
    fn every_media_message_is_defined_in_both_catalogs_with_the_arguments_the_code_passes() {
        for (key, arguments) in [
            ("soundboard_error_source_missing", &["name"][..]),
            ("soundboard_error_clip_gone", &[][..]),
            ("soundboard_import_unsupported", &["file"][..]),
            (
                "soundboard_import_type_mismatch",
                &["file", "named", "detected"][..],
            ),
            (
                "soundboard_import_too_large",
                &["file", "size", "limit"][..],
            ),
            ("soundboard_pad_source_missing", &[][..]),
            ("soundboard_pad_source_missing_hint", &[][..]),
            ("soundboard_pad_adopt_blocked", &[][..]),
            ("soundboard_pad_not_in_library", &[][..]),
            ("soundboard_footer_unadopted", &["count"][..]),
            ("soundboard_footer_adopt_action", &["count"][..]),
            ("soundboard_footer_adopt_busy", &[][..]),
            ("soundboard_adopt_copied", &["count"][..]),
            ("soundboard_adopt_refused", &["count"][..]),
            ("soundboard_adopt_missing", &["count"][..]),
            ("soundboard_adopt_unfinished", &["count"][..]),
            ("soundboard_adopt_nothing", &[][..]),
        ] {
            for locale in ["en", "uk"] {
                let entry = catalog_entry(locale, key);
                for argument in arguments {
                    assert!(
                        entry.contains(&format!("${argument}")),
                        "{locale}/main.ftl defines {key} without ${argument}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_counted_adoption_message_offers_the_plural_categories_its_locale_needs() {
        for key in [
            "soundboard_footer_unadopted",
            "soundboard_footer_adopt_action",
        ] {
            for (locale, categories) in [
                ("en", &["[one]", "*[other]"][..]),
                ("uk", &["[one]", "[few]", "[many]", "*[other]"][..]),
            ] {
                let entry = catalog_entry(locale, key);
                assert!(
                    entry.contains("$count ->"),
                    "{locale}/main.ftl states {key} without selecting on $count"
                );
                for category in categories {
                    assert!(
                        entry.contains(category),
                        "{locale}/main.ftl defines {key} without the {category} plural form"
                    );
                }
            }
        }
    }
}
