use super::step_presentation::{sub_category_label, sub_category_slug};
use super::*;
use crate::actions_screen::sub_action_modal::{SubFormLaunch, SubFormTarget};
use crate::presentation::ActivePresentation;
use forge_components::{
    ForgePalette, GlyphArt, GridPicker, GridPickerArt, GridPickerConfig, GridPickerEvent,
    GridPickerGroup, GridPickerItem, GridPickerItemState, GridPickerSubtitle, Icon, tr,
};
use forge_registry::{FormSchemaSource, SubActionRegistry, SubActionRunner};
use gpui::{Context, Entity, SharedString, Window};
use std::collections::HashMap;

fn build_step_groups(
    registry: &SubActionRegistry,
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, String>) {
    let mut runners: Vec<&dyn SubActionRunner> = registry.all().collect();
    runners.sort_by(|a, b| {
        sub_category_slug(a.category())
            .cmp(sub_category_slug(b.category()))
            .then_with(|| a.label().cmp(b.label()))
    });

    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, String> = HashMap::new();
    for runner in runners {
        let cat = runner.category();
        let scope = SharedString::from(sub_category_slug(cat));
        let color = sub_category_color(cat, palette);
        let id = SharedString::from(format!("step-{}", runner.id()));
        picks.insert(id.clone(), runner.id().to_owned());
        let item = GridPickerItem {
            id,
            glyph: GlyphArt::Icon(Icon::from_name(runner.icon_name())),
            tint: color,
            name: runner.label().to_string().into(),
            desc: runner.summary().to_string().into(),
            state: GridPickerItemState::Normal,
            matches: None,
        };
        match groups.iter_mut().find(|g| g.scope == scope) {
            Some(g) => g.items.push(item),
            None => groups.push(GridPickerGroup {
                label: sub_category_label(cat).into(),
                dot_color: color,
                scope,
                items: vec![item],
            }),
        }
    }
    (groups, picks)
}

impl ScreenActionsView {
    pub(super) fn open_grid_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        if self.detail.is_none() {
            return;
        }
        let palette = cx.palette();
        let ctx_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| tr!("action_editor_this_action"));
        let (groups, picks) = build_step_groups(&self.sub_action_registry, &palette);
        let count = self.sub_action_registry.all().count();
        let config = GridPickerConfig {
            accent: palette.brand,
            art: GridPickerArt::default(),
            header_icon: Icon::LayoutGrid,
            title: tr!("action_editor_picker_add_sub_title").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("action_editor_picker_inserting_into").into(),
                name: ctx_name.into(),
                note: tr!("action_editor_picker_sub_count", count = count as i64).into(),
            },
            footer_hint: tr!("action_editor_picker_footer_hint").into(),
            search_placeholder: tr!("action_editor_picker_search", count = count as i64).into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.sub_action_favorites.clone();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_grid_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.step_menu_open = None;
        self.grid_picker = Some(GridPickerForm {
            picker,
            picks,
            action_id,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_grid_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                if let Some(kind_id) = self
                    .grid_picker
                    .as_ref()
                    .and_then(|f| f.picks.get(id).cloned())
                {
                    self.grid_pick_step(kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(id) => {
                if self.sub_action_favorites.contains(id) {
                    self.sub_action_favorites.remove(id);
                } else {
                    self.sub_action_favorites.insert(id.clone());
                }
                let favorites = self.sub_action_favorites.clone();
                self.persist_favorites(reserved_keys::PICKER_FAVORITES_SUB_ACTIONS, favorites, cx);
            }
            GridPickerEvent::Dismissed => self.cancel_grid_picker(cx),
        }
    }

    pub(in crate::actions_screen) fn cancel_grid_picker(&mut self, cx: &mut Context<Self>) {
        self.grid_picker = None;
        cx.notify();
    }

    fn grid_pick_step(&mut self, kind_id: String, cx: &mut Context<Self>) {
        let same = self
            .grid_picker
            .as_ref()
            .zip(self.detail.as_ref())
            .is_some_and(|(f, d)| f.action_id == d.action.id);
        let prepared = self
            .sub_action_registry
            .get(&kind_id)
            .filter(|_| same)
            .map(|runner| {
                (
                    runner.default_config(),
                    runner.config_fields(),
                    runner.icon_name().to_owned(),
                    runner.category(),
                    runner.config_refinement(),
                )
            });
        self.grid_picker = None;
        let Some((config, specs, icon_name, category, refinement)) = prepared else {
            cx.notify();
            return;
        };
        let kind_label = self.kind_label(&kind_id);
        let chain_len = self.current_chain().len();
        let launch = SubFormLaunch {
            kind_id,
            target: SubFormTarget::Add,
            specs,
            config,
            name_value: kind_label.clone(),
            condition_value: String::new(),
            continue_on_error: false,
            kind_label,
            icon_name,
            category: Some(category),
            chain_len,
            options_seed: self.select_options.clone(),
            refinement,
            schema: Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>,
        };
        self.step_menu_open = None;
        self.open_sub_form(launch, cx);
    }
}
