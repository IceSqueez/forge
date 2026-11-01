use super::choice_feed::{ChoiceFeed, ChoiceHost, start_option_choices};
use super::{TriggerInstanceRow, TriggerScreenDeps, on_repo_error, save_then_list};
use crate::app::async_bridge;
use crate::automation::trigger_fill_form::{
    TriggerFillForm, TriggerFillFormEvent, TriggerFillIds, TriggerFillSeed,
};
use crate::integrations::integration_catalog::{catalog_text, tint_color};
use crate::shared::picker_favorites::{StoredFavorites, bind_picker, follow_picker_event};
use forge_components::focus_inside_modal;
use forge_components::{
    ActivePresentation, ForgePalette, GlyphArt, GridPicker, GridPickerArt, GridPickerConfig,
    GridPickerEvent, GridPickerGroup, GridPickerItem, GridPickerItemState, GridPickerSubtitle,
    GridScope, Icon, OverlayPosition, overlay, tr,
};
use forge_host::integration_catalog::{declaration_of, declarations};
use forge_host::integrations::BuiltinRegistry;
use forge_platform_core::IntegrationDeclaration;
use forge_registry::{TriggerCategory, TriggerKindDescriptor, TriggerRegistry};
use forge_types::TriggerInstanceId;
use gpui::Rgba;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, SharedString, Subscription, Window, prelude::*,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const FILL_IDS: TriggerFillIds = TriggerFillIds {
    field: "triggers-create-field",
    back: "triggers-create-back",
    cancel: "triggers-create-cancel",
    submit: "triggers-create-submit",
    close: "triggers-create-close",
    scrim: "triggers-create-scrim",
};

enum CreateStage {
    PickKind(KindPickerForm),
    Fill(CreateFillForm),
}

struct KindPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    _sub: Subscription,
    _favorites: Subscription,
}

struct CreateFillForm {
    form: Entity<TriggerFillForm>,
    _sub: Subscription,
}

pub(super) enum TriggerCreateEvent {
    Created {
        id: TriggerInstanceId,
        rows: Vec<TriggerInstanceRow>,
    },
}

impl EventEmitter<TriggerCreateEvent> for TriggerCreateFlow {}

pub(super) struct TriggerCreateFlow {
    deps: TriggerScreenDeps,
    favorites: Entity<StoredFavorites>,
    stage: Option<CreateStage>,
    claims_focus: bool,
    choice_feed: ChoiceFeed,
}

impl TriggerCreateFlow {
    pub(super) fn new(deps: TriggerScreenDeps, favorites: Entity<StoredFavorites>) -> Self {
        let choice_feed = ChoiceFeed::new(deps.rt_handle.clone());
        Self {
            deps,
            favorites,
            stage: None,
            claims_focus: false,
            choice_feed,
        }
    }

    pub(super) fn set_builtins(&mut self, builtins: BuiltinRegistry) {
        self.choice_feed.set_builtins(builtins);
    }

    pub(super) fn is_open(&self) -> bool {
        self.stage.is_some()
    }

    pub(super) fn is_filling(&self) -> bool {
        matches!(self.stage, Some(CreateStage::Fill(_)))
    }

    pub(super) fn open(&mut self, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let (groups, picks) = build_kind_groups(&self.deps.registry, &palette);
        let count: usize = groups.iter().map(|g| g.items.len()).sum();
        let config = GridPickerConfig {
            accent: palette.brand,
            art: GridPickerArt::default(),
            header_icon: Icon::Bolt,
            title: tr!("triggers_new_trigger").into(),
            subtitle: GridPickerSubtitle::Plain(
                tr!(
                    "triggers_create_type_count",
                    count = i64::try_from(count).unwrap_or(i64::MAX)
                )
                .into(),
            ),
            footer_hint: tr!("triggers_create_footer_hint").into(),
            search_placeholder: tr!(
                "triggers_create_search_types",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            )
            .into(),
            favorites_label: tr!("picker_favorites").into(),
            favorites_empty: tr!("picker_favorites_empty").into(),
        };
        let favorites = self.favorites.read(cx).ids().cloned().unwrap_or_default();
        let picker = cx.new(|cx| GridPicker::new(config, groups, favorites, cx));
        let sub = cx.subscribe(&picker, Self::on_create_picker_event);
        let favorites_sub = bind_picker(&self.favorites, &picker, cx);
        self.claims_focus = true;
        self.stage = Some(CreateStage::PickKind(KindPickerForm {
            picker,
            picks,
            _sub: sub,
            _favorites: favorites_sub,
        }));
        cx.notify();
    }

    fn on_create_picker_event(
        &mut self,
        _picker: Entity<GridPicker>,
        event: &GridPickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GridPickerEvent::Picked(id) => {
                let kind_id = match self.stage.as_ref() {
                    Some(CreateStage::PickKind(form)) => form.picks.get(id).cloned(),
                    _ => None,
                };
                if let Some(kind_id) = kind_id {
                    self.enter_fill(kind_id, cx);
                }
            }
            GridPickerEvent::FavoriteToggled(_) | GridPickerEvent::FavoritesRetry => {
                follow_picker_event(&self.favorites, event, cx);
            }
            GridPickerEvent::Dismissed => self.cancel_create(cx),
        }
    }

    fn enter_fill(&mut self, kind_id: String, cx: &mut Context<Self>) {
        let flow = cx.entity().downgrade();
        let seed = TriggerFillSeed {
            registry: Arc::clone(&self.deps.registry),
            kind_id,
            ids: FILL_IDS,
            on_back: Rc::new(move |_window, cx| {
                if let Err(closed) = flow.update(cx, |flow, cx| flow.open(cx)) {
                    tracing::debug!(error = %closed, "the trigger flow closed under its form");
                }
            }),
        };
        let form = cx.new(|cx| TriggerFillForm::new(seed, cx));
        let sub = cx.subscribe(&form, Self::on_fill_event);
        self.stage = Some(CreateStage::Fill(CreateFillForm { form, _sub: sub }));
        start_option_choices(self, cx);
        cx.notify();
    }

    fn on_fill_event(
        &mut self,
        _form: Entity<TriggerFillForm>,
        event: &TriggerFillFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TriggerFillFormEvent::Cancelled => self.cancel_create(cx),
            TriggerFillFormEvent::SubmitRequested => self.submit_create(cx),
        }
    }

    pub(super) fn cancel_create(&mut self, cx: &mut Context<Self>) {
        self.stage = None;
        cx.notify();
    }

    pub(super) fn submit_create(&mut self, cx: &mut Context<Self>) {
        let Some(CreateStage::Fill(CreateFillForm { form, .. })) = self.stage.as_ref() else {
            return;
        };
        let form = form.clone();
        let Some(instance) = form.update(cx, |form, cx| form.instance_to_save(cx)) else {
            return;
        };
        let new_id = instance.id;
        form.update(cx, |form, cx| form.set_saving(true, cx));

        let repo = Arc::clone(&self.deps.repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { save_then_list(&*repo, &instance).await },
            move |this, result, cx| match result {
                Ok(rows) => {
                    this.stage = None;
                    cx.emit(TriggerCreateEvent::Created { id: new_id, rows });
                    cx.notify();
                }
                Err(message) => {
                    form.update(cx, |form, cx| form.set_saving(false, cx));
                    on_repo_error(&message, cx);
                }
            },
            cx,
        );
    }

    pub(super) fn render_layer(
        &mut self,
        palette: &ForgePalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if std::mem::take(&mut self.claims_focus)
            && let Some(CreateStage::PickKind(form)) = self.stage.as_ref()
        {
            let picker = form.picker.clone();
            focus_inside_modal(window, cx, |window, cx| {
                picker.update(cx, |f, cx| f.focus(window, cx))
            });
        }
        let stage = self.stage.as_ref()?;
        Some(self.render_stage(stage, palette, cx))
    }

    fn render_stage(
        &self,
        stage: &CreateStage,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match stage {
            CreateStage::PickKind(form) => {
                let view = cx.entity();
                overlay(form.picker.clone(), palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("triggers-create-kind-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel_create(cx));
                    })
                    .into_any_element()
            }
            CreateStage::Fill(fill) => fill.form.clone().into_any_element(),
        }
    }
}

impl ChoiceHost for TriggerCreateFlow {
    fn feed(&mut self) -> &mut ChoiceFeed {
        &mut self.choice_feed
    }

    fn option_keys(&self, cx: &App) -> Vec<String> {
        match self.stage.as_ref() {
            Some(CreateStage::Fill(fill)) => fill.form.read(cx).option_keys(),
            _ => Vec::new(),
        }
    }

    fn refresh_choices(&mut self, blank: &str, cx: &mut Context<Self>) {
        if let Some(CreateStage::Fill(fill)) = self.stage.as_ref() {
            let options = self.choice_feed.options();
            fill.form
                .update(cx, |form, cx| form.refresh_choices(options, blank, cx));
        }
    }
}

pub(crate) fn build_kind_groups(
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> (Vec<GridPickerGroup>, HashMap<SharedString, String>) {
    let mut groups: Vec<GridPickerGroup> = Vec::new();
    let mut picks: HashMap<SharedString, String> = HashMap::new();

    for slot in kind_slots() {
        let (label, slug, color) = slot.meta(palette);
        let mut descs: Vec<(String, &dyn TriggerKindDescriptor)> = registry
            .all()
            .filter(|d| slot.holds(registry, *d))
            .map(|d| (crate::shared::kind_copy::trigger_name(d), d))
            .collect();
        if descs.is_empty() {
            continue;
        }
        descs.sort_by(|(a_name, a), (b_name, b)| {
            a_name.cmp(b_name).then_with(|| a.id().cmp(b.id()))
        });

        let mut items = Vec::with_capacity(descs.len());
        for (name, d) in descs {
            let id = SharedString::from(format!("kind-{}", d.id()));
            picks.insert(id.clone(), d.id().to_owned());
            items.push(GridPickerItem {
                id,
                glyph: GlyphArt::Icon(Icon::from_name(d.icon_name())),
                tint: color,
                name: name.into(),
                desc: crate::shared::kind_copy::trigger_summary(d).into(),
                state: GridPickerItemState::Normal,
                matches: None,
            });
        }
        groups.push(GridPickerGroup {
            label,
            dot_color: color,
            scope: GridScope::Named(slug),
            items,
        });
    }

    (groups, picks)
}

#[derive(Clone)]
enum KindSlot {
    Category(TriggerCategory),
    Owner(Box<IntegrationDeclaration>),
}

impl KindSlot {
    fn holds(&self, registry: &TriggerRegistry, descriptor: &dyn TriggerKindDescriptor) -> bool {
        let category = descriptor.category();
        let owner = || registry.owning_integration(descriptor.id());
        match self {
            Self::Owner(declaration) => {
                category == TriggerCategory::Integration && owner() == Some(&declaration.id)
            }
            Self::Category(TriggerCategory::Ungrouped) => {
                category == TriggerCategory::Ungrouped
                    || (category == TriggerCategory::Integration
                        && owner().and_then(declaration_of).is_none())
            }
            Self::Category(own) => category == *own,
        }
    }

    fn meta(&self, palette: &ForgePalette) -> (SharedString, SharedString, Rgba) {
        match self {
            Self::Owner(declaration) => (
                catalog_text(declaration.catalog_group.label).into(),
                declaration.id.to_string().into(),
                tint_color(declaration.catalog_group.trigger_tint, palette),
            ),
            Self::Category(category) => {
                let (label, slug, color) = category_meta(*category, palette);
                (label, slug.into(), color)
            }
        }
    }
}

fn kind_slots() -> Vec<KindSlot> {
    CATEGORY_ORDER
        .into_iter()
        .flat_map(|category| match category {
            TriggerCategory::Integration => declarations()
                .map(|declaration| KindSlot::Owner(Box::new(declaration)))
                .to_vec(),
            other => vec![KindSlot::Category(other)],
        })
        .collect()
}

const CATEGORY_ORDER: [TriggerCategory; 20] = [
    TriggerCategory::Chat,
    TriggerCategory::Subscriptions,
    TriggerCategory::Bits,
    TriggerCategory::Raids,
    TriggerCategory::ChannelPoints,
    TriggerCategory::Polls,
    TriggerCategory::Predictions,
    TriggerCategory::Hype,
    TriggerCategory::Charity,
    TriggerCategory::Donations,
    TriggerCategory::Goals,
    TriggerCategory::Clips,
    TriggerCategory::Streams,
    TriggerCategory::Users,
    TriggerCategory::Moderation,
    TriggerCategory::Integration,
    TriggerCategory::Timer,
    TriggerCategory::Server,
    TriggerCategory::Core,
    TriggerCategory::Ungrouped,
];

fn category_meta(
    cat: TriggerCategory,
    palette: &ForgePalette,
) -> (SharedString, &'static str, Rgba) {
    match cat {
        TriggerCategory::Chat => (tr!("trigger_cat_chat").into(), "chat", palette.info),
        TriggerCategory::Subscriptions => (
            tr!("trigger_cat_subscriptions").into(),
            "subs",
            palette.brand,
        ),
        TriggerCategory::Bits => (tr!("trigger_cat_bits").into(), "bits", palette.accent_peach),
        TriggerCategory::Raids => (
            tr!("trigger_cat_raids").into(),
            "raids",
            palette.theme_brand,
        ),
        TriggerCategory::Moderation => (
            tr!("trigger_cat_moderation").into(),
            "moderation",
            palette.danger,
        ),
        TriggerCategory::ChannelPoints => (
            tr!("trigger_cat_channel_points").into(),
            "points",
            palette.accent_pink_light,
        ),
        TriggerCategory::Polls => (tr!("trigger_cat_polls").into(), "polls", palette.warning),
        TriggerCategory::Predictions => (
            tr!("trigger_cat_predictions").into(),
            "predictions",
            palette.warning,
        ),
        TriggerCategory::Hype => (tr!("trigger_cat_hype").into(), "hype", palette.theme_brand),
        TriggerCategory::Charity => (
            tr!("trigger_cat_charity").into(),
            "charity",
            palette.success,
        ),
        TriggerCategory::Donations => (
            tr!("trigger_cat_donations").into(),
            "donations",
            palette.success,
        ),
        TriggerCategory::Goals => (tr!("trigger_cat_goals").into(), "goals", palette.success),
        TriggerCategory::Clips => (tr!("trigger_cat_clips").into(), "clips", palette.info),
        TriggerCategory::Streams => (tr!("trigger_cat_streams").into(), "streams", palette.info),
        TriggerCategory::Users => (tr!("trigger_cat_users").into(), "users", palette.info),
        TriggerCategory::Core => (tr!("trigger_cat_core").into(), "core", palette.info),
        TriggerCategory::Server => (
            tr!("triggers_create_cat_server").into(),
            "server",
            palette.info,
        ),
        TriggerCategory::Timer => (
            tr!("triggers_create_cat_timer").into(),
            "timer",
            palette.warning,
        ),
        TriggerCategory::Ungrouped | TriggerCategory::Integration => {
            (tr!("trigger_cat_other").into(), "other", palette.text_muted)
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_components::Toasts;
    use forge_types::TriggerConfig;
    use gpui::TestAppContext;

    use super::super::detail::tests::{
        Fixture, GREETING_FIELD, KIND, REFUSED_GREETING, refusing_registry,
    };
    use super::*;
    use crate::automation::triggers_screen::TriggersRegistryView;
    use crate::shared::config_form::ConfigField;
    use crate::shared::picker_favorites::TRIGGER_FAVORITES;
    use crate::test_support::{
        StubActions, error_toasts, owned_triggers, pump_until, runtime, test_backend,
    };
    use forge_components::{Density, Presentation, ThemeId};
    use forge_storage::MockTriggerInstanceRepo;

    const TWITCH_CHAT: &str = "twitch.chat";

    fn fill_form(cx: &mut TestAppContext, greeting: &str) -> Fixture {
        crate::app::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let fixture = Fixture::build(
            cx,
            refusing_registry(TriggerConfig::new()),
            KIND,
            TriggerConfig::new(),
            BuiltinRegistry::default(),
        );
        fixture.flow().update(cx, |view, cx| {
            view.enter_fill(KIND.to_owned(), cx);
            let Some(CreateStage::Fill(CreateFillForm { form, .. })) = view.stage.as_ref() else {
                panic!("the fill stage must be open");
            };
            let form = form.read(cx);
            let name = form.name_field.clone();
            let field = form
                .fields
                .iter()
                .find_map(|field| match field {
                    ConfigField::Input { key, input, .. } if key == GREETING_FIELD => {
                        Some(input.clone())
                    }
                    _ => None,
                })
                .unwrap();
            name.update(cx, |input, cx| input.set_content("New".to_owned(), cx));
            field.update(cx, |input, cx| input.set_content(greeting.to_owned(), cx));
        });
        fixture
    }

    #[gpui::test]
    fn creating_with_a_value_the_kind_refuses_saves_nothing_and_stays_on_the_form(
        cx: &mut TestAppContext,
    ) {
        let mut fixture = fill_form(cx, REFUSED_GREETING);

        fixture.flow().update(cx, |view, cx| view.submit_create(cx));
        fixture.settle(cx);

        let still_filling = fixture.flow().read_with(
            cx,
            |view, cx| matches!(&view.stage, Some(CreateStage::Fill(fill)) if !fill.form.read(cx).saving),
        );
        assert_eq!(
            (
                fixture.repo.last_saved_overrides(),
                still_filling,
                cx.update(|cx| error_toasts(cx)),
            ),
            (None, true, 1)
        );
    }

    #[gpui::test]
    fn creating_with_a_value_the_kind_accepts_saves_it(cx: &mut TestAppContext) {
        let mut fixture = fill_form(cx, "accepted");

        fixture.flow().update(cx, |view, cx| view.submit_create(cx));
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture.repo.last_saved_overrides().is_some()
        });
        fixture.settle(cx);

        assert_eq!(
            fixture
                .repo
                .last_saved_overrides()
                .and_then(|saved| saved.get(GREETING_FIELD).cloned()),
            Some(forge_types::Variant::String("accepted".to_owned()))
        );
    }

    #[gpui::test]
    fn a_created_trigger_takes_the_cursor_with_the_selection(cx: &mut TestAppContext) {
        let mut fixture = fill_form(cx, "accepted");

        fixture.flow().update(cx, |view, cx| view.submit_create(cx));
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture.flow().read_with(cx, |view, _| view.stage.is_none())
        });
        fixture.settle(cx);

        let (selected, cursor) = fixture
            .list()
            .read_with(cx, |list, _| (list.selected, list.list_cursor));
        assert_eq!(cursor, selected);
    }

    #[gpui::test]
    fn the_create_picker_stars_into_the_app_wide_trigger_favorites(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
        let rt = runtime();
        let mut repo = MockTriggerInstanceRepo::new();
        repo.expect_list_all().returning(|| Ok(Vec::new()));
        let (settings, _writes) = test_backend();
        let handle = rt.handle().clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                TriggersRegistryView::new(
                    Arc::new(repo),
                    Arc::new(StubActions),
                    Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
                    settings.settings_repo(),
                    handle.clone(),
                    None,
                    cx,
                )
            })
        });

        let app_wide = cx.update(|cx| {
            StoredFavorites::shared(TRIGGER_FAVORITES, settings.settings_repo(), &handle, cx)
                .entity_id()
        });

        let flow = view.read_with(cx, |view, _| view.create.clone());
        assert_eq!(
            flow.read_with(cx, |flow, _| flow.favorites.entity_id()),
            app_wide
        );
    }

    fn successor(category: TriggerCategory) -> Option<TriggerCategory> {
        use TriggerCategory as C;
        match category {
            C::Chat => Some(C::Subscriptions),
            C::Subscriptions => Some(C::Bits),
            C::Bits => Some(C::Raids),
            C::Raids => Some(C::Moderation),
            C::Moderation => Some(C::ChannelPoints),
            C::ChannelPoints => Some(C::Polls),
            C::Polls => Some(C::Predictions),
            C::Predictions => Some(C::Hype),
            C::Hype => Some(C::Charity),
            C::Charity => Some(C::Donations),
            C::Donations => Some(C::Goals),
            C::Goals => Some(C::Clips),
            C::Clips => Some(C::Streams),
            C::Streams => Some(C::Users),
            C::Users => Some(C::Integration),
            C::Integration => Some(C::Core),
            C::Core => Some(C::Server),
            C::Server => Some(C::Timer),
            C::Timer => Some(C::Ungrouped),
            C::Ungrouped => None,
        }
    }

    #[test]
    fn every_trigger_category_has_a_place_in_the_kind_picker() {
        let every_category: Vec<TriggerCategory> =
            std::iter::successors(Some(TriggerCategory::Chat), |c| successor(*c)).collect();

        let missing: Vec<TriggerCategory> = every_category
            .iter()
            .copied()
            .filter(|category| !CATEGORY_ORDER.contains(category))
            .collect();

        assert_eq!(
            (missing, CATEGORY_ORDER.len()),
            (Vec::new(), every_category.len())
        );
    }
}
