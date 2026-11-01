use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::TINT_BORDER_ALPHA;
use forge_components::TINT_FILL_ALPHA;
use std::rc::Rc;
use std::sync::Arc;

use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BORDER_THIN, BreadcrumbCrumb, FONT_XS, Icon, Radius, Spacing, icon,
    page_frame, radius, scaled, spacing, tr, with_alpha,
};

use forge_runtime::EventBus;
use forge_soundboard::{ClipLibrary, SoundboardPlayer};
use forge_storage::SettingsRepo;
use forge_types::ClipId;
use gpui::{
    App, Context, Entity, EventEmitter, Pixels, SharedString, Subscription, Task, Window, div,
    prelude::*, px,
};

use crate::app::async_bridge::{BridgeFlow, drain_events};
use crate::app::navigation::NavRequested;
use crate::app::navigation::Screen;
use crate::audio::clip_editor::ClipEditor;
use crate::audio::clip_key_state::ClipKeyState;
use crate::integrations::hub_crumb::{core_category, hub_crumb};

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
mod pads;
mod playback;
mod routing;

use footer::{FooterEvent, FooterFacts, SoundboardFooter};
use library::{LibraryEvent, SoundboardLibrary};
use pads::{EditRequest, PadsEvent, SoundboardPads};
use routing::SoundboardRouting;

pub(crate) use categories::{category_color, category_label};
pub(crate) use editor::audio_dialog_extensions;
pub(crate) use labels::field_lite_label;

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);
const SECTION_GAP: Pixels = px(14.0);
const LABEL_FS: Pixels = FONT_LABEL;
const HOTKEY_FS: Pixels = FONT_MICRO;
const SMALL_GLYPH: Pixels = px(10.0);

#[derive(Clone)]
struct SoundboardHandles {
    player: Arc<SoundboardPlayer>,
    library: Arc<ClipLibrary>,
    rt_handle: tokio::runtime::Handle,
}

pub(crate) struct SoundboardView {
    pads: Entity<SoundboardPads>,
    routing: Entity<SoundboardRouting>,
    library: Entity<SoundboardLibrary>,
    footer: Entity<SoundboardFooter>,
    modal: Option<Entity<ClipEditor>>,
    _modal_sub: Option<Subscription>,
    integrations: Option<crate::integrations::integration_switch::SwitchWatch>,
    handles: SoundboardHandles,
    _subscriptions: [Subscription; 6],
    _event_bridge: Task<()>,
}

impl SoundboardView {
    pub(crate) fn new(
        player: Arc<SoundboardPlayer>,
        settings_repo: Arc<dyn SettingsRepo>,
        rt_handle: tokio::runtime::Handle,
        bus: Arc<EventBus>,
        keys: ClipKeyState,
        cx: &mut Context<Self>,
    ) -> Self {
        let library = Arc::clone(player.library());
        let handles = SoundboardHandles {
            player,
            library,
            rt_handle,
        };
        let on_edit = Self::edit_request(cx);
        let pads = cx.new(|cx| SoundboardPads::new(handles.clone(), keys, on_edit, cx));
        let routing = cx.new(|cx| SoundboardRouting::new(handles.clone(), settings_repo, cx));
        let library = cx.new(|_| SoundboardLibrary::new(handles.clone()));
        let footer = cx.new(|_| SoundboardFooter::new(handles.clone()));

        let subscriptions = [
            cx.subscribe(&pads, Self::on_pads_event),
            cx.subscribe(&library, Self::on_library_event),
            cx.subscribe(&footer, Self::on_footer_event),
            cx.observe(&pads, Self::on_child_changed),
            cx.observe(&routing, Self::on_child_changed),
            cx.observe(&library, |_, _, cx| cx.notify()),
        ];

        let bridge_pads = pads.downgrade();
        let event_bridge = cx.spawn(async move |_, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                match bridge_pads.update(cx, |pads, cx| {
                    for event in batch {
                        pads.on_bus_event(event, cx);
                    }
                }) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        });

        let view = Self {
            pads,
            routing,
            library,
            footer,
            modal: None,
            _modal_sub: None,
            integrations: None,
            handles,
            _subscriptions: subscriptions,
            _event_bridge: event_bridge,
        };
        view.sync_footer(cx);
        view
    }

    fn edit_request(cx: &Context<Self>) -> EditRequest {
        let view = cx.weak_entity();
        Rc::new(move |id: ClipId, window: &mut Window, cx: &mut App| {
            let view = view.clone();
            window.defer(cx, move |window, cx| {
                view.update(cx, |this, cx| this.open_edit(id, window, cx))
                    .ok();
            });
        })
    }

    fn on_child_changed<T: 'static>(&mut self, _: Entity<T>, cx: &mut Context<Self>) {
        self.sync_footer(cx);
        cx.notify();
    }

    fn sync_footer(&self, cx: &mut Context<Self>) {
        let pads = self.pads.read(cx);
        let facts = FooterFacts {
            sounds: pads.clip_count(),
            categories: pads.categories_present().len(),
            unadopted: pads.unadopted(),
            output_ready: self.routing.read(cx).output_ready(),
        };
        self.footer
            .update(cx, |footer, cx| footer.set_facts(facts, cx));
    }

    fn on_pads_event(
        &mut self,
        _pads: Entity<SoundboardPads>,
        event: &PadsEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PadsEvent::ClipsLoaded => {
                let imported = self.pads.read(cx).imported_builtin_ids();
                self.library
                    .update(cx, |library, cx| library.refresh_builtins(imported, cx));
                self.footer
                    .update(cx, |footer, cx| footer.clips_reloaded(cx));
            }
            PadsEvent::LibraryGrew => {
                self.footer
                    .update(cx, |footer, cx| footer.refresh_library_size(cx));
            }
            PadsEvent::HoldersChanged => {
                let holders = self.pads.read(cx).keys().holders();
                if let Some(modal) = self.modal.as_ref() {
                    modal.update(cx, |editor, cx| editor.set_holders(holders, cx));
                }
            }
        }
    }

    fn on_library_event(
        &mut self,
        _library: Entity<SoundboardLibrary>,
        event: &LibraryEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            LibraryEvent::Imported => self.pads.update(cx, |pads, cx| pads.reload(cx)),
        }
    }

    fn on_footer_event(
        &mut self,
        _footer: Entity<SoundboardFooter>,
        event: &FooterEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            FooterEvent::AdoptionFinished => {
                self.pads
                    .update(cx, |pads, cx| pads.reload_availability(cx));
            }
        }
    }
}

impl EventEmitter<NavRequested> for SoundboardView {}

impl SoundboardView {
    fn render_error_banner(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let palette = cx.palette();
        let density = cx.density();
        let message: SharedString = self.pads.read(cx).error()?;
        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .p(spacing(Spacing::Xs, density))
                .rounded(radius(Radius::Sm))
                .bg(with_alpha(palette.danger, TINT_FILL_ALPHA))
                .border(BORDER_THIN)
                .border_color(with_alpha(palette.danger, TINT_BORDER_ALPHA))
                .child(icon(Icon::AlertCircle, FONT_XS, palette.danger))
                .child(
                    div()
                        .font_family(cx.body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_primary)
                        .child(message),
                ),
        )
    }
}

impl Render for SoundboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.render_header_right(&palette, density, cx);
        let subheader_left = self.pads.update(cx, |pads, cx| {
            pads.render_subheader_left(&palette, density, cx)
        });
        let subheader_right = self.pads.update(cx, |pads, cx| {
            pads.render_subheader_right(&palette, density, cx)
        });

        let notice = self.pads.update(cx, |pads, cx| {
            pads.render_load_notice(&palette, density, cx)
        });
        let inner = if let Some(notice) = notice {
            notice
        } else {
            let hero = self
                .routing
                .update(cx, |routing, cx| routing.render_hero(&palette, density, cx));
            let library = self
                .library
                .read(cx)
                .has_entries()
                .then(|| self.library.clone());
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(scaled(SECTION_GAP, density))
                .children(self.render_error_banner(cx))
                .child(hero)
                .children(self.render_hotkeys_notice(&palette, cx))
                .child(self.pads.clone())
                .children(library)
                .child(self.render_add_bar(&palette, cx))
                .child(self.routing.clone())
                .child(self.footer.clone())
                .into_any_element()
        };

        let body = div()
            .id("sb-scroll")
            .flex_1()
            .h_full()
            .overflow_y_revealing_focus()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(SCROLL_PAD_Y, density))
                    .px(scaled(SCROLL_PAD_X, density))
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
            self.pads
                .update(cx, |pads, cx| pads.render_delete_overlay(&palette, cx))
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
mod tests {
    use forge_soundboard::SoundboardError;
    use forge_storage::{MediaFormat, MediaKind, MediaRefusal};

    use crate::audio::clip_messages::failure_message;

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
                SoundboardError::ImportRefused(MediaRefusal::Unsupported {
                    label: "notes.txt".to_owned(),
                }),
                "soundboard_import_unsupported",
            ),
            (
                SoundboardError::ImportRefused(MediaRefusal::TypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                }),
                "soundboard_import_type_mismatch",
            ),
            (
                SoundboardError::ImportRefused(MediaRefusal::TooLarge {
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
        let error = SoundboardError::Storage(forge_storage::StorageError::Connection {
            reason: "disk is busy".to_owned(),
        });
        assert_eq!(failure_message(&error), error.to_string());
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
