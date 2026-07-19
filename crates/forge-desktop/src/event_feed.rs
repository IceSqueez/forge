use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XS,
    FONT_XXS, ForgePalette, Icon, Radius, SheetWidth, Spacing, ToastKind, badge, breadcrumb, chip,
    icon, radius, spacing, status_dot, tr, with_alpha,
};
use forge_events::EventSource;
use gpui::{
    ClickEvent, Context, Div, Entity, Pixels, Rgba, ScrollHandle, ScrollWheelEvent, Stateful,
    Subscription, Window, div, prelude::*, px,
};

use crate::event_log::{EventFilter, EventItem, EventLog};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const AT_BOTTOM_SLACK: f32 = 40.0;
const INSPECTOR_INITIAL: f32 = 300.0;
const INSPECTOR_MIN: f32 = 220.0;
const INSPECTOR_MAX: f32 = 540.0;

const TS_COL_W: Pixels = px(88.0);
const TYPE_COL_W: Pixels = px(104.0);
const ROW_FS: Pixels = px(11.0);
const BADGE_FS: Pixels = px(9.0);
const SUFFIX_FS: Pixels = px(10.0);
const STATUS_DOT: Pixels = px(6.0);
const ROW_RAIL_W: Pixels = px(2.0);
const ERROR_ROW_ALPHA: f32 = 0.06;

const EXPORT_CANCELLED: &str = "export cancelled";

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
    /// `None` falls back to the newest row, keeping the inspector populated.
    selected: Option<gpui::SharedString>,
    auto_scroll: bool,
    inspector_width: f32,
    list_scroll: ScrollHandle,
    rt_handle: tokio::runtime::Handle,
    _log_obs: Subscription,
}

impl EventFeedView {
    pub fn new(
        log: Entity<EventLog>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
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
            rt_handle,
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

    fn export(&mut self, cx: &mut Context<Self>) {
        let events: Vec<EventItem> = self.log.read(cx).items().to_vec();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<PathBuf, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async move {
                let Some(handle) = rfd::AsyncFileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("forge-events.json")
                    .save_file()
                    .await
                else {
                    return Err(EXPORT_CANCELLED.to_owned());
                };
                let path = handle.path().to_path_buf();
                let json = serde_json::to_string_pretty(&events).map_err(|e| e.to_string())?;
                tokio::fs::write(&path, json)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(path)
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(path)) => {
                let path_str = path.display().to_string();
                let _ = this.update(cx, |_this, cx| {
                    cx.push_toast(
                        ToastKind::Success,
                        tr!("event_feed_export_success", path = path_str.as_str()),
                    );
                });
            }
            Ok(Err(e)) => {
                if e == EXPORT_CANCELLED {
                    return;
                }
                let _ = this.update(cx, |_this, cx| {
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("event_feed_export_failed", error = e.as_str()),
                    );
                });
            }
            Err(_) => {}
        })
        .detach();
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
        let remaining = self.list_scroll.max_offset().y + self.list_scroll.offset().y;
        self.auto_scroll = remaining <= px(AT_BOTTOM_SLACK);
        cx.notify();
    }

    fn render_header(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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
                    .child(tr!("event_feed_events_live_stream")),
            );

        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf(tr!("event_feed_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("nav_event_feed")),
            ],
            palette,
        )
        .right(count_readout)
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

        let actions = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(pause)
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
        idx: usize,
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
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(ROW_FS)
            .text_color(type_color(&item.kind, item.is_error, palette))
            .child(item.kind.clone());

        let summary = div().flex_1().overflow_hidden().child(
            div()
                .truncate()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(ROW_FS)
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
                    .whitespace_nowrap()
                    .font_family(DEFAULT_MONO_FAMILY)
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
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(SUFFIX_FS)
                    .text_color(result_color(&item.kind, item.is_error, palette))
                    .child(tag)
            }));

        let mut row = div()
            .id(("event-row", idx))
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
        let (visible, downstream, matched) = {
            let log = self.log.read(cx);
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
                .filter(|item| item.matches(filter))
                .cloned()
                .collect();
            (visible, downstream, matched)
        };

        let empty = visible.is_empty();
        let mut list = div().w_full().flex().flex_col();
        for (idx, item) in visible.iter().enumerate() {
            let is_sel = selected_id == Some(&item.id);
            let tag = Self::outcome_tag(item, &downstream, &matched);
            list = list.child(self.render_row(idx, item, is_sel, tag, palette, cx));
        }

        let empty_note = empty.then(|| {
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
            .pt(spacing(Spacing::Sm, density))
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
                    .font_family(DEFAULT_MONO_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
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
