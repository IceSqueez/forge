use super::ScriptEditorView;
use crate::navigation::{NavRequested, Screen};
use forge_components::{
    BORDER_THIN, Density, FONT_SECTION_LABEL, FONT_XS, FONT_XXS, ForgePalette, HOVER_FILL_ALPHA,
    Icon, Radius, Spacing, body_family, fmt_display_relative, icon, mono_family, radius, scaled,
    spacing, status_dot, tr, with_alpha,
};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, Pixels, Rgba, SharedString, Window, div, px,
};

const GLYPH_FILE: Pixels = px(12.0);
const GLYPH_STATUS: Pixels = px(12.0);
const GLYPH_TOOLBAR: Pixels = px(13.0);
const GLYPH_RUN: Pixels = px(11.0);
const SCRIPT_LANGUAGE_LABEL: &str = "Rhai";
const FILE_BAR_STATUS_DOT: Pixels = px(6.0);
const FILE_BAR_DIRTY_DOT: Pixels = px(5.0);
const DIVIDER_H: Pixels = px(16.0);

impl ScriptEditorView {
    pub(super) fn render_header_right(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let total = self.scripts.len();
        let ok = self.scripts.iter().filter(|e| e.status_ok).count();

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
                        .font_family(body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_muted)
                        .child(tr!(
                            "script_editor_health",
                            ok = ok as i64,
                            total = total as i64
                        )),
                );
            right = right.child(health).child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("·"),
            );
        }

        let right = right.child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(SCRIPT_LANGUAGE_LABEL),
        );

        right.into_any_element()
    }

    pub(super) fn file_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.open.as_ref()?;
        let name = open.record.name.clone();
        let edited = format!(
            "{} {}",
            tr!("script_editor_edited_prefix"),
            fmt_display_relative(Some(open.record.last_modified))
        );

        let (status_color, status_text): (Rgba, String) = if self.compile_error.is_some() {
            (palette.warning, tr!("script_editor_type_check_failed"))
        } else {
            (palette.success, tr!("script_editor_type_check_passed"))
        };

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(name),
            );
        if let Some(linked) = self.find_entry(open.id).and_then(|e| e.linked.as_ref()) {
            name_row = name_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .py(scaled(px(1.0), density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(px(8.0))
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::ExternalLink, px(10.0), palette.brand))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_SECTION_LABEL)
                            .text_color(palette.brand)
                            .child(linked.name.clone()),
                    ),
            );
        }
        if self.current_dirty(cx) {
            name_row = name_row.child(status_dot(palette.warning, FILE_BAR_DIRTY_DOT));
        }

        let status = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(status_color, FILE_BAR_STATUS_DOT))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_text),
            );

        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Md, density))
                .bg(palette.base)
                .border_b(BORDER_THIN)
                .border_color(palette.surface_overlay)
                .child(icon(Icon::FileCode, GLYPH_FILE, palette.brand))
                .child(name_row)
                .child(status)
                .child(div().flex_1())
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(edited),
                )
                .into_any_element(),
        )
    }

    pub(super) fn toolbar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let run_idle = !self.run_in_flight;
        let run = div()
            .id("script-run")
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.success)
            .when(run_idle, |el| {
                el.cursor_pointer()
                    .hover(|s| s.bg(with_alpha(palette.success, HOVER_FILL_ALPHA)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.run(cx)))
            })
            .when(!run_idle, |el| {
                el.opacity(forge_components::DISABLED_OPACITY)
            })
            .child(icon(Icon::PlayerPlay, GLYPH_RUN, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("script_editor_run")),
            );

        let save = self.toolbar_button(
            "script-save",
            Icon::Download,
            tr!("script_editor_save"),
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
        );
        let format = self.toolbar_button(
            "script-format",
            Icon::Refresh,
            tr!("script_editor_format"),
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.format(cx)),
        );
        let debug =
            self.disabled_toolbar_button(Icon::Bolt, tr!("script_editor_debug"), palette, density);
        let api = self.toolbar_button(
            "script-api-docs",
            Icon::Notebook,
            tr!("script_editor_api_docs"),
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_api_docs(cx)),
        );

        let open_action = self
            .open
            .as_ref()
            .and_then(|o| self.find_entry(o.id))
            .and_then(|e| e.linked.as_ref())
            .map(|linked| {
                let action_id = linked.id;
                let hover = palette.elevated;
                div()
                    .id("script-open-action")
                    .flex()
                    .items_center()
                    .gap(scaled(px(5.0), density))
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |_this, _: &ClickEvent, _, cx| {
                        cx.emit(NavRequested(Screen::Actions(Some(action_id))));
                    }))
                    .child(icon(Icon::ExternalLink, GLYPH_TOOLBAR, palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.brand)
                            .child(tr!("script_editor_open_action")),
                    )
            });

        let divider = div()
            .w(BORDER_THIN)
            .h(DIVIDER_H)
            .mx(spacing(Spacing::Xs, density))
            .bg(palette.surface_overlay);

        let left = div()
            .flex()
            .items_center()
            .child(run)
            .child(save)
            .child(debug)
            .child(format)
            .child(divider)
            .child(api)
            .children(open_action);

        let sandbox = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_sandbox_label")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.success)
                    .child(tr!("script_editor_sandbox_enabled")),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(sandbox)
            .children(self.engine_timeout_ms.map(|ms| {
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_timeout_ms", ms = ms as i64))
            }));

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn toolbar_button(
        &self,
        id: &'static str,
        glyph: Icon,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let hover = palette.elevated;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(handler)
            .child(icon(glyph, GLYPH_TOOLBAR, palette.text_secondary))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(label.into()),
            )
            .into_any_element()
    }

    fn disabled_toolbar_button(
        &self,
        glyph: Icon,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .child(icon(glyph, GLYPH_TOOLBAR, palette.text_faint))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .into_any_element()
    }
}
