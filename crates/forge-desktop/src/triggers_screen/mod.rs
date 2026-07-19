use forge_components::{ForgePalette, InlineEdit, TextInput, ToastKind, search_input, tr};
use forge_registry::TriggerRegistry;
use forge_storage::{ActionRepo, SettingsRepo, TriggerInstanceRepo, reserved_keys};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId};
use gpui::{
    App, Context, Entity, EventEmitter, Pixels, Point, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

mod config_form;
mod create;
mod detail;
mod list;

pub(crate) use config_form::{
    ConfigField, FILL_VAL_FS, fold_config_field, overlay_field_values, render_config_row,
    sparse_overrides,
};
use create::CreateStage;
pub(crate) use create::build_kind_groups;

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
const STATS_FS: Pixels = px(11.5);
const BADGE_FS: Pixels = px(9.0);
const FILTER_PAD_V: Pixels = px(8.0);
const FILTER_DIV_W: Pixels = px(1.0);
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
    Timer,
    Script,
    Core,
}

impl Platform {
    const ORDER: [Platform; 7] = [
        Platform::Twitch,
        Platform::Youtube,
        Platform::Kick,
        Platform::Obs,
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
            Platform::Timer => tr!("triggers_platform_timer"),
            Platform::Script => tr!("triggers_platform_script"),
            Platform::Core => tr!("triggers_platform_core"),
        }
    }

    fn dot(self, palette: &ForgePalette) -> Rgba {
        match self {
            Platform::Twitch => palette.brand,
            Platform::Youtube => palette.random,
            Platform::Kick => palette.info,
            Platform::Obs => palette.accent_teal,
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
    global_cooldown_secs: u32,
    user_cooldown_secs: u32,
}

struct TriggerDetail {
    instance: TriggerInstance,
    fields: Vec<ConfigField>,
    used_in: Vec<(ActionId, String)>,
    cooldown_input: Entity<TextInput>,
    cooldown_per_user: bool,
    _cooldown_sub: Subscription,
}

pub(crate) fn cooldown_suffix(global_cooldown_secs: u32, user_cooldown_secs: u32) -> String {
    let mut out = String::new();
    if global_cooldown_secs > 0 {
        out.push_str(&tr!(
            "triggers_cooldown_global_suffix",
            secs = global_cooldown_secs as i64
        ));
    }
    if user_cooldown_secs > 0 {
        out.push_str(&tr!(
            "triggers_cooldown_user_suffix",
            secs = user_cooldown_secs as i64
        ));
    }
    out
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
    selected: Option<TriggerInstanceId>,
    detail: Option<TriggerDetail>,
    hovered: Option<TriggerInstanceId>,
    menu_open: Option<TriggerInstanceId>,
    menu_click_pos: Option<Point<Pixels>>,
    search: String,
    search_field: Entity<TextInput>,
    platforms: Vec<Platform>,
    usage_filter: UsageFilter,
    rename: Option<RenameForm>,
    pending_delete: Option<TriggerInstanceId>,
    confirm_disable: Option<TriggerInstanceId>,
    create: Option<CreateStage>,
    _search_sub: Subscription,
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
        let search_field =
            cx.new(|cx| search_input(tr!("triggers_search_placeholder"), palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

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
            selected: preselect,
            detail: None,
            hovered: None,
            menu_open: None,
            menu_click_pos: None,
            search: String::new(),
            search_field,
            platforms: Vec::new(),
            usage_filter: UsageFilter::All,
            rename: None,
            pending_delete: None,
            confirm_disable: None,
            create: None,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view.load_favorites(cx);
        view
    }

    fn load_favorites(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.settings_repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
        self.rt_handle.spawn(async move {
            let raw = repo
                .get_string(reserved_keys::PICKER_FAVORITES_TRIGGERS_KEY)
                .await
                .ok()
                .flatten();
            let _ = tx.send(raw);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(raw) = rx.await {
                let _ = this.update(cx, |this, _cx| {
                    this.favorites = crate::picker_favorites::parse(raw);
                });
            }
        })
        .detach();
    }

    pub(super) fn persist_favorites(
        &self,
        favorites: HashSet<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let repo = Arc::clone(&self.settings_repo);
        let json = crate::picker_favorites::encode(&favorites);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                repo.set_string(reserved_keys::PICKER_FAVORITES_TRIGGERS_KEY, &json)
                    .await
                    .map_err(|e| e.to_string()),
            );
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Err(message)) = rx.await {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
        })
        .detach();
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
        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            let _ = tx.send(work.await);
        });
        app.spawn(async move |cx| match rx.await {
            Ok(Ok(rows)) => {
                view.update(cx, |this, cx| this.apply_rows(rows, cx));
            }
            Ok(Err(message)) => {
                view.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_rows(&mut self, rows: Vec<TriggerInstanceRow>, cx: &mut Context<Self>) {
        self.instances = rows;
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let header = self.render_header(&palette);
        let filter_bar = self.render_filter_bar(&palette, cx);
        let list = self.render_list(&palette, cx);
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
            .map(|id| self.render_disable_confirm(id, &palette, cx));
        let delete_modal = self
            .pending_delete
            .map(|id| self.render_delete_confirm(id, &palette, cx));
        let row_menu = self.render_row_context_menu(&palette, cx);
        let create_overlay = self
            .create
            .as_ref()
            .map(|stage| self.render_create(stage, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(filter_bar)
            .child(body)
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
            global_cooldown_secs: instance.global_cooldown_secs,
            user_cooldown_secs: instance.user_cooldown_secs,
        });
    }
    Ok(rows)
}
