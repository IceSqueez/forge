use std::collections::{HashMap, HashSet};

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent,
    PlatformKind, Radius, SearchState, SheetWidth, Spacing, TextInput, ToastKind, badge,
    body_family, chip, empty_state, header_status, icon, mono_family, page_frame, platform_color,
    radius, spacing, status_dot, tr, with_alpha,
};
use forge_events::EventSource;
use gpui::{
    AnyElement, ClickEvent, Context, Div, Entity, FocusHandle, Pixels, Rgba, ScrollStrategy,
    ScrollWheelEvent, Stateful, Subscription, UniformListScrollHandle, Window, div, prelude::*, px,
    uniform_list,
};

use crate::actions::{LIST_CONTEXT, ListActivate, ListSelectNext, ListSelectPrev};
use crate::async_bridge;
use crate::event_log::{EventFilter, EventItem, EventLog};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const INSPECTOR_INITIAL: f32 = 300.0;
const INSPECTOR_MIN: f32 = 220.0;
const INSPECTOR_MAX: f32 = 540.0;

const SEARCH_W: Pixels = px(240.0);

const TS_COL_W: Pixels = px(88.0);
const TYPE_COL_W: Pixels = px(104.0);
const ROW_FS: Pixels = px(11.0);
const BADGE_FS: Pixels = px(9.0);
const SUFFIX_FS: Pixels = px(10.0);
const STATUS_DOT: Pixels = px(6.0);
const ROW_RAIL_W: Pixels = px(2.0);
const ERROR_ROW_ALPHA: f32 = 0.06;

const FILTER_TABS: [(&str, EventFilter); 7] = [
    ("event-tab-all", EventFilter::All),
    ("event-tab-chat", EventFilter::Chat),
    ("event-tab-subs", EventFilter::Subs),
    ("event-tab-bits", EventFilter::Bits),
    ("event-tab-timers", EventFilter::Timers),
    ("event-tab-obs", EventFilter::Obs),
    ("event-tab-errors", EventFilter::Errors),
];

fn filter_tab_key(filter: EventFilter) -> &'static str {
    match filter {
        EventFilter::All => "event_feed_filter_all",
        EventFilter::Chat => "event_feed_filter_chat",
        EventFilter::Subs => "event_feed_filter_subs",
        EventFilter::Bits => "event_feed_filter_bits",
        EventFilter::Timers => "event_feed_filter_timers",
        EventFilter::Obs => "event_feed_filter_obs",
        EventFilter::Errors => "event_feed_filter_errors",
    }
}

pub struct EventFeedView {
    log: Entity<EventLog>,
    active_filter: EventFilter,
    search: SearchState,
    /// `None` falls back to the newest row, keeping the inspector populated.
    selected: Option<gpui::SharedString>,
    visible: Vec<EventItem>,
    downstream: HashMap<gpui::SharedString, u32>,
    matched: HashSet<gpui::SharedString>,
    auto_scroll: bool,
    inspector_width: f32,
    list_scroll: UniformListScrollHandle,
    list_focus: FocusHandle,
    focused_once: bool,
    rt_handle: tokio::runtime::Handle,
    _log_obs: Subscription,
    _search_sub: Subscription,
}

fn matches_query(item: &EventItem, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    item.kind.to_lowercase().contains(query)
        || source_label(item.source).to_lowercase().contains(query)
        || item.summary.to_lowercase().contains(query)
}

fn compute_projection(
    log: &EventLog,
    filter: EventFilter,
    query: &str,
) -> (
    Vec<EventItem>,
    HashMap<gpui::SharedString, u32>,
    HashSet<gpui::SharedString>,
) {
    let mut downstream: HashMap<gpui::SharedString, u32> = HashMap::new();
    let mut matched: HashSet<gpui::SharedString> = HashSet::new();
    for item in log.items() {
        match item.kind.as_ref() {
            "action.start" => {
                if let Some(cb) = &item.caused_by {
                    *downstream.entry(cb.clone()).or_default() += 1;
                }
            }
            "command.matched" => {
                if let Some(cb) = &item.caused_by {
                    matched.insert(cb.clone());
                }
            }
            _ => {}
        }
    }
    let visible: Vec<EventItem> = log
        .items()
        .iter()
        .filter(|item| item.matches(filter) && matches_query(item, query))
        .cloned()
        .collect();
    (visible, downstream, matched)
}

impl EventFeedView {
    pub fn new(
        log: Entity<EventLog>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("event_feed_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let log_obs = cx.observe(&log, Self::on_log_changed);
        let list_scroll = UniformListScrollHandle::new();
        list_scroll.scroll_to_bottom();
        let (visible, downstream, matched) =
            compute_projection(log.read(cx), EventFilter::default(), "");
        Self {
            log,
            active_filter: EventFilter::default(),
            search,
            selected: None,
            visible,
            downstream,
            matched,
            auto_scroll: true,
            inspector_width: INSPECTOR_INITIAL,
            list_scroll,
            list_focus: cx.focus_handle(),
            focused_once: false,
            rt_handle,
            _log_obs: log_obs,
            _search_sub: search_sub,
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.visible.is_empty() {
            return;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|id| self.visible.iter().position(|i| &i.id == id))
            .unwrap_or(self.visible.len() - 1);
        let last = self.visible.len() as isize - 1;
        let next = (cur as isize + delta).clamp(0, last) as usize;
        self.selected = Some(self.visible[next].id.clone());
        self.auto_scroll = false;
        self.list_scroll
            .scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn on_list_prev(&mut self, _: &ListSelectPrev, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(-1, cx);
    }

    fn on_list_next(&mut self, _: &ListSelectNext, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(1, cx);
    }

    fn on_list_activate(&mut self, _: &ListActivate, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(0, cx);
    }

    fn on_log_changed(&mut self, _log: Entity<EventLog>, cx: &mut Context<Self>) {
        self.rebuild_projection(cx);
        if self.auto_scroll {
            self.list_scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn set_filter(&mut self, filter: EventFilter, cx: &mut Context<Self>) {
        self.active_filter = filter;
        self.rebuild_projection(cx);
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            self.rebuild_projection(cx);
            cx.notify();
        }
    }

    fn rebuild_projection(&mut self, cx: &mut Context<Self>) {
        let (visible, downstream, matched) = {
            let log = self.log.read(cx);
            compute_projection(log, self.active_filter, self.search.query())
        };
        self.visible = visible;
        self.downstream = downstream;
        self.matched = matched;
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

    fn export(&mut self, cx: &mut Context<Self>) {
        let events: Vec<EventItem> = self.log.read(cx).items().iter().cloned().collect();
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async move {
                let filter = async_bridge::DialogFilter {
                    name: "JSON".to_owned(),
                    extensions: &["json"],
                };
                let path =
                    async_bridge::save_file(Some(filter), Some("forge-events.json".to_owned()))
                        .await?;
                let json = serde_json::to_string_pretty(&events).map_err(|e| e.to_string())?;
                tokio::fs::write(&path, json)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(path)
            },
            |_this, result, cx| match result {
                Ok(path) => {
                    let path_str = path.display().to_string();
                    cx.push_toast(
                        ToastKind::Success,
                        tr!("event_feed_export_success", path = path_str.as_str()),
                    );
                }
                Err(e) if e == async_bridge::DIALOG_CANCELLED => {}
                Err(e) => {
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("event_feed_export_failed", error = e.as_str()),
                    );
                }
            },
            cx,
        );
    }

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
        self.auto_scroll = self.list_scroll.is_scrolled_to_end().unwrap_or(true);
        cx.notify();
    }

    fn render_header_right(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let paused = self.log.read(cx).is_paused();
        let (live_color, live_label) = if paused {
            (palette.warning, tr!("event_feed_status_paused"))
        } else {
            (palette.success, tr!("event_feed_status_live"))
        };
        let count = self.log.read(cx).items().len();
        let count_readout = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(format!("{count}")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("event_feed_events_live_stream")),
            );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(header_status(live_color, live_label))
            .child(count_readout)
            .into_any_element()
    }

    fn render_filter_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let counts = self.filter_counts(cx);

        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, filter) in FILTER_TABS {
            let glyph = Self::tab_glyph(filter, palette);
            let label = tr!(filter_tab_key(filter), n = i64::from(counts.get(filter)));
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

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(div().w(SEARCH_W).child(self.search.field().clone()))
            .child(chips)
            .into_any_element()
    }

    fn render_filter_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let paused = self.log.read(cx).is_paused();

        let (pause_icon, pause_ink) = if paused {
            (Icon::PlayerPlay, palette.success)
        } else {
            (Icon::PlayerPause, palette.warning)
        };
        let pause = Self::action_shell("event-action-pause", palette, density)
            .child(icon(pause_icon, px(14.0), pause_ink))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pause(cx)));

        let clear = Self::action_shell("event-action-clear", palette, density)
            .child(icon(Icon::Eraser, px(14.0), palette.random))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear(cx)));

        let export = Self::action_shell("event-action-export", palette, density)
            .child(icon(Icon::Download, px(14.0), palette.success))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export(cx)));

        let (scroll_ink, scroll_label) = if self.auto_scroll {
            (palette.success, tr!("event_feed_auto_scroll_on"))
        } else {
            (palette.disabled, tr!("event_feed_auto_scroll_off"))
        };
        let auto_scroll = Self::action_shell("event-action-autoscroll", palette, density)
            .child(status_dot(scroll_ink, STATUS_DOT))
            .child(Self::action_label(scroll_label, palette))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_auto_scroll(cx)));

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(pause)
            .child(clear)
            .child(export)
            .child(auto_scroll)
            .into_any_element()
    }

    fn outcome_tag(
        item: &EventItem,
        downstream: &HashMap<gpui::SharedString, u32>,
        matched: &HashSet<gpui::SharedString>,
    ) -> Option<gpui::SharedString> {
        let acted = downstream.get(&item.id).copied().unwrap_or(0);
        let actions_tag = |n: u32| -> gpui::SharedString {
            if n == 1 {
                "\u{2192} 1 action".into()
            } else {
                format!("\u{2192} {n} actions").into()
            }
        };
        match item.kind.as_ref() {
            "chat.send" => Some("sent".into()),
            "command.matched" => Some("\u{2192} trigger".into()),
            "action.start" => item.sub_action_count.map(|n| {
                if n == 1 {
                    "1 sub-action".into()
                } else {
                    format!("{n} sub-actions").into()
                }
            }),
            "action.done" => item.total_ms.map(|ms| format!("{ms:.2}ms").into()),
            k if is_chat_message_kind(k) => {
                if acted > 0 {
                    Some(actions_tag(acted))
                } else if matched.contains(&item.id) {
                    Some("\u{2192} trigger".into())
                } else {
                    Some("no match".into())
                }
            }
            _ => (acted > 0).then(|| actions_tag(acted)),
        }
    }

    fn render_row(
        &self,
        item: &EventItem,
        selected: bool,
        tag: Option<gpui::SharedString>,
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
            BADGE_FS,
        );

        let type_cell = div()
            .flex_none()
            .w(TYPE_COL_W)
            .whitespace_nowrap()
            .overflow_hidden()
            .font_family(mono_family())
            .text_size(ROW_FS)
            .text_color(type_color(&item.kind, item.is_error, palette))
            .child(item.kind.clone());

        let summary = div().flex_1().overflow_hidden().child(
            div()
                .truncate()
                .font_family(mono_family())
                .text_size(ROW_FS)
                .text_color(palette.text_primary)
                .child(item.summary.clone()),
        );

        // gpui shares one `border_color` across all sides, so the selection rail is a separate leading strip rather than a left border.
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
                    .whitespace_nowrap()
                    .font_family(mono_family())
                    .text_size(ROW_FS)
                    .text_color(palette.text_faint)
                    .child(item.timestamp.clone()),
            )
            .child(source_badge)
            .child(type_cell)
            .child(summary)
            .children(tag.map(|tag| {
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .font_family(mono_family())
                    .text_size(SUFFIX_FS)
                    .text_color(result_color(&item.kind, item.is_error, palette))
                    .child(tag)
            }));

        let mut row = div()
            .id((gpui::ElementId::from("event-row"), item.id.clone()))
            .w_full()
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
        let count = self.visible.len();

        let body: AnyElement = if count == 0 {
            let label = if matches!(filter, EventFilter::All) {
                tr!("event_feed_no_events")
            } else {
                tr!("event_feed_no_filter_match")
            };
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state(label, palette).density(density))
                .into_any_element()
        } else {
            let selected = selected_id.cloned();
            let pal = *palette;
            uniform_list(
                "event-feed-list",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                    let mut rows = Vec::with_capacity(range.len());
                    for ix in range {
                        let Some(item) = this.visible.get(ix).cloned() else {
                            continue;
                        };
                        let is_sel = selected.as_ref() == Some(&item.id);
                        let tag = Self::outcome_tag(&item, &this.downstream, &this.matched);
                        rows.push(
                            this.render_row(&item, is_sel, tag, &pal, cx)
                                .into_any_element(),
                        );
                    }
                    rows
                }),
            )
            .track_scroll(&self.list_scroll)
            .flex_1()
            .min_h(px(0.0))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .into_any_element()
        };

        div()
            .id("event-feed-scroll")
            .track_focus(&self.list_focus)
            .key_context(LIST_CONTEXT)
            .on_action(cx.listener(Self::on_list_prev))
            .on_action(cx.listener(Self::on_list_next))
            .on_action(cx.listener(Self::on_list_activate))
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .pt(spacing(Spacing::Sm, density))
            .child(body)
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
            .header(tr!("event_feed_inspector_title"))
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
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .p(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .overflow_hidden()
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
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(item.kind.clone()),
                    ),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(format!("{} · #{last6}", item.timestamp)),
            );

        let payload = self.payload_block(item, palette);

        let caused_text: gpui::SharedString = item
            .caused_by
            .as_ref()
            .and_then(|cid| {
                let log = self.log.read(cx);
                log.items().iter().find(|e| &e.id == cid).map(|e| {
                    if e.summary.is_empty() {
                        e.kind.clone()
                    } else {
                        e.summary.clone()
                    }
                })
            })
            .unwrap_or_else(|| gpui::SharedString::from("-"));

        let caused = div()
            .w_full()
            .flex()
            .items_center()
            .overflow_hidden()
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
                    .min_w_0()
                    .truncate()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(caused_text),
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child(tr!("widget_event_replay")),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(summary_card)
            .child(self.section_label(tr!("widget_event_payload_header"), palette))
            .child(payload)
            .child(self.section_label(tr!("widget_event_caused_header"), palette))
            .child(caused)
            .child(replay)
    }

    fn section_label(&self, text: String, palette: &ForgePalette) -> impl IntoElement + use<> {
        div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(text)
    }

    fn payload_block(&self, item: &EventItem, palette: &ForgePalette) -> impl IntoElement + use<> {
        let brace = palette.text_muted;
        let mut lines: Vec<gpui::AnyElement> = Vec::new();
        lines.push(json_line(0, vec![json_span("{".to_owned(), brace)]));
        push_json_lines(
            &serde_json::json!(item.kind.as_ref()),
            1,
            Some("event"),
            true,
            palette,
            &mut lines,
        );
        push_json_lines(
            &serde_json::json!(source_label(item.source).to_lowercase()),
            1,
            Some("source"),
            true,
            palette,
            &mut lines,
        );
        push_json_lines(
            &serde_json::json!(item.timestamp.as_ref()),
            1,
            Some("timestamp"),
            true,
            palette,
            &mut lines,
        );
        if let Some(caused_by) = &item.caused_by {
            push_json_lines(
                &serde_json::json!(caused_by.as_ref()),
                1,
                Some("caused_by"),
                true,
                palette,
                &mut lines,
            );
        }
        push_json_lines(
            &item.payload,
            1,
            Some("payload"),
            false,
            palette,
            &mut lines,
        );
        lines.push(json_line(0, vec![json_span("}".to_owned(), brace)]));

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .p(spacing(Spacing::Sm, Density::Cozy))
            .rounded(radius(Radius::Md))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .children(lines)
    }

    fn resolved_selection(&self, cx: &Context<Self>) -> Option<EventItem> {
        let log = self.log.read(cx);
        if let Some(id) = &self.selected
            && let Some(found) = log.items().iter().find(|i| &i.id == id)
        {
            return Some(found.clone());
        }
        log.items().back().cloned()
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
            EventFilter::All => ChipGlyph::Dot(palette.brand),
            EventFilter::Chat => ChipGlyph::DotIcon(palette.info, Icon::MessageCircle),
            EventFilter::Subs => ChipGlyph::DotIcon(palette.brand, Icon::Star),
            EventFilter::Bits => ChipGlyph::DotIcon(palette.warning, Icon::Coin),
            EventFilter::Timers => ChipGlyph::DotIcon(palette.warning, Icon::Clock),
            EventFilter::Obs => ChipGlyph::DotIcon(palette.success, Icon::Broadcast),
            EventFilter::Errors => ChipGlyph::DotIcon(palette.random, Icon::AlertTriangle),
        }
    }

    fn action_shell(id: &'static str, palette: &ForgePalette, _density: Density) -> Stateful<Div> {
        let hover = palette.surface_overlay;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .p(px(5.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
    }

    fn action_label(text: String, palette: &ForgePalette) -> impl IntoElement + use<> {
        div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_secondary)
            .child(text)
    }
}

impl Render for EventFeedView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.list_focus, cx);
        }

        let selection = self.resolved_selection(cx);
        let selected_id = selection.as_ref().map(|i| i.id.clone());

        let header_right = self.render_header_right(&palette, cx);
        let filter_left = self.render_filter_left(&palette, density, cx);
        let filter_right = self.render_filter_right(&palette, density, cx);
        let list = self.render_list(&palette, density, selected_id.as_ref(), cx);
        let inspector = selection
            .as_ref()
            .map(|item| self.render_inspector(item, &palette, cx));

        let body = div()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(list)
            .children(inspector);

        page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("event_feed_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("nav_event_feed")),
            ],
            &palette,
        )
        .header_right(header_right)
        .subheader_left(filter_left)
        .subheader_right(filter_right)
        .density(density)
        .body(body)
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

const JSON_INDENT_PX: f32 = 10.0;

fn json_line(indent: usize, children: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    div()
        .flex()
        .pl(px(indent as f32 * JSON_INDENT_PX))
        .children(children)
        .into_any_element()
}

fn json_span(text: String, color: Rgba) -> gpui::AnyElement {
    div().text_color(color).child(text).into_any_element()
}

fn json_scalar_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(_) => value.to_string(),
        other => other.to_string(),
    }
}

fn json_scalar_color(value: &serde_json::Value, palette: &ForgePalette) -> Rgba {
    match value {
        serde_json::Value::String(_) => palette.success,
        serde_json::Value::Number(_) => palette.warning,
        _ => palette.text_muted,
    }
}

fn push_json_lines(
    value: &serde_json::Value,
    indent: usize,
    key: Option<&str>,
    trailing_comma: bool,
    palette: &ForgePalette,
    out: &mut Vec<gpui::AnyElement>,
) {
    let brace = palette.text_muted;
    let key_color = palette.info;
    let comma = if trailing_comma { "," } else { "" };
    let prefix = |open: &str| {
        let mut spans = Vec::new();
        if let Some(k) = key {
            spans.push(json_span(format!("\"{k}\""), key_color));
            spans.push(json_span(": ".to_owned(), brace));
        }
        spans.push(json_span(open.to_owned(), brace));
        spans
    };

    match value {
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                out.push(json_line(indent, prefix(&format!("{{}}{comma}"))));
                return;
            }
            out.push(json_line(indent, prefix("{")));
            for (i, (k, v)) in map.iter().enumerate() {
                let last = i + 1 == map.len();
                push_json_lines(v, indent + 1, Some(k), !last, palette, out);
            }
            out.push(json_line(
                indent,
                vec![json_span(format!("}}{comma}"), brace)],
            ));
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                out.push(json_line(indent, prefix(&format!("[]{comma}"))));
                return;
            }
            out.push(json_line(indent, prefix("[")));
            for (i, v) in items.iter().enumerate() {
                let last = i + 1 == items.len();
                push_json_lines(v, indent + 1, None, !last, palette, out);
            }
            out.push(json_line(
                indent,
                vec![json_span(format!("]{comma}"), brace)],
            ));
        }
        scalar => {
            let mut spans = Vec::new();
            if let Some(k) = key {
                spans.push(json_span(format!("\"{k}\""), key_color));
                spans.push(json_span(": ".to_owned(), brace));
            }
            spans.push(json_span(
                json_scalar_text(scalar),
                json_scalar_color(scalar, palette),
            ));
            if trailing_comma {
                spans.push(json_span(",".to_owned(), brace));
            }
            out.push(json_line(indent, spans));
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
        EventSource::Twitch => platform_color(PlatformKind::Twitch, palette),
        EventSource::YouTube => platform_color(PlatformKind::YouTube, palette),
        EventSource::Kick => platform_color(PlatformKind::Kick, palette),
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
        "twitch.channel.chat.message"
            | "youtube.chat.message"
            | "kick.chat.message.sent"
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
        "action.done" | "obs.scene.changed" => palette.success,
        "chat.send" => palette.info,
        "action.start" | "script.exec" | "timer.tick" | "subaction.run" => palette.warning,
        k if k.starts_with("global.") => palette.warning,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_changed_success_color_tracks_namespaced_obs_kind() {
        let p = forge_components::FORGE_DEFAULT;
        assert_eq!(type_color("obs.scene.changed", false, &p), p.success);
        assert_eq!(type_color("scene.changed", false, &p), p.text_secondary);
    }
}
