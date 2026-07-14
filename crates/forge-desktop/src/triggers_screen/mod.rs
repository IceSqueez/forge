//! Triggers registry screen: the `TriggersRegistryView` view-entity, its cached
//! roster model, the shared render tokens and the `Render` dispatcher. The filter
//! bar, list rows, row menu and the rename/disable/delete modals live in the
//! sibling `list` submodule.

use forge_components::{ForgePalette, TextInput, ToastKind, search_input};
use forge_registry::TriggerRegistry;
use forge_storage::{ActionRepo, TriggerInstanceRepo};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId, Variant};
use gpui::{App, Context, Entity, Pixels, Rgba, Subscription, Window, div, prelude::*, px};
use std::future::Future;
use std::sync::Arc;

use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

mod detail;
mod list;

/// Leading selection stripe width down a row's edge (fixed 2px in the source).
const STRIPE_W: Pixels = px(2.0);
/// Row leading pad after the stripe (16px) and its trailing pad (18px); the caption
/// row carries an even 18px on both edges so its column starts align with the rows.
const ROW_PAD_L: Pixels = px(16.0);
const ROW_PAD_R: Pixels = px(18.0);
const ROW_PAD_V: Pixels = px(9.0);
const CAPTION_PAD_H: Pixels = px(18.0);
const CAPTION_PAD_V: Pixels = px(7.0);
/// The fixed column widths reproducing the design's
/// `gridTemplateColumns: 24px 220px 1fr 110px 36px 32px` with flex cells.
const COL_DOT: Pixels = px(24.0);
const COL_NAME: Pixels = px(220.0);
const COL_USED: Pixels = px(110.0);
const COL_ON: Pixels = px(36.0);
const COL_MENU: Pixels = px(32.0);
/// Leading status-dot diameter on a row (fixed 7px, off the `Spacing` scale).
const ROW_DOT: Pixels = px(7.0);
/// Kind-cell platform glyph size (fixed 11px, off the `FONT_*` scale).
const KIND_GLYPH: Pixels = px(11.0);
/// Off-scale row font sizes pinned to the design: name 12.5, mono kind 11, used-in
/// and header stats 11.5, override badge 9.
const NAME_FS: Pixels = px(12.5);
const KIND_FS: Pixels = px(11.0);
const USED_FS: Pixels = px(11.5);
const STATS_FS: Pixels = px(11.5);
const BADGE_FS: Pixels = px(9.0);
/// Filter-bar vertical inset and the divider bars between its groups.
const FILTER_PAD_V: Pixels = px(8.0);
const FILTER_DIV_W: Pixels = px(0.5);
const FILTER_DIV_H: Pixels = px(16.0);
/// Opacity a disabled row dims to (fixed 0.55 in the source).
const DISABLED_OPACITY: f32 = 0.55;
/// Empty-state envelope: outer pad, the rounded icon tile (48px / 10px corner) and
/// its centred glyph (22px).
const EMPTY_PAD_V: Pixels = px(60.0);
const EMPTY_PAD_H: Pixels = px(20.0);
const EMPTY_TILE: Pixels = px(48.0);
const EMPTY_TILE_RADIUS: Pixels = px(10.0);
const EMPTY_GLYPH: Pixels = px(22.0);
const EMPTY_TITLE_FS: Pixels = px(14.0);
const EMPTY_BODY_FS: Pixels = px(12.0);

/// The platform (event source) a trigger kind belongs to, derived from the leading
/// segment of its `kind_id`. Fixes the row dot hue and the filter-chip grouping.
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
    /// Filter-bar iteration order, matching the design's `PLATFORM_META` key order.
    const ORDER: [Platform; 7] = [
        Platform::Twitch,
        Platform::Youtube,
        Platform::Kick,
        Platform::Obs,
        Platform::Timer,
        Platform::Script,
        Platform::Core,
    ];

    /// Resolves the leading `kind_id` segment to a platform. `rhai` aliases `script`;
    /// any other prefix is unmapped and shows only under the `All` filter.
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

    /// The brand dot hue, mapped from the design's Catppuccin accent per source:
    /// twitch=mauve, youtube=red, kick/core=sky, obs=teal, timer=yellow, script=peach.
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

/// The row dot hue for a `kind_id`: its platform accent, or `info` when the prefix is
/// unmapped.
fn platform_dot_color(kind_id: &str, palette: &ForgePalette) -> Rgba {
    Platform::from_kind_id(kind_id)
        .map(|p| p.dot(palette))
        .unwrap_or(palette.info)
}

/// A cached trigger-instance summary — the row's payload, folded from a persisted
/// [`TriggerInstance`] plus its live link count on each pull. The storage provider is
/// the source of truth; the roster reconciles by a full re-pull after every write,
/// never a local patch. `used_in_count` is the number of actions linked to it;
/// `override_count` the number of config keys it re-authors.
struct TriggerInstanceRow {
    id: TriggerInstanceId,
    name: String,
    kind_id: String,
    enabled: bool,
    used_in_count: usize,
    override_count: usize,
}

/// The open detail side-sheet: the freshly pulled instance (source of overrides,
/// name, enabled, kind, scope), the per-field config editing surface folded from
/// the kind's `config_fields`, and the resolved names of the actions linking it.
/// Every config write reconciles by a full re-pull, so this never holds a
/// view-minted placeholder.
struct TriggerDetail {
    instance: TriggerInstance,
    fields: Vec<ConfigField>,
    used_in: Vec<(ActionId, String)>,
}

/// One row in the detail sheet's configuration editor, folded from the kind's
/// `config_fields` over the effective (default-merged) config. `Hint` marks a key
/// authored elsewhere (a nested sub-chain), rendered inert.
enum ConfigField {
    Input {
        key: String,
        /// Committed as `Variant::Int` (lenient parse — a non-numeric value keeps the
        /// field's prior value) rather than `Variant::String`.
        integer: bool,
        /// Set on the inner member of an `Optional` group; committed only while the
        /// gate toggle (a sibling `Bool` on this key) is on.
        gate: Option<String>,
        input: Entity<TextInput>,
        _sub: Subscription,
    },
    Bool {
        key: String,
        gate: Option<String>,
        value: bool,
    },
    Hint {
        key: String,
    },
}

/// The runtime-thread payload of a detail pull: the persisted instance plus each
/// linking action resolved to its display name. The foreground folds it into a
/// [`TriggerDetail`] (the config inputs need a UI context to build).
struct TriggerDetailData {
    instance: TriggerInstance,
    used_in: Vec<(ActionId, String)>,
}

/// Single-select usage filter over the list. `All` shows every instance; `Used`
/// keeps `used_in_count > 0`; `Unused` keeps `used_in_count == 0`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UsageFilter {
    All,
    Used,
    Unused,
}

/// An in-progress rename: the target instance plus the field entity holding the draft
/// name and the subscription routing its submit/cancel back to the view.
struct RenameForm {
    id: TriggerInstanceId,
    field: Entity<TextInput>,
    _sub: Subscription,
}

/// The Triggers registry screen view-entity: a page header (breadcrumb + instance
/// stats), a filter bar (search, platform chips, usage chips) and a scrolling instance
/// list with a column caption.
///
/// The roster is a cached read folded from [`TriggerInstanceRepo::list_user_defined`]:
/// every CRUD op (enable/disable, rename, delete) writes through the repo then
/// reconciles by a full re-pull, so the list always mirrors persisted state, never a
/// view-minted placeholder.
pub struct TriggersRegistryView {
    repo: Arc<dyn TriggerInstanceRepo>,
    action_repo: Arc<dyn ActionRepo>,
    registry: Arc<TriggerRegistry>,
    rt_handle: tokio::runtime::Handle,
    /// True until the first pull lands, so the list shows a loading caption rather than
    /// the empty-roster caption before any row arrives.
    loading: bool,
    instances: Vec<TriggerInstanceRow>,
    selected: Option<TriggerInstanceId>,
    /// The open detail side-sheet for the selected instance. `None` while the async
    /// pull is in flight (the sheet shows a loading body) or when nothing is selected.
    detail: Option<TriggerDetail>,
    hovered: Option<TriggerInstanceId>,
    menu_open: Option<TriggerInstanceId>,
    search: String,
    search_field: Entity<TextInput>,
    platforms: Vec<Platform>,
    usage_filter: UsageFilter,
    rename: Option<RenameForm>,
    pending_delete: Option<TriggerInstanceId>,
    confirm_disable: Option<TriggerInstanceId>,
    _search_sub: Subscription,
}

impl TriggersRegistryView {
    pub fn new(
        repo: Arc<dyn TriggerInstanceRepo>,
        action_repo: Arc<dyn ActionRepo>,
        registry: Arc<TriggerRegistry>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search_field = cx.new(|cx| search_input("Search instances\u{2026}", palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let view = Self {
            repo,
            action_repo,
            registry,
            rt_handle,
            loading: true,
            instances: Vec::new(),
            selected: None,
            detail: None,
            hovered: None,
            menu_open: None,
            search: String::new(),
            search_field,
            platforms: Vec::new(),
            usage_filter: UsageFilter::All,
            rename: None,
            pending_delete: None,
            confirm_disable: None,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    // --- async pull + reconcile -------------------------------------------

    /// Pulls the full user-defined roster off the storage provider and reconciles the
    /// cached list with it. Every enable/rename/delete routes back here for a full
    /// re-pull rather than patching a row locally.
    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(async move { load_rows(&*repo).await }, cx);
    }

    /// Spawns `work` (a repo verb that ends by rebuilding the row set) on the tokio
    /// runtime, then folds the result back on the foreground executor: the new roster
    /// on success, a PII-safe error toast on failure. A released view makes the apply a
    /// no-op.
    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<TriggerInstanceRow>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        Self::reload_entity(cx.entity(), self.rt_handle.clone(), work, cx);
    }

    /// The context-free reload path: usable both from a screen handler and from a toast
    /// action closure (which only has an [`App`] and the view handle). Hops the tokio
    /// runtime for `work`, then applies the outcome to `view`.
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
                let _ = view.update(cx, |this, cx| this.apply_rows(rows, cx));
            }
            Ok(Err(message)) => {
                let _ = view.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Reconciles the cached list with a freshly pulled roster and keeps the current
    /// selection in sync — clearing it when the selected instance no longer exists.
    fn apply_rows(&mut self, rows: Vec<TriggerInstanceRow>, cx: &mut Context<Self>) {
        self.instances = rows;
        if let Some(selected) = self.selected
            && !self.instances.iter().any(|r| r.id == selected)
        {
            self.selected = None;
            self.detail = None;
        }
        self.loading = false;
        // A roster re-pull follows every write; refresh the open sheet from the same
        // committed state so its config, override badges and used-in list stay coherent.
        if self.selected.is_some() && self.detail.is_some() {
            self.reload_detail(cx);
        }
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: triggers operation failed: {message}");
        self.loading = false;
        cx.push_toast(ToastKind::Error, format!("Triggers: {message}"));
        cx.notify();
    }
}

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
        let rename_modal = self
            .rename
            .as_ref()
            .map(|form| self.render_rename_modal(form, &palette, cx));

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
            .children(rename_modal)
    }
}

/// Folds the persisted user-defined roster into row summaries, pulling each instance's
/// live link count so the list, the usage filter and the header census agree. The
/// per-instance `actions_using` lookup mirrors the count the source computes.
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
        });
    }
    Ok(rows)
}
