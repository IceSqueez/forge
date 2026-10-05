use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Confirm, Density, ForgePalette, InlineEdit, PlatformKind,
    SearchState, TextInput, ToastKind, page_frame, platform_color, tr,
};
use forge_registry::TriggerRegistry;
use forge_runtime::EventBus;
use forge_storage::{ActionRepo, SettingsRepo, TriggerInstanceRepo, reserved_keys};
use forge_types::{ActionId, PermissionRung, TriggerConfig, TriggerInstance, TriggerInstanceId};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Pixels, Point, Rgba, SharedString,
    Subscription, UniformListScrollHandle, Window, div, prelude::*, px,
};
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use crate::actions::LIST_CONTEXT;
use crate::async_bridge;
use crate::collection_options::ChoiceOptions;
use crate::config_form::{CollectionChoices, ConfigField};
use crate::integration_switch::{IntegrationSwitch, SwitchWatch};
use crate::integrations::BuiltinRegistry;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

mod collection_choices;
mod create;
mod detail;
mod list;
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
const NAME_FS: Pixels = px(11.0);
const KIND_FS: Pixels = px(11.0);
const USED_FS: Pixels = px(11.0);
const BADGE_FS: Pixels = px(9.0);
const FILTER_DIV_W: Pixels = BORDER_THIN;
const FILTER_DIV_H: Pixels = px(16.0);
const SEARCH_W: Pixels = px(240.0);
const USED_CELL_GAP: Pixels = px(10.0);
const DISABLED_OPACITY: f32 = 0.55;
const EMPTY_PAD_V: Pixels = px(60.0);
const EMPTY_PAD_H: Pixels = px(20.0);
const EMPTY_TILE: Pixels = px(48.0);
const EMPTY_TILE_RADIUS: Pixels = px(10.0);
const EMPTY_GLYPH: Pixels = px(22.0);
const EMPTY_TITLE_FS: Pixels = px(14.0);
const EMPTY_BODY_FS: Pixels = px(12.0);

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
            Platform::Twitch => platform_color(PlatformKind::Twitch, palette),
            Platform::Youtube => platform_color(PlatformKind::YouTube, palette),
            Platform::Kick => platform_color(PlatformKind::Kick, palette),
            Platform::Obs | Platform::Vtube => palette.accent_teal,
            Platform::Midi => palette.random,
            Platform::Hotkey => palette.warning,
            Platform::Donation => palette.success,
            Platform::Timer => palette.warning,
            Platform::Script => palette.bits,
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

pub struct TriggersRegistryView {
    repo: Arc<dyn TriggerInstanceRepo>,
    action_repo: Arc<dyn ActionRepo>,
    registry: Arc<TriggerRegistry>,
    settings_repo: Arc<dyn SettingsRepo>,
    favorites: HashSet<SharedString>,
    rt_handle: tokio::runtime::Handle,
    detail_width: Pixels,
    loading: bool,
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
    _collection_watch: Vec<gpui::Task<()>>,
    _search_sub: Subscription,
    _release: Subscription,
}

impl TriggersRegistryView {
    pub fn new(
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

        let view = Self {
            repo,
            action_repo,
            registry,
            settings_repo,
            favorites: HashSet::new(),
            rt_handle,
            detail_width: detail::DETAIL_SHEET_W,
            loading: true,
            instances: Vec::new(),
            visible: Vec::new(),
            list_scroll: UniformListScrollHandle::new(),
            list_focus: cx.focus_handle(),
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
            _collection_watch: Vec::new(),
            _search_sub: search_sub,
            _release: release,
        };
        view.reload(cx);
        view.load_favorites(cx);
        view
    }

    #[must_use]
    pub fn with_integration_switch(
        mut self,
        switch: IntegrationSwitch,
        cx: &mut Context<Self>,
    ) -> Self {
        self.integrations = Some(switch.watch(cx, |this: &mut Self, off, _cx| {
            if let Some(integrations) = &mut this.integrations {
                integrations.replace(off);
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

    fn load_favorites(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                forge_storage::get_json_setting::<Vec<String>>(
                    repo.as_ref(),
                    reserved_keys::PICKER_FAVORITES_TRIGGERS,
                )
                .await
                .unwrap_or_default()
            },
            |this, ids: Vec<String>, _cx| {
                this.favorites = crate::picker_favorites::to_set(ids);
            },
            cx,
        );
    }

    pub(super) fn persist_favorites(
        &self,
        favorites: HashSet<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let repo = Arc::clone(&self.settings_repo);
        let ids = crate::picker_favorites::to_ids(&favorites);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                forge_storage::set_json_setting(
                    repo.as_ref(),
                    reserved_keys::PICKER_FAVORITES_TRIGGERS,
                    &ids,
                )
                .await
                .map_err(|e| e.to_string())
            },
            |this, result: Result<(), String>, cx| {
                if let Err(message) = result {
                    this.on_repo_error(&message, cx);
                }
            },
            cx,
        );
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
        work: impl Future<Output = Result<Vec<TriggerInstanceRow>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        Self::reload_entity(cx.entity(), self.rt_handle.clone(), work, cx);
    }

    fn reload_entity(
        view: Entity<TriggersRegistryView>,
        rt_handle: tokio::runtime::Handle,
        work: impl Future<Output = Result<Vec<TriggerInstanceRow>, String>> + Send + 'static,
        app: &mut App,
    ) {
        async_bridge::run_async_entity(
            &rt_handle,
            view,
            work,
            |this, result, cx| match result {
                Ok(rows) => this.apply_rows(rows, cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            app,
        );
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
        if let Some(id) = self.selected {
            self.load_detail(id, cx);
        }
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: triggers operation failed: {message}");
        self.loading = false;
        cx.push_toast(
            ToastKind::Error,
            tr!("triggers_toast_error", message = message),
        );
        cx.notify();
    }
}

impl EventEmitter<NavRequested> for TriggersRegistryView {}

impl Render for TriggersRegistryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.list_focus, cx);
        }

        let stats = self.render_stats(&palette);
        let filter_left = self.render_filter_left(&palette, cx);
        let filter_right = self.render_filter_right(&palette, cx);
        let list = div()
            .track_focus(&self.list_focus)
            .key_context(LIST_CONTEXT)
            .on_action(cx.listener(Self::on_list_prev))
            .on_action(cx.listener(Self::on_list_next))
            .on_action(cx.listener(Self::on_list_activate))
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
        .density(Density::Cozy)
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

async fn load_rows(repo: &dyn TriggerInstanceRepo) -> Result<Vec<TriggerInstanceRow>, String> {
    let instances = repo.list_user_defined().await.map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(instances.len());
    for instance in instances {
        let used_in_count = repo
            .actions_using(instance.id)
            .await
            .map(|links| links.len())
            .unwrap_or(0);
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
    use crate::integration_supervisor::{LifecycleState, LifecycleStates};
    use crate::presentation::Presentation;
    use crate::test_support::{
        StubActions, lifecycle_switch, owned_triggers, runtime, switch_lifecycle, test_backend,
    };

    const TWITCH_CHAT: &str = "twitch.chat";

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
}
