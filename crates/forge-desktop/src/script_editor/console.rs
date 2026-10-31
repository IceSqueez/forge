use super::details_pane::muted_line;
use super::diagnostics::ProblemLevel;
use super::{GLYPH_ACTION, ScriptEditorView};
use crate::async_bridge::{BridgeFlow, drain_events};
use crate::runtime_event;
use crate::runtime_event::RuntimeEvent;
use forge_components::{
    BORDER_THIN, DateShape, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ResizeEdge,
    ResizeRange, Spacing, badge, body_family, fmt_display, icon, install_resize, mono_family,
    scaled, spacing, tr,
};
use forge_events::{ScriptLogLevel, ScriptLogged};
use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Pixels, Rgba, SharedString, div, px, uniform_list,
};
use std::sync::Arc;
use time::OffsetDateTime;

const GLYPH_TAB: Pixels = px(12.0);
const CONSOLE_LOG_CAP: usize = 1000;
const CONSOLE_MAX_H: Pixels = px(400.0);
const CONSOLE_MIN_H: Pixels = px(80.0);

struct ConsoleResizeDrag;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConsoleTab {
    Output,
    Problems,
}

#[derive(Clone, Copy)]
pub(super) enum LogTag {
    Run,
    Ok,
    Stats,
    Log,
    Warn,
    Err,
}

impl LogTag {
    pub(super) fn label(self) -> &'static str {
        match self {
            LogTag::Run => "[run]",
            LogTag::Ok => "[ok]",
            LogTag::Stats => "[stats]",
            LogTag::Log => "[log]",
            LogTag::Warn => "[warn]",
            LogTag::Err => "[error]",
        }
    }

    pub(super) fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            LogTag::Run => palette.info,
            LogTag::Ok => palette.success,
            LogTag::Stats => palette.theme_brand,
            LogTag::Log => palette.text_secondary,
            LogTag::Warn => palette.warning,
            LogTag::Err => palette.danger,
        }
    }
}

pub(super) struct ConsoleLine {
    at: OffsetDateTime,
    pub(super) tag: LogTag,
    pub(super) text: SharedString,
}

impl ScriptEditorView {
    pub(super) fn start_log_bridge(&self, cx: &mut Context<Self>) {
        let bus = Arc::clone(&self.bus);
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                match this.update(cx, |this, cx| {
                    for event in batch {
                        if let Some(RuntimeEvent::ScriptLogged(logged)) =
                            runtime_event::decode(event)
                        {
                            this.on_script_log(logged, cx);
                        }
                    }
                }) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        })
        .detach();
    }

    fn on_script_log(&mut self, logged: ScriptLogged, cx: &mut Context<Self>) {
        let Some(open_id) = self.open.as_ref().map(|o| o.id) else {
            return;
        };
        if logged.script_id != Some(open_id) {
            return;
        }
        let tag = match logged.level {
            ScriptLogLevel::Error => LogTag::Err,
            ScriptLogLevel::Warn => LogTag::Warn,
            ScriptLogLevel::Info => LogTag::Log,
        };
        self.push_console(tag, logged.message);
        cx.notify();
    }

    pub(super) fn push_console(&mut self, tag: LogTag, text: impl Into<SharedString>) {
        if self.console.len() >= CONSOLE_LOG_CAP {
            self.console.pop_front();
        }
        self.console.push_back(ConsoleLine {
            at: OffsetDateTime::now_utc(),
            tag,
            text: text.into(),
        });
    }

    fn set_console_tab(&mut self, tab: ConsoleTab, cx: &mut Context<Self>) {
        self.console_tab = tab;
        self.console_collapsed = false;
        cx.notify();
    }

    fn clear_console(&mut self, cx: &mut Context<Self>) {
        self.console.clear();
        cx.notify();
    }

    fn toggle_console(&mut self, cx: &mut Context<Self>) {
        self.console_collapsed = !self.console_collapsed;
        cx.notify();
    }

    fn set_console_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        let clamped = height.clamp(CONSOLE_MIN_H, CONSOLE_MAX_H);
        if self.console_height != clamped {
            self.console_height = clamped;
            cx.notify();
        }
    }

    pub(super) fn console(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.console_header(palette, density, cx);

        let console = div()
            .w_full()
            .flex()
            .flex_col()
            .flex_none()
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(header);

        if self.console_collapsed {
            return console.into_any_element();
        }

        let console = console.child(self.console_body(palette, density, cx));

        install_resize(
            console,
            ConsoleResizeDrag,
            "scripts-console-resize",
            ResizeEdge::Top,
            ResizeRange {
                min: CONSOLE_MIN_H,
                max: CONSOLE_MAX_H,
            },
            palette,
            cx.listener(|this, height: &Pixels, _, cx| this.set_console_height(*height, cx)),
        )
        .into_any_element()
    }

    fn console_header(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let output_active = self.console_tab == ConsoleTab::Output;
        let output = div()
            .id("console-tab-output")
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_console_tab(ConsoleTab::Output, cx)
            }))
            .child(icon(
                Icon::Terminal,
                GLYPH_TAB,
                if output_active {
                    palette.text_primary
                } else {
                    palette.text_muted
                },
            ))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(if output_active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_size(FONT_XS)
                    .text_color(if output_active {
                        palette.text_primary
                    } else {
                        palette.text_muted
                    })
                    .child(tr!("script_editor_output_header")),
            );

        let problems_active = self.console_tab == ConsoleTab::Problems;
        let problems = (!self.problems.is_empty()).then(|| {
            div()
                .id("console-tab-problems")
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_console_tab(ConsoleTab::Problems, cx)
                }))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(if problems_active {
                            palette.text_primary
                        } else {
                            palette.text_muted
                        })
                        .child(tr!("script_editor_problems_tab")),
                )
                .child(badge(
                    palette.surface_overlay,
                    palette.warning,
                    self.problems.len().to_string(),
                    true,
                    FONT_XXS,
                ))
        });

        let tabs = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(output)
            .children(problems);

        let collapse_glyph = if self.console_collapsed {
            Icon::ChevronUp
        } else {
            Icon::X
        };
        let actions = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .id("console-clear")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_console(cx)))
                    .child(icon(Icon::Eraser, GLYPH_ACTION, palette.text_faint)),
            )
            .child(
                div()
                    .id("console-collapse")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_console(cx)))
                    .child(icon(collapse_glyph, GLYPH_ACTION, palette.text_faint)),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(tabs)
            .child(actions)
            .into_any_element()
    }

    fn console_body(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let base = div()
            .id("script-console-scroll")
            .w_full()
            .flex()
            .flex_col()
            .h(self.console_height)
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .font_family(mono_family())
            .text_size(FONT_XXS);

        match self.console_tab {
            ConsoleTab::Output => {
                if self.console.is_empty() {
                    base.overflow_y_scroll()
                        .child(muted_line(tr!("script_editor_console_cleared"), palette))
                } else {
                    let count = self.console.len();
                    let pal = *palette;
                    let rows = uniform_list(
                        "script-console-rows",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _window, _cx| {
                            let mut rows = Vec::with_capacity(range.len());
                            for ix in range {
                                let Some(line) = this.console.get(ix) else {
                                    continue;
                                };
                                rows.push(console_row(line, &pal, density).into_any_element());
                            }
                            rows
                        }),
                    )
                    .track_scroll(&self.console_scroll)
                    .gap(spacing(Spacing::Xxs, density))
                    .flex_1()
                    .min_h(px(0.0));
                    base.child(rows)
                }
            }
            ConsoleTab::Problems => {
                let base = base.overflow_y_scroll().gap(spacing(Spacing::Xxs, density));
                if self.problems.is_empty() {
                    base.child(muted_line(tr!("script_editor_no_problems"), palette))
                } else {
                    let mut b = base;
                    for problem in &self.problems {
                        let (glyph, tint) = match problem.level {
                            ProblemLevel::Error => (Icon::AlertTriangle, palette.warning),
                            ProblemLevel::Info => (Icon::InfoCircle, palette.info),
                        };
                        b = b.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(spacing(Spacing::Xs, density))
                                .child(icon(glyph, GLYPH_ACTION, tint))
                                .child(
                                    div()
                                        .text_color(palette.text_muted)
                                        .child(problem.text.clone()),
                                ),
                        );
                    }
                    b
                }
            }
        }
        .into_any_element()
    }
}

fn console_row(line: &ConsoleLine, palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .text_color(palette.text_muted)
        .child(
            div()
                .text_color(palette.text_faint)
                .child(fmt_display(DateShape::TimeWithSeconds, line.at)),
        )
        .child(
            div()
                .text_color(line.tag.color(palette))
                .child(line.tag.label()),
        )
        .child(div().child(line.text.clone()))
}
