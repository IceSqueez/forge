mod console;
mod document;
mod list;
#[cfg(test)]
mod tests;

use self::document::{ScriptDocument, ScriptDocumentEvent};
use self::list::{ScriptList, ScriptListEvent};
use crate::app::navigation::{NavRequested, Screen};
use crate::app::screen_shortcuts::ScreenShortcuts;
use crate::automation::script_lifecycle::ScriptLifecycle;
use crate::shared::unsaved_work::UnsavedWork;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, ConfirmTone, Density, FONT_XXS, ForgePalette, Icon,
    OverlayPosition, Spacing, confirm_modal, drive_overlay_focus, icon, overlay, page_frame,
    spacing, tr,
};
use forge_runtime::{EventBus, ScriptCatalog};
use forge_storage::DataProvider;
use forge_types::{IntegrationAvailability, ScriptId};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Pixels, SharedString,
    Subscription, Window, div, px,
};
use std::sync::Arc;

const GLYPH_ACTION: Pixels = px(12.0);
const GLYPH_STATUS: Pixels = px(12.0);
const SCRIPT_LANGUAGE_LABEL: &str = "Rhai";

#[derive(Clone)]
struct ScriptDeps {
    backend: Arc<dyn DataProvider>,
    script_catalog: Arc<ScriptCatalog>,
    bus: Arc<EventBus>,
    integrations: Arc<dyn IntegrationAvailability>,
    rt_handle: tokio::runtime::Handle,
}

impl ScriptDeps {
    fn lifecycle(&self) -> ScriptLifecycle {
        ScriptLifecycle::new(
            self.backend.script_repo(),
            Arc::clone(&self.script_catalog),
            Arc::clone(&self.bus),
        )
    }
}

enum PendingNav {
    SelectScript(ScriptId),
    NewScript,
}

pub(crate) struct ScriptEditorView {
    list: Entity<ScriptList>,
    document: Entity<ScriptDocument>,
    pending_nav: Option<PendingNav>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<NavRequested> for ScriptEditorView {}

impl UnsavedWork for ScriptEditorView {
    fn has_unsaved_work(&self, cx: &App) -> bool {
        self.document.read(cx).current_dirty(cx)
    }

    fn unsaved_work_name(&self, cx: &App) -> Option<SharedString> {
        self.document.read(cx).open_name()
    }

    fn start_saving_unsaved_work(&mut self, cx: &mut Context<Self>) -> bool {
        self.document.update(cx, |document, cx| {
            document.save(cx);
            document.is_saving()
        })
    }

    fn is_saving_unsaved_work(&self, cx: &App) -> bool {
        self.document.read(cx).is_saving()
    }

    fn discard_unsaved_work(&mut self, cx: &mut Context<Self>) {
        self.pending_nav = None;
        self.document
            .update(cx, |document, cx| document.revert_current(cx));
        cx.notify();
    }
}

impl ScreenShortcuts for ScriptEditorView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        if !self.prompt_open(cx) {
            self.start_saving_unsaved_work(cx);
        }
    }

    fn run_shortcut(&mut self, cx: &mut Context<Self>) {
        if !self.prompt_open(cx) {
            self.document.update(cx, |document, cx| document.run(cx));
        }
    }
}

impl ScriptEditorView {
    pub(crate) fn new(
        backend: Arc<dyn DataProvider>,
        script_catalog: Arc<ScriptCatalog>,
        bus: Arc<EventBus>,
        integrations: Arc<dyn IntegrationAvailability>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let deps = ScriptDeps {
            backend,
            script_catalog,
            bus,
            integrations,
            rt_handle,
        };
        let console = cx.new(|cx| console::ScriptConsole::new(&deps.bus, cx));
        let list = cx.new(|cx| ScriptList::new(deps.clone(), console.clone(), cx));
        let document = cx.new(|cx| ScriptDocument::new(deps, console, cx));
        let subscriptions = [
            cx.subscribe(&list, Self::on_list_event),
            cx.subscribe(&document, Self::on_document_event),
        ];
        Self {
            list,
            document,
            pending_nav: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    fn parts(&self) -> (Entity<ScriptList>, Entity<ScriptDocument>) {
        (self.list.clone(), self.document.clone())
    }

    fn prompt_open(&self, cx: &App) -> bool {
        self.document.read(cx).run_modal_open()
            || self.list.read(cx).prompt_open()
            || self.pending_nav.is_some()
    }

    fn on_list_event(
        &mut self,
        _list: Entity<ScriptList>,
        event: &ScriptListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ScriptListEvent::RowChosen(id) => self.select(*id, cx),
            ScriptListEvent::NewRequested => self.new_script(cx),
            ScriptListEvent::Loaded { first } => {
                if let Some(id) = *first
                    && !self.document.read(cx).has_open()
                {
                    self.open_script(id, cx);
                }
                self.sync_linked(cx);
            }
            ScriptListEvent::Created(record) => {
                let record = record.clone();
                self.document.update(cx, |document, cx| {
                    document.apply_script_opened(Ok(record), cx);
                });
            }
            ScriptListEvent::SelectionLost => {
                self.document.update(cx, |document, cx| document.close(cx));
            }
            ScriptListEvent::Renamed { id, name } => {
                self.document.update(cx, |document, cx| {
                    document.apply_renamed(*id, name.clone(), cx);
                });
            }
            ScriptListEvent::RowMenuOpened => {
                self.document
                    .update(cx, |document, cx| document.close_kind_menu(cx));
            }
        }
    }

    fn on_document_event(
        &mut self,
        _document: Entity<ScriptDocument>,
        event: &ScriptDocumentEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ScriptDocumentEvent::Opened => self.sync_linked(cx),
            ScriptDocumentEvent::Saved(id) => {
                self.list.update(cx, |list, cx| list.mark_valid(*id, cx));
            }
            ScriptDocumentEvent::KindMenuOpened => {
                self.list.update(cx, |list, cx| list.close_row_menu(cx));
            }
            ScriptDocumentEvent::OpenAction(action_id) => {
                cx.emit(NavRequested(Screen::Actions(Some(*action_id))));
            }
        }
    }

    fn sync_linked(&mut self, cx: &mut Context<Self>) {
        let linked = self
            .document
            .read(cx)
            .open_id()
            .and_then(|id| self.list.read(cx).linked_action(id));
        self.document
            .update(cx, |document, cx| document.set_linked(linked, cx));
    }

    fn select(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        if self.document.read(cx).current_dirty(cx) {
            self.pending_nav = Some(PendingNav::SelectScript(id));
            cx.notify();
            return;
        }
        self.open_script(id, cx);
    }

    fn open_script(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.mark_selected(id, cx));
        self.document
            .update(cx, |document, cx| document.open_script(id, cx));
    }

    fn new_script(&mut self, cx: &mut Context<Self>) {
        if self.document.read(cx).current_dirty(cx) {
            self.pending_nav = Some(PendingNav::NewScript);
            cx.notify();
            return;
        }
        self.list.update(cx, |list, cx| list.create_script(cx));
    }

    fn confirm_discard(&mut self, cx: &mut Context<Self>) {
        let Some(nav) = self.pending_nav.take() else {
            return;
        };
        self.document
            .update(cx, |document, cx| document.revert_current(cx));
        match nav {
            PendingNav::SelectScript(id) => self.open_script(id, cx),
            PendingNav::NewScript => self.new_script(cx),
        }
        cx.notify();
    }

    fn cancel_discard(&mut self, cx: &mut Context<Self>) {
        self.pending_nav = None;
        cx.notify();
    }

    fn discard_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.pending_nav.as_ref()?;

        let card = confirm_modal(
            tr!("script_editor_discard_title"),
            tr!("script_editor_discard_body"),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint(tr!("script_editor_discard_esc_hint"))
        .on_cancel(
            "script-discard-cancel",
            tr!("script_editor_discard_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_discard(cx)),
        )
        .on_confirm(
            "script-discard-confirm",
            tr!("script_editor_discard_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_discard(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-discard-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_discard(cx));
                })
                .into_any_element(),
        )
    }

    fn render_header_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let (ok, total) = self.list.read(cx).health();

        let mut right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density));

        if total > 0 {
            let health = div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(Icon::CircleCheck, GLYPH_STATUS, palette.success))
                .child(
                    div()
                        .font_family(cx.body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_muted)
                        .child(tr!(
                            "script_editor_health",
                            ok = i64::try_from(ok).unwrap_or(i64::MAX),
                            total = i64::try_from(total).unwrap_or(i64::MAX)
                        )),
                );
            right = right.child(health).child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("·"),
            );
        }

        right
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(SCRIPT_LANGUAGE_LABEL),
            )
            .into_any_element()
    }
}

impl Render for ScriptEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_open =
            self.list.read(cx).row_menu_open() || self.document.read(cx).kind_menu_open();
        drive_overlay_focus(
            menu_open,
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.render_header_right(&palette, density, cx);

        let body = div()
            .w_full()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(self.list.clone())
            .child(self.document.clone());

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("script_editor_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("nav_script_editor")),
            ],
            &palette,
        )
        .header_right(header_right)
        .body(body);

        let overlay = self
            .document
            .read(cx)
            .run_modal_layer()
            .or_else(|| {
                self.list
                    .update(cx, |list, cx| list.render_prompt_layer(&palette, cx))
            })
            .or_else(|| self.discard_overlay(&palette, cx));

        let menu_focus = self.menu_focus.clone();
        let row_menu = self.list.update(cx, |list, cx| {
            list.render_row_context_menu(&palette, &menu_focus, cx)
        });
        let kind_menu = self.document.update(cx, |document, cx| {
            document.render_kind_menu(&palette, &menu_focus, cx)
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(overlay)
            .children(row_menu)
            .children(kind_menu)
    }
}
