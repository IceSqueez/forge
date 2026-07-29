use forge_components::{
    BORDER_THIN, Confirm, ConfirmTone, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent,
    OverlayPosition, TextArea, body_family, confirm_modal, empty_state, ghost_button_with_icon,
    icon, mono_family, overlay, primary_button_with_icon, status_dot, tr,
};
use forge_overlay::OVERRIDABLE_FILES;
use forge_storage::{OverlayDefinition, OverlayId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, div,
    prelude::*, px,
};
use std::sync::Arc;

use crate::async_bridge;
use crate::presentation::ActivePresentation;

use super::{EditorMode, OverlaysView};

const TABS_PAD_V: Pixels = px(6.0);
const TABS_PAD_H: Pixels = px(12.0);
const TABS_GAP: Pixels = px(2.0);
const TAB_PAD_V: Pixels = px(4.0);
const TAB_PAD_H: Pixels = px(12.0);
const TAB_RADIUS: Pixels = px(6.0);
const TAB_FS: Pixels = px(11.5);
const TAB_GAP: Pixels = px(6.0);
const TAB_DOT: Pixels = px(6.0);
const TAB_MARK: Pixels = px(11.0);

const ACTIONS_GAP: Pixels = px(8.0);
const ACTION_H: Pixels = px(24.0);

const HINT_GAP: Pixels = px(6.0);
const HINT_PAD_V: Pixels = px(8.0);
const HINT_PAD_H: Pixels = px(16.0);
const HINT_GLYPH: Pixels = px(12.0);

const EDITOR_FS: Pixels = px(12.5);

const FOOT_GAP: Pixels = px(10.0);
const FOOT_PAD_V: Pixels = px(7.0);
const FOOT_PAD_H: Pixels = px(14.0);
const FOOT_FS: Pixels = px(10.5);
const FOOT_GLYPH: Pixels = px(12.0);

/// What the user asked for while the editor still held unsaved text.
pub(super) enum LeaveIntent {
    File(&'static str),
    Overlay(OverlayId),
    Design,
}

struct OpenSource {
    id: OverlayId,
    file: &'static str,
    original: String,
}

pub(super) struct CodeState {
    editor: Entity<TextArea>,
    file: &'static str,
    open: Option<OpenSource>,
    loading: bool,
    saving: bool,
    /// Files the record claims as user-owned whose copy is gone from disk.
    missing: Vec<String>,
    pending_revert: Confirm<&'static str>,
    pending_leave: Confirm<LeaveIntent>,
    _sub: Subscription,
}

impl CodeState {
    pub(super) fn new(cx: &mut Context<OverlaysView>) -> Self {
        let palette = cx.palette();
        let editor = cx.new(|cx| {
            TextArea::new(tr!("overlays_code_placeholder"), cx)
                .with_palette(palette)
                .mono()
                .with_gutter()
                .with_font_size(EDITOR_FS)
                .fill()
        });
        let sub = cx.subscribe(&editor, |_this, _area, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                cx.notify();
            }
        });

        Self {
            editor,
            file: first_file(),
            open: None,
            loading: false,
            saving: false,
            missing: Vec::new(),
            pending_revert: Confirm::default(),
            pending_leave: Confirm::default(),
            _sub: sub,
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending_revert.is_pending() || self.pending_leave.is_pending()
    }
}

fn first_file() -> &'static str {
    OVERRIDABLE_FILES.first().copied().unwrap_or_default()
}

/// The tab reads as the language the file carries, which is what the user is looking for.
fn tab_label(file: &str) -> String {
    file.rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or(file)
        .to_uppercase()
}

fn element_id(prefix: &str, file: &str) -> SharedString {
    SharedString::from(format!("{prefix}-{}", file.replace('.', "-")))
}

impl OverlaysView {
    fn code_body(&self, cx: &App) -> String {
        self.code.editor.read(cx).content().to_owned()
    }

    pub(super) fn code_dirty(&self, cx: &App) -> bool {
        self.code
            .open
            .as_ref()
            .is_some_and(|open| open.original != self.code_body(cx))
    }

    fn is_overridden(&self, file: &str) -> bool {
        self.selected_definition()
            .is_some_and(|definition| definition.source_overrides.iter().any(|held| held == file))
    }

    fn is_missing(&self, file: &str) -> bool {
        self.code.missing.iter().any(|held| held == file)
    }

    fn mark_missing(&mut self, file: &'static str, missing: bool) {
        self.code.missing.retain(|held| held != file);
        if missing {
            self.code.missing.push(file.to_owned());
        }
    }

    /// Replaces the whole set: a materialize pass reports on every file the record claims.
    pub(super) fn note_missing_overrides(&mut self, missing: Vec<String>) {
        self.code.missing = missing;
    }

    /// Loads whatever the disk holds for the current selection whenever it stops matching the open
    /// buffer; an unchanged pair is left alone so a reload never discards what the user is typing.
    pub(super) fn sync_source(&mut self, cx: &mut Context<Self>) {
        if self.mode() != EditorMode::Code {
            return;
        }
        let Some(id) = self.selected.clone() else {
            self.code.open = None;
            self.code.loading = false;
            self.code.editor.update(cx, |area, cx| area.clear(cx));
            return;
        };
        let file = self.code.file;
        let matched = self
            .code
            .open
            .as_ref()
            .is_some_and(|open| open.id == id && open.file == file);
        if matched || self.code.loading {
            return;
        }

        self.code.loading = true;
        let service = self.service.clone();
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .read_source(&target, file)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |this, result, cx| this.apply_source(id, file, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_source(
        &mut self,
        id: OverlayId,
        file: &'static str,
        result: Result<Option<String>, String>,
        cx: &mut Context<Self>,
    ) {
        self.code.loading = false;
        match result {
            Ok(body) => {
                self.mark_missing(file, body.is_none() && self.is_overridden(file));
                let body = body.unwrap_or_default();
                self.code
                    .editor
                    .update(cx, |area, cx| area.set_content(body.clone(), cx));
                self.code.open = Some(OpenSource {
                    id,
                    file,
                    original: body,
                });
                self.sync_source(cx);
            }
            Err(message) => self.report(&message, cx),
        }
        cx.notify();
    }

    fn select_file(&mut self, file: &'static str, cx: &mut Context<Self>) {
        if self.code.file == file {
            return;
        }
        if self.code_dirty(cx) {
            self.code.pending_leave.request(LeaveIntent::File(file));
            cx.notify();
            return;
        }
        self.code.file = file;
        self.sync_source(cx);
        cx.notify();
    }

    /// Saving is what claims the file: the record gains the override, the page keeps the text
    /// verbatim, and only the connected pages are told to reload.
    fn save_source(&mut self, cx: &mut Context<Self>) {
        if self.code.saving || !self.code_dirty(cx) {
            return;
        }
        let Some(id) = self.selected.clone() else {
            return;
        };
        let file = self.code.file;
        let body = self.code_body(cx);

        self.code.saving = true;
        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        let stored = body.clone();
        let saved = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let Some(mut definition) = repo.get(&id).await.map_err(|e| e.to_string())? else {
                    return Ok(false);
                };
                if !definition.source_overrides.iter().any(|held| held == file) {
                    definition.source_overrides.push(file.to_owned());
                    repo.save(&definition).await.map_err(|e| e.to_string())?;
                }
                service
                    .write_source(&id, file, stored)
                    .await
                    .map_err(|e| e.to_string())?;
                service.reload_page(&id);
                Ok(true)
            },
            move |this, result: Result<bool, String>, cx| {
                this.code.saving = false;
                match result {
                    Ok(true) => {
                        if let Some(open) = this
                            .code
                            .open
                            .as_mut()
                            .filter(|open| open.id == saved && open.file == file)
                        {
                            open.original = body;
                        }
                        this.mark_missing(file, false);
                        this.load(cx);
                    }
                    Ok(false) => this.report(&tr!("overlays_toast_missing"), cx),
                    Err(message) => this.report(&message, cx),
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn prompt_revert(&mut self, cx: &mut Context<Self>) {
        self.code.pending_revert.request(self.code.file);
        cx.notify();
    }

    fn cancel_revert(&mut self, cx: &mut Context<Self>) {
        self.code.pending_revert.cancel();
        cx.notify();
    }

    /// Dropping the override hands the file back to the generator, so the shipped asset is written
    /// afresh and the page reloads onto it.
    fn confirm_revert(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.code.pending_revert.take() else {
            return;
        };
        let Some(id) = self.selected.clone() else {
            return;
        };

        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let Some(mut definition) = repo.get(&id).await.map_err(|e| e.to_string())? else {
                    return Ok((false, Vec::new()));
                };
                definition.source_overrides.retain(|held| held != file);
                repo.save(&definition).await.map_err(|e| e.to_string())?;
                let report = service.materialize(&id).await.map_err(|e| e.to_string())?;
                Ok((true, report.missing_overrides))
            },
            move |this, result: Result<(bool, Vec<String>), String>, cx| {
                match result {
                    Ok((true, missing)) => {
                        this.note_missing_overrides(missing);
                        this.code.open = None;
                        this.load(cx);
                        this.sync_source(cx);
                    }
                    Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                    Err(message) => this.report(&message, cx),
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn request_leave(&mut self, intent: LeaveIntent, cx: &mut Context<Self>) {
        self.code.pending_leave.request(intent);
        cx.notify();
    }

    fn cancel_leave(&mut self, cx: &mut Context<Self>) {
        self.code.pending_leave.cancel();
        cx.notify();
    }

    fn confirm_leave(&mut self, cx: &mut Context<Self>) {
        let Some(intent) = self.code.pending_leave.take() else {
            return;
        };
        if let Some(open) = self.code.open.as_ref() {
            let original = open.original.clone();
            self.code
                .editor
                .update(cx, |area, cx| area.set_content(original, cx));
        }
        match intent {
            LeaveIntent::File(file) => self.select_file(file, cx),
            LeaveIntent::Overlay(id) => self.select(id, cx),
            LeaveIntent::Design => self.set_mode(EditorMode::Design, cx),
        }
        cx.notify();
    }

    pub(super) fn render_code_stage(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.render_file_tabs(definition, palette, cx))
            .child(self.render_escape_hint(palette, cx))
            .child(self.render_editor(palette))
            .child(self.render_code_footer(definition, palette))
            .into_any_element()
    }

    fn render_file_tabs(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = self.visuals(definition, palette).accent;
        let mut strip = div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(TABS_GAP)
            .px(TABS_PAD_H)
            .py(TABS_PAD_V)
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);

        for file in OVERRIDABLE_FILES {
            let file = *file;
            let active = self.code.file == file;
            let marker = if self.is_missing(file) {
                Some((Icon::AlertTriangle, palette.random))
            } else if self.is_overridden(file) {
                Some((Icon::Pencil, palette.warning))
            } else {
                None
            };

            strip = strip.child(
                div()
                    .id(element_id("overlays-code-tab", file))
                    .flex()
                    .items_center()
                    .gap(TAB_GAP)
                    .px(TAB_PAD_H)
                    .py(TAB_PAD_V)
                    .rounded(TAB_RADIUS)
                    .cursor_pointer()
                    .when(active, |tab| tab.bg(palette.surface_overlay))
                    .font_family(mono_family())
                    .text_size(TAB_FS)
                    .text_color(if active {
                        palette.text_primary
                    } else {
                        palette.text_secondary
                    })
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.select_file(file, cx)),
                    )
                    .child(status_dot(
                        if active { accent } else { palette.text_faint },
                        TAB_DOT,
                    ))
                    .child(tab_label(file))
                    .children(marker.map(|(glyph, tint)| icon(glyph, TAB_MARK, tint))),
            );
        }

        strip
            .child(div().flex_1().min_w(px(0.0)))
            .child(self.render_code_meta(palette, cx))
            .children(self.render_code_actions(palette, cx))
            .into_any_element()
    }

    fn render_code_meta(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let lines = self.code_body(cx).lines().count().max(1);
        let state = if self.code.saving {
            tr!("overlays_code_state_saving")
        } else if self.code_dirty(cx) {
            tr!("overlays_code_state_unsaved")
        } else {
            tr!("overlays_code_state_saved")
        };

        div()
            .flex_none()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "overlays_code_meta",
                lines = lines as i64,
                state = state.as_str()
            ))
            .into_any_element()
    }

    fn render_code_actions(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.selected_definition()?;
        let file = self.code.file;
        let mut row = div().flex_none().flex().items_center().gap(ACTIONS_GAP);

        if self.is_overridden(file) {
            row = row.child(
                ghost_button_with_icon(Icon::ArrowBackUp, tr!("overlays_code_revert"), palette)
                    .height(ACTION_H)
                    .on_click(
                        element_id("overlays-code-revert", file),
                        cx.listener(|this, _: &ClickEvent, _, cx| this.prompt_revert(cx)),
                    ),
            );
        }

        Some(
            row.child(
                primary_button_with_icon(Icon::DeviceFloppy, tr!("overlays_code_save"), palette)
                    .height(ACTION_H)
                    .disabled(self.code.saving || !self.code_dirty(cx))
                    .on_click(
                        element_id("overlays-code-save", file),
                        cx.listener(|this, _: &ClickEvent, _, cx| this.save_source(cx)),
                    ),
            )
            .into_any_element(),
        )
    }

    /// The framing is shown in both states: before the first save it says what saving costs, after
    /// it says what the user already owns.
    fn render_escape_hint(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let file = self.code.file;
        let (glyph, tint, message) = if self.is_missing(file) {
            (
                Icon::AlertTriangle,
                palette.random,
                tr!("overlays_code_hint_missing", file = file),
            )
        } else if self.is_overridden(file) {
            (
                Icon::Pencil,
                palette.warning,
                tr!("overlays_code_hint_owned", file = file),
            )
        } else {
            (
                Icon::InfoCircle,
                palette.text_faint,
                tr!("overlays_code_hint_generated", file = file),
            )
        };

        let mut row = div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(HINT_GAP)
            .px(HINT_PAD_H)
            .py(HINT_PAD_V)
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(glyph, HINT_GLYPH, tint))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(message),
            );

        if self.is_missing(file) {
            row = row.child(
                ghost_button_with_icon(Icon::ArrowBackUp, tr!("overlays_code_restore"), palette)
                    .height(ACTION_H)
                    .on_click(
                        element_id("overlays-code-restore", file),
                        cx.listener(|this, _: &ClickEvent, _, cx| this.prompt_revert(cx)),
                    ),
            );
        }

        row.into_any_element()
    }

    fn render_editor(&self, palette: &ForgePalette) -> AnyElement {
        let frame = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .bg(palette.base);

        if self.code.loading && self.code.open.is_none() {
            return frame
                .items_center()
                .justify_center()
                .child(empty_state(tr!("overlays_code_loading"), palette).glyph(Icon::FileCode))
                .into_any_element();
        }

        frame.child(self.code.editor.clone()).into_any_element()
    }

    fn render_code_footer(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(FOOT_GAP)
            .px(FOOT_PAD_H)
            .py(FOOT_PAD_V)
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::InfoCircle, FOOT_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FOOT_FS)
                    .text_color(palette.text_faint)
                    .child(tr!("overlays_code_footer_reload")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FOOT_FS)
                    .text_color(palette.text_muted)
                    .child(format!(
                        "/overlays/{}/{}",
                        definition.id.as_str(),
                        self.code.file
                    )),
            )
            .into_any_element()
    }

    /// Only fires while a file is claimed and the type moved on, so a build that never bumped a
    /// schema shows nothing.
    pub(super) fn render_schema_notice(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
    ) -> Option<AnyElement> {
        if definition.source_overrides.is_empty() {
            return None;
        }
        let descriptor = self.kinds.get(&definition.kind_id)?;
        if descriptor.config_schema_version() == definition.config_schema_version {
            return None;
        }
        let files = definition.source_overrides.join(", ");

        Some(
            div()
                .flex_none()
                .w_full()
                .flex()
                .items_center()
                .gap(HINT_GAP)
                .px(HINT_PAD_H)
                .py(HINT_PAD_V)
                .bg(palette.shell)
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular)
                .child(icon(Icon::InfoCircle, HINT_GLYPH, palette.info))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("overlays_code_schema_notice", files = files.as_str())),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_code_confirms(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut open = Vec::new();
        if let Some(file) = self.code.pending_revert.get() {
            open.push(self.render_revert_confirm(file, palette, cx));
        }
        if self.code.pending_leave.is_pending() {
            open.push(self.render_leave_confirm(palette, cx));
        }
        open
    }

    fn render_revert_confirm(
        &self,
        file: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = confirm_modal(
            tr!("overlays_code_revert_title"),
            tr!("overlays_code_revert_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(file.to_owned())
        .on_cancel(
            "overlays-code-revert-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_revert(cx)),
        )
        .on_confirm(
            "overlays-code-revert-confirm",
            tr!("overlays_code_revert_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_revert(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("overlays-code-revert-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.cancel_revert(cx));
            })
            .into_any_element()
    }

    fn render_leave_confirm(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let card = confirm_modal(
            tr!("overlays_code_discard_title"),
            tr!("overlays_code_discard_body"),
            ConfirmTone::Warning,
            palette,
        )
        .item_name(self.code.file.to_owned())
        .on_cancel(
            "overlays-code-discard-cancel",
            tr!("overlays_code_discard_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_leave(cx)),
        )
        .on_confirm(
            "overlays-code-discard-confirm",
            tr!("overlays_code_discard_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_leave(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("overlays-code-discard-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.cancel_leave(cx));
            })
            .into_any_element()
    }
}
