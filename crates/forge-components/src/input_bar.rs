use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, px,
};

use crate::chat_row::Platform;
use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, Radius,
    Spacing, radius, spacing,
};

/// Bit each platform occupies in [`InputBar::targets_bitset`]. Twitch is the low
/// bit, then YouTube, then Kick — a stable order so a persisted or asserted
/// bitset stays meaningful across restarts.
pub fn platform_bit(platform: Platform) -> u8 {
    match platform {
        Platform::Twitch => 1 << 0,
        Platform::YouTube => 1 << 1,
        Platform::Kick => 1 << 2,
    }
}

/// The composer's outward reports. The caller holds the [`InputBar`] entity and
/// `cx.subscribe`s to react.
#[derive(Clone, Debug)]
pub enum InputBarEvent {
    /// A submit fired — from the send glyph or Enter in the field. Carries the
    /// field's current text and the platforms currently toggled on. The bar does
    /// NOT gate this on non-empty text or a selected target and does NOT clear the
    /// field: the composer only reports intent, the caller decides what to do with
    /// it (and calls [`InputBar::clear`] once it has consumed the text).
    Send {
        text: SharedString,
        targets: Vec<Platform>,
    },
    /// The emoji-picker toggle flipped. The open flag is owned internally; this is
    /// only a notification for a caller that wants to mirror the state.
    EmojiToggled,
}

// The composer's small fixed geometry lives off the `Spacing`/`Radius`/font
// scales — these are literal pixel values pinned to the source layout, carried as
// consts so the render tree reads as intent, not as bare magic numbers.

/// Gap between every item in the composer row (target letters, divider, field,
/// glyphs). A literal that sits between `Spacing::Xs` (6) and `Spacing::Sm` (10).
const COMPOSER_GAP: gpui::Pixels = px(8.0);
/// Side of a square platform-target toggle.
const TARGET_SIZE: gpui::Pixels = px(20.0);
/// Corner radius of a target toggle — tighter than `Radius::Sm`.
const TARGET_RADIUS: gpui::Pixels = px(4.0);
/// Font size of the single-letter target glyph.
const TARGET_LETTER_SIZE: gpui::Pixels = px(9.0);
/// The thin vertical rule separating the target toggles from the field.
const DIVIDER_WIDTH: gpui::Pixels = px(0.5);
const DIVIDER_HEIGHT: gpui::Pixels = px(18.0);
/// Message-field text size — a literal one step below `FONT_SM` (14).
const FIELD_TEXT_SIZE: gpui::Pixels = px(13.0);
/// Send-glyph and emoji-toggle glyph size.
const GLYPH_SIZE: gpui::Pixels = px(15.0);
/// Fixed height of the scrolling emoji grid.
const EMOJI_GRID_HEIGHT: gpui::Pixels = px(120.0);
/// Gap between emoji tiles in the wrapped grid.
const EMOJI_GAP: gpui::Pixels = px(4.0);
/// Vertical gap slipped between the open emoji panel and the composer below it.
const EMOJI_PANEL_GAP: gpui::Pixels = px(8.0);
/// Gap between an affordance hint's glyph and its label.
const HINT_GAP: gpui::Pixels = px(4.0);
/// Gap between the three affordance hints in the footer row.
const HINT_ROW_GAP: gpui::Pixels = px(14.0);

/// The emoji-picker palette, in the source's order.
const EMOJIS: &[&str] = &[
    "😀",
    "😃",
    "😄",
    "😁",
    "😆",
    "😅",
    "😂",
    "🤣",
    "😊",
    "😇",
    "🙂",
    "🙃",
    "😉",
    "😌",
    "😍",
    "🥰",
    "😘",
    "😗",
    "😙",
    "😚",
    "😋",
    "😛",
    "😝",
    "😜",
    "🤪",
    "🤨",
    "🧐",
    "🤓",
    "😎",
    "🥸",
    "🤩",
    "🥳",
    "😏",
    "😒",
    "😞",
    "😔",
    "😟",
    "😕",
    "🙁",
    "☹️",
    "😣",
    "😖",
    "😫",
    "😩",
    "🥺",
    "😢",
    "😭",
    "😤",
    "😠",
    "😡",
    "🤬",
    "🤯",
    "😳",
    "🥵",
    "🥶",
    "😱",
    "😨",
    "😰",
    "😥",
    "😓",
    "🤗",
    "🤔",
    "🫣",
    "🤭",
    "🤫",
    "🤥",
    "😶",
    "😶‍🌫️",
    "😐",
    "😑",
];

/// The chat message composer: a per-platform send-target strip, an embedded
/// message field, an emoji-picker toggle, and a send glyph, over a footer of
/// affordance hints. Owns its [`TextInput`] child entity and the selected state of
/// each target platform, and (when open) an emoji panel that appends to the field.
///
/// Build inside `cx.new(…)`; the caller `cx.subscribe`s for [`InputBarEvent`]. The
/// binary must call [`crate::bind_text_input_keys`] once at boot for the field's
/// editing keys to fire.
pub struct InputBar {
    field: Entity<TextInput>,
    /// The platforms shown in the target strip, in render order, each paired with
    /// its selected flag. The caller picks which platforms appear via
    /// [`InputBar::with_targets`]; the selected flags are owned and toggled here.
    targets: Vec<(Platform, bool)>,
    emoji_open: bool,
    palette: ForgePalette,
    density: Density,
    emoji_scroll: ScrollHandle,
    _field_sub: Subscription,
}

impl EventEmitter<InputBarEvent> for InputBar {}

impl InputBar {
    /// Builds a composer whose target strip shows all three platforms, each
    /// selected. `placeholder` seeds the empty-field prompt. Creates the embedded
    /// field and subscribes to it so Enter reports a submit.
    pub fn new(
        placeholder: impl Into<SharedString>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .with_palette(palette)
                .with_font_size(FIELD_TEXT_SIZE)
        });
        let field_sub = cx.subscribe(&field, Self::on_field_event);

        Self {
            field,
            targets: vec![
                (Platform::Twitch, true),
                (Platform::YouTube, true),
                (Platform::Kick, true),
            ],
            emoji_open: false,
            palette,
            density: Density::default(),
            emoji_scroll: ScrollHandle::new(),
            _field_sub: field_sub,
        }
    }

    /// Overrides which platforms the target strip shows and their initial selected
    /// state, in render order.
    pub fn with_targets(mut self, targets: Vec<(Platform, bool)>) -> Self {
        self.targets = targets;
        self
    }

    /// Overrides the density used to scale the composer's token-based paddings.
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// The selected platforms, in strip order — the `targets` carried by a
    /// [`InputBarEvent::Send`].
    pub fn selected_targets(&self) -> Vec<Platform> {
        self.targets
            .iter()
            .filter(|(_, active)| *active)
            .map(|(platform, _)| *platform)
            .collect()
    }

    /// The selected platforms packed into a bitset (see [`platform_bit`]).
    pub fn targets_bitset(&self) -> u8 {
        self.targets
            .iter()
            .filter(|(_, active)| *active)
            .fold(0, |acc, (platform, _)| acc | platform_bit(*platform))
    }

    /// True when at least one target platform is toggled on.
    pub fn any_target_selected(&self) -> bool {
        self.targets.iter().any(|(_, active)| *active)
    }

    /// The field's current text.
    pub fn content(&self, cx: &App) -> String {
        self.field.read(cx).content().to_string()
    }

    /// Empties the field. The caller invokes this after it has consumed a
    /// [`InputBarEvent::Send`]; the bar never clears itself.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// Focuses the embedded field so typing and Enter reach it.
    pub fn focus(&self, window: &mut Window, cx: &App) {
        self.field.read(cx).focus(window);
    }

    /// Re-themes the composer and its embedded field.
    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.field
            .update(cx, |field, cx| field.set_palette(palette, cx));
        cx.notify();
    }

    fn on_field_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Submitted(_) = event {
            self.emit_send(cx);
        }
    }

    fn emit_send(&mut self, cx: &mut Context<Self>) {
        let text = self.field.read(cx).content().to_string();
        let targets = self.selected_targets();
        cx.emit(InputBarEvent::Send {
            text: text.into(),
            targets,
        });
    }

    fn toggle_target(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.targets.get_mut(idx) {
            entry.1 = !entry.1;
        }
        cx.notify();
    }

    fn toggle_emoji(&mut self, cx: &mut Context<Self>) {
        self.emoji_open = !self.emoji_open;
        cx.emit(InputBarEvent::EmojiToggled);
        cx.notify();
    }

    /// Appends `emoji` to the end of the field (matching the source, which grows
    /// the value regardless of caret position) and leaves the picker open.
    fn insert_emoji(&mut self, emoji: &'static str, cx: &mut Context<Self>) {
        self.field.update(cx, |field, cx| {
            let mut next = field.content().to_string();
            next.push_str(emoji);
            field.set_content(next, cx);
        });
        cx.notify();
    }

    fn render_target(
        &self,
        idx: usize,
        platform: Platform,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette;
        let letter_color = if active {
            platform.color(&p)
        } else {
            p.text_faint
        };

        let mut tile = div()
            .id(("forge-inputbar-target", idx))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .w(TARGET_SIZE)
            .h(TARGET_SIZE)
            .rounded(TARGET_RADIUS)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, _window, cx| this.toggle_target(idx, cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(TARGET_LETTER_SIZE)
                    .text_color(letter_color)
                    .child(SharedString::from(platform.letter())),
            );
        if active {
            tile = tile.bg(p.surface_overlay);
        }
        tile
    }

    fn render_hint(&self, glyph: &'static str, label: &'static str) -> impl IntoElement {
        let color = self.palette.text_faint;
        div()
            .flex()
            .items_center()
            .gap(HINT_GAP)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(glyph),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(label),
            )
    }

    fn render_emoji_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let d = self.density;

        let mut grid = div().flex().flex_wrap().gap(EMOJI_GAP);
        for (idx, &emoji) in EMOJIS.iter().enumerate() {
            grid = grid.child(
                div()
                    .id(("forge-inputbar-emoji", idx))
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(spacing(Spacing::Xxs, d))
                    .px(spacing(Spacing::Xs, d))
                    .cursor_pointer()
                    .text_color(p.text_primary)
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .hover(move |style| style.bg(p.surface_overlay))
                    .on_click(
                        cx.listener(move |this, _event, _window, cx| this.insert_emoji(emoji, cx)),
                    )
                    .child(emoji),
            );
        }

        div()
            .bg(p.shell)
            .border(BORDER_THIN)
            .border_color(p.border_regular)
            .rounded(radius(Radius::Md))
            .py(spacing(Spacing::Xs, d))
            .px(spacing(Spacing::Xs, d))
            .mb(EMOJI_PANEL_GAP)
            .child(
                div()
                    .id("forge-inputbar-emoji-grid")
                    .h(EMOJI_GRID_HEIGHT)
                    .track_scroll(&self.emoji_scroll)
                    .overflow_y_scroll()
                    .child(grid),
            )
    }
}

impl Render for InputBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let d = self.density;

        let top_border = div().w_full().h(BORDER_THIN).bg(p.border_regular);

        let divider = div()
            .flex_none()
            .w(DIVIDER_WIDTH)
            .h(DIVIDER_HEIGHT)
            .bg(p.border_regular);

        let field = div().flex_1().overflow_hidden().child(self.field.clone());

        let emoji_toggle = div()
            .id("forge-inputbar-emoji-toggle")
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _event, _window, cx| this.toggle_emoji(cx)))
            .child(icon(Icon::MoodSmile, GLYPH_SIZE, p.text_faint));

        let send = div()
            .id("forge-inputbar-send")
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _event, _window, cx| this.emit_send(cx)))
            .child(icon(Icon::Send, GLYPH_SIZE, p.brand));

        let strip = self.targets.clone();
        let mut composer = div()
            .flex()
            .items_center()
            .gap(COMPOSER_GAP)
            .py(spacing(Spacing::Xs, d))
            .px(spacing(Spacing::Sm, d))
            .bg(p.elevated)
            .border(BORDER_THIN)
            .border_color(p.border_input)
            .rounded(radius(Radius::Md));
        for (idx, (platform, active)) in strip.into_iter().enumerate() {
            composer = composer.child(self.render_target(idx, platform, active, cx));
        }
        let composer = composer
            .child(divider)
            .child(field)
            .child(emoji_toggle)
            .child(send);

        let hints = div()
            .pt(spacing(Spacing::Xs, d))
            .pr(spacing(Spacing::Xxs, d))
            .pl(spacing(Spacing::Xxs, d))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(HINT_ROW_GAP)
                    .child(self.render_hint("/", " commands"))
                    .child(self.render_hint("@", " mention"))
                    .child(self.render_hint("!", " trigger action")),
            );

        let emoji_panel = if self.emoji_open {
            Some(self.render_emoji_panel(cx))
        } else {
            None
        };

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .py(spacing(Spacing::Sm, d))
            .px(spacing(Spacing::Md, d))
            .bg(p.shell)
            .children(emoji_panel)
            .child(composer)
            .child(hints);

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(top_border)
            .child(body)
    }
}
