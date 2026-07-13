use std::collections::BTreeMap;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement, ModalSize,
    OverlayPosition, Radius, Spacing, TextInput, badge, breadcrumb, chip, confirm_modal,
    ghost_button_with_icon, icon, icon_inherit, menu_button, menu_divider, menu_item, modal,
    overlay, primary_button, primary_button_with_icon, search_input, secondary_button, spacing,
    status_dot, toggle,
};
use gpui::{
    AnyElement, ClickEvent, Context, Div, Entity, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::presentation::ActivePresentation;

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

/// Config side-sheet geometry pinned to the design (`TrigSideSheet`, line 420). The
/// sheet reproduces a fixed 420px right panel; its paddings, tile, column widths and
/// off-scale font sizes sit between the `Spacing`/`FONT_*` steps and are named here.
const SHEET_W: Pixels = px(420.0);
const SHEET_HEADER_PAD_V: Pixels = px(12.0);
const SHEET_BODY_PAD_V: Pixels = px(14.0);
const SHEET_FOOTER_PAD_V: Pixels = px(10.0);
const SHEET_TILE: Pixels = px(30.0);
const SHEET_TILE_RADIUS: Pixels = px(8.0);
const SHEET_TILE_GLYPH: Pixels = px(16.0);
const SHEET_KIND_DOT: Pixels = px(5.0);
const SHEET_CLOSE_GLYPH: Pixels = px(15.0);
/// Right-hand note beside a section label ("N overridden" / "all defaults"), 10px.
const SECTION_NOTE_FS: Pixels = px(10.0);
/// Config-fields card: 8px corner, and the `[110 key][flex value][22 clear]` row grid.
const CFG_CARD_RADIUS: Pixels = px(8.0);
const CFG_ROW_PAD_V: Pixels = px(8.0);
const CFG_ROW_PAD_H: Pixels = px(12.0);
const CFG_KEY_W: Pixels = px(110.0);
const CFG_CLEAR_W: Pixels = px(22.0);
const CFG_KEY_FS: Pixels = px(11.0);
const CFG_CLEAR_GLYPH: Pixels = px(11.0);
/// Shared 11.5px body size for the config value, the empty-usage card and the Delete
/// label (all `fontSize: 11.5` in the design).
const SHEET_TEXT_FS: Pixels = px(11.5);
/// Used-in list rows: 7px vertical inset, 6px corner, and its two glyphs.
const USED_ROW_PAD_V: Pixels = px(7.0);
const USED_ROW_RADIUS: Pixels = px(6.0);
const USED_BOLT: Pixels = px(12.0);
const USED_ARROW: Pixels = px(11.0);
/// Empty-usage card inset (14/12).
const EMPTY_USED_PAD_V: Pixels = px(14.0);
const EMPTY_USED_PAD_H: Pixels = px(12.0);
/// Delete footer button: 5px vertical inset, 6px corner, 11px trash glyph and the
/// 0.6 opacity it dims to while blocked by a non-empty usage list.
const DELETE_BTN_PAD_V: Pixels = px(5.0);
const DELETE_BTN_RADIUS: Pixels = px(6.0);
const DELETE_GLYPH: Pixels = px(11.0);
const DISABLED_BTN_OPACITY: f32 = 0.6;

/// Local id for a seeded trigger instance. `forge-desktop` wires no trigger-instance
/// repo yet, so instances are seeded in-memory and ids minted from a per-view counter
/// rather than the runtime's persistent id.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct InstanceId(u64);

/// The platform (event source) a trigger kind belongs to. Fixes the row dot hue and
/// the filter-chip grouping.
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

/// A seeded trigger kind — the reusable descriptor a trigger instance configures.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TriggerKind {
    NewSubscriber,
    RaidReceived,
    ChatCommand,
    IntervalWhenLive,
    CronSchedule,
    SceneChanged,
    ReplaySaved,
}

impl TriggerKind {
    fn platform(self) -> Platform {
        match self {
            TriggerKind::NewSubscriber | TriggerKind::RaidReceived | TriggerKind::ChatCommand => {
                Platform::Twitch
            }
            TriggerKind::IntervalWhenLive | TriggerKind::CronSchedule => Platform::Timer,
            TriggerKind::SceneChanged | TriggerKind::ReplaySaved => Platform::Obs,
        }
    }

    fn label(self) -> &'static str {
        match self {
            TriggerKind::NewSubscriber => "Channel Subscriber",
            TriggerKind::RaidReceived => "Channel Raid",
            TriggerKind::ChatCommand => "Chat command",
            TriggerKind::IntervalWhenLive => "Interval (when live)",
            TriggerKind::CronSchedule => "Cron schedule",
            TriggerKind::SceneChanged => "Scene changed",
            TriggerKind::ReplaySaved => "Replay buffer saved",
        }
    }

    fn kind_id(self) -> &'static str {
        match self {
            TriggerKind::NewSubscriber => "twitch.subs-bits.new-subscriber",
            TriggerKind::RaidReceived => "twitch.raids.raid-received",
            TriggerKind::ChatCommand => "twitch.chat.chat-command",
            TriggerKind::IntervalWhenLive => "timer.intervals.interval-when-live",
            TriggerKind::CronSchedule => "timer.intervals.cron-schedule",
            TriggerKind::SceneChanged => "obs.scenes.scene-changed",
            TriggerKind::ReplaySaved => "obs.streaming.replay-saved",
        }
    }

    /// The kind glyph shown leading the kind cell. Uses the nearest available kit
    /// glyph where the design's Tabler name has no imported counterpart.
    fn glyph(self) -> Icon {
        match self {
            TriggerKind::NewSubscriber => Icon::Star,
            TriggerKind::RaidReceived => Icon::Flag,
            TriggerKind::ChatCommand => Icon::MessageCircle,
            TriggerKind::IntervalWhenLive => Icon::Clock,
            TriggerKind::CronSchedule => Icon::Clock,
            TriggerKind::SceneChanged => Icon::Repeat,
            TriggerKind::ReplaySaved => Icon::Download,
        }
    }

    /// The kind's default config schema: ordered `(key, default)` pairs. The real
    /// screen reads these off the trigger-kind descriptor from `forge-registry`; here
    /// they are seeded to mirror the design's sample schemas so the side-sheet renders
    /// populated field rows with live override/revert.
    fn default_config(self) -> &'static [(&'static str, &'static str)] {
        match self {
            TriggerKind::NewSubscriber => &[
                ("tier_filter", "any"),
                ("notify_anon", "true"),
                ("cooldown", "0s"),
            ],
            TriggerKind::RaidReceived => &[("min_viewers", "1")],
            TriggerKind::ChatCommand => &[
                ("command", "!cmd"),
                ("aliases", "\u{2014}"),
                ("cooldown", "5s"),
                ("global_cd", "0s"),
                ("permission", "everyone"),
            ],
            TriggerKind::IntervalWhenLive => {
                &[("every", "60s"), ("jitter", "0s"), ("only_live", "true")]
            }
            TriggerKind::CronSchedule => &[("cron", "0 * * * *"), ("timezone", "Europe/Kyiv")],
            TriggerKind::SceneChanged => &[("from", "\u{2014}"), ("to", "\u{2014}")],
            TriggerKind::ReplaySaved => &[("rename_to", "\u{2014}")],
        }
    }

    /// The default value for a config key, if the schema declares it.
    fn default_value(self, key: &str) -> Option<&'static str> {
        self.default_config()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
    }
}

/// A cached trigger-instance summary — the row's payload. The real screen reads these
/// from the trigger-instance repo over the runtime→UI bridge; here they are seeded.
/// `overrides` maps the config keys this instance re-authors to their values (absent
/// keys fall back to the kind's default); `used_in` lists the action ids linked to it.
struct TriggerInstance {
    id: InstanceId,
    name: String,
    kind: TriggerKind,
    enabled: bool,
    overrides: BTreeMap<String, String>,
    used_in: Vec<String>,
}

impl TriggerInstance {
    /// The effective config: every schema key in order, carrying its override value
    /// when present (flagged overridden) else the kind default, followed by any
    /// override-only keys the schema does not declare.
    fn effective_config(&self) -> Vec<ConfigField> {
        let mut out = Vec::new();
        for (key, default) in self.kind.default_config() {
            let overridden = self.overrides.get(*key);
            out.push(ConfigField {
                key: (*key).to_owned(),
                value: overridden.map(String::as_str).unwrap_or(default).to_owned(),
                overridden: overridden.is_some(),
            });
        }
        for (key, value) in &self.overrides {
            if self.kind.default_value(key).is_none() {
                out.push(ConfigField {
                    key: key.clone(),
                    value: value.clone(),
                    overridden: true,
                });
            }
        }
        out
    }
}

/// One rendered config row: a key, its effective value and whether the instance
/// overrides the kind default for it.
struct ConfigField {
    key: String,
    value: String,
    overridden: bool,
}

/// Single-select usage filter over the list. `All` shows every instance; `Used`
/// keeps `used_in > 0`; `Unused` keeps `used_in == 0`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UsageFilter {
    All,
    Used,
    Unused,
}

/// An in-progress rename: the target instance plus the field entity holding the draft
/// name and the subscription routing its submit/cancel back to the view.
struct RenameForm {
    id: InstanceId,
    field: Entity<TextInput>,
    _sub: Subscription,
}

/// An in-progress inline config-field edit inside the side-sheet: the target instance,
/// the field key, the value shown when editing began (so an unchanged commit is a
/// no-op), the input entity and the subscription routing Enter/Esc back to the view.
struct FieldEditForm {
    id: InstanceId,
    key: String,
    seed: String,
    field: Entity<TextInput>,
    _sub: Subscription,
}

/// The Triggers registry screen view-entity: a page header (breadcrumb + instance
/// stats), a filter bar (search, platform chips, usage chips, New-trigger button), and
/// a scrolling instance list with a column caption.
///
/// Owns its instances, selection and filter state as seeded stub state — the real
/// screen loads instances from the repo over the runtime→UI bridge and drives every
/// mutation through the runtime handle. Here the CRUD (enable/disable with
/// confirm-when-used, delete blocked-when-used, rename, use-as-template clone) mutates
/// this cached state locally. Selecting a row opens the config side-sheet (TR-B): an
/// inline-rename header, per-field override/revert config editor and a linked-actions
/// list. The kind-picker create flow (TR-C) lands in a follow-up slice.
pub struct TriggersRegistryView {
    instances: Vec<TriggerInstance>,
    selected: Option<InstanceId>,
    hovered: Option<InstanceId>,
    menu_open: Option<InstanceId>,
    search: String,
    search_field: Entity<TextInput>,
    platforms: Vec<Platform>,
    usage_filter: UsageFilter,
    rename: Option<RenameForm>,
    sheet_rename: Option<RenameForm>,
    field_edit: Option<FieldEditForm>,
    pending_delete: Option<InstanceId>,
    confirm_disable: Option<InstanceId>,
    next_id: u64,
    _search_sub: Subscription,
}

impl TriggersRegistryView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let search_field = cx.new(|cx| search_input("Search instances\u{2026}", palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let mut next_id = 0u64;
        let mut mint = || {
            let id = InstanceId(next_id);
            next_id += 1;
            id
        };
        let instances = seed_instances(&mut mint);

        Self {
            instances,
            selected: None,
            hovered: None,
            menu_open: None,
            search: String::new(),
            search_field,
            platforms: Vec::new(),
            usage_filter: UsageFilter::All,
            rename: None,
            sheet_rename: None,
            field_edit: None,
            pending_delete: None,
            confirm_disable: None,
            next_id,
            _search_sub: search_sub,
        }
    }

    fn mint_id(&mut self) -> InstanceId {
        let id = InstanceId(self.next_id);
        self.next_id += 1;
        id
    }

    // --- lookup + derivation ----------------------------------------------

    fn find(&self, id: InstanceId) -> Option<&TriggerInstance> {
        self.instances.iter().find(|i| i.id == id)
    }

    fn used_count(&self) -> usize {
        self.instances
            .iter()
            .filter(|i| !i.used_in.is_empty())
            .count()
    }

    fn disabled_count(&self) -> usize {
        self.instances.iter().filter(|i| !i.enabled).count()
    }

    /// The platforms present in the list with a non-zero instance count, in the
    /// design's fixed order — the source list for the platform filter chips.
    fn platform_counts(&self) -> Vec<(Platform, usize)> {
        Platform::ORDER
            .into_iter()
            .filter_map(|p| {
                let count = self
                    .instances
                    .iter()
                    .filter(|i| i.kind.platform() == p)
                    .count();
                (count > 0).then_some((p, count))
            })
            .collect()
    }

    fn has_active_filter(&self) -> bool {
        !self.search.trim().is_empty()
            || !self.platforms.is_empty()
            || self.usage_filter != UsageFilter::All
    }

    /// An instance survives the current platform, usage and search filters. Combines
    /// all three so the list and the empty-state gate share one predicate.
    fn passes(&self, instance: &TriggerInstance) -> bool {
        if !self.platforms.is_empty() && !self.platforms.contains(&instance.kind.platform()) {
            return false;
        }
        match self.usage_filter {
            UsageFilter::Used if instance.used_in.is_empty() => return false,
            UsageFilter::Unused if !instance.used_in.is_empty() => return false,
            _ => {}
        }
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        instance.name.to_lowercase().contains(&q)
            || instance.kind.kind_id().contains(&q)
            || instance.kind.label().to_lowercase().contains(&q)
    }

    // --- filter handlers ---------------------------------------------------

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search = text.to_string();
            cx.notify();
        }
    }

    fn toggle_platform(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if let Some(pos) = self.platforms.iter().position(|&p| p == platform) {
            self.platforms.remove(pos);
        } else {
            self.platforms.push(platform);
        }
        cx.notify();
    }

    fn clear_platforms(&mut self, cx: &mut Context<Self>) {
        self.platforms.clear();
        cx.notify();
    }

    fn set_usage_filter(&mut self, filter: UsageFilter, cx: &mut Context<Self>) {
        self.usage_filter = if self.usage_filter == filter {
            UsageFilter::All
        } else {
            filter
        };
        cx.notify();
    }

    fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.search.clear();
        let field = self.search_field.clone();
        field.update(cx, |input, cx| input.set_content("", cx));
        self.platforms.clear();
        self.usage_filter = UsageFilter::All;
        cx.notify();
    }

    // --- selection + row interaction --------------------------------------

    fn select(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            // Switching instances abandons any inline edit bound to the old one.
            self.sheet_rename = None;
            self.field_edit = None;
        }
        self.selected = Some(id);
        cx.notify();
    }

    /// Closes the side-sheet, dropping selection and any inline edit it hosted.
    fn close_sheet(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.sheet_rename = None;
        self.field_edit = None;
        cx.notify();
    }

    /// Cross-navigation to the Action editor for a linked action. The deep-link from
    /// the Triggers screen into Actions is not wired in `forge-desktop` yet, so this is
    /// intentionally inert (a harmless repaint) rather than a dead click that panics.
    fn navigate_to_action(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn set_hover(&mut self, id: InstanceId, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered != Some(id) {
                self.hovered = Some(id);
                cx.notify();
            }
        } else if self.hovered == Some(id) {
            self.hovered = None;
            cx.notify();
        }
    }

    fn toggle_menu(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open == Some(id) {
            None
        } else {
            Some(id)
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    // --- enable / disable (confirm when used) -----------------------------

    fn set_enabled(&mut self, id: InstanceId, enabled: bool) {
        if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
            instance.enabled = enabled;
        }
    }

    /// Toggling a used instance OFF arms the confirm dialog; enabling, or disabling an
    /// unused instance, applies immediately — mirroring the source's `onToggleEnable`.
    fn toggle_enable(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        let Some(instance) = self.find(id) else {
            return;
        };
        if !instance.enabled {
            self.set_enabled(id, true);
        } else if !instance.used_in.is_empty() {
            self.confirm_disable = Some(id);
        } else {
            self.set_enabled(id, false);
        }
        cx.notify();
    }

    fn confirm_disable_now(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.confirm_disable.take() {
            self.set_enabled(id, false);
        }
        cx.notify();
    }

    fn cancel_disable(&mut self, cx: &mut Context<Self>) {
        self.confirm_disable = None;
        cx.notify();
    }

    // --- delete (blocked when used) ---------------------------------------

    fn request_delete(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        self.pending_delete = Some(id);
        self.menu_open = None;
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    /// Removes the instance unless it is still used — the mocked FK constraint the
    /// source enforces by disabling the delete affordance for a used instance.
    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_delete.take()
            && self.find(id).is_some_and(|i| i.used_in.is_empty())
        {
            self.instances.retain(|i| i.id != id);
            if self.selected == Some(id) {
                self.selected = None;
                self.sheet_rename = None;
                self.field_edit = None;
            }
        }
        cx.notify();
    }

    // --- use as template + new --------------------------------------------

    fn use_as_template(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        let new_id = self.mint_id();
        if let Some(pos) = self.instances.iter().position(|i| i.id == id) {
            let src = &self.instances[pos];
            let clone = TriggerInstance {
                id: new_id,
                name: format!("{} copy", src.name),
                kind: src.kind,
                enabled: src.enabled,
                overrides: src.overrides.clone(),
                used_in: Vec::new(),
            };
            self.instances.insert(0, clone);
            self.selected = Some(new_id);
        }
        self.menu_open = None;
        cx.notify();
    }

    /// Seeds a default chat-command instance and selects it — the design's inline New
    /// behavior. The full kind-picker create wizard is TR-C.
    fn new_trigger(&mut self, cx: &mut Context<Self>) {
        let id = self.mint_id();
        self.instances.insert(
            0,
            TriggerInstance {
                id,
                name: "New trigger".to_owned(),
                kind: TriggerKind::ChatCommand,
                enabled: true,
                overrides: BTreeMap::new(),
                used_in: Vec::new(),
            },
        );
        self.selected = Some(id);
        cx.notify();
    }

    // --- rename ------------------------------------------------------------

    fn start_rename(&mut self, id: InstanceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(instance) = self.find(id) else {
            return;
        };
        let palette = cx.palette();
        let seed = instance.name.clone();
        let field = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm);
            input.set_content(seed, cx);
            input
        });
        field.read(cx).focus(window);
        let sub = cx.subscribe(&field, Self::on_rename_event);
        self.menu_open = None;
        self.rename = Some(RenameForm {
            id,
            field,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_rename_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(text) => self.commit_rename(text.to_string(), cx),
            InputEvent::Cancelled => {
                self.rename = None;
                cx.notify();
            }
            InputEvent::Changed(_) => {}
        }
    }

    fn submit_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.rename.as_ref() {
            let name = form.field.read(cx).content().to_string();
            self.commit_rename(name, cx);
        }
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let trimmed = name.trim();
        if let Some(form) = self.rename.take()
            && !trimmed.is_empty()
            && let Some(instance) = self.instances.iter_mut().find(|i| i.id == form.id)
        {
            instance.name = trimmed.to_owned();
        }
        cx.notify();
    }

    // --- side-sheet: inline header rename ---------------------------------

    /// Opens the inline rename field in the side-sheet header, seeded with the current
    /// name and focused. Enter commits, Esc reverts — the design's `InlineRename`.
    fn start_sheet_rename(&mut self, id: InstanceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(instance) = self.find(id) else {
            return;
        };
        let palette = cx.palette();
        let seed = instance.name.clone();
        let field = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
                .static_chrome(palette.brand, Radius::Sm);
            input.set_content(seed, cx);
            input
        });
        field.read(cx).focus(window);
        let sub = cx.subscribe(
            &field,
            move |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(text) => this.commit_sheet_rename(id, text.to_string(), cx),
                InputEvent::Cancelled => {
                    this.sheet_rename = None;
                    cx.notify();
                }
                InputEvent::Changed(_) => {}
            },
        );
        self.sheet_rename = Some(RenameForm {
            id,
            field,
            _sub: sub,
        });
        cx.notify();
    }

    fn commit_sheet_rename(&mut self, id: InstanceId, name: String, cx: &mut Context<Self>) {
        self.sheet_rename = None;
        let trimmed = name.trim();
        if !trimmed.is_empty()
            && let Some(instance) = self.instances.iter_mut().find(|i| i.id == id)
        {
            instance.name = trimmed.to_owned();
        }
        cx.notify();
    }

    // --- side-sheet: per-field config override / revert -------------------

    /// Opens the inline editor for one config field, seeded with its effective value
    /// (override or kind default) and focused. Enter commits, Esc reverts.
    fn start_field_edit(
        &mut self,
        id: InstanceId,
        key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(instance) = self.find(id) else {
            return;
        };
        let default = instance.kind.default_value(&key).unwrap_or("");
        let seed = instance
            .overrides
            .get(&key)
            .map(String::as_str)
            .unwrap_or(default)
            .to_owned();
        let palette = cx.palette();
        let field = cx.new(|cx| {
            let mut input = TextInput::new("value", cx)
                .with_palette(palette)
                .with_font_size(SHEET_TEXT_FS)
                .static_chrome(palette.brand, Radius::Sm);
            input.set_content(seed.clone(), cx);
            input
        });
        field.read(cx).focus(window);
        let sub = cx.subscribe(
            &field,
            move |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(text) => this.commit_field(text.to_string(), cx),
                InputEvent::Cancelled => this.cancel_field_edit(cx),
                InputEvent::Changed(_) => {}
            },
        );
        self.field_edit = Some(FieldEditForm {
            id,
            key,
            seed,
            field,
            _sub: sub,
        });
        cx.notify();
    }

    fn cancel_field_edit(&mut self, cx: &mut Context<Self>) {
        self.field_edit = None;
        cx.notify();
    }

    /// Commits the inline field editor. A value equal to the one shown when editing
    /// began is a no-op; any other value is written as an override for that key.
    fn commit_field(&mut self, value: String, cx: &mut Context<Self>) {
        if let Some(form) = self.field_edit.take()
            && value != form.seed
            && let Some(instance) = self.instances.iter_mut().find(|i| i.id == form.id)
        {
            instance.overrides.insert(form.key, value);
        }
        cx.notify();
    }

    /// Reverts one field to its kind default by dropping the override, and closes the
    /// inline editor if it was targeting that field.
    fn clear_override(&mut self, id: InstanceId, key: &str, cx: &mut Context<Self>) {
        if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
            instance.overrides.remove(key);
        }
        if self
            .field_edit
            .as_ref()
            .is_some_and(|f| f.id == id && f.key == key)
        {
            self.field_edit = None;
        }
        cx.notify();
    }

    // --- render: page header ----------------------------------------------

    fn render_header(&self, palette: &ForgePalette) -> AnyElement {
        let sep = || {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(STATS_FS)
                .text_color(palette.text_faint)
                .child("\u{b7}")
        };
        let stat = |value: String, value_color: Rgba, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(STATS_FS)
                        .text_color(value_color)
                        .child(value),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(STATS_FS)
                        .text_color(palette.text_muted)
                        .child(label),
                )
        };

        let stats = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(stat(
                self.instances.len().to_string(),
                palette.text_primary,
                "instances",
            ))
            .child(sep())
            .child(stat(self.used_count().to_string(), palette.success, "used"))
            .child(sep())
            .child(stat(
                self.disabled_count().to_string(),
                palette.warning,
                "disabled",
            ));

        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Automation"),
                BreadcrumbCrumb::leaf("Triggers"),
            ],
            palette,
        )
        .right(stats)
        .into_any_element()
    }

    // --- render: filter bar -----------------------------------------------

    fn divider(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w(FILTER_DIV_W)
            .h(FILTER_DIV_H)
            .bg(palette.border_regular)
            .into_any_element()
    }

    fn render_filter_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut platform_chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        for (platform, count) in self.platform_counts() {
            let active = self.platforms.contains(&platform);
            let label = format!("{} {}", platform.label(), count);
            platform_chips = platform_chips.child(
                chip(
                    label,
                    ChipGlyph::Dot(platform.dot(palette)),
                    active,
                    palette,
                )
                .on_click(
                    SharedString::from(format!("triggers-platform-{}", platform.label())),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_platform(platform, cx)
                    }),
                ),
            );
        }
        if !self.platforms.is_empty() {
            platform_chips = platform_chips.child(
                div()
                    .id("triggers-platform-clear")
                    .cursor_pointer()
                    .px(spacing(Spacing::Xs, Density::Cozy))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("clear")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_platforms(cx))),
            );
        }

        let usage_chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                chip(
                    "Used",
                    ChipGlyph::Dot(palette.success),
                    self.usage_filter == UsageFilter::Used,
                    palette,
                )
                .on_click(
                    "triggers-usage-used",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_usage_filter(UsageFilter::Used, cx)
                    }),
                ),
            )
            .child(
                chip(
                    "Unused",
                    ChipGlyph::Dot(palette.text_faint),
                    self.usage_filter == UsageFilter::Unused,
                    palette,
                )
                .on_click(
                    "triggers-usage-unused",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_usage_filter(UsageFilter::Unused, cx)
                    }),
                ),
            );

        let left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(div().child(self.search_field.clone()))
            .child(self.divider(palette))
            .child(platform_chips)
            .child(self.divider(palette))
            .child(usage_chips);

        let new_btn = primary_button_with_icon(Icon::Plus, "New trigger", palette).on_click(
            "triggers-new",
            cx.listener(|this, _: &ClickEvent, _, cx| this.new_trigger(cx)),
        );

        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(FILTER_PAD_V)
            .px(spacing(Spacing::Md, Density::Cozy))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(left)
            .child(new_btn)
            .into_any_element()
    }

    // --- render: list -----------------------------------------------------

    /// Lays six cells across the shared column skeleton so the caption and every row
    /// line up. The caller frames the skeleton with the row/caption padding.
    fn columns(
        dot: AnyElement,
        name: AnyElement,
        kind: AnyElement,
        used: AnyElement,
        on: AnyElement,
        menu: AnyElement,
    ) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .w_full()
            .child(
                div()
                    .w(COL_DOT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(dot),
            )
            .child(div().w(COL_NAME).flex_none().overflow_hidden().child(name))
            .child(div().flex_1().min_w(px(0.0)).child(kind))
            .child(
                div()
                    .w(COL_USED)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(used),
            )
            .child(
                div()
                    .w(COL_ON)
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(on),
            )
            .child(
                div()
                    .w(COL_MENU)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(menu),
            )
    }

    fn caption_cell(&self, palette: &ForgePalette, label: &'static str) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(label)
            .into_any_element()
    }

    fn render_caption(&self, palette: &ForgePalette) -> AnyElement {
        let cols = Self::columns(
            div().into_any_element(),
            self.caption_cell(palette, "NAME"),
            self.caption_cell(palette, "KIND"),
            self.caption_cell(palette, "USED IN"),
            self.caption_cell(palette, "ON"),
            div().into_any_element(),
        );
        div()
            .w_full()
            .flex_none()
            .py(CAPTION_PAD_V)
            .px(CAPTION_PAD_H)
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(cols)
            .into_any_element()
    }

    fn render_list(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let visible: Vec<&TriggerInstance> =
            self.instances.iter().filter(|i| self.passes(i)).collect();

        let inner = if visible.is_empty() {
            self.render_empty(palette, cx)
        } else {
            let mut col = div().flex().flex_col().child(self.render_caption(palette));
            for instance in visible {
                col = col.child(self.render_row(instance, palette, cx));
            }
            col.into_any_element()
        };

        div()
            .id("triggers-list")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(inner)
            .into_any_element()
    }

    fn render_row(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let selected = self.selected == Some(id);
        let hovered = self.hovered == Some(id);
        let dot_color = instance.kind.platform().dot(palette);

        let stripe_color = if selected {
            dot_color
        } else {
            gpui::transparent_black().into()
        };
        let row_bg: Rgba = if selected || hovered {
            palette.elevated
        } else {
            gpui::transparent_black().into()
        };
        let name_color = if !instance.enabled {
            palette.text_muted
        } else {
            palette.text_primary
        };

        // Kind cell: platform glyph + ellipsised label + optional override badge.
        let mut kind = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .min_w(px(0.0))
            .child(icon(instance.kind.glyph(), KIND_GLYPH, dot_color))
            .child(
                div()
                    .flex_shrink()
                    .overflow_hidden()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(KIND_FS)
                    .text_color(palette.text_muted)
                    .child(instance.kind.label()),
            );
        let override_count = instance.overrides.len();
        if override_count > 0 {
            let label = if override_count == 1 {
                "1 override".to_owned()
            } else {
                format!("{override_count} overrides")
            };
            kind = kind.child(badge(
                palette.surface_overlay,
                palette.bits,
                label,
                true,
                BADGE_FS,
            ));
        }

        // Used-in cell: "used in N" (N green) or an italic "unused".
        let used: AnyElement = if !instance.used_in.is_empty() {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(USED_FS)
                .text_color(palette.text_primary)
                .child("used in")
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.success)
                        .child(instance.used_in.len().to_string()),
                )
                .into_any_element()
        } else {
            div()
                .italic()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(USED_FS)
                .text_color(palette.text_faint)
                .child("unused")
                .into_any_element()
        };

        // The select region spans the first four columns; the toggle and menu are
        // separate cells so a click on either never selects the row.
        let select_region = div()
            .id(SharedString::from(format!("triggers-row-select-{}", id.0)))
            .flex_1()
            .flex()
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)))
            .child(
                div()
                    .w(COL_DOT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(status_dot(dot_color, ROW_DOT)),
            )
            .child(
                div()
                    .w(COL_NAME)
                    .flex_none()
                    .overflow_hidden()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(NAME_FS)
                    .text_color(name_color)
                    .child(instance.name.clone()),
            )
            .child(div().flex_1().min_w(px(0.0)).child(kind))
            .child(
                div()
                    .w(COL_USED)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(used),
            );

        let on_cell = div().w(COL_ON).flex_none().flex().justify_center().child(
            toggle(instance.enabled, palette).on_click(
                SharedString::from(format!("triggers-toggle-{}", id.0)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_enable(id, cx)),
            ),
        );

        let menu_cell = div()
            .w(COL_MENU)
            .flex_none()
            .flex()
            .justify_end()
            .child(self.render_row_menu(instance, palette, cx));

        let content = div()
            .w_full()
            .flex()
            .items_center()
            .pl(ROW_PAD_L)
            .pr(ROW_PAD_R)
            .py(ROW_PAD_V)
            .child(select_region)
            .child(on_cell)
            .child(menu_cell);

        div()
            .id(SharedString::from(format!("triggers-row-{}", id.0)))
            .w_full()
            .flex()
            .bg(row_bg)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .when(!instance.enabled, |row| row.opacity(DISABLED_OPACITY))
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
            .child(div().w(STRIPE_W).flex_none().bg(stripe_color))
            .child(content)
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let menu_open = self.menu_open == Some(id);
        let block_delete = !instance.used_in.is_empty();
        let view = cx.entity();

        menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from(format!("triggers-menu-rename-{}", id.0)),
                    "Rename\u{2026}",
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.start_rename(id, window, cx)
                    }),
                )
                .icon(Icon::Pencil)
                .into(),
                menu_item(
                    SharedString::from(format!("triggers-menu-template-{}", id.0)),
                    "Use as template",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.use_as_template(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("triggers-menu-delete-{}", id.0)),
                    "Delete\u{2026}",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .disabled(block_delete)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("triggers-menu-trigger-{}", id.0)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_menu(id, cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn render_empty(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let has_filter = self.has_active_filter();
        let (glyph, glyph_color) = if has_filter {
            (Icon::MoodSmile, palette.text_faint)
        } else {
            (Icon::Bolt, palette.warning)
        };
        let title = if has_filter {
            "No matches"
        } else {
            "No custom trigger instances yet"
        };
        let body = if has_filter {
            "Try a different filter combination.".to_owned()
        } else {
            "Triggers are named, reusable configurations of an event source. \
             Multiple actions can share one trigger."
                .to_owned()
        };

        let action: AnyElement = if has_filter {
            ghost_button_with_icon(Icon::X, "Clear filters", palette)
                .on_click(
                    "triggers-empty-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_filters(cx)),
                )
                .into_any_element()
        } else {
            primary_button_with_icon(Icon::Plus, "Create your first trigger", palette)
                .on_click(
                    "triggers-empty-new",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.new_trigger(cx)),
                )
                .into_any_element()
        };

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(EMPTY_TILE)
            .rounded(EMPTY_TILE_RADIUS)
            .bg(palette.shell)
            .child(icon(glyph, EMPTY_GLYPH, glyph_color));

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(EMPTY_PAD_V)
            .px(EMPTY_PAD_H)
            .child(tile)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(EMPTY_TITLE_FS)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(360.0))
                    .text_center()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(EMPTY_BODY_FS)
                    .text_color(palette.text_muted)
                    .child(body),
            )
            .child(action)
            .into_any_element()
    }

    // --- render: config side-sheet ----------------------------------------

    /// A section caption row: the mono meta label on the left, an optional note on the
    /// right (e.g. the override count beside "CONFIGURATION").
    fn section_label(
        &self,
        palette: &ForgePalette,
        label: impl Into<SharedString>,
        note: Option<AnyElement>,
    ) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .mb(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label.into()),
            )
            .children(note)
    }

    fn render_side_sheet(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w(SHEET_W)
            .flex_none()
            .h_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(palette.elevated)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(self.render_sheet_header(instance, palette, cx))
            .child(self.render_sheet_body(instance, palette, cx))
            .child(self.render_sheet_footer(instance, palette, cx))
            .into_any_element()
    }

    fn render_sheet_header(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let dot_color = instance.kind.platform().dot(palette);

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(SHEET_TILE)
            .rounded(SHEET_TILE_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(instance.kind.glyph(), SHEET_TILE_GLYPH, dot_color));

        // Name: the inline rename field while this instance is being renamed in the
        // header, otherwise a click-to-rename label.
        let name_el: AnyElement = match self.sheet_rename.as_ref().filter(|r| r.id == id) {
            Some(form) => div().w_full().child(form.field.clone()).into_any_element(),
            None => div()
                .id(SharedString::from(format!("triggers-sheet-name-{}", id.0)))
                .cursor_pointer()
                .overflow_hidden()
                .font_family(DEFAULT_BODY_FAMILY)
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(instance.name.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_sheet_rename(id, window, cx)
                }))
                .into_any_element(),
        };

        let kind_id_row = div()
            .flex()
            .items_center()
            .gap(SHEET_KIND_DOT)
            .mt(px(2.0))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(status_dot(dot_color, SHEET_KIND_DOT))
            .child(instance.kind.kind_id());

        let title_col = div()
            .flex_1()
            .min_w(px(0.0))
            .child(name_el)
            .child(kind_id_row);

        let toggle_el = toggle(instance.enabled, palette).on_click(
            SharedString::from(format!("triggers-sheet-toggle-{}", id.0)),
            cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_enable(id, cx)),
        );

        let close = div()
            .id("triggers-sheet-close")
            .flex_none()
            .cursor_pointer()
            .p(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::X, SHEET_CLOSE_GLYPH, palette.text_faint))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_sheet(cx)));

        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(SHEET_HEADER_PAD_V)
            .px(spacing(Spacing::Md, Density::Cozy))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(tile)
            .child(title_col)
            .child(toggle_el)
            .child(close)
            .into_any_element()
    }

    fn render_sheet_body(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("triggers-sheet-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(SHEET_BODY_PAD_V)
            .px(spacing(Spacing::Md, Density::Cozy))
            .child(self.render_config_section(instance, palette, cx))
            .child(self.render_used_in_section(instance, palette, cx))
            .into_any_element()
    }

    fn render_config_section(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let fields = instance.effective_config();
        let override_count = instance.overrides.len();

        let note: AnyElement = if override_count > 0 {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(SECTION_NOTE_FS)
                .text_color(palette.bits)
                .child(format!("{override_count} overridden"))
                .into_any_element()
        } else {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(SECTION_NOTE_FS)
                .text_color(palette.text_faint)
                .child("all defaults")
                .into_any_element()
        };

        let inner: AnyElement = if fields.is_empty() {
            div()
                .italic()
                .py(px(12.0))
                .px(SHEET_BODY_PAD_V)
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(SHEET_TEXT_FS)
                .text_color(palette.text_faint)
                .child("This trigger kind has no configurable fields.")
                .into_any_element()
        } else {
            let last = fields.len() - 1;
            let mut col = div().flex().flex_col();
            for (i, field) in fields.iter().enumerate() {
                col = col.child(self.render_config_field(id, field, i == last, palette, cx));
            }
            col.into_any_element()
        };

        let card = div()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .rounded(CFG_CARD_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(inner);

        div()
            .mb(spacing(Spacing::Md, Density::Cozy))
            .child(self.section_label(palette, "CONFIGURATION", Some(note)))
            .child(card)
            .into_any_element()
    }

    fn render_config_field(
        &self,
        id: InstanceId,
        field: &ConfigField,
        last: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editing = self
            .field_edit
            .as_ref()
            .filter(|f| f.id == id && f.key == field.key);

        let key_color = if field.overridden {
            palette.bits
        } else {
            palette.text_muted
        };
        let key_cell = div()
            .w(CFG_KEY_W)
            .flex_none()
            .overflow_hidden()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(CFG_KEY_FS)
            .text_color(key_color)
            .child(field.key.clone());

        let value_cell: AnyElement = match editing {
            Some(form) => div()
                .flex_1()
                .min_w(px(0.0))
                .child(form.field.clone())
                .into_any_element(),
            None => {
                let (value_color, italicize) = if field.overridden {
                    (palette.text_primary, false)
                } else {
                    (palette.text_faint, true)
                };
                let key = field.key.clone();
                let cell = div()
                    .id(SharedString::from(format!(
                        "triggers-cfg-{}-{}",
                        id.0, field.key
                    )))
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .cursor_pointer()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(SHEET_TEXT_FS)
                    .text_color(value_color)
                    .when(italicize, |c| c.italic())
                    .child(field.value.clone())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.start_field_edit(id, key.clone(), window, cx)
                    }));
                cell.into_any_element()
            }
        };

        let clear_cell: AnyElement =
            if field.overridden {
                let key = field.key.clone();
                div()
                    .id(SharedString::from(format!(
                        "triggers-cfg-clear-{}-{}",
                        id.0, field.key
                    )))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(CFG_CLEAR_W)
                    .cursor_pointer()
                    .rounded(px(3.0))
                    .text_color(palette.text_faint)
                    .hover(|s| s.bg(palette.surface_overlay).text_color(palette.random))
                    .child(icon_inherit(Icon::X, CFG_CLEAR_GLYPH))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.clear_override(id, &key, cx)
                    }))
                    .into_any_element()
            } else {
                div().w(CFG_CLEAR_W).flex_none().into_any_element()
            };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(CFG_ROW_PAD_V)
            .px(CFG_ROW_PAD_H)
            .when(!last, |row| {
                row.border_b(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .border_dashed()
            })
            .child(key_cell)
            .child(value_cell)
            .child(clear_cell)
            .into_any_element()
    }

    fn render_used_in_section(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = instance.used_in.len();
        let label = if count > 0 {
            format!("USED IN ({count})")
        } else {
            "USED IN".to_owned()
        };

        let content: AnyElement = if instance.used_in.is_empty() {
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .py(EMPTY_USED_PAD_V)
                .px(EMPTY_USED_PAD_H)
                .rounded(CFG_CARD_RADIUS)
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .border_dashed()
                .text_center()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(SHEET_TEXT_FS)
                .text_color(palette.text_faint)
                .child("Not linked to any action yet.")
                .child("Open an action and add this trigger from the picker.")
                .into_any_element()
        } else {
            let mut col = div().flex().flex_col().gap(px(3.0));
            for action_id in &instance.used_in {
                col = col.child(self.render_used_in_row(action_id, palette, cx));
            }
            col.into_any_element()
        };

        div()
            .child(self.section_label(palette, label, None))
            .child(content)
            .into_any_element()
    }

    fn render_used_in_row(
        &self,
        action_id: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let transparent: Rgba = gpui::transparent_black().into();
        let label = SharedString::from(action_id.to_owned());
        div()
            .id(SharedString::from(format!("triggers-usedin-{action_id}")))
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(USED_ROW_PAD_V)
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(USED_ROW_RADIUS)
            .bg(palette.shell)
            .cursor_pointer()
            .border(BORDER_THIN)
            .border_color(transparent)
            .hover(|s| s.border_color(palette.border_regular))
            // The linked action jumps to the Action editor; the deep-link is not wired
            // in forge-desktop yet, so the click is inert (never panics).
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.navigate_to_action(cx)))
            .child(icon(Icon::Bolt, USED_BOLT, palette.brand))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(icon(Icon::ExternalLink, USED_ARROW, palette.text_faint))
            .into_any_element()
    }

    fn render_sheet_footer(
        &self,
        instance: &TriggerInstance,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let can_delete = instance.used_in.is_empty();

        let template = ghost_button_with_icon(Icon::Copy, "Use as template", palette).on_click(
            SharedString::from(format!("triggers-sheet-template-{}", id.0)),
            cx.listener(move |this, _: &ClickEvent, _, cx| this.use_as_template(id, cx)),
        );

        let (delete_color, delete_opacity) = if can_delete {
            (palette.random, 1.0)
        } else {
            (palette.disabled, DISABLED_BTN_OPACITY)
        };
        let delete_base = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(DELETE_BTN_PAD_V)
            .px(spacing(Spacing::Sm, Density::Cozy))
            .rounded(DELETE_BTN_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .opacity(delete_opacity)
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(SHEET_TEXT_FS)
            .text_color(delete_color)
            .child(icon(Icon::Eraser, DELETE_GLYPH, delete_color))
            .child("Delete");
        let delete_el: AnyElement = if can_delete {
            delete_base
                .id("triggers-sheet-delete")
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .into_any_element()
        } else {
            delete_base.into_any_element()
        };

        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(SHEET_FOOTER_PAD_V)
            .px(spacing(Spacing::Md, Density::Cozy))
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(template)
            .child(div().flex_1())
            .child(delete_el)
            .into_any_element()
    }

    // --- render: modals ---------------------------------------------------

    fn render_disable_confirm(
        &self,
        id: InstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, count) = self
            .find(id)
            .map(|i| (i.name.clone(), i.used_in.len()))
            .unwrap_or_default();
        let plural = if count == 1 { "action" } else { "actions" };
        let card = confirm_modal(
            format!("Disable {name}?"),
            format!(
                "Disabling this trigger will pause it for {count} {plural}. \
                 They won't fire until the trigger is re-enabled."
            ),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint("to cancel")
        .on_cancel(
            "triggers-disable-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disable(cx)),
        )
        .on_confirm(
            "triggers-disable-confirm",
            "Disable anyway",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disable_now(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-disable-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disable(cx));
            })
            .into_any_element()
    }

    fn render_delete_confirm(
        &self,
        id: InstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, used_in) = self
            .find(id)
            .map(|i| (i.name.clone(), i.used_in.len()))
            .unwrap_or_default();
        let message = if used_in > 0 {
            let plural = if used_in == 1 { "action" } else { "actions" };
            format!("This trigger is used by {used_in} {plural}. Remove it from them first.")
        } else {
            "This deletes the trigger instance permanently.".to_owned()
        };
        let card = confirm_modal(
            "Delete trigger?",
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "triggers-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "triggers-delete-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }

    fn render_rename_modal(
        &self,
        form: &RenameForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let valid = !form.field.read(cx).content().trim().is_empty();

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("NAME"),
            )
            .child(div().child(form.field.clone()));

        let cancel = secondary_button("Cancel", palette).on_click(
            "triggers-rename-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.rename = None;
                cx.notify();
            }),
        );
        let save = primary_button("Save", palette).disabled(!valid).on_click(
            "triggers-rename-save",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit_rename(cx)),
        );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let card = modal("Rename trigger", body, palette)
            .size(ModalSize::Sm)
            .footer(footer)
            .kbd_hint("ENTER to save \u{b7} ESC to cancel")
            .on_close(
                "triggers-rename-close",
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.rename = None;
                    cx.notify();
                }),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-rename-scrim", move |_window, cx| {
                view.update(cx, |this, cx| {
                    this.rename = None;
                    cx.notify();
                });
            })
            .into_any_element()
    }
}

impl Render for TriggersRegistryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let header = self.render_header(&palette);
        let filter_bar = self.render_filter_bar(&palette, cx);
        let list = self.render_list(&palette, cx);

        // The selected instance opens the config side-sheet to the right of the list.
        let side_sheet = self
            .selected
            .and_then(|id| self.instances.iter().find(|i| i.id == id))
            .map(|instance| self.render_side_sheet(instance, &palette, cx));

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(list)
            .children(side_sheet);

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

// ── seeded stub state ─────────────────────────────────────────────────────

/// The representative instance list the screen seeds before a trigger-instance repo is
/// wired, mirroring the design's sample so every platform chip, both enabled states,
/// the used/unused split, the override badge and the side-sheet config editor render
/// populated. Each `overrides` entry re-authors one schema key; `used_in` carries the
/// linked action ids surfaced in the side-sheet's "used in" list.
fn seed_instances(mint: &mut impl FnMut() -> InstanceId) -> Vec<TriggerInstance> {
    let ovr = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    };
    let used = |ids: &[&str]| -> Vec<String> { ids.iter().map(|s| (*s).to_owned()).collect() };

    let seed = |mint: &mut dyn FnMut() -> InstanceId,
                name: &str,
                kind: TriggerKind,
                enabled: bool,
                overrides: BTreeMap<String, String>,
                used_in: Vec<String>| TriggerInstance {
        id: mint(),
        name: name.to_owned(),
        kind,
        enabled,
        overrides,
        used_in,
    };

    vec![
        seed(
            mint,
            "MySubs",
            TriggerKind::NewSubscriber,
            true,
            ovr(&[("cooldown", "60s")]),
            used(&["!so", "SocialReminder", "HydrateCheck"]),
        ),
        seed(
            mint,
            "MySubs v2",
            TriggerKind::NewSubscriber,
            true,
            ovr(&[("cooldown", "25s")]),
            used(&["GoalProgress"]),
        ),
        seed(
            mint,
            "VIPSubs",
            TriggerKind::NewSubscriber,
            true,
            ovr(&[("tier_filter", "Tier 3")]),
            used(&["BangerMode", "TTS Boost"]),
        ),
        seed(
            mint,
            "HypeRaid",
            TriggerKind::RaidReceived,
            true,
            ovr(&[("min_viewers", "10")]),
            used(&["SocialReminder"]),
        ),
        seed(
            mint,
            "SoCmd",
            TriggerKind::ChatCommand,
            true,
            ovr(&[
                ("command", "!so"),
                ("cooldown", "30s"),
                ("permission", "mods+"),
            ]),
            used(&["!so"]),
        ),
        seed(
            mint,
            "QuoteCmd",
            TriggerKind::ChatCommand,
            true,
            ovr(&[("command", "!quote"), ("cooldown", "5s")]),
            used(&["!quote"]),
        ),
        seed(
            mint,
            "FollowageCmd",
            TriggerKind::ChatCommand,
            true,
            ovr(&[("command", "!followage"), ("cooldown", "5s")]),
            used(&["!followage"]),
        ),
        seed(
            mint,
            "StatsCmd",
            TriggerKind::ChatCommand,
            false,
            ovr(&[("command", "!stats"), ("permission", "vip+")]),
            used(&["!stats"]),
        ),
        seed(
            mint,
            "UptimeCmd",
            TriggerKind::ChatCommand,
            true,
            ovr(&[("command", "!uptime"), ("cooldown", "5s")]),
            used(&["!uptime"]),
        ),
        seed(
            mint,
            "HourlyPromo",
            TriggerKind::IntervalWhenLive,
            true,
            ovr(&[("every", "10m"), ("jitter", "30s")]),
            used(&["SocialReminder"]),
        ),
        seed(
            mint,
            "Hydrate15",
            TriggerKind::IntervalWhenLive,
            true,
            ovr(&[("every", "15m")]),
            used(&["HydrateCheck"]),
        ),
        seed(
            mint,
            "DailyReset",
            TriggerKind::CronSchedule,
            true,
            ovr(&[("cron", "0 4 * * *")]),
            used(&[]),
        ),
        seed(
            mint,
            "BangerScene",
            TriggerKind::SceneChanged,
            true,
            ovr(&[("to", "BangerCam")]),
            used(&[]),
        ),
        seed(
            mint,
            "RecStopHook",
            TriggerKind::ReplaySaved,
            true,
            ovr(&[("rename_to", "clip-{ts}.mkv")]),
            used(&[]),
        ),
    ]
}
