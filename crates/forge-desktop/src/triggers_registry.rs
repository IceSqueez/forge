use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement, ModalSize, OverlayPosition,
    Radius, Spacing, TextInput, badge, breadcrumb, chip, confirm_modal, ghost_button_with_icon,
    icon, menu_button, menu_divider, menu_item, modal, overlay, primary_button,
    primary_button_with_icon, search_input, secondary_button, spacing, status_dot, toggle,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString, Subscription,
    Window, div, prelude::*, px,
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
            Platform::Script => palette.accent_pink_light,
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
}

/// A cached trigger-instance summary — the row's payload. The real screen reads these
/// from the trigger-instance repo over the runtime→UI bridge; here they are seeded.
/// `override_count` and `used_in` stand in for the design's override map and usage
/// list — TR-A needs only their cardinalities.
struct TriggerInstance {
    id: InstanceId,
    name: String,
    kind: TriggerKind,
    enabled: bool,
    override_count: usize,
    used_in: usize,
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

/// The Triggers registry screen view-entity: a page header (breadcrumb + instance
/// stats), a filter bar (search, platform chips, usage chips, New-trigger button), and
/// a scrolling instance list with a column caption.
///
/// Owns its instances, selection and filter state as seeded stub state — the real
/// screen loads instances from the repo over the runtime→UI bridge and drives every
/// mutation through the runtime handle. Here the CRUD (enable/disable with
/// confirm-when-used, delete blocked-when-used, rename, use-as-template clone) mutates
/// this cached state locally. The config side-sheet (TR-B) and the kind-picker create
/// flow (TR-C) land in follow-up slices; for now a selected row is only highlighted.
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
        self.instances.iter().filter(|i| i.used_in > 0).count()
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
            UsageFilter::Used if instance.used_in == 0 => return false,
            UsageFilter::Unused if instance.used_in > 0 => return false,
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
        self.selected = Some(id);
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
        } else if instance.used_in > 0 {
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
            && self.find(id).is_some_and(|i| i.used_in == 0)
        {
            self.instances.retain(|i| i.id != id);
            if self.selected == Some(id) {
                self.selected = None;
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
                override_count: src.override_count,
                used_in: 0,
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
                override_count: 0,
                used_in: 0,
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
        if instance.override_count > 0 {
            let label = if instance.override_count == 1 {
                "1 override".to_owned()
            } else {
                format!("{} overrides", instance.override_count)
            };
            kind = kind.child(badge(
                palette.surface_overlay,
                palette.accent_pink_light,
                label,
                true,
                BADGE_FS,
            ));
        }

        // Used-in cell: "used in N" (N green) or an italic "unused".
        let used: AnyElement = if instance.used_in > 0 {
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
                        .child(instance.used_in.to_string()),
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
        let block_delete = instance.used_in > 0;
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

    // --- render: modals ---------------------------------------------------

    fn render_disable_confirm(
        &self,
        id: InstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, count) = self
            .find(id)
            .map(|i| (i.name.clone(), i.used_in))
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
            .map(|i| (i.name.clone(), i.used_in))
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

        let body = div().flex_1().min_h(px(0.0)).flex().flex_row().child(list);

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
/// the used/unused split and the override badge render populated.
fn seed_instances(mint: &mut impl FnMut() -> InstanceId) -> Vec<TriggerInstance> {
    let seed = |mint: &mut dyn FnMut() -> InstanceId,
                name: &str,
                kind: TriggerKind,
                enabled: bool,
                override_count: usize,
                used_in: usize| TriggerInstance {
        id: mint(),
        name: name.to_owned(),
        kind,
        enabled,
        override_count,
        used_in,
    };

    vec![
        seed(mint, "MySubs", TriggerKind::NewSubscriber, true, 1, 3),
        seed(mint, "MySubs v2", TriggerKind::NewSubscriber, true, 1, 1),
        seed(mint, "VIPSubs", TriggerKind::NewSubscriber, true, 1, 2),
        seed(mint, "HypeRaid", TriggerKind::RaidReceived, true, 1, 1),
        seed(mint, "SoCmd", TriggerKind::ChatCommand, true, 3, 1),
        seed(mint, "QuoteCmd", TriggerKind::ChatCommand, true, 2, 1),
        seed(mint, "FollowageCmd", TriggerKind::ChatCommand, true, 2, 1),
        seed(mint, "StatsCmd", TriggerKind::ChatCommand, false, 2, 1),
        seed(mint, "UptimeCmd", TriggerKind::ChatCommand, true, 2, 1),
        seed(
            mint,
            "HourlyPromo",
            TriggerKind::IntervalWhenLive,
            true,
            2,
            1,
        ),
        seed(mint, "Hydrate15", TriggerKind::IntervalWhenLive, true, 1, 1),
        seed(mint, "DailyReset", TriggerKind::CronSchedule, true, 1, 0),
        seed(mint, "BangerScene", TriggerKind::SceneChanged, true, 1, 0),
        seed(mint, "RecStopHook", TriggerKind::ReplaySaved, true, 1, 0),
    ]
}
