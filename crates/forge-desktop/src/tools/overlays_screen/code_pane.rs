use forge_components::Density;
use forge_components::FONT_LABEL;
use forge_components::FONT_ROW;
use forge_components::Language;
use forge_components::Radius;
use forge_components::Spacing;
use forge_components::radius;
use forge_components::spacing;
use forge_components::{
    ActivePresentation, BORDER_THIN, CodeEditor, Confirm, ConfirmTone, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, KeyActivated, OverlayPosition, SaveCode, confirm_modal,
    empty_state, ghost_button_with_icon, icon, overlay, primary_button_with_icon, scaled,
    status_dot, tr,
};
use forge_overlay::{MaterializeReport, MediaIssue, OVERRIDABLE_FILES};
use forge_runtime::{OverlaySampleSource, OverlayServiceHandle};
use forge_storage::{OverlayDefinition, OverlayId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, SharedString,
    Subscription, div, prelude::*, px,
};
use std::sync::Arc;

use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;
use crate::shared::unsaved_work::UnsavedWork;

use super::editor_state::EditorMode;
use super::{OverlayProblem, OverlaysView, report_problem};

const TABS_PAD_H: Pixels = px(12.0);
const TABS_GAP: Pixels = px(2.0);
const TAB_PAD_H: Pixels = px(12.0);
const TAB_RADIUS: Pixels = radius(Radius::Sm);
const TAB_FS: Pixels = FONT_LABEL;
const TAB_DOT: Pixels = px(6.0);
const TAB_MARK: Pixels = px(11.0);

const ACTIONS_GAP: Pixels = px(8.0);
const ACTION_H: Pixels = px(24.0);

const HINT_PAD_V: Pixels = px(8.0);
const HINT_GLYPH: Pixels = px(12.0);

const EDITOR_FS: Pixels = FONT_ROW;
const EDITOR_LINE_HEIGHT: Pixels = px(20.0);

const FOOT_PAD_V: Pixels = px(7.0);
const FOOT_PAD_H: Pixels = px(14.0);
const FOOT_FS: Pixels = FONT_XXS;
const FOOT_GLYPH: Pixels = px(12.0);

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

pub(super) enum CodeEvent {
    SourceSaved,
    SourceReverted {
        id: OverlayId,
        media_issues: Vec<MediaIssue>,
    },
    LeftForOverlay(OverlayId),
    LeftForDesign,
}

#[derive(Default)]
pub(super) struct CodeTarget {
    pub(super) selected: Option<OverlayId>,
    pub(super) overrides: Vec<String>,
    pub(super) active: bool,
}

pub(super) struct CodeStageFacts {
    pub(super) accent: Rgba,
    pub(super) served_path: String,
}

pub(super) struct CodeServices {
    pub(super) service: OverlayServiceHandle,
    pub(super) samples: Arc<dyn OverlaySampleSource>,
    pub(super) rt_handle: tokio::runtime::Handle,
}

pub(super) struct OverlayCodeEditor {
    editor: Entity<CodeEditor>,
    file: &'static str,
    open: Option<OpenSource>,
    loading: bool,
    revision: u64,
    saving: bool,
    missing: Vec<String>,
    pending_revert: Confirm<&'static str>,
    pending_leave: Confirm<LeaveIntent>,
    target: CodeTarget,
    services: CodeServices,
    _sub: Subscription,
}

impl EventEmitter<CodeEvent> for OverlayCodeEditor {}

impl OverlayCodeEditor {
    pub(super) fn new(services: CodeServices, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            CodeEditor::new(
                file_language(first_file()),
                tr!("overlays_code_placeholder"),
                cx,
            )
            .with_font_size(EDITOR_FS)
            .with_line_height(EDITOR_LINE_HEIGHT)
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
            revision: 0,
            saving: false,
            missing: Vec::new(),
            pending_revert: Confirm::default(),
            pending_leave: Confirm::default(),
            target: CodeTarget::default(),
            services,
            _sub: sub,
        }
    }
}

fn first_file() -> &'static str {
    OVERRIDABLE_FILES.first().copied().unwrap_or_default()
}

fn file_language(file: &str) -> Language {
    match file.rsplit_once('.').map(|(_, extension)| extension) {
        Some("css") => Language::Css,
        Some("js") => Language::JavaScript,
        _ => Language::Html,
    }
}

fn tab_label(file: &str) -> String {
    file.rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or(file)
        .to_uppercase()
}

fn element_id(prefix: &str, file: &str) -> SharedString {
    SharedString::from(format!("{prefix}-{}", file.replace('.', "-")))
}

impl OverlayCodeEditor {
    pub(super) fn file(&self) -> &'static str {
        self.file
    }

    fn code_body(&self, cx: &App) -> String {
        self.editor.read(cx).content().to_owned()
    }

    pub(super) fn is_dirty(&self, cx: &App) -> bool {
        let editor = self.editor.read(cx);
        self.open
            .as_ref()
            .is_some_and(|open| open.original != editor.content())
    }

    fn is_overridden(&self, file: &str) -> bool {
        self.target.overrides.iter().any(|held| held == file)
    }

    fn is_missing(&self, file: &str) -> bool {
        self.missing.iter().any(|held| held == file)
    }

    fn mark_missing(&mut self, file: &'static str, missing: bool) {
        self.missing.retain(|held| held != file);
        if missing {
            self.missing.push(file.to_owned());
        }
    }

    pub(super) fn note_missing_overrides(&mut self, missing: Vec<String>) {
        self.missing = missing;
    }

    pub(super) fn invalidate_source(&mut self, cx: &App) {
        if self.is_dirty(cx) {
            return;
        }
        self.open = None;
        self.revision += 1;
    }

    pub(super) fn retarget(&mut self, target: CodeTarget, cx: &mut Context<Self>) {
        self.target = target;
        self.sync_source(cx);
    }

    fn sync_source(&mut self, cx: &mut Context<Self>) {
        if !self.target.active {
            return;
        }
        let Some(id) = self.target.selected.clone() else {
            self.open = None;
            self.loading = false;
            self.editor.update(cx, |area, cx| area.clear(cx));
            return;
        };
        let file = self.file;
        let matched = self
            .open
            .as_ref()
            .is_some_and(|open| open.id == id && open.file == file);
        if matched || self.loading {
            return;
        }

        self.loading = true;
        let revision = self.revision;
        let service = self.services.service.clone();
        let target = id.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                service
                    .read_source(&target, file)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |this, result, cx| this.apply_source(id, file, revision, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_source(
        &mut self,
        id: OverlayId,
        file: &'static str,
        revision: u64,
        result: Result<Option<String>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        if revision != self.revision {
            self.sync_source(cx);
            cx.notify();
            return;
        }
        match result {
            Ok(body) => {
                self.mark_missing(file, body.is_none() && self.is_overridden(file));
                let body = body.unwrap_or_default();
                self.editor.update(cx, |editor, cx| {
                    editor.set_language(file_language(file), cx);
                    editor.set_content(body.clone(), cx);
                });
                self.open = Some(OpenSource {
                    id,
                    file,
                    original: body,
                });
                self.sync_source(cx);
            }
            Err(message) => report_problem(message.into(), cx),
        }
        cx.notify();
    }

    fn select_file(&mut self, file: &'static str, cx: &mut Context<Self>) {
        if self.file == file {
            return;
        }
        if self.is_dirty(cx) {
            self.pending_leave.request(LeaveIntent::File(file));
            cx.notify();
            return;
        }
        self.file = file;
        self.sync_source(cx);
        cx.notify();
    }

    fn save_source(&mut self, cx: &mut Context<Self>) {
        if self.saving || !self.is_dirty(cx) {
            return;
        }
        let Some(id) = self.target.selected.clone() else {
            return;
        };
        let file = self.file;
        let body = self.code_body(cx);

        self.saving = true;
        let service = self.services.service.clone();
        let stored = body.clone();
        let saved = id.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                service
                    .write_override(&id, file, stored)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |this, result: Result<bool, RepoFailure>, cx| {
                this.saving = false;
                match result {
                    Ok(true) => {
                        if let Some(open) = this
                            .open
                            .as_mut()
                            .filter(|open| open.id == saved && open.file == file)
                        {
                            open.original = body;
                        }
                        this.mark_missing(file, false);
                        cx.emit(CodeEvent::SourceSaved);
                    }
                    Ok(false) => report_problem(OverlayProblem::Missing, cx),
                    Err(message) => report_problem(message.into(), cx),
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn prompt_revert(&mut self, cx: &mut Context<Self>) {
        self.pending_revert.request(self.file);
        cx.notify();
    }

    fn cancel_revert(&mut self, cx: &mut Context<Self>) {
        self.pending_revert.cancel();
        cx.notify();
    }

    fn confirm_revert(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.pending_revert.take() else {
            return;
        };
        let Some(id) = self.target.selected.clone() else {
            return;
        };

        let service = self.services.service.clone();
        let samples = Arc::clone(&self.services.samples);
        let target = id.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                service
                    .revert_override(&id, file, samples.as_ref())
                    .await
                    .map_err(RepoFailure::from)
            },
            move |this, result: Result<Option<MaterializeReport>, RepoFailure>, cx| {
                match result {
                    Ok(Some(report)) => {
                        this.note_missing_overrides(report.missing_overrides);
                        this.open = None;
                        this.revision += 1;
                        cx.emit(CodeEvent::SourceReverted {
                            id: target,
                            media_issues: report.media_issues,
                        });
                    }
                    Ok(None) => report_problem(OverlayProblem::Missing, cx),
                    Err(message) => report_problem(message.into(), cx),
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn request_leave(&mut self, intent: LeaveIntent, cx: &mut Context<Self>) {
        self.pending_leave.request(intent);
        cx.notify();
    }

    fn cancel_leave(&mut self, cx: &mut Context<Self>) {
        self.pending_leave.cancel();
        cx.notify();
    }

    fn confirm_leave(&mut self, cx: &mut Context<Self>) {
        let Some(intent) = self.pending_leave.take() else {
            return;
        };
        self.restore_open_source(cx);
        match intent {
            LeaveIntent::File(file) => self.select_file(file, cx),
            LeaveIntent::Overlay(id) => cx.emit(CodeEvent::LeftForOverlay(id)),
            LeaveIntent::Design => cx.emit(CodeEvent::LeftForDesign),
        }
        cx.notify();
    }

    fn restore_open_source(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = self.open.as_ref() {
            let original = open.original.clone();
            self.editor
                .update(cx, |area, cx| area.set_content(original, cx));
        }
    }

    pub(super) fn render_code_stage(
        &self,
        facts: CodeStageFacts,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.render_file_tabs(facts.accent, palette, cx))
            .child(self.render_escape_hint(palette, cx))
            .child(self.render_editor(palette, cx))
            .child(self.render_code_footer(facts.served_path, palette, density, cx))
            .into_any_element()
    }

    fn render_file_tabs(
        &self,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let mut strip = div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(TABS_GAP, density))
            .px(scaled(TABS_PAD_H, density))
            .py(spacing(Spacing::Xs, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);

        for file in OVERRIDABLE_FILES {
            let file = *file;
            let active = self.file == file;
            let marker = if self.is_missing(file) {
                Some((Icon::AlertTriangle, palette.danger))
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
                    .gap(spacing(Spacing::Xs, density))
                    .px(scaled(TAB_PAD_H, density))
                    .py(spacing(Spacing::Xxs, density))
                    .rounded(TAB_RADIUS)
                    .cursor_pointer()
                    .when(active, |tab| tab.bg(palette.surface_overlay))
                    .font_family(cx.mono_family())
                    .text_size(TAB_FS)
                    .text_color(if active {
                        palette.text_primary
                    } else {
                        palette.text_secondary
                    })
                    .key_activated(palette.text_primary)
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
            .child(self.render_code_actions(palette, cx))
            .into_any_element()
    }

    fn render_code_meta(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let lines = self.editor.read(cx).line_count();
        let state = if self.saving {
            tr!("overlays_code_state_saving")
        } else if self.is_dirty(cx) {
            tr!("overlays_code_state_unsaved")
        } else {
            tr!("overlays_code_state_saved")
        };

        div()
            .flex_none()
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "overlays_code_meta",
                lines = i64::try_from(lines).unwrap_or(i64::MAX),
                state = state.as_str()
            ))
            .into_any_element()
    }

    fn render_code_actions(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let file = self.file;
        let mut row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(ACTIONS_GAP, density));

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

        row.child(
            primary_button_with_icon(Icon::DeviceFloppy, tr!("overlays_code_save"), palette)
                .height(ACTION_H)
                .disabled(!self.is_dirty(cx))
                .busy(self.saving)
                .on_click(
                    element_id("overlays-code-save", file),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.save_source(cx)),
                ),
        )
        .into_any_element()
    }

    fn render_escape_hint(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let file = self.file;
        let (glyph, tint, message) = if self.is_missing(file) {
            (
                Icon::AlertTriangle,
                palette.danger,
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
            .gap(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .py(scaled(HINT_PAD_V, density))
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(glyph, HINT_GLYPH, tint))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(cx.body_family())
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

    fn render_editor(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let frame = div()
            .on_action(cx.listener(|this, _: &SaveCode, _, cx| this.save_source(cx)))
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .bg(palette.base);

        if self.loading && self.open.is_none() {
            return frame
                .items_center()
                .justify_center()
                .child(empty_state(tr!("overlays_code_loading"), palette).glyph(Icon::FileCode))
                .into_any_element();
        }

        frame.child(self.editor.clone()).into_any_element()
    }

    fn render_code_footer(
        &self,
        served_path: String,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .px(scaled(FOOT_PAD_H, density))
            .py(scaled(FOOT_PAD_V, density))
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::InfoCircle, FOOT_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FOOT_FS)
                    .text_color(palette.text_faint)
                    .child(tr!("overlays_code_footer_reload")),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FOOT_FS)
                    .text_color(palette.text_muted)
                    .child(served_path),
            )
            .into_any_element()
    }

    pub(super) fn render_code_confirms(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut open = Vec::new();
        if let Some(file) = self.pending_revert.get() {
            open.push(self.render_revert_confirm(file, palette, cx));
        }
        if self.pending_leave.is_pending() {
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
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
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
            .on_dismiss("overlays-code-revert-dismiss", move |_window, cx| {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
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
        .item_name(self.file.to_owned())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
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
            .on_dismiss("overlays-code-discard-dismiss", move |_window, cx| {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = weak.update(cx, |this, cx| this.cancel_leave(cx));
            })
            .into_any_element()
    }
}

impl OverlaysView {
    pub(super) fn code_dirty(&self, cx: &App) -> bool {
        self.editor.code.read(cx).is_dirty(cx)
    }

    pub(super) fn sync_source(&mut self, cx: &mut Context<Self>) {
        let target = CodeTarget {
            selected: self.registry.selected.clone(),
            overrides: self
                .selected_definition()
                .map(|definition| definition.source_overrides.clone())
                .unwrap_or_default(),
            active: self.mode() == EditorMode::Code,
        };
        self.editor
            .code
            .update(cx, |code, cx| code.retarget(target, cx));
    }

    pub(super) fn request_leave(&mut self, intent: LeaveIntent, cx: &mut Context<Self>) {
        self.editor
            .code
            .update(cx, |code, cx| code.request_leave(intent, cx));
        cx.notify();
    }

    pub(super) fn on_code_event(
        &mut self,
        _code: Entity<OverlayCodeEditor>,
        event: &CodeEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            CodeEvent::SourceSaved => self.load(cx),
            CodeEvent::SourceReverted { id, media_issues } => {
                self.set_media_issues(id, media_issues.clone(), cx);
                self.load(cx);
                self.sync_source(cx);
            }
            CodeEvent::LeftForOverlay(id) => self.select(id.clone(), cx),
            CodeEvent::LeftForDesign => self.set_mode(EditorMode::Design, cx),
        }
        cx.notify();
    }

    pub(super) fn render_code_stage(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let facts = CodeStageFacts {
            accent: self.visuals(definition, palette).accent,
            served_path: self
                .displayed_served_path(&definition.id, self.editor.code.read(cx).file()),
        };
        self.editor
            .code
            .update(cx, |code, cx| code.render_code_stage(facts, palette, cx))
    }

    pub(super) fn render_code_confirms(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.editor
            .code
            .update(cx, |code, cx| code.render_code_confirms(palette, cx))
    }

    pub(super) fn render_schema_notice(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> Option<AnyElement> {
        if definition.source_overrides.is_empty() {
            return None;
        }
        let descriptor = self.handles.kinds.get(&definition.kind_id)?;
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
                .gap(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Md, density))
                .py(scaled(HINT_PAD_V, density))
                .bg(palette.shell)
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular)
                .child(icon(Icon::InfoCircle, HINT_GLYPH, palette.info))
                .child(
                    div()
                        .font_family(cx.body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("overlays_code_schema_notice", files = files.as_str())),
                )
                .into_any_element(),
        )
    }
}

impl UnsavedWork for OverlaysView {
    fn has_unsaved_work(&self, cx: &App) -> bool {
        self.code_dirty(cx)
    }

    fn unsaved_work_name(&self, cx: &App) -> Option<SharedString> {
        Some(SharedString::from(self.editor.code.read(cx).file))
    }

    fn start_saving_unsaved_work(&mut self, cx: &mut Context<Self>) -> bool {
        self.editor.code.update(cx, |code, cx| {
            code.save_source(cx);
            code.saving
        })
    }

    fn is_saving_unsaved_work(&self, cx: &App) -> bool {
        self.editor.code.read(cx).saving
    }

    fn discard_unsaved_work(&mut self, cx: &mut Context<Self>) {
        self.editor.code.update(cx, |code, cx| {
            code.pending_leave.cancel();
            code.restore_open_source(cx);
            cx.notify();
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use forge_overlay::{BEHAVIOR_FILE, MARKUP_FILE, STYLE_FILE};

    use super::*;

    #[test]
    fn each_overridable_file_opens_in_the_language_of_its_extension() {
        for (file, expected) in [
            (MARKUP_FILE, Language::Html),
            (STYLE_FILE, Language::Css),
            (BEHAVIOR_FILE, Language::JavaScript),
        ] {
            assert!(OVERRIDABLE_FILES.contains(&file), "{file} not overridable");
            assert_eq!(file_language(file), expected, "{file}");
        }
    }
}
