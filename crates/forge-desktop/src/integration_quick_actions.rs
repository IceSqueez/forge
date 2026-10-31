mod commands;

pub(crate) use self::commands::{enqueue_builtin_step, reachable_quick_actions};
use forge_components::FONT_LABEL;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    KeyActivated, Radius, SearchState, Spacing, body_family, icon, mono_family, pulse_dot, radius,
    scaled, spacing, tooltip_builder, tr, with_alpha,
};
use forge_platform_core::{
    BuiltinCollections, BuiltinOptionSource, QuickAction, QuickActionAccent, QuickActionGroup,
    QuickActionLiveness, QuickActions,
};
use forge_registry::TriggerRegistry;
use forge_runtime::ActionEngineHandle;
use forge_storage::HistoryRepo;
use forge_types::IntegrationId;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, Stateful, Subscription,
    Window, div, prelude::*, px,
};
use std::sync::Arc;

use crate::collection_manager::CollectionManager;
use crate::integration_quick_action_modal::QuickActionModal;
use crate::navigation::Screen;
use crate::run_history_modal::RunHistoryModal;
use crate::user_text::user_text_string;

const GRID_COLUMNS: u16 = 3;
const LIVE_DOT: Pixels = px(5.0);
const SEARCH_FONT: Pixels = px(11.5);
const SEARCH_W: Pixels = px(240.0);
const CARD_GAP: Pixels = px(12.0);
const CARD_PAD_X: Pixels = px(14.0);
const TITLE_GAP: Pixels = px(7.0);
const GRID_GAP: Pixels = px(7.0);
const BUTTON_GAP: Pixels = px(7.0);
const BUTTON_PAD_Y: Pixels = px(7.0);
const BUTTON_LINE_H: Pixels = px(14.0);
const OUTLINE_BUTTON_GAP: Pixels = px(5.0);
const OUTLINE_BUTTON_PAD_Y: Pixels = px(5.0);
const OUTLINE_BUTTON_PAD_X: Pixels = px(11.0);

pub(crate) fn outline_button(
    id: impl Into<gpui::ElementId>,
    palette: &ForgePalette,
    density: Density,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(scaled(OUTLINE_BUTTON_GAP, density))
        .py(scaled(OUTLINE_BUTTON_PAD_Y, density))
        .px(scaled(OUTLINE_BUTTON_PAD_X, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
}

#[derive(Clone)]
pub(crate) struct QuickActionServices {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) action_engine: ActionEngineHandle,
    pub(crate) history: Arc<dyn HistoryRepo>,
    pub(crate) trigger_registry: Arc<TriggerRegistry>,
}

pub(crate) struct QuickActionSource {
    pub(crate) id: IntegrationId,
    pub(crate) quick: Arc<dyn QuickActions>,
    pub(crate) collections: Option<Arc<dyn BuiltinCollections>>,
    pub(crate) option_source: Option<Arc<dyn BuiltinOptionSource>>,
}

pub(crate) enum QuickActionsPanelEvent {
    Ran,
    Navigate(Screen),
}

#[derive(Default)]
struct PanelModals {
    quick_action: Option<Entity<QuickActionModal>>,
    collection_manager: Option<Entity<CollectionManager>>,
    history: Option<Entity<RunHistoryModal>>,
    _quick_action_sub: Option<Subscription>,
    _collection_manager_sub: Option<Subscription>,
    _history_subs: Vec<Subscription>,
}

pub(crate) struct QuickActionsPanel {
    services: QuickActionServices,
    source: QuickActionSource,
    display_name: String,
    pub(crate) actions: Vec<QuickAction>,
    search: SearchState,
    modals: PanelModals,
    _search_sub: Subscription,
}

impl EventEmitter<QuickActionsPanelEvent> for QuickActionsPanel {}

impl QuickActionsPanel {
    pub(crate) fn new(
        services: QuickActionServices,
        source: QuickActionSource,
        display_name: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::from_field(cx.new(|cx| {
            forge_components::search_input(tr!("integration_qa_filter_placeholder"), palette, cx)
                .compact()
                .with_font_size(SEARCH_FONT)
        }));
        let search_sub = cx.subscribe(search.field(), Self::on_search);
        let actions = reachable_quick_actions(&*source.quick, source.collections.as_deref());
        Self {
            services,
            source,
            display_name,
            actions,
            search,
            modals: PanelModals::default(),
            _search_sub: search_sub,
        }
    }

    pub(crate) fn modal_layers(&self) -> Vec<AnyElement> {
        let quick = self.modals.quick_action.clone();
        let manager = self.modals.collection_manager.clone();
        quick
            .map(IntoElement::into_any_element)
            .into_iter()
            .chain(manager.map(IntoElement::into_any_element))
            .collect()
    }

    pub(crate) fn history_layer(&self) -> Option<AnyElement> {
        self.modals
            .history
            .clone()
            .map(IntoElement::into_any_element)
    }
}

impl Render for QuickActionsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        self.quick_actions_card(&palette, density, cx)
    }
}

impl QuickActionsPanel {
    fn quick_actions_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(CARD_GAP, density))
            .py(spacing(Spacing::Sm, density))
            .px(scaled(CARD_PAD_X, density))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(scaled(TITLE_GAP, density))
                    .child(icon(Icon::Bolt, FONT_SM, palette.warning))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("widget_quick_actions_title")),
                    ),
            )
            .child(div().flex_1().min_w(px(0.0)))
            .child(
                div()
                    .flex_none()
                    .w(SEARCH_W)
                    .child(self.search.field().clone()),
            )
            .child(self.history_button(palette, cx));

        let divider = div().w_full().h(BORDER_THIN).bg(palette.border_regular);

        let matches: Vec<(usize, &QuickAction)> = self
            .actions
            .iter()
            .enumerate()
            .filter(|(_, a)| self.search.matches(&user_text_string(&a.label)))
            .collect();

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(CARD_GAP, density))
            .pt(spacing(Spacing::Xxs, density))
            .pb(scaled(CARD_GAP, density))
            .px(scaled(CARD_PAD_X, density));

        if matches.is_empty() {
            body = body.child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Lg, density))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("integration_qa_no_matches")),
                    ),
            );
        } else {
            for group in group_order(&matches) {
                body = body.child(self.qa_group(group, &matches, palette, density, cx));
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(header)
            .child(divider)
            .child(body)
            .into_any_element()
    }

    fn history_button(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let text_color = palette.text_secondary;
        let hover_border = palette.border_input;
        outline_button("integration-run-history", palette, density)
            .flex_none()
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_run_history(cx)))
            .child(icon(Icon::History, FONT_XS, text_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text_color)
                    .child(tr!("integration_run_history")),
            )
            .into_any_element()
    }

    fn qa_group(
        &self,
        group: Option<&QuickActionGroup>,
        matches: &[(usize, &QuickAction)],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let members: Vec<(usize, &QuickAction)> = matches
            .iter()
            .filter(|(_, a)| a.group.as_ref() == group)
            .map(|(i, a)| (*i, *a))
            .collect();

        let mut section = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(GRID_GAP, density));
        if let Some(group) = group {
            let mut label_row = div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density));
            label_row = label_row.child(icon(
                Icon::from_name(group.icon.as_str()),
                FONT_XXS,
                accent_color(group.accent, palette),
            ));
            label_row = label_row.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(user_text_string(&group.label).to_uppercase()),
            );
            section = section.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(label_row)
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(members.len().to_string()),
                    ),
            );
        }

        let mut grid = div()
            .w_full()
            .grid()
            .grid_cols(GRID_COLUMNS)
            .gap(spacing(Spacing::Xs, density));
        for (idx, action) in &members {
            grid = grid.child(self.qa_button(*idx, action, palette, density, cx));
        }

        section.child(grid).into_any_element()
    }

    fn qa_button(
        &self,
        idx: usize,
        action: &QuickAction,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locked = action.locked_reason.as_ref().map(user_text_string);
        let disabled = !action.is_runnable();
        let destructive = action.destructive;
        let live = action.liveness == QuickActionLiveness::Live;

        let (icon_color, label_color, border_color) = if disabled {
            (
                with_alpha(palette.text_faint, 0.6),
                with_alpha(palette.text_faint, 0.6),
                with_alpha(palette.border_regular, 0.6),
            )
        } else if destructive {
            (
                palette.danger,
                palette.danger,
                with_alpha(palette.danger, 0.4),
            )
        } else {
            (
                accent_color(action.accent, palette),
                palette.text_primary,
                palette.border_regular,
            )
        };

        let mut content = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(BUTTON_GAP, density))
            .child(icon(
                Icon::from_name(action.icon.as_str()),
                FONT_XS,
                icon_color,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_LABEL)
                    .line_height(BUTTON_LINE_H)
                    .text_color(label_color)
                    .child(user_text_string(&action.label)),
            );
        if locked.is_some() {
            content = content.child(icon(Icon::Lock, FONT_XXS, icon_color));
        }
        if live {
            content = content.child(pulse_dot(
                ("quick-action-live", idx),
                palette.danger,
                LIVE_DOT,
            ));
        }

        let mut btn = div()
            .id(("quick-action", idx))
            .min_w(px(0.0))
            .py(scaled(BUTTON_PAD_Y, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.shell)
            .child(content);

        if disabled {
            let reason = locked.unwrap_or_else(|| {
                if action.liveness == QuickActionLiveness::Absent {
                    tr!("integration_quick_action_not_live")
                } else {
                    tr!("integration_quick_action_na")
                }
            });
            btn = btn.tooltip(tooltip_builder(reason, palette));
        } else {
            let hover_border = if destructive {
                palette.danger
            } else {
                palette.border_input
            };
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.border_color(hover_border))
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.on_quick_action(idx, window, cx)
                }));
        }
        btn.into_any_element()
    }
}

fn group_order<'a>(matches: &[(usize, &'a QuickAction)]) -> Vec<Option<&'a QuickActionGroup>> {
    let mut order: Vec<Option<&QuickActionGroup>> = Vec::new();
    for (_, action) in matches {
        let key = action.group.as_ref();
        if key.is_some() && !order.contains(&key) {
            order.push(key);
        }
    }
    if matches.iter().any(|(_, a)| a.group.is_none()) {
        order.push(None);
    }
    order
}

pub(crate) fn accent_color(accent: QuickActionAccent, palette: &ForgePalette) -> Rgba {
    match accent {
        QuickActionAccent::Brand => palette.brand,
        QuickActionAccent::Success => palette.success,
        QuickActionAccent::Warning => palette.warning,
        QuickActionAccent::Info => palette.info,
        QuickActionAccent::Bits => palette.accent_peach,
        QuickActionAccent::AccentPinkLight => palette.accent_pink_light,
        QuickActionAccent::Danger => palette.danger,
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{QuickActionGroup, QuickActionLiveness, SectionIcon};
    use forge_types::SubActionStep;

    use super::*;

    fn group(name: &str) -> QuickActionGroup {
        QuickActionGroup {
            label: forge_types::VerbatimText::new(name).into(),
            icon: SectionIcon::new("dot"),
            accent: QuickActionAccent::Brand,
        }
    }

    fn action(group: Option<QuickActionGroup>) -> QuickAction {
        QuickAction {
            label: forge_types::VerbatimText::new("action").into(),
            icon: SectionIcon::new("dot"),
            enabled: true,
            locked_reason: None,
            liveness: QuickActionLiveness::Unknown,
            group,
            destructive: false,
            accent: QuickActionAccent::Brand,
            subaction_template: SubActionStep {
                kind_id: "test.kind".to_owned(),
                config: Default::default(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
            fields: Vec::new(),
            collection: None,
        }
    }

    #[test]
    fn actions_of_one_group_fold_into_one_section_in_first_seen_order_with_ungrouped_last() {
        let actions = [
            action(None),
            action(Some(group("Polls"))),
            action(Some(group("Chat"))),
            action(Some(group("Polls"))),
            action(None),
            action(Some(group("Chat"))),
        ];
        let matches: Vec<(usize, &QuickAction)> = actions.iter().enumerate().collect();

        assert_eq!(
            group_order(&matches),
            vec![Some(&group("Polls")), Some(&group("Chat")), None]
        );
    }

    #[test]
    fn a_list_without_groups_has_a_single_ungrouped_section() {
        let actions = [action(None), action(None)];
        let matches: Vec<(usize, &QuickAction)> = actions.iter().enumerate().collect();

        assert_eq!(group_order(&matches), vec![None]);
    }
}
