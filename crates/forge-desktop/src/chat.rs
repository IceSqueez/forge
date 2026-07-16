use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChatRow, ChipGlyph, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_XS, ForgePalette, Icon, InputBar, InputBarEvent, InputEvent, Platform, Radius,
    Spacing, TextInput, breadcrumb, chat_row, chip, icon, radius, search_input, spacing,
    status_dot,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Pixels, Rgba, ScrollHandle, ScrollWheelEvent,
    Subscription, Window, div, prelude::*, px,
};

use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::presentation::ActivePresentation;

const AT_BOTTOM_SLACK: f32 = 60.0;
const PILL_BOTTOM_LIFT: Pixels = px(16.0);
const SEARCH_FIELD_WIDTH: Pixels = px(220.0);
const VIEWER_DOT: Pixels = px(6.0);
const CHIP_DIVIDER_W: Pixels = px(0.5);
const CHIP_DIVIDER_H: Pixels = px(14.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlatformFilter {
    All,
    Single(Platform),
}

pub struct ChatView {
    feed: Entity<ChatFeed>,
    input: Entity<InputBar>,
    search_field: Entity<TextInput>,
    platform_filter: PlatformFilter,
    events_only: bool,
    hide_bots: bool,
    search_open: bool,
    search_query: String,
    drawer_open: bool,
    auto_scroll: bool,
    unread: usize,
    last_seen_len: usize,
    chat_scroll: ScrollHandle,
    _feed_obs: Subscription,
    _input_sub: Subscription,
    _search_sub: Subscription,
}

impl ChatView {
    pub fn new(feed: Entity<ChatFeed>, palette: ForgePalette, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputBar::new("Send a message to chat…", palette, cx));
        let search_field = cx.new(|cx| search_input("Search chat…", palette, cx));

        let feed_obs = cx.observe(&feed, Self::on_feed_changed);
        let input_sub = cx.subscribe(&input, Self::on_input_event);
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let last_seen_len = feed.read(cx).messages().len();
        let chat_scroll = ScrollHandle::new();
        chat_scroll.scroll_to_bottom();

        Self {
            feed,
            input,
            search_field,
            platform_filter: PlatformFilter::All,
            events_only: false,
            hide_bots: false,
            search_open: false,
            search_query: String::new(),
            drawer_open: false,
            auto_scroll: true,
            unread: 0,
            last_seen_len,
            chat_scroll,
            _feed_obs: feed_obs,
            _input_sub: input_sub,
            _search_sub: search_sub,
        }
    }

    fn on_feed_changed(&mut self, feed: Entity<ChatFeed>, cx: &mut Context<Self>) {
        let len = feed.read(cx).messages().len();
        if self.auto_scroll {
            self.chat_scroll.scroll_to_bottom();
            self.unread = 0;
        } else {
            self.unread = self
                .unread
                .saturating_add(len.saturating_sub(self.last_seen_len));
        }
        self.last_seen_len = len;
        cx.notify();
    }

    fn on_input_event(
        &mut self,
        _input: Entity<InputBar>,
        event: &InputBarEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputBarEvent::Send { .. } = event {
            self.input.update(cx, |bar, cx| bar.clear(cx));
            cx.notify();
        }
    }

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search_query = text.to_string();
            cx.notify();
        }
    }

    fn set_platform_filter(&mut self, filter: PlatformFilter, cx: &mut Context<Self>) {
        self.platform_filter = filter;
        cx.notify();
    }

    fn toggle_events(&mut self, cx: &mut Context<Self>) {
        self.events_only = !self.events_only;
        cx.notify();
    }

    fn toggle_hide_bots(&mut self, cx: &mut Context<Self>) {
        self.hide_bots = !self.hide_bots;
        cx.notify();
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            self.search_field.read(cx).focus(window);
        }
        cx.notify();
    }

    fn toggle_drawer(&mut self, cx: &mut Context<Self>) {
        self.drawer_open = !self.drawer_open;
        cx.notify();
    }

    fn open_viewer(&mut self, cx: &mut Context<Self>) {
        if !self.drawer_open {
            self.drawer_open = true;
            cx.notify();
        }
    }

    fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.auto_scroll = true;
        self.chat_scroll.scroll_to_bottom();
        self.unread = 0;
        self.last_seen_len = self.feed.read(cx).messages().len();
        cx.notify();
    }

    fn on_wheel(
        &mut self,
        _event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // `offset().y` is <= 0 (content scrolled up), so adding it to the total
        // scrollable height yields the remaining distance to the bottom edge.
        let remaining = self.chat_scroll.max_offset().height + self.chat_scroll.offset().y;
        let at_bottom = remaining <= px(AT_BOTTOM_SLACK);
        self.auto_scroll = at_bottom;
        if at_bottom {
            self.unread = 0;
            self.last_seen_len = self.feed.read(cx).messages().len();
        }
        cx.notify();
    }

    /// Search is deliberately excluded - it dims non-matches rather than filtering them.
    fn row_visible(&self, msg: &ChatMessage) -> bool {
        let platform_ok = match self.platform_filter {
            PlatformFilter::All => true,
            PlatformFilter::Single(p) => msg.platform == p,
        };
        let events_ok = !self.events_only || msg.is_event;
        let bots_ok = !self.hide_bots || !msg.is_bot;
        platform_ok && events_ok && bots_ok
    }

    fn username_color(platform: Platform, palette: &ForgePalette) -> Rgba {
        match platform {
            Platform::Twitch => palette.brand,
            Platform::YouTube => palette.random,
            Platform::Kick => palette.info,
        }
    }

    fn render_header(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let viewers = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(palette.text_faint, VIEWER_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child("- viewers"),
            );

        let separator = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child("·");

        let uptime = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::Clock, px(12.0), palette.text_muted))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("-"),
            );

        let drawer_label = if self.drawer_open {
            "Hide viewers"
        } else {
            "Show viewers"
        };
        let border = palette.border_regular;
        let drawer_btn = div()
            .id("chat-drawer-toggle")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.border_color(border))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_drawer(cx)))
            .child(icon(Icon::LayoutSidebar, px(11.0), palette.text_secondary))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(drawer_label),
            );

        let cluster = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(viewers)
            .child(separator)
            .child(uptime)
            .child(drawer_btn);

        breadcrumb(vec![BreadcrumbCrumb::leaf("Chat")], palette).right(cluster)
    }

    fn render_filter_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let platform_chips = [
            ("chat-chip-all", "All", PlatformFilter::All, palette.brand),
            (
                "chat-chip-twitch",
                "Twitch",
                PlatformFilter::Single(Platform::Twitch),
                palette.brand,
            ),
            (
                "chat-chip-youtube",
                "YouTube",
                PlatformFilter::Single(Platform::YouTube),
                palette.random,
            ),
            (
                "chat-chip-kick",
                "Kick",
                PlatformFilter::Single(Platform::Kick),
                palette.info,
            ),
        ];

        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, label, filter, dot) in platform_chips {
            let active = self.platform_filter == filter;
            chips = chips.child(
                chip(label, ChipGlyph::Dot(dot), active, palette)
                    .density(density)
                    .on_click(
                        id,
                        cx.listener(move |this, _, _, cx| this.set_platform_filter(filter, cx)),
                    ),
            );
        }
        chips = chips.child(
            div()
                .w(CHIP_DIVIDER_W)
                .h(CHIP_DIVIDER_H)
                .bg(palette.border_regular),
        );
        chips = chips.child(
            chip("Events only", ChipGlyph::None, self.events_only, palette)
                .density(density)
                .on_click(
                    "chat-chip-events",
                    cx.listener(|this, _, _, cx| this.toggle_events(cx)),
                ),
        );
        chips = chips.child(
            chip(
                "Hide bots",
                ChipGlyph::Icon(Icon::EyeOff, palette.text_faint),
                self.hide_bots,
                palette,
            )
            .density(density)
            .on_click(
                "chat-chip-hide-bots",
                cx.listener(|this, _, _, cx| this.toggle_hide_bots(cx)),
            ),
        );

        let toggle_icon = if self.search_open {
            Icon::X
        } else {
            Icon::Search
        };
        let search_toggle = div()
            .id("chat-search-toggle")
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_search(window, cx)),
            )
            .child(icon(toggle_icon, px(14.0), palette.text_faint));

        let mut search_control = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        if self.search_open {
            search_control =
                search_control.child(div().w(SEARCH_FIELD_WIDTH).child(self.search_field.clone()));
        }
        let search_control = search_control.child(search_toggle);

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
            .child(search_control)
    }

    fn render_chat_area(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let query = self.search_query.to_lowercase();
        let search_active = self.search_open && !query.is_empty();

        let visible: Vec<ChatMessage> = self
            .feed
            .read(cx)
            .messages()
            .iter()
            .filter(|m| self.row_visible(m))
            .cloned()
            .collect();

        let mut list = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density));

        for (idx, msg) in visible.iter().enumerate() {
            let data = ChatRow {
                timestamp: msg.timestamp.clone(),
                platform: msg.platform,
                badges: msg.badges.clone(),
                username: msg.username.clone(),
                username_color: Self::username_color(msg.platform, palette),
                body: msg.body.clone(),
            };
            let row = chat_row(palette, data).on_username_click(
                ("chat-username", idx),
                cx.listener(|this, _: &ClickEvent, _, cx| this.open_viewer(cx)),
            );
            let dim = search_active && !msg.matches_query(&query);
            let row_el: AnyElement = if dim {
                div().opacity(0.3).child(row).into_any_element()
            } else {
                row.into_any_element()
            };
            list = list.child(row_el);
        }

        let empty = visible.is_empty();
        let empty_note = if empty {
            Some(
                div()
                    .w_full()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("No messages match these filters."),
            )
        } else {
            None
        };

        let scroll = div()
            .id("chat-scroll")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.chat_scroll)
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .child(list)
            .children(empty_note);

        let mut area = div()
            .relative()
            .flex_1()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(scroll);

        if self.unread > 0 {
            let label = if self.unread == 1 {
                "1 new message".to_owned()
            } else {
                format!("{} new messages", self.unread)
            };
            let pill = div()
                .id("chat-unread-pill")
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Pill))
                .bg(palette.brand)
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.jump_to_latest(cx)))
                .child(icon(Icon::ArrowDown, FONT_XS, palette.shell))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.shell)
                        .child(label),
                );
            let overlay = div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_end()
                .pb(PILL_BOTTOM_LIFT)
                .child(pill);
            area = area.child(overlay);
        }

        area
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = self.render_header(&palette, cx);
        let filter_bar = self.render_filter_bar(&palette, density, cx);
        let chat_area = self.render_chat_area(&palette, density, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(filter_bar)
            .child(
                div().flex_1().flex().flex_row().overflow_hidden().child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(chat_area)
                        .child(self.input.clone()),
                ),
            )
    }
}
