use super::step_presentation::StepGroup;
use super::*;
use crate::actions_screen::sub_action_modal::{SubFormLaunch, SubFormTarget};
use crate::picker_favorites::{bind_picker, follow_picker_event};
use forge_components::ActivePresentation;
use forge_components::focus_inside_modal;
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
    let mut runners: Vec<(String, String, StepGroup, &dyn SubActionRunner)> = registry
        .all()
        .map(|runner| {
            let group = StepGroup::of(registry, runner);
            (
                group.scope(),
                crate::kind_copy::step_name(runner),
                group,
                runner,
            )
        })
        .collect();
    runners.sort_by(
        |(a_scope, a_name, _, a_runner), (b_scope, b_name, _, b_runner)| {
            a_scope
                .cmp(b_scope)
                .then_with(|| a_name.cmp(b_name))
                .then_with(|| a_runner.id().cmp(b_runner.id()))
        },
    );

    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, String> = HashMap::new();
    for (scope, name, group, runner) in runners {
        let scope = SharedString::from(scope);
        let color = group.color(palette);
        let id = SharedString::from(format!("step-{}", runner.id()));
        picks.insert(id.clone(), runner.id().to_owned());
        let item = GridPickerItem {
            id,
            glyph: GlyphArt::Icon(Icon::from_name(runner.icon_name())),
            tint: color,
            name: name.into(),
            desc: crate::kind_copy::step_summary(runner).into(),
            state: GridPickerItemState::Normal,
            matches: None,
        };
        match groups.iter_mut().find(|g| g.scope == scope) {
            Some(g) => g.items.push(item),
            None => groups.push(GridPickerGroup {
                label: group.label().into(),
                dot_color: color,
                scope,
                items: vec![item],
            }),
        }
    }
    (groups, picks)
}

impl ActionEditor {
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
        let (groups, picks) = build_step_groups(&self.deps.sub_action_registry, &palette);
        let count = self.deps.sub_action_registry.all().count();
        let config = GridPickerConfig {
            accent: palette.brand,
            art: GridPickerArt::default(),
            header_icon: Icon::LayoutGrid,
            title: tr!("action_editor_picker_add_sub_title").into(),
            subtitle: GridPickerSubtitle::Context {
                lead: tr!("action_editor_picker_inserting_into").into(),
                name: ctx_name.into(),
                note: tr!(
                    "action_editor_picker_sub_count",
                    count = i64::try_from(count).unwrap_or(i64::MAX)
                )
                .into(),
            },
            footer_hint: tr!("action_editor_picker_footer_hint").into(),
            search_placeholder: tr!(
                "action_editor_picker_search",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            )
            .into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self
            .sub_action_favorites
            .read(cx)
            .ids()
            .cloned()
            .unwrap_or_default();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_grid_picker_event);
        let favorites_sub = bind_picker(&self.sub_action_favorites, &picker, cx);
        focus_inside_modal(window, cx, |window, cx| {
            picker.update(cx, |f, cx| f.focus(window, cx))
        });
        self.step_open = None;
        self.modals.grid_picker = Some(GridPickerForm {
            picker,
            picks,
            action_id,
            _sub: sub,
            _favorites: favorites_sub,
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
                    .modals
                    .grid_picker
                    .as_ref()
                    .and_then(|f| f.picks.get(id).cloned())
                {
                    self.grid_pick_step(kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(_) | GridPickerEvent::FavoritesRetry => {
                follow_picker_event(&self.sub_action_favorites, event, cx);
            }
            GridPickerEvent::Dismissed => self.cancel_grid_picker(cx),
        }
    }

    pub(in crate::actions_screen) fn cancel_grid_picker(&mut self, cx: &mut Context<Self>) {
        self.modals.grid_picker = None;
        cx.notify();
    }

    fn grid_pick_step(&mut self, kind_id: String, cx: &mut Context<Self>) {
        let same = self
            .modals
            .grid_picker
            .as_ref()
            .zip(self.detail.as_ref())
            .is_some_and(|(f, d)| f.action_id == d.action.id);
        let prepared = self
            .deps
            .sub_action_registry
            .get(&kind_id)
            .filter(|_| same)
            .map(|runner| {
                (
                    runner.default_config(),
                    runner.config_fields(),
                    super::sub_form::output_name_keys(runner),
                    runner.icon_name().to_owned(),
                    StepGroup::of(&self.deps.sub_action_registry, runner),
                    runner.config_refinement(),
                )
            });
        self.modals.grid_picker = None;
        let Some((config, specs, output_name_keys, icon_name, group, refinement)) = prepared else {
            cx.notify();
            return;
        };
        let kind_label = self.kind_label(&kind_id);
        let chain_len = self.current_chain().len();
        let launch = SubFormLaunch {
            kind_id,
            target: SubFormTarget::Add,
            specs,
            output_name_keys,
            config,
            name_value: kind_label.clone(),
            condition_value: String::new(),
            continue_on_error: false,
            kind_label,
            icon_name,
            group: Some(group),
            chain_len,
            options_seed: self.choices.select_options.clone(),
            refinement,
            schema: Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>,
        };
        self.step_open = None;
        self.open_sub_form(launch, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn owned_steps_keep_their_group_scope_label_and_tint() {
        crate::i18n::install_language(forge_storage::Language::En);
        let palette = forge_components::FORGE_DEFAULT;
        let (_, steps) = crate::test_support::every_registry_entry().await;

        let (groups, _) = build_step_groups(&steps, &palette);

        let found = |scope: &str| {
            groups
                .iter()
                .find(|group| group.scope.as_ref() == scope)
                .map(|group| (group.label.to_string(), group.dot_color))
        };
        for (scope, label, tint) in [
            ("twitch", "Twitch", palette.theme_brand),
            ("obs", "OBS", palette.text_secondary),
            ("vtube", "VTube Studio", palette.accent_teal),
            ("discord", "Discord", palette.info),
            ("midi", "MIDI", palette.danger),
        ] {
            assert_eq!(found(scope), Some((label.to_owned(), tint)), "{scope}");
        }
        let scopes: Vec<&str> = groups.iter().map(|group| group.scope.as_ref()).collect();
        let mut sorted = scopes.clone();
        sorted.sort_unstable();
        assert_eq!(scopes, sorted);
    }
}
