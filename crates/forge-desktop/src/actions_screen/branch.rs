use super::*;
use crate::presentation::ActivePresentation;
use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XS, ForgePalette, Icon, InputEvent,
    Spacing, TextInput, icon, spacing, tr,
};
use forge_registry::FormField;
use forge_types::SubActionStep;
use gpui::{AnyElement, App, ClickEvent, Context, ElementId, SharedString, Window, div};
use std::collections::BTreeMap;

fn branch_field_label(chain_key: &str) -> String {
    match chain_key {
        "then_chain" => tr!("action_editor_branch_then"),
        "else_chain" => tr!("action_editor_branch_else"),
        "body" => tr!("action_editor_branch_body"),
        "default_chain" => tr!("action_editor_branch_default"),
        _ => tr!("action_editor_branch_fallback"),
    }
}

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
    fn enter_branch(
        &mut self,
        step_index: usize,
        chain_key: String,
        case_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let count = self
            .current_chain()
            .get(step_index)
            .map(|s| nav::branch_step_count(s, &chain_key, case_index))
            .unwrap_or(0);
        if self.nav_path.len() >= nav::UI_MAX_NESTING_DEPTH && count == 0 {
            return;
        }
        self.nav_path.push(nav::NavFrame {
            step_index,
            chain_key,
            case_index,
        });
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn breadcrumb_pop(&mut self, depth: usize, cx: &mut Context<Self>) {
        self.nav_path.truncate(depth);
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn add_switch_case(&mut self, step_index: usize, cx: &mut Context<Self>) {
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    nav::append_empty_case(&mut step.config);
                }
            },
            cx,
        );
    }

    fn remove_switch_case(&mut self, step_index: usize, case_index: usize, cx: &mut Context<Self>) {
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    nav::remove_case(&mut step.config, case_index);
                }
            },
            cx,
        );
    }

    fn move_switch_case(
        &mut self,
        step_index: usize,
        case_index: usize,
        up: bool,
        cx: &mut Context<Self>,
    ) {
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    nav::move_case(&mut step.config, case_index, up);
                }
            },
            cx,
        );
    }

    fn commit_case_match(
        &mut self,
        step_index: usize,
        case_index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.persist_chain_mutation(
            move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    nav::set_case_match(&mut step.config, case_index, value.trim());
                }
            },
            cx,
        );
    }

    pub(super) fn sync_case_fields(&mut self, cx: &mut Context<Self>) {
        let chain = self.current_chain();
        let mut specs: Vec<(usize, usize, String)> = Vec::new();
        for (si, step) in chain.iter().enumerate() {
            for ci in 0..nav::case_count(step) {
                if !nav::case_match_is_multi(step, ci) {
                    specs.push((
                        si,
                        ci,
                        nav::case_match_display(step, ci).unwrap_or_default(),
                    ));
                }
            }
        }

        let palette = cx.palette();
        let mut fields = BTreeMap::new();
        for (si, ci, seed) in specs {
            let field = cx.new(|cx| {
                let mut input = TextInput::new(tr!("action_editor_case_match_placeholder"), cx)
                    .with_palette(palette);
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

    pub(super) fn render_breadcrumb(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut segments: Vec<(String, Option<usize>)> =
            vec![(tr!("action_editor_breadcrumb_steps"), Some(0))];
        for (depth, frame) in self.nav_path.iter().enumerate() {
            let prefix = nav::resolve_chain(&detail.action.sub_actions, &self.nav_path[..depth]);
            let step_label = prefix
                .get(frame.step_index)
                .and_then(|s| {
                    self.sub_action_registry
                        .get(&s.kind_id)
                        .map(|r| r.label().to_owned())
                })
                .unwrap_or_else(|| tr!("action_editor_kind_sub_action"));
            let branch_label = match frame.case_index {
                Some(ci) => format!("{} {}", tr!("action_editor_branch_case"), ci + 1),
                None => branch_field_label(&frame.chain_key),
            };
            let pop_target = if depth + 1 == self.nav_path.len() {
                None
            } else {
                Some(depth + 1)
            };
            segments.push((format!("{step_label} \u{2023} {branch_label}"), pop_target));
        }

        let last = segments.len().saturating_sub(1);
        let text_primary = palette.text_primary;
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
                            .hover(move |s| s.text_color(text_primary).underline())
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

        let back_depth = self.nav_path.len() - 1;
        let back_button = step_icon_btn(
            SharedString::from("actions-breadcrumb-back"),
            Icon::ChevronLeft,
            false,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.breadcrumb_pop(back_depth, cx)),
        );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(back_button)
            .child(row)
            .into_any_element()
    }

    pub(super) fn render_branch_affordances(
        &self,
        step: &SubActionStep,
        step_index: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let fields = self.sub_action_registry.get(&step.kind_id)?.config_fields();
        let at_cap = depth >= nav::UI_MAX_NESTING_DEPTH;
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut capped_empty = false;

        for field in &fields {
            match field {
                FormField::SubChain { key, label } => {
                    let count = nav::branch_step_count(step, key, None);
                    let disabled = at_cap && count == 0;
                    capped_empty |= disabled;
                    let key_owned = (*key).to_owned();
                    rows.push(drill_in_chip(
                        SharedString::from(format!("actions-drill-{step_index}-{key}")),
                        label,
                        count,
                        disabled,
                        palette,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.enter_branch(step_index, key_owned.clone(), None, cx)
                        }),
                    ));
                }
                FormField::CaseList { key, label } => {
                    let case_total = nav::case_count(step);
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
                        capped_empty |= (0..case_total)
                            .any(|ci| nav::branch_step_count(step, key, Some(ci)) == 0);
                    }
                }
                _ => {}
            }
        }

        if rows.is_empty() {
            return None;
        }

        if capped_empty {
            rows.push(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(tr!("action_editor_branch_cap"))
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
        step: &SubActionStep,
        step_index: usize,
        ci: usize,
        key: &str,
        case_total: usize,
        at_cap: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = nav::branch_step_count(step, key, Some(ci));
        let disabled = at_cap && count == 0;

        let match_el: AnyElement = if nav::case_match_is_multi(step, ci) {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(tr!("action_editor_case_multi"))
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

        let key_owned = key.to_owned();
        let drill = drill_in_chip(
            SharedString::from(format!("actions-drill-{step_index}-case-{ci}")),
            &tr!("action_editor_branch_chain"),
            count,
            disabled,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.enter_branch(step_index, key_owned.clone(), Some(ci), cx)
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
                    .child(tr!("action_editor_add_case")),
            )
            .into_any_element()
    }
}
