use std::collections::BTreeSet;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent,
    ModalSize, OverlayPosition, Radius, SearchState, Spacing, TextInput, body_family, checkbox,
    failure_note, field_column, field_frame, ghost_button, icon, modal, modal_width, mono_family,
    overlay, primary_button, radius, section_label, spacing, tr,
};
use forge_transfer::{PackInventory, PackMetadata, PackSelection, resolve_pack_closure};
use forge_types::ActionId;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::async_bridge::{self, Generation};
use crate::integration_switch::integration_name;
use crate::pack_services::{PackServices, PackSource};

const ACTION_LIST_H: Pixels = px(320.0);
const COUNT_COLUMNS: usize = 2;

pub enum PackExportModalEvent {
    Confirmed,
    Cancelled,
}

pub struct PackDraft {
    pub inventory: PackInventory,
    pub selection: PackSelection,
    pub metadata: PackMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PackItemCount {
    Actions,
    Triggers,
    Queues,
    Scripts,
    Globals,
    Clips,
    Overlays,
    Media,
}

impl PackItemCount {
    const ALL: [PackItemCount; 8] = [
        PackItemCount::Actions,
        PackItemCount::Triggers,
        PackItemCount::Queues,
        PackItemCount::Scripts,
        PackItemCount::Globals,
        PackItemCount::Clips,
        PackItemCount::Overlays,
        PackItemCount::Media,
    ];

    pub(crate) fn count(self, selection: &PackSelection) -> usize {
        match self {
            Self::Actions => selection.actions.len(),
            Self::Triggers => selection.triggers.len(),
            Self::Queues => selection.queues.len(),
            Self::Scripts => selection.scripts.len(),
            Self::Globals => selection.globals.len(),
            Self::Clips => selection.clips.len(),
            Self::Overlays => selection.overlays.len(),
            Self::Media => selection.media.len(),
        }
    }

    fn label(self) -> String {
        match self {
            Self::Actions => tr!("pack_export_kind_actions"),
            Self::Triggers => tr!("pack_export_kind_triggers"),
            Self::Queues => tr!("pack_export_kind_queues"),
            Self::Scripts => tr!("pack_export_kind_scripts"),
            Self::Globals => tr!("pack_export_kind_globals"),
            Self::Clips => tr!("pack_export_kind_clips"),
            Self::Overlays => tr!("pack_export_kind_overlays"),
            Self::Media => tr!("pack_export_kind_media"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActionChoice {
    pub id: ActionId,
    pub name: String,
}

pub(crate) fn action_choices(inventory: &PackInventory) -> Vec<ActionChoice> {
    let mut choices: Vec<ActionChoice> = inventory
        .actions()
        .map(|action| ActionChoice {
            id: action.id,
            name: action.name.clone(),
        })
        .collect();
    choices.sort_by_key(|choice| choice.name.to_lowercase());
    choices
}

pub(crate) fn ordered_roots(
    choices: &[ActionChoice],
    selected: &BTreeSet<ActionId>,
) -> Vec<ActionId> {
    choices
        .iter()
        .filter(|choice| selected.contains(&choice.id))
        .map(|choice| choice.id)
        .collect()
}

struct Ready {
    source: PackSource,
    choices: Vec<ActionChoice>,
}

enum SourceState {
    Loading,
    Failed(String),
    Ready(Ready),
}

pub struct PackExportModal {
    services: PackServices,
    source: SourceState,
    selected: BTreeSet<ActionId>,
    closure: Option<PackSelection>,
    closure_failure: Option<String>,
    resolving: bool,
    generation: Generation,
    search: SearchState,
    name: Entity<TextInput>,
    author: Entity<TextInput>,
    description: Entity<TextInput>,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<PackExportModalEvent> for PackExportModal {}

impl PackExportModal {
    pub fn new(services: PackServices, preselected: Vec<ActionId>, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("pack_export_search_placeholder"));
        let text_field = |placeholder: String, cx: &mut Context<Self>| {
            cx.new(|cx| {
                TextInput::new(placeholder, cx)
                    .with_palette(palette)
                    .with_font_size(FONT_XS)
                    .plain()
            })
        };
        let name = text_field(tr!("pack_export_name_placeholder"), cx);
        let author = text_field(tr!("pack_export_author_placeholder"), cx);
        let description = text_field(tr!("pack_export_description_placeholder"), cx);
        let subs = vec![
            cx.subscribe(search.field(), Self::on_search_event),
            cx.subscribe(&name, Self::on_field_event),
            cx.subscribe(&author, Self::on_field_event),
            cx.subscribe(&description, Self::on_field_event),
        ];
        let loader = services.clone();
        async_bridge::run_async(
            &services.rt_handle,
            async move { loader.load_source().await },
            |this, result, cx| this.source_loaded(result, cx),
            cx,
        );
        Self {
            services,
            source: SourceState::Loading,
            selected: preselected.into_iter().collect(),
            closure: None,
            closure_failure: None,
            resolving: false,
            generation: Generation::default(),
            search,
            name,
            author,
            description,
            focus_pending: true,
            _subs: subs,
        }
    }

    fn source_loaded(&mut self, result: Result<PackSource, String>, cx: &mut Context<Self>) {
        self.source = match result {
            Ok(source) => {
                let choices = action_choices(&source.inventory);
                self.selected
                    .retain(|id| choices.iter().any(|choice| choice.id == *id));
                SourceState::Ready(Ready { source, choices })
            }
            Err(message) => {
                tracing::warn!(error = %message, "pack inventory failed to load");
                SourceState::Failed(message)
            }
        };
        self.resolve(cx);
        cx.notify();
    }

    pub(crate) fn toggle(&mut self, id: ActionId, cx: &mut Context<Self>) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
        self.resolve(cx);
        cx.notify();
    }

    fn resolve(&mut self, cx: &mut Context<Self>) {
        let ticket = self.generation.next();
        self.closure_failure = None;
        let SourceState::Ready(ready) = &self.source else {
            self.closure = None;
            self.resolving = false;
            return;
        };
        let roots = ordered_roots(&ready.choices, &self.selected);
        if roots.is_empty() {
            self.closure = None;
            self.resolving = false;
            return;
        }
        self.resolving = true;
        let services = self.services.clone();
        let source = ready.source.clone();
        async_bridge::run_blocking(
            &self.services.rt_handle,
            move || {
                resolve_pack_closure(&source.inventory, services.registries(&source), &roots)
                    .map_err(|error| error.to_string())
            },
            move |this, result, cx| this.resolved(ticket, result, cx),
            cx,
        );
    }

    fn resolved(
        &mut self,
        ticket: u64,
        result: Result<PackSelection, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.generation.is_current(ticket) {
            return;
        }
        self.resolving = false;
        match result {
            Ok(selection) => self.closure = Some(selection),
            Err(message) => {
                self.closure = None;
                self.closure_failure = Some(message);
            }
        }
        cx.notify();
    }

    fn pack_name(&self, cx: &Context<Self>) -> String {
        self.name.read(cx).content().trim().to_owned()
    }

    pub(crate) fn can_export(&self, cx: &Context<Self>) -> bool {
        !self.resolving && self.closure.is_some() && !self.pack_name(cx).is_empty()
    }

    pub fn take_draft(&mut self, cx: &mut Context<Self>) -> Option<PackDraft> {
        if !self.can_export(cx) {
            return None;
        }
        let SourceState::Ready(ready) = &self.source else {
            return None;
        };
        let selection = self.closure.clone()?;
        Some(PackDraft {
            inventory: PackInventory::clone(&ready.source.inventory),
            selection,
            metadata: PackMetadata {
                name: self.pack_name(cx),
                author: self.author.read(cx).content().trim().to_owned(),
                description: self.description.read(cx).content().trim().to_owned(),
            },
        })
    }

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Cancelled) {
            self.cancel(cx);
            return;
        }
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn on_field_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Submitted(_) => self.confirm(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.can_export(cx) {
            cx.emit(PackExportModalEvent::Confirmed);
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(PackExportModalEvent::Cancelled);
    }

    fn render_actions(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = match &self.source {
            SourceState::Loading => list_message(tr!("pack_export_loading"), palette, density),
            SourceState::Failed(message) => list_message(
                tr!("pack_export_load_failed", error = message.as_str()),
                palette,
                density,
            ),
            SourceState::Ready(ready) if ready.choices.is_empty() => {
                list_message(tr!("pack_export_no_actions"), palette, density)
            }
            SourceState::Ready(ready) => {
                let rows: Vec<AnyElement> = ready
                    .choices
                    .iter()
                    .filter(|choice| self.search.matches(&choice.name))
                    .enumerate()
                    .map(|(index, choice)| {
                        self.render_action_row(index, choice, palette, density, cx)
                    })
                    .collect();
                if rows.is_empty() {
                    list_message(tr!("pack_export_no_matches"), palette, density)
                } else {
                    div().w_full().flex().flex_col().children(rows)
                }
            }
        };
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(section_label(
                tr!("pack_export_actions_label").to_uppercase(),
                palette,
            ))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "pack_export_selected_count",
                        count = self.selected.len().to_string()
                    )),
            );
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(header)
            .child(self.search.field().clone())
            .child(
                div()
                    .id("pack-export-actions")
                    .w_full()
                    .h(ACTION_LIST_H)
                    .overflow_y_scroll()
                    .p(spacing(Spacing::Xxs, density))
                    .rounded(radius(Radius::Md))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .child(list),
            )
            .into_any_element()
    }

    fn render_action_row(
        &self,
        index: usize,
        choice: &ActionChoice,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self.selected.contains(&choice.id);
        let id = choice.id;
        let hover_bg = palette.base;
        let check = checkbox(checked, palette);
        div()
            .id(("pack-export-action", index))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(id, cx)))
            .child(check)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(if checked {
                        palette.text_primary
                    } else {
                        palette.text_secondary
                    })
                    .child(choice.name.clone()),
            )
            .into_any_element()
    }

    fn render_details(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let name_missing = self.pack_name(cx).is_empty();
        let required = tr!("pack_export_name_required");
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(field_column(
                palette,
                &tr!("pack_export_name_label"),
                name_missing.then_some(required.as_str()),
                None,
                field_frame(palette)
                    .child(self.name.clone())
                    .into_any_element(),
            ))
            .child(field_column(
                palette,
                &tr!("pack_export_author_label"),
                None,
                None,
                field_frame(palette)
                    .child(self.author.clone())
                    .into_any_element(),
            ))
            .child(field_column(
                palette,
                &tr!("pack_export_description_label"),
                None,
                None,
                field_frame(palette)
                    .child(self.description.clone())
                    .into_any_element(),
            ))
            .child(self.render_summary(palette, density))
            .into_any_element()
    }

    fn render_summary(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let body: AnyElement = match (&self.closure, &self.closure_failure) {
            (_, Some(message)) => failure_note(
                &tr!("pack_export_summary_failed", error = message.as_str()),
                palette,
            ),
            (Some(selection), None) => self.render_closure(selection, palette, density),
            (None, None) if self.resolving => summary_line(
                tr!("pack_export_summary_resolving"),
                palette.text_muted,
                Icon::Refresh,
                density,
            ),
            (None, None) => summary_line(
                tr!("pack_export_summary_empty"),
                palette.text_muted,
                Icon::InfoCircle,
                density,
            ),
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(section_label(
                tr!("pack_export_summary_label").to_uppercase(),
                palette,
            ))
            .child(body)
            .into_any_element()
    }

    fn render_closure(
        &self,
        selection: &PackSelection,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let counts = PackItemCount::ALL.chunks(COUNT_COLUMNS).map(|pair| {
            div()
                .w_full()
                .flex()
                .gap(spacing(Spacing::Md, density))
                .children(
                    pair.iter()
                        .map(|kind| count_cell(kind.label(), kind.count(selection), palette)),
                )
        });
        let integrations = if selection.required_integrations.is_empty() {
            tr!("pack_export_integrations_none")
        } else {
            selection
                .required_integrations
                .iter()
                .map(|id| integration_name(id).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .children(counts)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(section_label(
                        tr!("pack_export_integrations_label").to_uppercase(),
                        palette,
                    ))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(integrations),
                    ),
            );
        if !selection.unresolved_references.is_empty() {
            column = column.child(summary_line(
                tr!(
                    "pack_export_unresolved",
                    count = selection.unresolved_references.len().to_string()
                ),
                palette.warning,
                Icon::AlertTriangle,
                density,
            ));
        }
        column
            .child(summary_line(
                tr!("pack_export_names_not_followed"),
                palette.text_muted,
                Icon::InfoCircle,
                density,
            ))
            .into_any_element()
    }

    fn footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(ghost_button(tr!("common_cancel"), palette).on_click(
                "pack-export-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            ))
            .child(
                primary_button(tr!("pack_export_confirm_btn"), palette)
                    .disabled(!self.can_export(cx))
                    .on_click(
                        "pack-export-confirm",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                    ),
            )
            .into_any_element()
    }
}

fn list_message(message: String, palette: &ForgePalette, density: Density) -> gpui::Div {
    div()
        .w_full()
        .py(spacing(Spacing::Lg, density))
        .flex()
        .justify_center()
        .font_family(body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(message)
}

fn count_cell(label: String, count: usize, palette: &ForgePalette) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(if count == 0 {
                    palette.text_faint
                } else {
                    palette.text_primary
                })
                .child(count.to_string()),
        )
        .into_any_element()
}

fn summary_line(text: String, color: gpui::Rgba, glyph: Icon, density: Density) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Xs, density))
        .child(icon(glyph, FONT_XS, color))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(color)
                .child(SharedString::from(text)),
        )
        .into_any_element()
}

impl Render for PackExportModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        if self.focus_pending {
            self.focus_pending = false;
            self.name.update(cx, |input, cx| input.focus(window, cx));
        }

        let body = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(self.render_actions(&palette, density, cx))
            .child(self.render_details(&palette, density, cx));

        let card = modal(tr!("pack_export_title"), body, &palette)
            .header_icon(Icon::Category, palette.brand)
            .subtitle(tr!("pack_export_subtitle"))
            .width(modal_width(ModalSize::Lg))
            .footer(self.footer(&palette, density, cx))
            .on_close(
                "pack-export-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("pack-export-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;

    use forge_storage::Language;
    use forge_transfer::{PackSelection, load_pack_inventory, resolve_pack_closure};
    use forge_types::ActionId;
    use gpui::{AppContext, Entity, TestAppContext};

    use super::{PackExportModal, SourceState, action_choices, ordered_roots};
    use crate::i18n::install_language;
    use crate::test_support::pack_flow::{PackWorld, pack_world, queue_named, seed_queued_action};
    use crate::test_support::{install_presentation, runtime};

    struct Rig {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        greeter: ActionId,
        farewell: ActionId,
        modal: Entity<PackExportModal>,
    }

    fn rig(cx: &mut TestAppContext, preselect: impl FnOnce(ActionId) -> Vec<ActionId>) -> Rig {
        install_language(Language::En);
        install_presentation(cx);
        let rt = runtime();
        let world = pack_world(&rt);
        let greeter = seed_queued_action(&rt, &world, "Greeter", &queue_named("Alerts"));
        let farewell = seed_queued_action(&rt, &world, "Farewell", &queue_named("Outro"));
        let stopped = runtime();
        let detached = world.with_runtime(&stopped);
        drop(stopped);
        let preselected = preselect(greeter);
        let modal = cx.new(|cx| PackExportModal::new(detached, preselected, cx));
        let source = rt.block_on(world.services.load_source());
        modal.update(cx, |modal, cx| modal.source_loaded(source, cx));
        let rig = Rig {
            rt,
            world,
            greeter,
            farewell,
            modal,
        };
        deliver_closure(cx, &rig);
        rig
    }

    fn current_ticket(modal: &PackExportModal) -> u64 {
        (0..=u64::from(u16::MAX))
            .find(|ticket| modal.generation.is_current(*ticket))
            .expect("the generation stays small in a test")
    }

    fn deliver_closure(cx: &mut TestAppContext, rig: &Rig) {
        let (ticket, roots) = rig.modal.read_with(cx, |modal, _| {
            let SourceState::Ready(ready) = &modal.source else {
                panic!("the inventory is not loaded");
            };
            (
                current_ticket(modal),
                ordered_roots(&ready.choices, &modal.selected),
            )
        });
        if roots.is_empty() {
            return;
        }
        let source = rig.rt.block_on(rig.world.services.load_source()).unwrap();
        let closure = resolve_pack_closure(
            &source.inventory,
            rig.world.services.registries(&source),
            &roots,
        )
        .map_err(|error| error.to_string());
        rig.modal
            .update(cx, |modal, cx| modal.resolved(ticket, closure, cx));
    }

    fn closure_actions(cx: &mut TestAppContext, rig: &Rig) -> Option<BTreeSet<ActionId>> {
        rig.modal.read_with(cx, |modal, _| {
            modal
                .closure
                .as_ref()
                .map(|closure| closure.actions.clone())
        })
    }

    fn name_pack(cx: &mut TestAppContext, rig: &Rig, name: &str) {
        let field = rig.modal.read_with(cx, |modal, _| modal.name.clone());
        field.update(cx, |field, cx| field.set_content(name.to_owned(), cx));
    }

    fn can_export(cx: &mut TestAppContext, rig: &Rig) -> bool {
        rig.modal.update(cx, |modal, cx| modal.can_export(cx))
    }

    #[test]
    fn action_choices_are_listed_alphabetically_ignoring_case() {
        let rt = runtime();
        let world = pack_world(&rt);
        let queue = queue_named("Alerts");
        for name in ["beta", "Alpha", "gamma", "Delta"] {
            seed_queued_action(&rt, &world, name, &queue);
        }
        let inventory = rt
            .block_on(load_pack_inventory(world.provider().as_ref()))
            .unwrap();

        let names: Vec<String> = action_choices(&inventory)
            .into_iter()
            .map(|choice| choice.name)
            .collect();

        assert_eq!(names, ["Alpha", "beta", "Delta", "gamma"]);
    }

    #[gpui::test]
    fn the_action_editor_entry_opens_with_its_action_selected_and_resolved(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx, |greeter| vec![greeter]);

        assert_eq!(
            closure_actions(cx, &rig),
            Some(BTreeSet::from([rig.greeter]))
        );
    }

    #[gpui::test]
    fn a_preselected_action_that_no_longer_exists_is_dropped(cx: &mut TestAppContext) {
        let rig = rig(cx, |_| vec![ActionId::new()]);

        let state = rig.modal.read_with(cx, |modal, _| {
            (
                modal.selected.is_empty(),
                modal.closure.is_none(),
                modal.resolving,
            )
        });
        assert_eq!(state, (true, true, false));
    }

    #[gpui::test]
    fn changing_the_selection_holds_export_until_the_new_closure_arrives(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);
        name_pack(cx, &rig, "Alerts pack");

        rig.modal
            .update(cx, |modal, cx| modal.toggle(rig.farewell, cx));

        assert!(!can_export(cx, &rig));
    }

    #[gpui::test]
    fn selecting_another_action_grows_the_closure_summary(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);

        rig.modal
            .update(cx, |modal, cx| modal.toggle(rig.farewell, cx));
        deliver_closure(cx, &rig);

        assert_eq!(
            closure_actions(cx, &rig),
            Some(BTreeSet::from([rig.greeter, rig.farewell]))
        );
    }

    #[gpui::test]
    fn deselecting_the_last_action_clears_the_closure_summary(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);

        rig.modal
            .update(cx, |modal, cx| modal.toggle(rig.greeter, cx));

        let state = rig
            .modal
            .read_with(cx, |modal, _| (modal.closure.is_none(), modal.resolving));
        assert_eq!(state, (true, false));
    }

    #[gpui::test]
    fn a_closure_result_for_an_earlier_selection_is_discarded(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);
        let earlier = rig.modal.read_with(cx, |modal, _| current_ticket(modal));
        rig.modal
            .update(cx, |modal, cx| modal.toggle(rig.farewell, cx));
        let stale = PackSelection {
            actions: BTreeSet::from([rig.greeter]),
            ..PackSelection::default()
        };

        rig.modal
            .update(cx, |modal, cx| modal.resolved(earlier, Ok(stale), cx));
        let still_resolving = rig.modal.read_with(cx, |modal, _| modal.resolving);
        deliver_closure(cx, &rig);

        assert_eq!(
            (still_resolving, closure_actions(cx, &rig)),
            (true, Some(BTreeSet::from([rig.greeter, rig.farewell])))
        );
    }

    #[gpui::test]
    fn a_failed_closure_blocks_export_and_keeps_the_reason(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);
        name_pack(cx, &rig, "Alerts pack");
        rig.modal
            .update(cx, |modal, cx| modal.toggle(rig.farewell, cx));
        let ticket = rig.modal.read_with(cx, |modal, _| current_ticket(modal));

        rig.modal.update(cx, |modal, cx| {
            modal.resolved(ticket, Err("unreadable action".to_owned()), cx)
        });

        let failure = rig
            .modal
            .read_with(cx, |modal, _| modal.closure_failure.clone());
        assert_eq!(
            (can_export(cx, &rig), failure.as_deref()),
            (false, Some("unreadable action"))
        );
    }

    #[gpui::test]
    fn export_is_refused_while_the_pack_name_is_blank(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);

        name_pack(cx, &rig, "   ");

        assert!(!can_export(cx, &rig));
    }

    #[gpui::test]
    fn export_is_refused_with_no_action_selected(cx: &mut TestAppContext) {
        let rig = rig(cx, |_| Vec::new());

        name_pack(cx, &rig, "Alerts pack");

        assert!(!can_export(cx, &rig));
    }

    #[gpui::test]
    fn the_draft_carries_the_trimmed_name_and_the_resolved_closure(cx: &mut TestAppContext) {
        let rig = rig(cx, |greeter| vec![greeter]);
        name_pack(cx, &rig, "  Alerts pack  ");

        let draft = rig
            .modal
            .update(cx, |modal, cx| modal.take_draft(cx))
            .expect("a named pack with a selected action exports");

        assert_eq!(
            (draft.metadata.name.as_str(), draft.selection.actions),
            ("Alerts pack", BTreeSet::from([rig.greeter]))
        );
    }
}
