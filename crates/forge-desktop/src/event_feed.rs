use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XS,
    FONT_XXS, ForgePalette, Icon, Radius, SheetWidth, Spacing, badge, breadcrumb, chip, icon,
    radius, spacing, status_dot, with_alpha,
};
use forge_events::EventSource;
use gpui::{
    ClickEvent, Context, Div, Entity, Pixels, Rgba, ScrollHandle, ScrollWheelEvent, Stateful,
    Subscription, Window, div, prelude::*, px,
};

use crate::event_log::{EventFilter, EventItem, EventLog};
use crate::presentation::ActivePresentation;

const AT_BOTTOM_SLACK: f32 = 40.0;
const INSPECTOR_INITIAL: f32 = 300.0;
const INSPECTOR_MIN: f32 = 220.0;
const INSPECTOR_MAX: f32 = 540.0;

const TS_COL_W: Pixels = px(64.0);
const TYPE_COL_W: Pixels = px(104.0);
const STATUS_DOT: Pixels = px(6.0);
const ACTION_DIVIDER_W: Pixels = px(0.5);
const ACTION_DIVIDER_H: Pixels = px(14.0);
const ROW_RAIL_W: Pixels = px(2.0);
const ERROR_ROW_ALPHA: f32 = 0.06;
const ACTION_HOVER_ALPHA: f32 = 0.05;

const FILTER_TABS: [(&str, &str, EventFilter); 7] = [
    ("event-tab-all", "All", EventFilter::All),
    ("event-tab-chat", "Chat", EventFilter::Chat),
    ("event-tab-subs", "Subs", EventFilter::Subs),
    ("event-tab-bits", "Bits", EventFilter::Bits),
    ("event-tab-timers", "Timers", EventFilter::Timers),
    ("event-tab-obs", "OBS", EventFilter::Obs),
    ("event-tab-errors", "Errors", EventFilter::Errors),
];

pub struct EventFeedView {
    log: Entity<EventLog>,
    active_filter: EventFilter,
    /// `None` falls back to the newest row, keeping the inspector populated.
    selected: Option<gpui::SharedString>,
    auto_scroll: bool,
    inspector_width: f32,
    list_scroll: ScrollHandle,
    _log_obs: Subscription,
}

impl EventFeedView {
    pub fn new(log: Entity<EventLog>, cx: &mut Context<Self>) -> Self {
        let log_obs = cx.observe(&log, Self::on_log_changed);
        let list_scroll = ScrollHandle::new();
        list_scroll.scroll_to_bottom();
        Self {
            log,
            active_filter: EventFilter::default(),
            selected: None,
            auto_scroll: true,
            inspector_width: INSPECTOR_INITIAL,
            list_scroll,
            _log_obs: log_obs,
        }
    }

    fn on_log_changed(&mut self, _log: Entity<EventLog>, cx: &mut Context<Self>) {
        if self.auto_scroll {
            self.list_scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn set_filter(&mut self, filter: EventFilter, cx: &mut Context<Self>) {
        self.active_filter = filter;
        cx.notify();
    }

    fn select(&mut self, id: gpui::SharedString, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    fn deselect(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.log.update(cx, |log, cx| {
            log.toggle_paused();
            cx.notify();
        });
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.log.update(cx, |log, cx| {
            log.clear();
            cx.notify();
        });
        cx.notify();
    }

    fn export(&mut self, _cx: &mut Context<Self>) {}

    fn replay(&mut self, _cx: &mut Context<Self>) {}

    fn toggle_auto_scroll(&mut self, cx: &mut Context<Self>) {
        self.auto_scroll = !self.auto_scroll;
        if self.auto_scroll {
            self.list_scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn set_inspector_width(&mut self, width: &Pixels, cx: &mut Context<Self>) {
        self.inspector_width = f32::from(*width).clamp(INSPECTOR_MIN, INSPECTOR_MAX);
        cx.notify();
    }

    fn on_wheel(
        &mut self,
        _event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remaining = self.list_scroll.max_offset().height + self.list_scroll.offset().y;
        self.auto_scroll = remaining <= px(AT_BOTTOM_SLACK);
        cx.notify();
    }

    fn render_header(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let paused = self.log.read(cx).is_paused();
        let (status_ink, status_label) = if paused {
            (palette.warning, "PAUSED")
        } else {
            (palette.success, "LIVE")
        };

        let status_badge = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(px(2.0))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .child(status_dot(status_ink, px(5.0)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(status_ink)
                    .child(status_label),
            );

        let count = self.log.read(cx).items().len();
        let count_readout = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(format!("{count}")),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("events · live stream"),
            );

        let cluster = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(status_badge)
            .child(count_readout);

        breadcrumb(vec![BreadcrumbCrumb::leaf("Event feed")], palette).right(cluster)
    }

    fn render_toolbar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let paused = self.log.read(cx).is_paused();
        let counts = self.filter_counts(cx);

        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, name, filter) in FILTER_TABS {
            let glyph = Self::tab_glyph(filter, palette);
            let label = format!("{name}  {}", counts.get(filter));
            let active = self.active_filter == filter;
            chips = chips.child(
                chip(label, glyph, active, palette)
                    .density(density)
                    .on_click(
                        id,
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.set_filter(filter, cx)),
                    ),
            );
        }

        let (pause_icon, pause_label) = if paused {
            (Icon::PlayerPlay, "Resume")
        } else {
            (Icon::PlayerPause, "Pause")
        };
        let pause = Self::action_shell("event-action-pause", palette, density)
            .child(icon(pause_icon, FONT_XS, palette.text_secondary))
            .child(Self::action_label(pause_label, palette))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pause(cx)));

        let divider = div()
            .w(ACTION_DIVIDER_W)
            .h(ACTION_DIVIDER_H)
            .bg(palette.border_regular);

        let clear = Self::action_shell("event-action-clear", palette, density)
            .child(icon(Icon::Eraser, FONT_XS, palette.text_secondary))
            .child(Self::action_label("Clear", palette))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear(cx)));

        let export = Self::action_shell("event-action-export", palette, density)
            .child(icon(Icon::Download, FONT_XS, palette.text_secondary))
            .child(Self::action_label("Export", palette))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export(cx)));

        let (scroll_ink, scroll_label) = if self.auto_scroll {
            (palette.success, "Auto-scroll on")
        } else {
            (palette.disabled, "Auto-scroll off")
        };
        let auto_scroll = Self::action_shell("event-action-autoscroll", palette, density)
            .child(status_dot(scroll_ink, STATUS_DOT))
            .child(Self::action_label(scroll_label, palette))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_auto_scroll(cx)));

        let actions = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(pause)
            .child(divider)
            .child(clear)
            .child(export)
            .child(auto_scroll);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(chips)
            .child(actions)
    }

    fn render_row(
        &self,
        idx: usize,
        item: &EventItem,
        selected: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let row_bg = if selected {
            Some(palette.elevated)
        } else if item.is_error {
            Some(with_alpha(palette.random, ERROR_ROW_ALPHA))
        } else {
            None
        };
        let rail = if selected {
            palette.brand
        } else {
            with_alpha(palette.brand, 0.0)
        };

        let source_badge = badge(
            palette.surface_overlay,
            source_color(item.source, palette),
            source_label(item.source),
            true,
            FONT_XXS,
        );

        let type_cell = div()
            .flex_none()
            .w(TYPE_COL_W)
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(type_color(&item.kind, item.is_error, palette))
            .child(item.kind.clone());

        let summary = div().flex_1().overflow_hidden().child(
            div()
                .truncate()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(item.summary.clone()),
        );

        // gpui shares one `border_color` across all sides, so the selection rail is a
        // separate leading strip rather than a left border (keeps the row's bottom
        // hairline a distinct color).
        let rail_strip = div().flex_none().w(ROW_RAIL_W).bg(rail);

        let content = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(10.0))
            .py(px(5.0))
            .px(px(14.0))
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w(TS_COL_W)
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(item.timestamp.clone()),
            )
            .child(source_badge)
            .child(type_cell)
            .child(summary)
            .children(item.result_tag.clone().map(|tag| {
                div()
                    .flex_none()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(result_color(&item.kind, item.is_error, palette))
                    .child(tag)
            }));

        let mut row = div()
            .id(("event-row", idx))
            .flex()
            .border_b(BORDER_THIN)
            .border_color(palette.elevated)
            .cursor_pointer()
            .on_click(cx.listener({
                let id = item.id.clone();
                move |this, _: &ClickEvent, _, cx| this.select(id.clone(), cx)
            }))
            .child(rail_strip)
            .child(content);
        if let Some(bg) = row_bg {
            row = row.bg(bg);
        }
        row
    }

    fn render_list(
        &self,
        palette: &ForgePalette,
        density: Density,
        selected_id: Option<&gpui::SharedString>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let filter = self.active_filter;
        let visible: Vec<EventItem> = self
            .log
            .read(cx)
            .items()
            .iter()
            .filter(|item| item.matches(filter))
            .cloned()
            .collect();

        let empty = visible.is_empty();
        let mut list = div().flex().flex_col();
        for (idx, item) in visible.iter().enumerate() {
            let is_sel = selected_id == Some(&item.id);
            list = list.child(self.render_row(idx, item, is_sel, palette, cx));
        }

        let empty_note = empty.then(|| {
            let label = if matches!(filter, EventFilter::All) {
                "No events yet."
            } else {
                "No events match this filter."
            };
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .p(spacing(Spacing::Md, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(label)
        });

        div()
            .id("event-feed-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .bg(palette.base)
            .child(list)
            .children(empty_note)
    }

    fn render_inspector(
        &self,
        item: &EventItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let content = self.inspector_content(item, palette, cx);
        let width = self.inspector_width.clamp(INSPECTOR_MIN, INSPECTOR_MAX);

        forge_components::side_sheet(px(width), content, palette)
            .header("Event inspector")
            .header_icon(Icon::Pin, palette.brand)
            .on_close(
                "event-inspector-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.deselect(cx)),
            )
            .on_resize(
                "event-inspector-resize",
                SheetWidth::new(width, INSPECTOR_MIN, INSPECTOR_MAX),
                cx.listener(|this, width: &Pixels, _, cx| this.set_inspector_width(width, cx)),
            )
    }

    fn inspector_content(
        &self,
        item: &EventItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id_str = item.id.as_ref();
        let last6 = &id_str[id_str.len().saturating_sub(6)..];

        let summary_card = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .p(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, Density::Cozy))
                    .child(badge(
                        palette.surface_overlay,
                        source_color(item.source, palette),
                        source_label(item.source),
                        true,
                        FONT_XXS,
                    ))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(item.kind.clone()),
                    ),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(format!("{} · #{last6}", item.timestamp)),
            );

        let payload = self.payload_block(item, palette);

        let caused = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .p(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Bolt, px(12.0), palette.brand))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child("—"),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.success)
                    .child("—"),
            );

        let replay = div()
            .id("event-inspector-replay")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.replay(cx)))
            .child(icon(Icon::Repeat, FONT_XS, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child("Replay this event"),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(summary_card)
            .child(self.section_label("PAYLOAD", palette))
            .child(payload)
            .child(self.section_label("CAUSED", palette))
            .child(caused)
            .child(replay)
    }

    fn section_label(
        &self,
        text: &'static str,
        palette: &ForgePalette,
    ) -> impl IntoElement + use<> {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(text)
    }

    fn payload_block(&self, item: &EventItem, palette: &ForgePalette) -> impl IntoElement + use<> {
        let key = palette.info;
        let val = palette.success;
        let brace = palette.text_muted;

        let line = |indent: f32, children: Vec<gpui::AnyElement>| {
            div().flex().pl(px(indent)).children(children)
        };
        let kv = |k: &str, v: String, k_color: Rgba, v_color: Rgba| -> Vec<gpui::AnyElement> {
            vec![
                div()
                    .text_color(k_color)
                    .child(format!("\"{k}\""))
                    .into_any_element(),
                div().text_color(brace).child(": ").into_any_element(),
                div()
                    .text_color(v_color)
                    .child(format!("\"{v}\""))
                    .into_any_element(),
            ]
        };

        let mut block = div()
            .flex()
            .flex_col()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .p(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(div().text_color(brace).child("{"))
            .child(line(10.0, kv("event", item.kind.to_string(), key, val)))
            .child(line(
                10.0,
                kv("source", source_label(item.source).to_lowercase(), key, val),
            ));

        if !item.user_login.is_empty() {
            block = block
                .child(line(
                    10.0,
                    vec![
                        div().text_color(key).child("\"user\"").into_any_element(),
                        div().text_color(brace).child(": {").into_any_element(),
                    ],
                ))
                .child(line(
                    20.0,
                    kv("login", item.user_login.to_string(), key, val),
                ))
                .child(line(
                    20.0,
                    kv("platform", item.user_platform.to_string(), key, val),
                ))
                .child(line(
                    10.0,
                    vec![div().text_color(brace).child("},").into_any_element()],
                ));
        }

        block
            .child(line(
                10.0,
                vec![
                    div()
                        .text_color(key)
                        .child("\"timestamp\"")
                        .into_any_element(),
                    div().text_color(brace).child(": ").into_any_element(),
                    div()
                        .text_color(palette.warning)
                        .child(item.timestamp.to_string())
                        .into_any_element(),
                ],
            ))
            .child(div().text_color(brace).child("}"))
    }

    fn resolved_selection(&self, cx: &Context<Self>) -> Option<EventItem> {
        let log = self.log.read(cx);
        if let Some(id) = &self.selected
            && let Some(found) = log.items().iter().find(|i| &i.id == id)
        {
            return Some(found.clone());
        }
        log.items().last().cloned()
    }

    fn filter_counts(&self, cx: &Context<Self>) -> FilterCounts {
        let mut counts = FilterCounts::default();
        for item in self.log.read(cx).items() {
            counts.all += 1;
            if item.matches(EventFilter::Chat) {
                counts.chat += 1;
            }
            if item.matches(EventFilter::Subs) {
                counts.subs += 1;
            }
            if item.matches(EventFilter::Bits) {
                counts.bits += 1;
            }
            if item.matches(EventFilter::Timers) {
                counts.timers += 1;
            }
            if item.matches(EventFilter::Obs) {
                counts.obs += 1;
            }
            if item.matches(EventFilter::Errors) {
                counts.errors += 1;
            }
        }
        counts
    }

    fn tab_glyph(filter: EventFilter, palette: &ForgePalette) -> forge_components::ChipGlyph {
        use forge_components::ChipGlyph;
        match filter {
            EventFilter::All => ChipGlyph::None,
            EventFilter::Chat => ChipGlyph::Icon(Icon::MessageCircle, palette.info),
            EventFilter::Subs => ChipGlyph::Icon(Icon::Star, palette.brand),
            EventFilter::Bits => ChipGlyph::Icon(Icon::Coin, palette.warning),
            EventFilter::Timers => ChipGlyph::Icon(Icon::Clock, palette.warning),
            EventFilter::Obs => ChipGlyph::Icon(Icon::Broadcast, palette.success),
            EventFilter::Errors => ChipGlyph::Icon(Icon::AlertTriangle, palette.random),
        }
    }

    fn action_shell(id: &'static str, palette: &ForgePalette, density: Density) -> Stateful<Div> {
        let hover = with_alpha(palette.border_regular, ACTION_HOVER_ALPHA);
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
    }

    fn action_label(text: &'static str, palette: &ForgePalette) -> impl IntoElement + use<> {
        div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_secondary)
            .child(text)
    }
}

impl Render for EventFeedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let selection = self.resolved_selection(cx);
        let selected_id = selection.as_ref().map(|i| i.id.clone());

        let header = self.render_header(&palette, cx);
        let toolbar = self.render_toolbar(&palette, density, cx);
        let list = self.render_list(&palette, density, selected_id.as_ref(), cx);
        let inspector = selection
            .as_ref()
            .map(|item| self.render_inspector(item, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(list)
                    .children(inspector),
            )
    }
}

#[derive(Default)]
struct FilterCounts {
    all: u32,
    chat: u32,
    subs: u32,
    bits: u32,
    timers: u32,
    obs: u32,
    errors: u32,
}

impl FilterCounts {
    fn get(&self, filter: EventFilter) -> u32 {
        match filter {
            EventFilter::All => self.all,
            EventFilter::Chat => self.chat,
            EventFilter::Subs => self.subs,
            EventFilter::Bits => self.bits,
            EventFilter::Timers => self.timers,
            EventFilter::Obs => self.obs,
            EventFilter::Errors => self.errors,
        }
    }
}

fn source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "TWITCH",
        EventSource::YouTube => "YOUTUBE",
        EventSource::Kick => "KICK",
        EventSource::Core => "CORE",
        EventSource::Rhai => "RHAI",
        EventSource::Http => "HTTP",
        EventSource::Obs => "OBS",
        EventSource::VTube => "VTUBE",
        EventSource::Discord => "DISCORD",
        EventSource::Midi => "MIDI",
        EventSource::Hotkey => "HOTKEY",
        EventSource::Timer => "TIMER",
        EventSource::Server => "SERVER",
        EventSource::Audio => "AUDIO",
    }
}

fn source_color(source: EventSource, palette: &ForgePalette) -> Rgba {
    match source {
        EventSource::Twitch => palette.brand,
        EventSource::YouTube => palette.random,
        EventSource::Kick => palette.info,
        EventSource::Obs => palette.success,
        EventSource::VTube => palette.warning,
        EventSource::Timer => palette.warning,
        EventSource::Rhai | EventSource::Core => palette.text_secondary,
        EventSource::Http => palette.info,
        EventSource::Discord => palette.info,
        EventSource::Midi | EventSource::Hotkey => palette.info,
        EventSource::Server => palette.success,
        EventSource::Audio => palette.warning,
    }
}

fn is_chat_message_kind(kind: &str) -> bool {
    matches!(
        kind,
        "chat.message"
            | "youtube.chat.message"
            | "kick.chat.message"
            | "youtube.chat.command"
            | "kick.chat.command"
    )
}

fn type_color(kind: &str, is_error: bool, palette: &ForgePalette) -> Rgba {
    if is_error {
        return palette.random;
    }
    match kind {
        k if is_chat_message_kind(k) => palette.info,
        "command.matched" => palette.brand,
        "action.done" | "scene.changed" => palette.success,
        "chat.send" => palette.info,
        "action.start" | "script.exec" | "timer.tick" | "subaction.run" | "global.set"
        | "global.incr" => palette.warning,
        k if k.contains("cheer") || k.contains("bits") => palette.warning,
        k if k.contains("raid") => palette.random,
        k if k.contains("sub") || k.contains("follow") => palette.brand,
        _ => palette.text_secondary,
    }
}

fn result_color(kind: &str, is_error: bool, palette: &ForgePalette) -> Rgba {
    if is_error {
        return palette.warning;
    }
    match kind {
        "action.done" | "command.matched" | "chat.send" => palette.success,
        k if is_chat_message_kind(k) => palette.success,
        _ => palette.text_muted,
    }
}
