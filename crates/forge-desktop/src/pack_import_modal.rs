use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    ModalSize, OverlayPosition, Radius, Spacing, badge, body_family, failure_note, ghost_button,
    icon, modal, modal_width, mono_family, overlay, primary_button, radius, section_label, segment,
    segmented, spacing, tr, with_alpha,
};
use forge_transfer::{
    ExecutableContent, PackConflict, PackImportLimit, PackImportPlan, PackItemKey, PackItemKind,
    PackManifest, PackPlanItem, PackResolution, PackWarning,
};
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, FontWeight, Pixels, Rgba, SharedString, Window,
    div, prelude::*, px, relative,
};

use crate::integration_switch::integration_name;

const ITEM_LIST_H: Pixels = px(360.0);
const NOTICE_TINT_ALPHA: f32 = 0.08;
const ITEM_COLUMN_SHARE: f32 = 0.62;

const KIND_ORDER: [PackItemKind; 8] = [
    PackItemKind::Action,
    PackItemKind::Trigger,
    PackItemKind::Queue,
    PackItemKind::Script,
    PackItemKind::Global,
    PackItemKind::Clip,
    PackItemKind::Overlay,
    PackItemKind::Media,
];

const RESOLUTIONS: [PackResolution; 3] = [
    PackResolution::Skip,
    PackResolution::Replace,
    PackResolution::KeepBoth,
];

pub enum PackImportModalEvent {
    Confirmed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackHeader {
    pub name: String,
    pub author: String,
    pub description: String,
}

impl PackHeader {
    pub fn of(manifest: &PackManifest) -> Self {
        Self {
            name: manifest.name.clone(),
            author: manifest.author.clone(),
            description: manifest.description.clone(),
        }
    }
}

pub(crate) struct KindGroup<'a> {
    pub kind: PackItemKind,
    pub items: Vec<(usize, &'a PackPlanItem)>,
}

pub(crate) fn grouped_items(plan: &PackImportPlan) -> Vec<KindGroup<'_>> {
    KIND_ORDER
        .iter()
        .filter_map(|kind| {
            let items: Vec<(usize, &PackPlanItem)> = plan
                .items()
                .iter()
                .enumerate()
                .filter(|(_, item)| item.key.kind == *kind)
                .collect();
            (!items.is_empty()).then_some(KindGroup { kind: *kind, items })
        })
        .collect()
}

pub(crate) fn carry_choices(previous: &PackImportPlan, fresh: &mut PackImportPlan) {
    let chosen: Vec<(PackItemKey, PackResolution)> = previous
        .items()
        .iter()
        .filter_map(|item| item.resolution.map(|choice| (item.key.clone(), choice)))
        .collect();
    for (key, choice) in chosen {
        let _ = fresh.set_resolution(&key, choice);
    }
}

pub(crate) fn kind_label(kind: PackItemKind) -> String {
    match kind {
        PackItemKind::Action => tr!("pack_export_kind_actions"),
        PackItemKind::Trigger => tr!("pack_export_kind_triggers"),
        PackItemKind::Queue => tr!("pack_export_kind_queues"),
        PackItemKind::Script => tr!("pack_export_kind_scripts"),
        PackItemKind::Global => tr!("pack_export_kind_globals"),
        PackItemKind::Clip => tr!("pack_export_kind_clips"),
        PackItemKind::Overlay => tr!("pack_export_kind_overlays"),
        PackItemKind::Media => tr!("pack_export_kind_media"),
    }
}

fn resolution_label(resolution: PackResolution) -> String {
    match resolution {
        PackResolution::Skip => tr!("pack_import_resolution_skip"),
        PackResolution::Replace => tr!("pack_import_resolution_replace"),
        PackResolution::KeepBoth => tr!("pack_import_resolution_keep_both"),
    }
}

fn resolution_slug(resolution: PackResolution) -> &'static str {
    match resolution {
        PackResolution::Skip => "pack-import-skip",
        PackResolution::Replace => "pack-import-replace",
        PackResolution::KeepBoth => "pack-import-keep-both",
    }
}

fn warning_text(warning: &PackWarning) -> String {
    match warning {
        PackWarning::MissingIntegration { integration } => tr!(
            "pack_import_warning_integration",
            integration = integration_name(integration).to_string()
        ),
        PackWarning::UnknownKind { kind_id } => {
            tr!("pack_import_warning_unknown_kind", kind = kind_id.as_str())
        }
        PackWarning::UnresolvedReference { reference } => tr!(
            "pack_import_warning_unresolved",
            namespace = reference.namespace.as_str(),
            value = reference.value.as_str()
        ),
        PackWarning::ExternalReference { reference } => tr!(
            "pack_import_warning_external",
            namespace = reference.namespace.as_str(),
            value = reference.value.as_str()
        ),
        PackWarning::ClipWithoutSound { clip } => {
            tr!("pack_import_warning_clip_silent", clip = clip.as_str())
        }
    }
}

fn executable_text(content: &ExecutableContent) -> String {
    match content {
        ExecutableContent::Script { name } => {
            tr!("pack_import_runs_script", name = name.as_str())
        }
        ExecutableContent::HttpStep { action, .. } => {
            tr!("pack_import_runs_http", action = action.as_str())
        }
        ExecutableContent::ScriptStep { action, .. } => {
            tr!("pack_import_runs_script_step", action = action.as_str())
        }
        ExecutableContent::OverlaySource { overlay, file } => tr!(
            "pack_import_runs_overlay",
            overlay = overlay.as_str(),
            file = file.as_str()
        ),
    }
}

fn limit_text(limit: PackImportLimit) -> String {
    match limit {
        PackImportLimit::NamesInScriptsAndTextNotFollowed => tr!("pack_import_limit_names"),
    }
}

pub struct PackImportModal {
    header: PackHeader,
    plan: PackImportPlan,
    applying: bool,
    failure: Option<String>,
}

impl EventEmitter<PackImportModalEvent> for PackImportModal {}

impl PackImportModal {
    pub fn new(header: PackHeader, plan: PackImportPlan) -> Self {
        Self {
            header,
            plan,
            applying: false,
            failure: None,
        }
    }

    pub fn plan(&self) -> &PackImportPlan {
        &self.plan
    }

    pub fn is_applying(&self) -> bool {
        self.applying
    }

    pub(crate) fn choose(
        &mut self,
        key: &PackItemKey,
        resolution: PackResolution,
        cx: &mut Context<Self>,
    ) {
        if self.applying {
            return;
        }
        if self.plan.set_resolution(key, resolution).is_ok() {
            self.failure = None;
            cx.notify();
        }
    }

    pub fn begin_apply(&mut self, cx: &mut Context<Self>) {
        self.applying = true;
        self.failure = None;
        cx.notify();
    }

    pub fn replan(&mut self, plan: PackImportPlan, cx: &mut Context<Self>) {
        self.plan = plan;
        self.applying = false;
        cx.notify();
    }

    pub fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        self.applying = false;
        self.failure = Some(message);
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if !self.applying {
            cx.emit(PackImportModalEvent::Confirmed);
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.applying {
            cx.emit(PackImportModalEvent::Cancelled);
        }
    }

    fn render_items(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let groups = grouped_items(&self.plan);
        let body: AnyElement = if groups.is_empty() {
            div()
                .w_full()
                .py(spacing(Spacing::Lg, density))
                .flex()
                .justify_center()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("pack_import_empty"))
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .children(
                    groups
                        .into_iter()
                        .map(|group| self.render_group(group, palette, density, cx)),
                )
                .into_any_element()
        };
        div()
            .flex_none()
            .w(relative(ITEM_COLUMN_SHARE))
            .max_w(relative(ITEM_COLUMN_SHARE))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(section_label(
                        tr!("pack_import_items_label").to_uppercase(),
                        palette,
                    ))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(tr!(
                                "pack_import_item_count",
                                count = self.plan.items().len().to_string()
                            )),
                    ),
            )
            .child(
                div()
                    .id("pack-import-items")
                    .w_full()
                    .h(ITEM_LIST_H)
                    .overflow_y_scroll()
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Md))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .child(body),
            )
            .into_any_element()
    }

    fn render_group(
        &self,
        group: KindGroup<'_>,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Xs, density))
            .child(section_label(
                kind_label(group.kind).to_uppercase(),
                palette,
            ))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(group.items.len().to_string()),
            );
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(header)
            .children(
                group
                    .items
                    .into_iter()
                    .map(|(index, item)| self.render_item(index, item, palette, density, cx)),
            )
            .into_any_element()
    }

    fn render_item(
        &self,
        index: usize,
        item: &PackPlanItem,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut badges = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(conflict_badge(&item.conflict, palette));
        if item.archived {
            badges = badges.child(tag(
                tr!("pack_import_conflict_archived"),
                palette.text_muted,
                palette,
            ));
        }
        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(item.name.clone()),
            )
            .child(badges);
        if item.conflict != PackConflict::None {
            row = row.child(self.render_resolution(index, item, palette, cx));
        }
        row.into_any_element()
    }

    fn render_resolution(
        &self,
        index: usize,
        item: &PackPlanItem,
        palette: &ForgePalette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let segments = RESOLUTIONS
            .iter()
            .map(|resolution| {
                let resolution = *resolution;
                let key = item.key.clone();
                let view = view.clone();
                segment(
                    (resolution_slug(resolution), index),
                    resolution_label(resolution),
                    item.resolution == Some(resolution),
                    move |_, _, app| {
                        view.update(app, |this, cx| this.choose(&key, resolution, cx));
                    },
                )
                .disabled(self.applying)
            })
            .collect();
        div()
            .flex_none()
            .child(segmented(segments, palette).subtle(palette))
            .into_any_element()
    }

    fn render_notices(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let mut column = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(self.render_header(palette, density));
        if !self.plan.executable_content().is_empty() {
            column = column.child(self.render_executable(palette, density));
        }
        if !self.plan.warnings().is_empty() {
            column = column.child(self.render_warnings(palette, density));
        }
        let mut notes = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        for limit in self.plan.limits() {
            notes = notes.child(summary_line(
                limit_text(*limit),
                palette.text_muted,
                Icon::InfoCircle,
                density,
            ));
        }
        if self.plan.replaces_anything() {
            notes = notes.child(summary_line(
                tr!("pack_import_snapshot_note"),
                palette.info,
                Icon::Shield,
                density,
            ));
        }
        column.child(notes).into_any_element()
    }

    fn render_header(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let mut card = card(palette.border_regular, palette.shell, density)
            .child(section_label(
                tr!("pack_import_pack_label").to_uppercase(),
                palette,
            ))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_primary)
                    .child(self.header.name.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(if self.header.author.is_empty() {
                        tr!("pack_import_author_unknown")
                    } else {
                        tr!("pack_import_author", author = self.header.author.as_str())
                    }),
            );
        if !self.header.description.is_empty() {
            card = card.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(self.header.description.clone()),
            );
        }
        card.into_any_element()
    }

    fn render_executable(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let lines = self.plan.executable_content().iter().map(|content| {
            div()
                .w_full()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_secondary)
                .child(executable_text(content))
        });
        card(
            palette.warning,
            with_alpha(palette.warning, NOTICE_TINT_ALPHA),
            density,
        )
        .child(summary_line(
            tr!("pack_import_runs_code_notice"),
            palette.warning,
            Icon::Code,
            density,
        ))
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .pl(spacing(Spacing::Md, density))
                .children(lines),
        )
        .into_any_element()
    }

    fn render_warnings(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        card(palette.border_regular, palette.shell, density)
            .child(section_label(
                tr!("pack_import_warnings_label").to_uppercase(),
                palette,
            ))
            .children(self.plan.warnings().iter().map(|warning| {
                summary_line(
                    warning_text(warning),
                    palette.warning,
                    Icon::AlertTriangle,
                    density,
                )
            }))
            .into_any_element()
    }

    fn footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        row = match &self.failure {
            Some(message) => row.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(failure_note(message, palette)),
            ),
            None => row.child(div().flex_1()),
        };
        let confirm_label = if self.applying {
            tr!("pack_import_applying")
        } else {
            tr!("pack_import_confirm_btn")
        };
        row.child(
            ghost_button(tr!("common_cancel"), palette)
                .disabled(self.applying)
                .on_click(
                    "pack-import-cancel",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                ),
        )
        .child(
            primary_button(confirm_label, palette)
                .disabled(self.applying || self.plan.items().is_empty())
                .on_click(
                    "pack-import-confirm",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                ),
        )
        .into_any_element()
    }
}

fn card(border: Rgba, background: Rgba, density: Density) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .p(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(border)
        .bg(background)
}

fn conflict_badge(conflict: &PackConflict, palette: &ForgePalette) -> AnyElement {
    match conflict {
        PackConflict::None => tag(
            tr!("pack_import_conflict_none"),
            palette.text_faint,
            palette,
        ),
        PackConflict::IdMatch => tag(tr!("pack_import_conflict_id"), palette.warning, palette),
        PackConflict::NameMatch { .. } => {
            tag(tr!("pack_import_conflict_name"), palette.info, palette)
        }
    }
}

fn tag(label: String, ink: Rgba, palette: &ForgePalette) -> AnyElement {
    badge(palette.surface_overlay, ink, label, true, FONT_XXS)
        .flex_none()
        .into_any_element()
}

fn summary_line(text: String, color: Rgba, glyph: Icon, density: Density) -> AnyElement {
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

impl Render for PackImportModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let body = div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Md, density))
            .child(self.render_items(&palette, density, cx))
            .child(self.render_notices(&palette, density));

        let card = modal(tr!("pack_import_title"), body, &palette)
            .header_icon(Icon::Download, palette.brand)
            .subtitle(tr!("pack_import_subtitle"))
            .width(modal_width(ModalSize::Lg))
            .footer(self.footer(&palette, density, cx))
            .on_close(
                "pack-import-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("pack-import-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    use forge_storage::Language;
    use forge_transfer::{PackImportPlan, PackItemKey, PackItemKind, PackResolution};
    use gpui::{AppContext, Entity, TestAppContext};

    use super::{PackHeader, PackImportModal, PackImportModalEvent, carry_choices, grouped_items};
    use crate::i18n::install_language;
    use crate::test_support::pack_flow::{
        PackWorld, export_pack, pack_world, queue_named, seed_queued_action, stage_into,
    };
    use crate::test_support::runtime;

    struct Planned {
        rt: tokio::runtime::Runtime,
        _world: PackWorld,
        bundle: PathBuf,
        plan: PackImportPlan,
        action: PackItemKey,
        queue: PackItemKey,
    }

    fn reimported_into_its_source() -> Planned {
        let rt = runtime();
        let world = pack_world(&rt);
        let queue = queue_named("Alerts");
        let action = seed_queued_action(&rt, &world, "Greeter", &queue);
        let bundle = export_pack(&rt, &world, &[action]);
        let (_staged, plan) = stage_into(&rt, &world, &bundle);
        Planned {
            rt,
            _world: world,
            bundle,
            plan,
            action: PackItemKey {
                kind: PackItemKind::Action,
                id: action.to_string(),
            },
            queue: PackItemKey {
                kind: PackItemKind::Queue,
                id: queue.id.to_string(),
            },
        }
    }

    impl PackImportModal {
        pub(crate) fn shown_failure(&self) -> Option<String> {
            self.failure.clone()
        }
    }

    fn resolution_of(plan: &PackImportPlan, key: &PackItemKey) -> Option<PackResolution> {
        plan.items()
            .iter()
            .find(|item| &item.key == key)
            .and_then(|item| item.resolution)
    }

    fn header() -> PackHeader {
        PackHeader {
            name: "Alerts pack".to_owned(),
            author: String::new(),
            description: String::new(),
        }
    }

    fn mount(cx: &mut TestAppContext, plan: PackImportPlan) -> Entity<PackImportModal> {
        install_language(Language::En);
        cx.new(|_| PackImportModal::new(header(), plan))
    }

    fn event_log(
        cx: &mut TestAppContext,
        modal: &Entity<PackImportModal>,
    ) -> Rc<RefCell<Vec<&'static str>>> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&log);
        cx.update(|cx| {
            cx.subscribe(modal, move |_, event: &PackImportModalEvent, _| {
                sink.borrow_mut().push(match event {
                    PackImportModalEvent::Confirmed => "confirmed",
                    PackImportModalEvent::Cancelled => "cancelled",
                });
            })
            .detach();
        });
        log
    }

    #[test]
    fn carried_choices_land_on_matching_keys_and_untouched_items_keep_their_defaults() {
        let previous_source = reimported_into_its_source();
        let mut previous = previous_source.plan.clone();
        previous
            .set_resolution(&previous_source.action, PackResolution::Replace)
            .unwrap();
        let mut fresh = previous_source.plan.clone();

        carry_choices(&previous, &mut fresh);

        assert_eq!(
            (
                resolution_of(&fresh, &previous_source.action),
                resolution_of(&fresh, &previous_source.queue),
            ),
            (Some(PackResolution::Replace), Some(PackResolution::Skip))
        );
    }

    #[test]
    fn a_carried_choice_for_an_item_that_no_longer_conflicts_is_dropped() {
        let planned = reimported_into_its_source();
        let mut previous = planned.plan.clone();
        previous
            .set_resolution(&planned.action, PackResolution::Replace)
            .unwrap();
        let empty_target = pack_world(&planned.rt);
        let (_staged, mut fresh) = stage_into(&planned.rt, &empty_target, &planned.bundle);
        let untouched = fresh.clone();

        carry_choices(&previous, &mut fresh);

        assert_eq!(fresh, untouched);
    }

    #[test]
    fn the_preview_groups_items_in_display_order_regardless_of_plan_order() {
        let planned = reimported_into_its_source();

        let kinds: Vec<PackItemKind> = grouped_items(&planned.plan)
            .iter()
            .map(|group| group.kind)
            .collect();

        assert_eq!(kinds, vec![PackItemKind::Action, PackItemKind::Queue]);
    }

    #[gpui::test]
    fn a_choice_made_while_applying_is_ignored(cx: &mut TestAppContext) {
        let planned = reimported_into_its_source();
        let modal = mount(cx, planned.plan.clone());

        modal.update(cx, |modal, cx| {
            modal.begin_apply(cx);
            modal.choose(&planned.action, PackResolution::Replace, cx);
        });

        let chosen = modal.read_with(cx, |modal, _| resolution_of(modal.plan(), &planned.action));
        assert_eq!(chosen, Some(PackResolution::Skip));
    }

    #[gpui::test]
    fn a_failed_apply_shows_the_error_and_unlocks_the_choices(cx: &mut TestAppContext) {
        let planned = reimported_into_its_source();
        let modal = mount(cx, planned.plan.clone());

        modal.update(cx, |modal, cx| {
            modal.begin_apply(cx);
            modal.fail("disk full".to_owned(), cx);
        });

        let state = modal.read_with(cx, |modal, _| (modal.is_applying(), modal.failure.clone()));
        assert_eq!(state, (false, Some("disk full".to_owned())));
    }

    #[gpui::test]
    fn changing_a_choice_after_a_failure_clears_the_error(cx: &mut TestAppContext) {
        let planned = reimported_into_its_source();
        let modal = mount(cx, planned.plan.clone());
        modal.update(cx, |modal, cx| modal.fail("disk full".to_owned(), cx));

        modal.update(cx, |modal, cx| {
            modal.choose(&planned.action, PackResolution::Replace, cx)
        });

        let state = modal.read_with(cx, |modal, _| {
            (
                modal.failure.clone(),
                resolution_of(modal.plan(), &planned.action),
            )
        });
        assert_eq!(state, (None, Some(PackResolution::Replace)));
    }

    #[gpui::test]
    fn confirm_and_cancel_are_swallowed_while_applying(cx: &mut TestAppContext) {
        let planned = reimported_into_its_source();
        let modal = mount(cx, planned.plan.clone());
        let log = event_log(cx, &modal);

        modal.update(cx, |modal, cx| {
            modal.begin_apply(cx);
            modal.confirm(cx);
            modal.cancel(cx);
        });
        cx.run_until_parked();

        assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    }

    #[gpui::test]
    fn a_replan_replaces_the_plan_and_unlocks_the_choices(cx: &mut TestAppContext) {
        let planned = reimported_into_its_source();
        let modal = mount(cx, planned.plan.clone());
        let mut replanned = planned.plan.clone();
        replanned
            .set_resolution(&planned.queue, PackResolution::Replace)
            .unwrap();

        modal.update(cx, |modal, cx| {
            modal.begin_apply(cx);
            modal.replan(replanned.clone(), cx);
        });

        let state = modal.read_with(cx, |modal, _| {
            (modal.is_applying(), modal.plan() == &replanned)
        });
        assert_eq!(state, (false, true));
    }
}
