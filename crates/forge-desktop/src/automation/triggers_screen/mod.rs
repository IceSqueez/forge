use crate::app::async_bridge::{KeyedSnapshotWriters, Loadable, WriteOutcome};
use crate::app::navigation::NavRequested;
use crate::app::navigation::Screen;
use crate::automation::actions_screen::config_problem_text::config_problem_text;
use crate::integrations::integration_catalog::{catalog_text, tint_color};
use crate::integrations::integration_switch::{IntegrationSwitch, SwitchWatch};
use crate::shared::config_form::{CollectionChoices, ConfigField};
use crate::shared::picker_favorites::{StoredFavorites, TRIGGER_FAVORITES};
use crate::shared::repo_failure::RepoFailure;
use forge_components::FONT_BADGE;
use forge_components::FONT_META;
use forge_components::FONT_SM;
use forge_components::FONT_XS;
use forge_components::PushToast;
use forge_components::drive_overlay_focus;
use forge_components::{
    ActivePresentation, BORDER_THIN, BreadcrumbCrumb, Confirm, ForgePalette, Icon, InlineEdit,
    SearchState, TextInput, ToastKind, page_frame, primary_button_with_icon, tr,
};
use forge_host::integration_catalog::{declaration_of, declarations};
use forge_host::integrations::BuiltinRegistry;
use forge_platform_core::IntegrationDeclaration;
use forge_registry::TriggerRegistry;
use forge_storage::{ActionRepo, SettingsRepo, TriggerInstanceRepo};
use forge_types::{
    ActionId, IntegrationId, PermissionRung, TriggerConfig, TriggerInstance, TriggerInstanceId,
};
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FocusHandle, Pixels, Rgba, SharedString,
    Subscription, UniformListScrollHandle, Window, div, prelude::*, px,
};
use std::future::Future;
use std::sync::Arc;

mod choice_feed;
mod create;
mod detail;
mod list;
mod permission_help;
mod shortcuts;
mod timer;

use choice_feed::{ChoiceFeed, ChoiceHost, start_option_choices};
pub(crate) use create::build_kind_groups;
use create::{TriggerCreateEvent, TriggerCreateFlow};
use detail::{TriggerDetailPane, TriggerPaneEvent};
use list::{ListedRow, TriggerList, TriggerListEvent};
pub(crate) use timer::{condition_display, cooldown_applies, timer_field_hints};

const STRIPE_W: Pixels = px(2.0);
const ROW_PAD_L: Pixels = px(16.0);
const ROW_PAD_R: Pixels = px(18.0);
const ROW_PAD_V: Pixels = px(4.0);
const CAPTION_PAD_H: Pixels = px(18.0);
const CAPTION_PAD_V: Pixels = px(7.0);
const COL_DOT: Pixels = px(24.0);
const COL_NAME: Pixels = px(220.0);
const COL_USED: Pixels = px(110.0);
const COL_ON: Pixels = px(36.0);
const COL_MENU: Pixels = px(32.0);
const ROW_DOT: Pixels = px(7.0);
const KIND_GLYPH: Pixels = px(11.0);
const NAME_FS: Pixels = FONT_META;
const KIND_FS: Pixels = FONT_META;
const USED_FS: Pixels = FONT_META;
const BADGE_FS: Pixels = FONT_BADGE;
const FILTER_DIV_W: Pixels = BORDER_THIN;
const FILTER_DIV_H: Pixels = px(16.0);
const USED_CELL_GAP: Pixels = px(10.0);
const EMPTY_PAD_V: Pixels = px(60.0);
const EMPTY_PAD_H: Pixels = px(20.0);
const EMPTY_TILE: Pixels = px(48.0);
const EMPTY_TILE_RADIUS: Pixels = px(10.0);
const EMPTY_GLYPH: Pixels = px(22.0);
const EMPTY_TITLE_FS: Pixels = FONT_SM;
const EMPTY_BODY_FS: Pixels = FONT_XS;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Platform {
    Owned(Box<IntegrationDeclaration>),
    Donation,
    Timer,
    Script,
    Core,
}

impl Platform {
    fn owned(declaration: IntegrationDeclaration) -> Platform {
        Platform::Owned(Box::new(declaration))
    }

    fn ordered() -> Vec<Platform> {
        let mut ordered: Vec<Platform> = declarations().map(Platform::owned).to_vec();
        ordered.extend([
            Platform::Donation,
            Platform::Timer,
            Platform::Script,
            Platform::Core,
        ]);
        ordered
    }

    fn from_kind_id(kind_id: &str, registry: &TriggerRegistry) -> Option<Platform> {
        if let Some(declaration) = registry
            .owning_integration(kind_id)
            .and_then(declaration_of)
        {
            return Some(Platform::owned(declaration));
        }
        let prefix = kind_id.split('.').next().unwrap_or("");
        match prefix {
            "donation" => Some(Platform::Donation),
            "timer" => Some(Platform::Timer),
            "script" | "rhai" => Some(Platform::Script),
            "core" => Some(Platform::Core),
            _ => declaration_of(&IntegrationId::new(prefix)).map(Platform::owned),
        }
    }

    fn element_key(&self) -> String {
        match self {
            Platform::Owned(declaration) => declaration.id.to_string(),
            Platform::Donation => "donation".to_owned(),
            Platform::Timer => "timer".to_owned(),
            Platform::Script => "script".to_owned(),
            Platform::Core => "core".to_owned(),
        }
    }

    fn display(&self) -> String {
        match self {
            Platform::Owned(declaration) => catalog_text(declaration.catalog_group.filter_label),
            Platform::Donation => tr!("trigger_cat_donations"),
            Platform::Timer => tr!("triggers_platform_timer"),
            Platform::Script => tr!("triggers_platform_script"),
            Platform::Core => tr!("triggers_platform_core"),
        }
    }

    fn dot(&self, palette: &ForgePalette) -> Rgba {
        match self {
            Platform::Owned(declaration) => {
                tint_color(declaration.catalog_group.trigger_tint, palette)
            }
            Platform::Donation => palette.success,
            Platform::Timer => palette.warning,
            Platform::Script => palette.accent_peach,
            Platform::Core => palette.info,
        }
    }
}

pub(crate) fn platform_dot_color(
    kind_id: &str,
    registry: &TriggerRegistry,
    palette: &ForgePalette,
) -> Rgba {
    Platform::from_kind_id(kind_id, registry)
        .map(|p| p.dot(palette))
        .unwrap_or(palette.info)
}

#[derive(Clone)]
struct TriggerInstanceRow {
    id: TriggerInstanceId,
    name: String,
    kind_id: String,
    enabled: bool,
    used_in_count: usize,
    override_count: usize,
    overrides: TriggerConfig,
    cooldown_secs: u32,
    cooldown_global: bool,
    permission_rung: PermissionRung,
}

struct TriggerDetail {
    instance: TriggerInstance,
    fields: Vec<ConfigField>,
    labels: Vec<SharedString>,
    used_in: Vec<(ActionId, String)>,
    cooldown_input: Entity<TextInput>,
    cooldown_per_user: bool,
    permission_rung: PermissionRung,
    choices: CollectionChoices,
    _cooldown_sub: Subscription,
}

pub(crate) fn cooldown_suffix(secs: u32, global: bool) -> String {
    if global {
        tr!("triggers_cooldown_suffix_global", secs = secs)
    } else {
        tr!("triggers_cooldown_suffix_per_user", secs = secs)
    }
}

pub(crate) const PERMISSION_RUNGS: [PermissionRung; 5] = [
    PermissionRung::Everyone,
    PermissionRung::Subscriber,
    PermissionRung::Vip,
    PermissionRung::Moderator,
    PermissionRung::Broadcaster,
];

pub(crate) fn permission_rung_label(rung: PermissionRung) -> String {
    match rung {
        PermissionRung::Everyone => tr!("triggers_permission_rung_everyone"),
        PermissionRung::Subscriber => tr!("triggers_permission_rung_subscriber"),
        PermissionRung::Vip => tr!("triggers_permission_rung_vip"),
        PermissionRung::Moderator => tr!("triggers_permission_rung_moderator"),
        PermissionRung::Broadcaster => tr!("triggers_permission_rung_broadcaster"),
    }
}

pub(crate) fn permission_suffix(rung: PermissionRung) -> Option<String> {
    (rung > PermissionRung::Everyone).then(|| {
        let label = permission_rung_label(rung);
        tr!("triggers_permission_suffix", rung = label.as_str())
    })
}

struct TriggerDetailData {
    instance: TriggerInstance,
    used_in: Vec<(ActionId, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UsageFilter {
    All,
    Used,
    Unused,
}

struct RenameForm {
    id: TriggerInstanceId,
    editor: Entity<InlineEdit>,
    _sub: Subscription,
}

#[derive(Clone)]
pub(super) struct TriggerScreenDeps {
    repo: Arc<dyn TriggerInstanceRepo>,
    action_repo: Arc<dyn ActionRepo>,
    registry: Arc<TriggerRegistry>,
    rt_handle: tokio::runtime::Handle,
}

pub(crate) struct TriggersRegistryView {
    list: Entity<TriggerList>,
    pane: Entity<TriggerDetailPane>,
    create: Entity<TriggerCreateFlow>,
    _children: [Subscription; 3],
}

impl TriggersRegistryView {
    pub(crate) fn new(
        repo: Arc<dyn TriggerInstanceRepo>,
        action_repo: Arc<dyn ActionRepo>,
        registry: Arc<TriggerRegistry>,
        settings_repo: Arc<dyn SettingsRepo>,
        rt_handle: tokio::runtime::Handle,
        preselect: Option<TriggerInstanceId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let favorites = StoredFavorites::shared(
            TRIGGER_FAVORITES,
            Arc::clone(&settings_repo),
            &rt_handle,
            cx,
        );
        let deps = TriggerScreenDeps {
            repo,
            action_repo,
            registry,
            rt_handle,
        };
        let list = cx.new(|cx| TriggerList::new(deps.clone(), preselect, cx));
        let pane = cx.new(|cx| TriggerDetailPane::new(deps.clone(), preselect, cx));
        let create = cx.new(|_| TriggerCreateFlow::new(deps, favorites));
        let children = [
            cx.subscribe(&list, Self::on_list_event),
            cx.subscribe(&pane, Self::on_pane_event),
            cx.subscribe(&create, Self::on_create_event),
        ];
        Self {
            list,
            pane,
            create,
            _children: children,
        }
    }

    #[must_use]
    pub(crate) fn with_builtins(self, builtins: BuiltinRegistry, cx: &mut Context<Self>) -> Self {
        self.create
            .update(cx, |flow, _| flow.set_builtins(builtins.clone()));
        self.pane.update(cx, |pane, _| pane.set_builtins(builtins));
        self
    }

    #[must_use]
    pub(crate) fn with_integration_switch(
        self,
        switch: IntegrationSwitch,
        cx: &mut Context<Self>,
    ) -> Self {
        self.pane
            .update(cx, |pane, cx| pane.attach_switch(switch.clone(), cx));
        self.list
            .update(cx, |list, cx| list.attach_switch(switch, cx));
        self
    }

    fn on_list_event(
        &mut self,
        _list: Entity<TriggerList>,
        event: &TriggerListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TriggerListEvent::Selected(row) => {
                self.pane.update(cx, |pane, cx| pane.open_row(row, cx));
            }
            TriggerListEvent::SelectedListed(row) => {
                self.pane.update(cx, |pane, cx| pane.row_listed(row, cx));
            }
            TriggerListEvent::SelectionLost => {
                self.pane.update(cx, |pane, cx| pane.clear_selection(cx));
            }
            TriggerListEvent::EnabledShown(id, enabled) => {
                self.pane
                    .update(cx, |pane, cx| pane.show_enabled(*id, *enabled, cx));
            }
            TriggerListEvent::OpenCreate => self.open_create(cx),
        }
    }

    fn on_pane_event(
        &mut self,
        _pane: Entity<TriggerDetailPane>,
        event: &TriggerPaneEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TriggerPaneEvent::Closed => {
                self.list.update(cx, |list, cx| list.clear_selection(cx));
            }
            TriggerPaneEvent::RowsChanged => {
                self.list.update(cx, |list, cx| list.reload(cx));
            }
            TriggerPaneEvent::ToggleEnable(id) => {
                self.list.update(cx, |list, cx| list.toggle_enable(*id, cx));
            }
            TriggerPaneEvent::RequestDelete(id) => {
                self.list
                    .update(cx, |list, cx| list.request_delete(*id, cx));
            }
            TriggerPaneEvent::StartRename(id) => {
                self.list
                    .update(cx, |list, cx| list.request_rename(*id, cx));
            }
            TriggerPaneEvent::OpenAction(id) => {
                cx.emit(NavRequested(Screen::Actions(Some(*id))));
            }
            TriggerPaneEvent::Duplicated { id, rows } => {
                self.list
                    .update(cx, |list, cx| list.land_created(*id, rows.clone(), cx));
            }
        }
    }

    fn on_create_event(
        &mut self,
        _flow: Entity<TriggerCreateFlow>,
        event: &TriggerCreateEvent,
        cx: &mut Context<Self>,
    ) {
        let TriggerCreateEvent::Created { id, rows } = event;
        self.list
            .update(cx, |list, cx| list.land_created(*id, rows.clone(), cx));
    }

    fn open_create(&mut self, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.close_menu(cx));
        self.create.update(cx, |flow, cx| flow.open(cx));
    }
}

#[cfg(test)]
type WeakParts = (
    gpui::WeakEntity<TriggerDetailPane>,
    gpui::WeakEntity<TriggerList>,
    gpui::WeakEntity<TriggerCreateFlow>,
);

#[cfg(test)]
impl TriggersRegistryView {
    fn parts(&self) -> WeakParts {
        (
            self.pane.downgrade(),
            self.list.downgrade(),
            self.create.downgrade(),
        )
    }
}

fn switched_off_owner(
    integrations: Option<&SwitchWatch>,
    registry: &TriggerRegistry,
    kind_id: &str,
) -> Option<IntegrationId> {
    integrations?
        .off_owner(registry.owning_integration(kind_id))
        .cloned()
}

pub(crate) fn refuse_invalid_config<V: 'static>(
    registry: &TriggerRegistry,
    kind_id: &str,
    config: &TriggerConfig,
    cx: &mut Context<V>,
) -> bool {
    let Some(descriptor) = registry.get(kind_id) else {
        return false;
    };
    let Err(problem) = descriptor.check_config(config) else {
        return false;
    };
    let message = config_problem_text(&problem, kind_id, &descriptor.config_fields());
    cx.push_toast(
        ToastKind::Error,
        tr!("triggers_toast_error", message = message.as_str()),
    );
    true
}

fn on_repo_error<V: 'static>(failure: &RepoFailure, cx: &mut Context<V>) {
    tracing::error!(error = %failure, "triggers operation failed");
    let message = failure.text();
    cx.push_toast(
        ToastKind::Error,
        tr!("triggers_toast_error", message = message.as_str()),
    );
    cx.notify();
}

impl EventEmitter<NavRequested> for TriggersRegistryView {}

impl Render for TriggersRegistryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();

        let stats = self.list.read(cx).render_stats(&palette, density, cx);
        let filter_left = self
            .list
            .update(cx, |list, cx| list.render_filter_left(&palette, cx));
        let filter_right =
            primary_button_with_icon(Icon::Plus, tr!("triggers_new_trigger"), &palette).on_click(
                "triggers-new",
                cx.listener(|this, _: &ClickEvent, _, cx| this.open_create(cx)),
            );

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(self.list.clone())
            .child(self.pane.clone());

        let list_layers = self
            .list
            .update(cx, |list, cx| list.render_modal_layers(&palette, cx));
        let create_overlay = self
            .create
            .update(cx, |flow, cx| flow.render_layer(&palette, window, cx));

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("triggers_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("triggers_breadcrumb_triggers")),
            ],
            &palette,
        )
        .header_right(stats)
        .subheader_left(filter_left)
        .subheader_right(filter_right)
        .density(density)
        .body(body);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(list_layers)
            .children(create_overlay)
    }
}

async fn save_then_list(
    repo: &dyn TriggerInstanceRepo,
    instance: &TriggerInstance,
) -> Result<Vec<TriggerInstanceRow>, RepoFailure> {
    repo.save(instance).await.map_err(RepoFailure::from)?;
    load_rows(repo).await
}

async fn load_rows(repo: &dyn TriggerInstanceRepo) -> Result<Vec<TriggerInstanceRow>, RepoFailure> {
    let instances = repo.list_user_defined().await.map_err(RepoFailure::from)?;
    let links = repo
        .actions_by_instance()
        .await
        .map_err(RepoFailure::from)?;
    let mut rows = Vec::with_capacity(instances.len());
    for instance in instances {
        let used_in_count = links.get(&instance.id).map_or(0, Vec::len);
        rows.push(TriggerInstanceRow {
            id: instance.id,
            name: instance.name,
            kind_id: instance.kind_id,
            enabled: instance.enabled,
            used_in_count,
            override_count: instance.overrides.len(),
            overrides: instance.overrides,
            cooldown_secs: instance.cooldown_secs,
            cooldown_global: instance.cooldown_global,
            permission_rung: instance.permission_rung,
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use forge_storage::MockTriggerInstanceRepo;
    use forge_types::IntegrationId;

    use super::*;
    use crate::test_support::runtime;

    const TWITCH_CHAT: &str = "twitch.chat";

    fn owned(id: &str) -> Option<Platform> {
        declaration_of(&IntegrationId::new(id)).map(Platform::owned)
    }

    #[test]
    fn a_kind_id_outside_the_registry_maps_to_its_source_chip_by_prefix() {
        let registry = TriggerRegistry::new();
        for (kind_id, expected) in [
            ("twitch.chat", owned("twitch")),
            ("youtube.super_chat", owned("youtube")),
            ("kick.chat", owned("kick")),
            ("obs.scene_changed", owned("obs")),
            ("vtube.model_loaded", owned("vtube")),
            ("midi.note_on", owned("midi")),
            ("hotkey.pressed", owned("hotkey")),
            ("donation.received", Some(Platform::Donation)),
            ("timer.tick", Some(Platform::Timer)),
            ("script.exec", Some(Platform::Script)),
            ("rhai.exec", Some(Platform::Script)),
            ("core.startup", Some(Platform::Core)),
            ("twitch", owned("twitch")),
            ("Twitch.chat", None),
            ("", None),
        ] {
            assert_eq!(
                Platform::from_kind_id(kind_id, &registry),
                expected,
                "{kind_id:?}"
            );
        }
    }

    #[test]
    fn a_registered_kind_follows_its_owner_not_its_prefix() {
        const ELSEWHERE: &str = "elsewhere.changed";
        let mut registry = TriggerRegistry::new();
        registry
            .owned_by(forge_obs::OBS_INTEGRATION.id)
            .register(Box::new(crate::test_support::StubTrigger::new(
                ELSEWHERE,
                ELSEWHERE,
                forge_registry::TriggerCategory::Integration,
                crate::test_support::Declares::Nothing,
            )))
            .unwrap();

        assert_eq!(Platform::from_kind_id(ELSEWHERE, &registry), owned("obs"));
    }

    #[tokio::test]
    async fn owned_kinds_group_after_moderation_in_declaration_order() {
        crate::app::i18n::install_language(forge_storage::Language::En);
        let (triggers, _) = crate::test_support::every_registry_entry().await;

        let (groups, _) = build_kind_groups(&triggers, &forge_components::FORGE_DEFAULT);

        let scopes: Vec<Option<&str>> = groups.iter().map(|g| g.scope.name()).collect();
        let start = scopes
            .iter()
            .position(|s| *s == Some("moderation"))
            .unwrap();
        let labels: Vec<(String, String)> = groups[start + 1..]
            .iter()
            .map(|g| {
                (
                    g.scope.name().unwrap_or_default().to_owned(),
                    g.label.to_string(),
                )
            })
            .collect();
        assert_eq!(
            labels,
            [
                ("obs", "OBS"),
                ("vtube", "VTube Studio"),
                ("midi", "MIDI"),
                ("hotkey", "Hotkey"),
                ("timer", "Timer"),
                ("core", "Core"),
            ]
            .map(|(scope, label)| (scope.to_owned(), label.to_owned()))
        );
    }

    #[test]
    fn the_kind_picker_shows_each_registered_category_in_the_canonical_order() {
        let mut registry = forge_registry::TriggerRegistry::new();
        for (id, category) in [
            ("core.startup", forge_registry::TriggerCategory::Core),
            ("twitch.chat", forge_registry::TriggerCategory::Chat),
            ("twitch.ban", forge_registry::TriggerCategory::Moderation),
        ] {
            registry
                .register(Box::new(crate::test_support::StubTrigger::new(
                    id,
                    id,
                    category,
                    crate::test_support::Declares::Nothing,
                )))
                .expect("every stub carries its own id");
        }

        let (groups, _) = build_kind_groups(&registry, &forge_components::FORGE_DEFAULT);

        assert_eq!(
            groups
                .iter()
                .map(|group| group.items[0].id.to_string())
                .collect::<Vec<_>>(),
            ["kind-twitch.chat", "kind-twitch.ban", "kind-core.startup"]
        );
    }

    #[test]
    fn every_registered_trigger_kind_has_a_source_chip() {
        let mut registry = forge_registry::TriggerRegistry::new();
        forge_runtime::register_core_triggers(&mut registry).unwrap();
        forge_platform_twitch::register_twitch_triggers(&mut registry).unwrap();
        forge_obs::register_obs_triggers(&mut registry).unwrap();
        forge_vtube::register_vtube_triggers(&mut registry).unwrap();
        forge_midi::register_midi_triggers(&mut registry).unwrap();
        forge_hotkey::register_hotkey_triggers(&mut registry).unwrap();
        forge_platform_youtube::register_youtube_triggers(&mut registry).unwrap();
        forge_platform_kick::register_kick_triggers(&mut registry).unwrap();
        forge_monobank::register_monobank_triggers(&mut registry).unwrap();

        let chipless: Vec<&str> = registry
            .all()
            .map(|descriptor| descriptor.id())
            .filter(|kind_id| Platform::from_kind_id(kind_id, &registry).is_none())
            .collect();

        assert!(chipless.is_empty(), "kinds without a chip: {chipless:?}");
    }

    #[tokio::test]
    async fn every_registered_trigger_kind_names_an_icon_this_build_carries() {
        let (triggers, _) = forge_host::test_support::every_registry_entry().await;

        let unknown: Vec<String> = triggers
            .all()
            .filter(|descriptor| forge_components::Icon::named(descriptor.icon_name()).is_none())
            .map(|descriptor| format!("{}: {}", descriptor.id(), descriptor.icon_name()))
            .collect();

        assert!(triggers.all().count() > 0);
        assert_eq!(unknown, Vec::<String>::new());
    }

    #[tokio::test]
    async fn every_registered_sub_action_kind_names_an_icon_this_build_carries() {
        let (_, sub_actions) = forge_host::test_support::every_registry_entry().await;

        let unknown: Vec<String> = sub_actions
            .all()
            .filter(|runner| forge_components::Icon::named(runner.icon_name()).is_none())
            .map(|runner| format!("{}: {}", runner.id(), runner.icon_name()))
            .collect();

        assert!(sub_actions.all().count() > 0);
        assert_eq!(unknown, Vec::<String>::new());
    }

    #[test]
    fn every_registered_trigger_kind_can_be_picked_when_creating_a_trigger() {
        let mut registry = forge_registry::TriggerRegistry::new();
        forge_runtime::register_core_triggers(&mut registry).unwrap();
        forge_platform_twitch::register_twitch_triggers(&mut registry).unwrap();
        forge_platform_kick::register_kick_triggers(&mut registry).unwrap();
        forge_platform_youtube::register_youtube_triggers(&mut registry).unwrap();
        forge_obs::register_obs_triggers(&mut registry).unwrap();
        forge_vtube::register_vtube_triggers(&mut registry).unwrap();
        forge_midi::register_midi_triggers(&mut registry).unwrap();
        forge_monobank::register_monobank_triggers(&mut registry).unwrap();

        let (_, picks) = build_kind_groups(&registry, &forge_components::FORGE_DEFAULT);

        let unreachable: Vec<&str> = registry
            .all()
            .map(|descriptor| descriptor.id())
            .filter(|id| !picks.values().any(|picked| picked == id))
            .collect();
        assert_eq!(unreachable, Vec::<&str>::new());
    }

    fn listed_instance() -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: TWITCH_CHAT.to_owned(),
            name: "listed".to_owned(),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    #[test]
    fn a_link_read_failure_reaches_the_screen_instead_of_showing_zero_uses() {
        let instance = listed_instance();
        let mut repo = MockTriggerInstanceRepo::new();
        repo.expect_list_user_defined()
            .returning(move || Ok(vec![instance.clone()]));
        repo.expect_actions_by_instance().returning(|| {
            Err(forge_storage::StorageError::Connection {
                reason: "store is gone".to_owned(),
            })
        });

        let loaded = runtime().block_on(load_rows(&repo));

        assert!(loaded.is_err());
    }

    #[test]
    fn rows_count_the_links_of_their_own_instance_only() {
        let (linked, unlinked) = (listed_instance(), listed_instance());
        let actions = vec![forge_types::ActionId::new(), forge_types::ActionId::new()];
        let links = std::collections::HashMap::from([(linked.id, actions)]);
        let listed = vec![linked.clone(), unlinked.clone()];
        let mut repo = MockTriggerInstanceRepo::new();
        repo.expect_list_user_defined()
            .returning(move || Ok(listed.clone()));
        repo.expect_actions_by_instance()
            .returning(move || Ok(links.clone()));

        let rows = runtime().block_on(load_rows(&repo)).unwrap();

        let counts: Vec<(TriggerInstanceId, usize)> =
            rows.iter().map(|row| (row.id, row.used_in_count)).collect();
        assert_eq!(counts, [(linked.id, 2), (unlinked.id, 0)]);
    }
}
