use crate::platform_visuals::platform_color;
use crate::repo_failure::RepoFailure;
use forge_components::FONT_BADGE;
use forge_components::FONT_META;
use forge_components::FONT_SM;
use forge_components::FONT_XS;
use forge_components::drive_overlay_focus;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Confirm, ForgePalette, InlineEdit, SearchState, TextInput,
    ToastKind, page_frame, tr,
};
use forge_registry::TriggerRegistry;
use forge_runtime::EventBus;
use forge_storage::{ActionRepo, SettingsRepo, TriggerInstanceRepo};
use forge_types::{
    ActionId, PermissionRung, PlatformId, TriggerConfig, TriggerInstance, TriggerInstanceId,
};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Pixels, Point, Rgba, SharedString,
    Subscription, UniformListScrollHandle, Window, div, prelude::*, px,
};
use std::future::Future;
use std::sync::Arc;

use crate::actions_screen::config_problem_text::config_problem_text;
use crate::async_bridge::Generation;
use crate::async_bridge::{self, KeyedSnapshotWriters, WriteOutcome};
use crate::collection_options::ChoiceOptions;
use crate::config_form::{CollectionChoices, ConfigField};
use crate::integration_switch::{IntegrationSwitch, SwitchWatch};
use crate::integrations::BuiltinRegistry;
use crate::navigable_list::navigable_list;
use crate::navigation::NavRequested;
use crate::navigation::Screen;
use crate::picker_favorites::{StoredFavorites, TRIGGER_FAVORITES};
use forge_components::ActivePresentation;
use forge_components::PushToast;

mod collection_choices;
mod create;
mod detail;
mod list;
mod permission_help;
mod shortcuts;
mod timer;

use create::CreateStage;
pub(crate) use create::build_kind_groups;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Platform {
    Twitch,
    Youtube,
    Kick,
    Obs,
    Vtube,
    Midi,
    Hotkey,
    Donation,
    Timer,
    Script,
    Core,
}

impl Platform {
    const ORDER: [Platform; 11] = [
        Platform::Twitch,
        Platform::Youtube,
        Platform::Kick,
        Platform::Obs,
        Platform::Vtube,
        Platform::Midi,
        Platform::Hotkey,
        Platform::Donation,
        Platform::Timer,
        Platform::Script,
        Platform::Core,
    ];

    fn from_kind_id(kind_id: &str) -> Option<Platform> {
        match kind_id.split('.').next().unwrap_or("") {
            "twitch" => Some(Platform::Twitch),
            "youtube" => Some(Platform::Youtube),
            "kick" => Some(Platform::Kick),
            "obs" => Some(Platform::Obs),
            "vtube" => Some(Platform::Vtube),
            "midi" => Some(Platform::Midi),
            "hotkey" => Some(Platform::Hotkey),
            "donation" => Some(Platform::Donation),
            "timer" => Some(Platform::Timer),
            "script" | "rhai" => Some(Platform::Script),
            "core" => Some(Platform::Core),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Platform::Twitch => "Twitch",
            Platform::Youtube => "YouTube",
            Platform::Kick => "Kick",
            Platform::Obs => "OBS",
            Platform::Vtube => "VTube",
            Platform::Midi => "MIDI",
            Platform::Hotkey => "Hotkey",
            Platform::Donation => "Donation",
            Platform::Timer => "Timer",
            Platform::Script => "Script",
            Platform::Core => "Core",
        }
    }

    fn display(self) -> String {
        match self {
            Platform::Twitch => "Twitch".to_owned(),
            Platform::Youtube => "YouTube".to_owned(),
            Platform::Kick => "Kick".to_owned(),
            Platform::Obs => "OBS".to_owned(),
            Platform::Vtube => "VTube".to_owned(),
            Platform::Midi => "MIDI".to_owned(),
            Platform::Hotkey => tr!("triggers_filter_hotkey"),
            Platform::Donation => tr!("trigger_cat_donations"),
            Platform::Timer => tr!("triggers_platform_timer"),
            Platform::Script => tr!("triggers_platform_script"),
            Platform::Core => tr!("triggers_platform_core"),
        }
    }

    fn dot(self, palette: &ForgePalette) -> Rgba {
        match self {
            Platform::Twitch => platform_color(PlatformId::Twitch, palette),
            Platform::Youtube => platform_color(PlatformId::YouTube, palette),
            Platform::Kick => platform_color(PlatformId::Kick, palette),
            Platform::Obs | Platform::Vtube => palette.accent_teal,
            Platform::Midi => palette.danger,
            Platform::Hotkey => palette.warning,
            Platform::Donation => palette.success,
            Platform::Timer => palette.warning,
            Platform::Script => palette.accent_peach,
            Platform::Core => palette.info,
        }
    }
}

pub(crate) fn platform_dot_color(kind_id: &str, palette: &ForgePalette) -> Rgba {
    Platform::from_kind_id(kind_id)
        .map(|p| p.dot(palette))
        .unwrap_or(palette.info)
}

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
        tr!("triggers_cooldown_suffix_global", secs = secs as i64)
    } else {
        tr!("triggers_cooldown_suffix_per_user", secs = secs as i64)
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

pub(crate) struct TriggersRegistryView {
    repo: Arc<dyn TriggerInstanceRepo>,
    action_repo: Arc<dyn ActionRepo>,
    registry: Arc<TriggerRegistry>,
    favorites: Entity<StoredFavorites>,
    rt_handle: tokio::runtime::Handle,
    detail_width: Pixels,
    loading: bool,
    reload_generation: async_bridge::Generation,
    detail_writers: KeyedSnapshotWriters<TriggerInstanceId>,
    enabled_writers: KeyedSnapshotWriters<TriggerInstanceId>,
    instances: Vec<TriggerInstanceRow>,
    visible: Vec<usize>,
    list_scroll: UniformListScrollHandle,
    list_focus: FocusHandle,
    focused_once: bool,
    list_cursor: Option<TriggerInstanceId>,
    selected: Option<TriggerInstanceId>,
    detail: Option<TriggerDetail>,
    hovered: Option<TriggerInstanceId>,
    menu_open: Option<TriggerInstanceId>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
    menu_click_pos: Option<Point<Pixels>>,
    search: SearchState,
    platforms: Vec<Platform>,
    usage_filter: UsageFilter,
    rename: Option<RenameForm>,
    pending_delete: Confirm<TriggerInstanceId>,
    confirm_disable: Confirm<TriggerInstanceId>,
    create: Option<CreateStage>,
    builtins: BuiltinRegistry,
    bus: Option<Arc<EventBus>>,
    integrations: Option<SwitchWatch>,
    collection_options: ChoiceOptions,
    collection_options_gen: Generation,
    obs_catalog_options_gen: Generation,
    _collection_watch: Vec<gpui::Task<()>>,
    _search_sub: Subscription,
    _release: Subscription,
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
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("triggers_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let release = cx.on_release(|this, cx| this.persist_detail_on_release(cx));
        let favorites = StoredFavorites::shared(
            TRIGGER_FAVORITES,
            Arc::clone(&settings_repo),
            &rt_handle,
            cx,
        );

        let detail_writers = KeyedSnapshotWriters::new(&rt_handle);
        let enabled_writers = KeyedSnapshotWriters::new(&rt_handle);
        let view = Self {
            repo,
            action_repo,
            registry,
            favorites,
            rt_handle,
            detail_width: detail::DETAIL_SHEET_W,
            loading: true,
            reload_generation: async_bridge::Generation::default(),
            detail_writers,
            enabled_writers,
            instances: Vec::new(),
            visible: Vec::new(),
            list_scroll: UniformListScrollHandle::new(),
            list_focus: cx.focus_handle(),
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
            focused_once: false,
            list_cursor: preselect,
            selected: preselect,
            detail: None,
            hovered: None,
            menu_open: None,
            menu_click_pos: None,
            search,
            platforms: Vec::new(),
            usage_filter: UsageFilter::All,
            rename: None,
            pending_delete: Confirm::default(),
            confirm_disable: Confirm::default(),
            create: None,
            builtins: BuiltinRegistry::default(),
            bus: None,
            integrations: None,
            collection_options: ChoiceOptions::new(),
            collection_options_gen: Generation::default(),
            obs_catalog_options_gen: Generation::default(),
            _collection_watch: Vec::new(),
            _search_sub: search_sub,
            _release: release,
        };
        view.reload(cx);
        view
    }

    #[must_use]
    pub(crate) fn with_integration_switch(
        mut self,
        switch: IntegrationSwitch,
        cx: &mut Context<Self>,
    ) -> Self {
        self.integrations = Some(switch.watch(cx, |this: &mut Self, off, cx| {
            if let Some(integrations) = &mut this.integrations {
                integrations.replace(off, cx);
            }
        }));
        self
    }

    fn uncovered_category(
        &self,
        kind_id: &str,
    ) -> Option<forge_platform_core::IntegrationCategory> {
        self.integrations.as_ref()?.uncovered_category(kind_id)
    }

    fn switched_off_owner(&self, kind_id: &str) -> Option<forge_types::IntegrationId> {
        let integrations = self.integrations.as_ref()?;
        integrations
            .off_owner(self.registry.owning_integration(kind_id))
            .cloned()
    }

    fn set_detail_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.detail_width != width {
            self.detail_width = width;
            cx.notify();
        }
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(async move { load_rows(&*repo).await }, cx);
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<TriggerInstanceRow>, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let ticket = self.reload_generation.next();
        async_bridge::run_async(
            &self.rt_handle,
            work,
            move |this, result, cx| match result {
                _ if !this.reload_generation.is_current(ticket) => {}
                Ok(rows) => this.apply_rows(rows, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn spawn_write<T: Send + 'static>(
        &self,
        write: impl Future<Output = Result<T, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            write,
            |this, written, cx| this.on_write_settled(written.map(drop), cx),
            cx,
        );
    }

    fn on_write_settled(&mut self, written: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        match written {
            Ok(()) => self.reload(cx),
            Err(failure) => {
                self.on_repo_error(&failure, cx);
                self.reload(cx);
            }
        }
    }

    fn apply_rows(&mut self, rows: Vec<TriggerInstanceRow>, cx: &mut Context<Self>) {
        self.instances = rows;
        self.rebuild_visible();
        if let Some(selected) = self.selected
            && !self.instances.iter().any(|r| r.id == selected)
        {
            self.selected = None;
            self.detail = None;
        }
        self.loading = false;
        if let Some(id) = self.selected
            && !self.adopt_row_into_detail(id)
        {
            self.load_detail(id, cx);
        }
        cx.notify();
    }

    fn adopt_row_into_detail(&mut self, id: TriggerInstanceId) -> bool {
        let Some(detail) = self.detail.as_mut().filter(|d| d.instance.id == id) else {
            return false;
        };
        if let Some(row) = self.instances.iter().find(|r| r.id == id) {
            detail.instance.enabled = row.enabled;
            detail.instance.name.clone_from(&row.name);
        }
        true
    }

    fn refuse_invalid_config(
        &self,
        kind_id: &str,
        config: &TriggerConfig,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(descriptor) = self.registry.get(kind_id) else {
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

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::error!(error = %failure, "triggers operation failed");
        let message = failure.text();
        self.loading = false;
        cx.push_toast(
            ToastKind::Error,
            tr!("triggers_toast_error", message = message.as_str()),
        );
        cx.notify();
    }
}

impl EventEmitter<NavRequested> for TriggersRegistryView {}

impl Render for TriggersRegistryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.menu_open.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        let density = cx.density();
        let palette = cx.palette();

        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.list_focus, cx);
        }

        let stats = self.render_stats(&palette, density);
        let filter_left = self.render_filter_left(&palette, cx);
        let filter_right = self.render_filter_right(&palette, cx);
        let list_focus = self.list_focus.clone();
        let list = navigable_list(div(), &list_focus, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(self.render_list(&palette, cx));
        let detail_pane = self
            .selected
            .map(|id| self.render_detail_sheet(id, &palette, cx));

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(list)
            .children(detail_pane);

        let disable_modal = self
            .confirm_disable
            .get()
            .copied()
            .map(|id| self.render_disable_confirm(id, &palette, cx));
        let delete_modal = self
            .pending_delete
            .get()
            .copied()
            .map(|id| self.render_delete_confirm(id, &palette, cx));
        let row_menu = self.render_row_context_menu(&palette, cx);
        let create_overlay = self
            .create
            .as_ref()
            .map(|stage| self.render_create(stage, &palette, cx));

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
            .children(disable_modal)
            .children(delete_modal)
            .children(row_menu)
            .children(create_overlay)
    }
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
    use forge_components::{Density, ThemeId};
    use forge_storage::MockTriggerInstanceRepo;
    use forge_types::IntegrationId;

    use super::*;
    use crate::test_support::{
        StubActions, error_toasts, lifecycle_switch, owned_triggers, pump, pump_until, runtime,
        switch_lifecycle, test_backend, trigger_of,
    };
    use forge_components::Presentation;
    use forge_runtime::{LifecycleState, LifecycleStates};
    use forge_types::TriggerInstance;
    use std::sync::atomic::{AtomicBool, Ordering};

    const TWITCH_CHAT: &str = "twitch.chat";

    #[test]
    fn a_kind_id_maps_to_its_source_chip_by_prefix() {
        for (kind_id, expected) in [
            ("twitch.chat", Some(Platform::Twitch)),
            ("youtube.super_chat", Some(Platform::Youtube)),
            ("kick.chat", Some(Platform::Kick)),
            ("obs.scene_changed", Some(Platform::Obs)),
            ("vtube.model_loaded", Some(Platform::Vtube)),
            ("midi.note_on", Some(Platform::Midi)),
            ("hotkey.pressed", Some(Platform::Hotkey)),
            ("donation.received", Some(Platform::Donation)),
            ("timer.tick", Some(Platform::Timer)),
            ("script.exec", Some(Platform::Script)),
            ("rhai.exec", Some(Platform::Script)),
            ("core.startup", Some(Platform::Core)),
            ("twitch", Some(Platform::Twitch)),
            ("discord.message", None),
            ("Twitch.chat", None),
            ("", None),
        ] {
            assert!(Platform::from_kind_id(kind_id) == expected, "{kind_id:?}");
        }
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

        let chipless: Vec<&str> = registry
            .all()
            .map(|descriptor| descriptor.id())
            .filter(|kind_id| Platform::from_kind_id(kind_id).is_none())
            .collect();

        assert!(chipless.is_empty(), "kinds without a chip: {chipless:?}");
    }

    fn twitch() -> IntegrationId {
        IntegrationId::new("twitch")
    }

    fn twitch_in(state: LifecycleState) -> LifecycleStates {
        LifecycleStates::from([(twitch(), state)])
    }

    #[gpui::test]
    fn a_trigger_tracks_its_integration_switching_off_and_back_on(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
        let rt = runtime();
        let (lifecycle, switch) = lifecycle_switch(cx, &rt, twitch_in(LifecycleState::Running));
        let mut repo = MockTriggerInstanceRepo::new();
        repo.expect_list_all().returning(|| Ok(Vec::new()));
        let (settings, _writes) = test_backend();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                TriggersRegistryView::new(
                    Arc::new(repo),
                    Arc::new(StubActions),
                    Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
                    settings as Arc<dyn SettingsRepo>,
                    rt.handle().clone(),
                    None,
                    cx,
                )
                .with_integration_switch(switch, cx)
            })
        });
        let owner = |cx: &mut gpui::TestAppContext| {
            view.read_with(cx, |view, _| view.switched_off_owner(TWITCH_CHAT))
        };

        let before = owner(cx);
        switch_lifecycle(cx, &lifecycle, twitch_in(LifecycleState::Disabled));
        let off = owner(cx);
        switch_lifecycle(cx, &lifecycle, twitch_in(LifecycleState::Starting));
        let back_on = owner(cx);

        assert_eq!((before, off, back_on), (None, Some(twitch()), None));
    }

    #[gpui::test]
    fn the_create_picker_stars_into_the_app_wide_trigger_favorites(cx: &mut gpui::TestAppContext) {
        use crate::picker_favorites::{StoredFavorites, TRIGGER_FAVORITES};
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
                    Arc::clone(&settings) as Arc<dyn SettingsRepo>,
                    handle.clone(),
                    None,
                    cx,
                )
            })
        });

        let app_wide = cx.update(|cx| {
            StoredFavorites::shared(
                TRIGGER_FAVORITES,
                settings as Arc<dyn SettingsRepo>,
                &handle,
                cx,
            )
            .entity_id()
        });

        assert_eq!(
            view.read_with(cx, |view, _| view.favorites.entity_id()),
            app_wide
        );
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

        let (_, picks) = build_kind_groups(&registry, &forge_components::FORGE_DEFAULT);

        let unreachable: Vec<&str> = registry
            .all()
            .map(|descriptor| descriptor.id())
            .filter(|id| !picks.values().any(|picked| picked == id))
            .collect();
        assert_eq!(unreachable, Vec::<&str>::new());
    }

    pub(super) struct TriggerList {
        pub(super) view: Entity<TriggersRegistryView>,
        rows: Arc<std::sync::Mutex<Vec<TriggerInstance>>>,
        rt: tokio::runtime::Runtime,
    }

    pub(super) fn open_trigger_list(
        cx: &mut gpui::TestAppContext,
        seeded: Vec<TriggerInstance>,
    ) -> TriggerList {
        open_list_over(cx, seeded, None)
    }

    pub(super) fn open_list_recording_writes(
        cx: &mut gpui::TestAppContext,
        seeded: Vec<TriggerInstance>,
        writes: Arc<std::sync::Mutex<Vec<bool>>>,
    ) -> TriggerList {
        open_list_over(cx, seeded, Some(writes))
    }

    fn open_list_over(
        cx: &mut gpui::TestAppContext,
        seeded: Vec<TriggerInstance>,
        writes: Option<Arc<std::sync::Mutex<Vec<bool>>>>,
    ) -> TriggerList {
        crate::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(forge_components::Toasts::new());
        });
        let rt = runtime();
        let rows = Arc::new(std::sync::Mutex::new(seeded));
        let mut repo = MockTriggerInstanceRepo::new();
        let listed = Arc::clone(&rows);
        repo.expect_list_user_defined()
            .returning(move || Ok(listed.lock().unwrap().clone()));
        repo.expect_list_all().returning(|| Ok(Vec::new()));
        repo.expect_actions_by_instance()
            .returning(|| Ok(Default::default()));
        let found = Arc::clone(&rows);
        repo.expect_get().returning(move |id| {
            let found = found.lock().unwrap();
            Ok(found.iter().find(|row| row.id == id).cloned())
        });
        repo.expect_actions_using().returning(|_| Ok(Vec::new()));
        repo.expect_set_enabled()
            .returning(move |_, enabled| match &writes {
                Some(writes) => {
                    writes.lock().unwrap().push(enabled);
                    Ok(())
                }
                None => Err(forge_storage::StorageError::Connection {
                    reason: "database is locked".to_owned(),
                }),
            });
        let archived = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (live, shelf) = (Arc::clone(&rows), Arc::clone(&archived));
        repo.expect_archive().returning(move |id| {
            let mut live = live.lock().unwrap();
            let position = live.iter().position(|row| row.id == id).unwrap();
            shelf.lock().unwrap().push(live.remove(position));
            Ok(true)
        });
        let (live, shelf) = (Arc::clone(&rows), archived);
        repo.expect_restore().returning(move |_| {
            live.lock().unwrap().append(&mut shelf.lock().unwrap());
            Ok(true)
        });
        let (settings, _writes) = test_backend();
        let handle = rt.handle().clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                TriggersRegistryView::new(
                    Arc::new(repo),
                    Arc::new(StubActions),
                    Arc::new(forge_registry::TriggerRegistry::new()),
                    settings as Arc<dyn SettingsRepo>,
                    handle,
                    None,
                    cx,
                )
            })
        });
        pump_until(&rt, || {
            cx.run_until_parked();
            view.read_with(cx, |view, _| !view.loading)
        });
        TriggerList { view, rows, rt }
    }

    impl TriggerList {
        pub(super) fn settle(&self, cx: &mut gpui::TestAppContext) {
            cx.run_until_parked();
            pump(&self.rt);
            cx.run_until_parked();
        }

        fn names(&self, cx: &mut gpui::TestAppContext) -> Vec<String> {
            self.view.read_with(cx, |view, _| {
                view.instances.iter().map(|row| row.name.clone()).collect()
            })
        }

        pub(super) fn wait_for_names(&self, cx: &mut gpui::TestAppContext, expected: &[String]) {
            pump_until(&self.rt, || {
                cx.run_until_parked();
                self.names(cx) == expected
            });
        }

        pub(super) fn errors(&self, cx: &mut gpui::TestAppContext) -> usize {
            cx.update(|cx| error_toasts(cx))
        }
    }

    pub(super) fn row_named(name: &str) -> TriggerInstance {
        let mut instance = trigger_of(TWITCH_CHAT);
        instance.name = name.to_owned();
        instance
    }

    #[gpui::test]
    fn opening_the_trigger_list_lands_its_rows(cx: &mut gpui::TestAppContext) {
        let list = open_trigger_list(cx, vec![row_named("Greeter")]);

        assert_eq!(list.names(cx), ["Greeter"]);
    }

    #[gpui::test]
    fn a_failed_write_toasts_even_with_a_newer_reload_in_flight(cx: &mut gpui::TestAppContext) {
        let list = open_trigger_list(cx, Vec::new());
        let (_held, reload_gate) = tokio::sync::oneshot::channel::<()>();

        list.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    reload_gate.await.map_err(RepoFailure::operation)?;
                    Ok(Vec::new())
                },
                cx,
            );
            view.spawn_write(
                async { Err::<(), _>(RepoFailure::operation("disk gone")) },
                cx,
            );
        });

        pump_until(&list.rt, || {
            cx.run_until_parked();
            list.errors(cx) == 1
        });
    }

    #[gpui::test]
    fn a_successful_write_relists_and_shows_what_it_wrote(cx: &mut gpui::TestAppContext) {
        let list = open_trigger_list(cx, Vec::new());
        let rows = Arc::clone(&list.rows);

        list.view.update(cx, |view, cx| {
            view.spawn_write(
                async move {
                    rows.lock().unwrap().push(row_named("Written"));
                    Ok(())
                },
                cx,
            );
        });

        list.wait_for_names(cx, &["Written".to_owned()]);
    }

    #[gpui::test]
    fn a_stale_reload_that_failed_is_dropped_without_a_toast(cx: &mut gpui::TestAppContext) {
        let list = open_trigger_list(cx, Vec::new());
        let (release_stale, stale_gate) = tokio::sync::oneshot::channel::<()>();
        let stale_landed = Arc::new(AtomicBool::new(false));
        let landed = Arc::clone(&stale_landed);

        list.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    stale_gate.await.map_err(RepoFailure::operation)?;
                    landed.store(true, Ordering::SeqCst);
                    Err(RepoFailure::operation("disk gone"))
                },
                cx,
            );
            view.reload(cx);
        });
        list.rows.lock().unwrap().push(row_named("Newest"));
        list.wait_for_names(cx, &["Newest".to_owned()]);
        release_stale.send(()).unwrap();
        pump_until(&list.rt, || stale_landed.load(Ordering::SeqCst));
        cx.run_until_parked();

        assert_eq!(list.errors(cx), 0);
    }

    #[gpui::test]
    fn a_failed_toggle_toasts_and_the_reload_restores_the_stored_state(
        cx: &mut gpui::TestAppContext,
    ) {
        let list = open_trigger_list(cx, vec![row_named("Greeter")]);
        let id = list.rows.lock().unwrap()[0].id;
        let stored = list.rows.lock().unwrap()[0].enabled;
        let shown = |list: &TriggerList, cx: &mut gpui::TestAppContext| {
            list.view
                .read_with(cx, |view, _| view.find(id).map(|row| row.enabled))
        };

        list.view.update(cx, |view, cx| view.toggle_enable(id, cx));
        let optimistic = shown(&list, cx);
        pump_until(&list.rt, || {
            cx.run_until_parked();
            list.errors(cx) == 1 && shown(&list, cx) == Some(stored)
        });

        assert_eq!(optimistic, Some(!stored));
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
