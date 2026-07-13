//! Actions screen — composite/branch drill-in: nav-path descent, the drilled-in
//! breadcrumb, branch affordances and switch-case editing.

use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XS, ForgePalette, Icon, InputEvent,
    Spacing, TextInput, icon, spacing,
};
use gpui::{AnyElement, App, ClickEvent, Context, ElementId, SharedString, Window, div};
use std::collections::BTreeMap;

/// Step count in the branch a drill-in frame would enter, used to gate descending
/// past the depth cap into an empty branch.
fn branch_count(step: &EditorStep, chain_key: &str, case_index: Option<usize>) -> usize {
    match case_index {
        None => step
            .branches
            .iter()
            .find(|b| b.key == chain_key)
            .map(|b| b.steps.len())
            .unwrap_or(0),
        Some(ci) => step
            .cases
            .as_ref()
            .and_then(|cases| cases.get(ci))
            .map(|c| c.chain.len())
            .unwrap_or(0),
    }
}

/// Human label for a single-sub-chain branch key in the breadcrumb.
fn branch_field_label(chain_key: &str) -> &'static str {
    match chain_key {
        "then_chain" => "Then",
        "else_chain" => "Else",
        "body" => "Body",
        "default_chain" => "Default",
        _ => "Branch",
    }
}

/// A drill-in chip entering a nested sub-chain: a "label · count" caption + a chevron,
/// framed by a 0.5px hairline with a 6px corner, washing `surface_overlay` on hover.
/// Disabled (past the depth cap on an empty branch) it inks `disabled` and takes no
/// click.
fn drill_in_chip(
    id: impl Into<ElementId>,
    label: &str,
    count: usize,
    disabled: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let color = if disabled {
        palette.disabled
    } else {
        palette.brand
    };
    let base = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(CHIP_RADIUS)
        .border(HALF_BORDER)
        .border_color(palette.border_regular)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(color)
                .child(format!("{label} \u{00b7} {count}")),
        )
        .child(icon(Icon::ChevronRight, BRANCH_GLYPH, color));
    if disabled {
        return base.into_any_element();
    }
    let hover = palette.surface_overlay;
    base.id(id.into())
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(handler)
        .into_any_element()
}

impl ScreenActionsView {
    // --- editor: branch drill-in + switch cases ---------------------------

    /// Descends into a composite step's nested sub-chain or a switch case, pushing a
    /// nav frame. Refuses to create new depth past the authoring cap on an empty
    /// branch — mirrors the disabled drill-in chip so a stale click is inert.
    fn enter_branch(
        &mut self,
        step_index: usize,
        chain_key: &'static str,
        case_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let count = self
            .current_chain()
            .get(step_index)
            .map(|s| branch_count(s, chain_key, case_index))
            .unwrap_or(0);
        if self.nav_path.len() >= UI_MAX_NESTING_DEPTH && count == 0 {
            return;
        }
        self.nav_path.push(NavFrame {
            step_index,
            chain_key,
            case_index,
        });
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    /// Pops the nav path back to `depth` (a breadcrumb ancestor segment).
    fn breadcrumb_pop(&mut self, depth: usize, cx: &mut Context<Self>) {
        self.nav_path.truncate(depth);
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn add_switch_case(&mut self, step_index: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(cases) = chain.get_mut(step_index).and_then(|s| s.cases.as_mut())
        {
            cases.push(SwitchCase {
                match_value: CaseMatch::Single(String::new()),
                chain: Vec::new(),
            });
        }
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn remove_switch_case(&mut self, step_index: usize, case_index: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(cases) = chain.get_mut(step_index).and_then(|s| s.cases.as_mut())
            && case_index < cases.len()
        {
            cases.remove(case_index);
        }
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn move_switch_case(
        &mut self,
        step_index: usize,
        case_index: usize,
        up: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(cases) = chain.get_mut(step_index).and_then(|s| s.cases.as_mut())
        {
            let target = if up {
                case_index.checked_sub(1)
            } else {
                case_index.checked_add(1).filter(|&t| t < cases.len())
            };
            if let Some(t) = target
                && case_index < cases.len()
            {
                cases.swap(case_index, t);
            }
        }
        self.sync_case_fields(cx);
        cx.notify();
    }

    /// Writes a switch case's single-value match back into the model. Multi-value
    /// imported matches carry no input, so they are never reached here.
    fn commit_case_match(
        &mut self,
        step_index: usize,
        case_index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(case) = chain
                .get_mut(step_index)
                .and_then(|s| s.cases.as_mut())
                .and_then(|cases| cases.get_mut(case_index))
            && let CaseMatch::Single(m) = &mut case.match_value
        {
            *m = value.trim().to_owned();
        }
        cx.notify();
    }

    /// Rebuilds the per-case match inputs for every switch step in the current chain.
    /// Called at each edge that reshapes the current chain (nav change, step reorder,
    /// case add/remove/move) so the `(step_index, case_index)` keys stay accurate.
    pub(super) fn sync_case_fields(&mut self, cx: &mut Context<Self>) {
        let specs: Vec<(usize, usize, String)> = {
            let chain = self.current_chain();
            let mut specs = Vec::new();
            for (si, step) in chain.iter().enumerate() {
                if let Some(cases) = &step.cases {
                    for (ci, case) in cases.iter().enumerate() {
                        if let CaseMatch::Single(m) = &case.match_value {
                            specs.push((si, ci, m.clone()));
                        }
                    }
                }
            }
            specs
        };

        let palette = cx.palette();
        let mut fields = BTreeMap::new();
        for (si, ci, seed) in specs {
            let field = cx.new(|cx| {
                let mut input = TextInput::new("match value", cx).with_palette(palette);
                if !seed.is_empty() {
                    input.set_content(seed, cx);
                }
                input
            });
            let sub = cx.subscribe(&field, move |this, _f, event: &InputEvent, cx| {
                if let InputEvent::Submitted(text) = event {
                    this.commit_case_match(si, ci, text.to_string(), cx);
                }
            });
            fields.insert((si, ci), CaseField { field, _sub: sub });
        }
        self.case_fields = fields;
    }

    /// The breadcrumb that replaces the step-count header while drilled in. Every
    /// ancestor segment pops the nav path to its depth; the current (final) segment
    /// is inert.
    pub(super) fn render_breadcrumb(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // `(label, pop_target)` — a `Some(depth)` target makes the segment a
        // pop-to-that-depth button; `None` is the inert current segment.
        let mut segments: Vec<(String, Option<usize>)> = vec![("Steps".to_owned(), Some(0))];
        for (depth, frame) in self.nav_path.iter().enumerate() {
            let prefix = resolve_chain(&detail.steps, &self.nav_path[..depth]).unwrap_or(&[]);
            let step_label = prefix
                .get(frame.step_index)
                .map(|s| s.kind.label().to_owned())
                .unwrap_or_else(|| "Sub-action".to_owned());
            let branch_label = match frame.case_index {
                Some(ci) => format!("Case {}", ci + 1),
                None => branch_field_label(frame.chain_key).to_owned(),
            };
            let pop_target = if depth + 1 == self.nav_path.len() {
                None
            } else {
                Some(depth + 1)
            };
            segments.push((format!("{step_label} \u{2023} {branch_label}"), pop_target));
        }

        let last = segments.len().saturating_sub(1);
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        for (idx, (label, target)) in segments.into_iter().enumerate() {
            if idx > 0 {
                row = row.child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child("\u{25B8}"),
                );
            }
            match target {
                Some(depth) => {
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("actions-breadcrumb-{depth}")))
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.breadcrumb_pop(depth, cx)
                            }))
                            .child(label),
                    );
                }
                None => {
                    let color = if idx == last {
                        palette.text_secondary
                    } else {
                        palette.text_muted
                    };
                    row = row.child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(color)
                            .child(label),
                    );
                }
            }
        }
        row.into_any_element()
    }

    /// The drill-in affordances under a composite / switch step: one chip per single
    /// sub-chain (then / else / body / default) and, for a switch, a full per-case row
    /// editor. `None` when the step declares no nested chains.
    pub(super) fn render_branch_affordances(
        &self,
        step: &EditorStep,
        step_index: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let specs = step.kind.branch_specs();
        if specs.is_empty() {
            return None;
        }
        let at_cap = depth >= UI_MAX_NESTING_DEPTH;
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut capped_empty = false;

        for spec in specs {
            match spec {
                BranchSpec::Chain { key, label } => {
                    let count = branch_count(step, key, None);
                    let disabled = at_cap && count == 0;
                    capped_empty |= disabled;
                    let key = *key;
                    rows.push(drill_in_chip(
                        SharedString::from(format!("actions-drill-{step_index}-{key}")),
                        label,
                        count,
                        disabled,
                        palette,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.enter_branch(step_index, key, None, cx)
                        }),
                    ));
                }
                BranchSpec::Cases { key, label } => {
                    let key = *key;
                    let case_total = step.cases.as_ref().map(Vec::len).unwrap_or(0);
                    rows.push(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(format!("{label}:"))
                            .into_any_element(),
                    );
                    for ci in 0..case_total {
                        rows.push(self.render_case_row(
                            step, step_index, ci, key, case_total, at_cap, palette, cx,
                        ));
                    }
                    rows.push(self.render_add_case(step_index, palette, cx));
                    if at_cap {
                        capped_empty |=
                            (0..case_total).any(|ci| branch_count(step, key, Some(ci)) == 0);
                    }
                }
            }
        }

        if capped_empty {
            rows.push(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child("Max nesting depth reached · cannot nest deeper here")
                    .into_any_element(),
            );
        }

        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .children(rows)
                .into_any_element(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_case_row(
        &self,
        step: &EditorStep,
        step_index: usize,
        ci: usize,
        key: &'static str,
        case_total: usize,
        at_cap: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = branch_count(step, key, Some(ci));
        let disabled = at_cap && count == 0;

        let is_multi = step
            .cases
            .as_ref()
            .and_then(|cases| cases.get(ci))
            .map(|c| matches!(c.match_value, CaseMatch::Multi))
            .unwrap_or(false);
        let match_el: AnyElement = if is_multi {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("multi-value match (read-only)")
                .into_any_element()
        } else {
            div()
                .w(CASE_MATCH_W)
                .flex_none()
                .children(
                    self.case_fields
                        .get(&(step_index, ci))
                        .map(|f| f.field.clone()),
                )
                .into_any_element()
        };

        let drill = drill_in_chip(
            SharedString::from(format!("actions-drill-{step_index}-case-{ci}")),
            "Chain",
            count,
            disabled,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.enter_branch(step_index, key, Some(ci), cx)
            }),
        );
        let move_up = step_icon_btn(
            SharedString::from(format!("actions-case-up-{step_index}-{ci}")),
            Icon::ArrowUp,
            ci == 0,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.move_switch_case(step_index, ci, true, cx)
            }),
        );
        let move_down = step_icon_btn(
            SharedString::from(format!("actions-case-down-{step_index}-{ci}")),
            Icon::ArrowDown,
            ci + 1 >= case_total,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.move_switch_case(step_index, ci, false, cx)
            }),
        );
        let remove = step_icon_btn(
            SharedString::from(format!("actions-case-del-{step_index}-{ci}")),
            Icon::Eraser,
            false,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.remove_switch_case(step_index, ci, cx)
            }),
        );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(match_el)
            .child(drill)
            .child(move_up)
            .child(move_down)
            .child(remove)
            .into_any_element()
    }

    fn render_add_case(
        &self,
        step_index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("actions-add-case-{step_index}")))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.add_switch_case(step_index, cx)
                }),
            )
            .child(icon(Icon::Plus, BRANCH_GLYPH, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child("Add case"),
            )
            .into_any_element()
    }
}
