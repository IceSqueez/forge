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
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XXS, Radius,
    Spacing, radius, spacing,
};

/// Bit order is a persistence contract: a stored bitset must decode to the same platforms across restarts.
pub fn platform_bit(platform: Platform) -> u8 {
    match platform {
        Platform::Twitch => 1 << 0,
        Platform::YouTube => 1 << 1,
        Platform::Kick => 1 << 2,
    }
}

#[derive(Clone, Debug)]
pub enum InputBarEvent {
    Send {
        text: SharedString,
        targets: Vec<Platform>,
    },
    EmojiToggled,
    TargetsChanged,
}

const COMPOSER_GAP: gpui::Pixels = px(8.0);
const TARGET_SIZE: gpui::Pixels = px(20.0);
const TARGET_RADIUS: gpui::Pixels = px(4.0);
const TARGET_LETTER_SIZE: gpui::Pixels = px(9.0);
const DIVIDER_WIDTH: gpui::Pixels = px(1.0);
const DIVIDER_HEIGHT: gpui::Pixels = px(18.0);
const FIELD_TEXT_SIZE: gpui::Pixels = px(13.0);
const GLYPH_SIZE: gpui::Pixels = px(15.0);
const EMOJI_GRID_HEIGHT: gpui::Pixels = px(120.0);
const EMOJI_GAP: gpui::Pixels = px(4.0);
const EMOJI_PANEL_GAP: gpui::Pixels = px(8.0);
const HINT_GAP: gpui::Pixels = px(4.0);
const HINT_ROW_GAP: gpui::Pixels = px(14.0);
const TARGET_GROUP_GAP: gpui::Pixels = px(4.0);
const WRAP_PAD_X: gpui::Pixels = px(14.0);

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

/// The binary must call [`crate::bind_text_input_keys`] once at boot or the field's editing keys are dead.
pub struct InputBar {
    field: Entity<TextInput>,
    targets: Vec<(Platform, bool)>,
    emoji_open: bool,
    palette: ForgePalette,
    density: Density,
    emoji_scroll: ScrollHandle,
    _field_sub: Subscription,
}

impl EventEmitter<InputBarEvent> for InputBar {}

impl InputBar {
    pub fn new(
        placeholder: impl Into<SharedString>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .with_palette(palette)
                .with_font_size(FIELD_TEXT_SIZE)
                .plain()
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

    pub fn with_targets(mut self, targets: Vec<(Platform, bool)>) -> Self {
        self.targets = targets;
        self
    }

    pub fn selected_targets(&self) -> Vec<Platform> {
        self.targets
            .iter()
            .filter(|(_, active)| *active)
            .map(|(platform, _)| *platform)
            .collect()
    }

    pub fn targets_bitset(&self) -> u8 {
        self.targets
            .iter()
            .filter(|(_, active)| *active)
            .fold(0, |acc, (platform, _)| acc | platform_bit(*platform))
    }

    pub fn any_target_selected(&self) -> bool {
        self.targets.iter().any(|(_, active)| *active)
    }

    pub fn content(&self, cx: &App) -> String {
        self.field.read(cx).content().to_string()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.field.update(cx, |f, cx| f.focus(window, cx));
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.field
            .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        cx.notify();
    }

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
        cx.emit(InputBarEvent::TargetsChanged);
        cx.notify();
    }

    fn toggle_emoji(&mut self, cx: &mut Context<Self>) {
        self.emoji_open = !self.emoji_open;
        cx.emit(InputBarEvent::EmojiToggled);
        cx.notify();
    }

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
                    .text_size(FONT_XXS)
                    .text_color(color)
                    .child(glyph),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
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
        let mut targets_group = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(TARGET_GROUP_GAP);
        for (idx, (platform, active)) in strip.into_iter().enumerate() {
            targets_group = targets_group.child(self.render_target(idx, platform, active, cx));
        }

        let composer = div()
            .flex()
            .items_center()
            .gap(COMPOSER_GAP)
            .py(spacing(Spacing::Xs, d))
            .px(spacing(Spacing::Sm, d))
            .bg(p.elevated)
            .border(BORDER_THIN)
            .border_color(p.border_input)
            .rounded(radius(Radius::Md))
            .child(targets_group)
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
                    .child(self.render_hint("/", "commands"))
                    .child(self.render_hint("@", "mention"))
                    .child(self.render_hint("!", "trigger action")),
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
            .px(WRAP_PAD_X)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::FORGE_DEFAULT;

    #[allow(clippy::unwrap_used)]
    fn with_bar<R>(
        cx: &mut gpui::TestAppContext,
        targets: Vec<(Platform, bool)>,
        f: impl FnOnce(&mut InputBar, &mut Window, &mut Context<InputBar>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| {
            InputBar::new("placeholder", FORGE_DEFAULT, cx).with_targets(targets)
        });
        window
            .update(cx, |bar, window, cx| f(bar, window, cx))
            .unwrap()
    }

    #[test]
    fn platform_bit_assigns_a_distinct_stable_bit_per_platform() {
        assert_eq!(platform_bit(Platform::Twitch), 0b001);
        assert_eq!(platform_bit(Platform::YouTube), 0b010);
        assert_eq!(platform_bit(Platform::Kick), 0b100);
    }

    #[gpui::test]
    fn targets_bitset_is_the_or_of_the_selected_platform_bits(cx: &mut gpui::TestAppContext) {
        for (targets, expected) in [
            (
                vec![
                    (Platform::Twitch, true),
                    (Platform::YouTube, true),
                    (Platform::Kick, true),
                ],
                0b111,
            ),
            (
                vec![
                    (Platform::Twitch, true),
                    (Platform::YouTube, false),
                    (Platform::Kick, true),
                ],
                0b101,
            ),
            (
                vec![
                    (Platform::Twitch, false),
                    (Platform::YouTube, true),
                    (Platform::Kick, false),
                ],
                0b010,
            ),
            (
                vec![
                    (Platform::Twitch, false),
                    (Platform::YouTube, false),
                    (Platform::Kick, false),
                ],
                0b000,
            ),
        ] {
            let bits = with_bar(cx, targets.clone(), |bar, _window, _cx| {
                bar.targets_bitset()
            });
            assert_eq!(bits, expected, "bitset for {targets:?}");
        }
    }

    #[gpui::test]
    fn any_target_selected_is_true_only_when_a_target_is_on(cx: &mut gpui::TestAppContext) {
        for (targets, expected) in [
            (
                vec![
                    (Platform::Twitch, true),
                    (Platform::YouTube, true),
                    (Platform::Kick, true),
                ],
                true,
            ),
            (
                vec![
                    (Platform::Twitch, false),
                    (Platform::YouTube, true),
                    (Platform::Kick, false),
                ],
                true,
            ),
            (
                vec![
                    (Platform::Twitch, false),
                    (Platform::YouTube, false),
                    (Platform::Kick, false),
                ],
                false,
            ),
        ] {
            let any = with_bar(cx, targets.clone(), |bar, _window, _cx| {
                bar.any_target_selected()
            });
            assert_eq!(any, expected, "any_target_selected for {targets:?}");
        }
    }

    #[gpui::test]
    fn selected_targets_yields_the_on_platforms_in_strip_order(cx: &mut gpui::TestAppContext) {
        for (targets, expected) in [
            (
                vec![
                    (Platform::Twitch, true),
                    (Platform::YouTube, false),
                    (Platform::Kick, true),
                ],
                vec![Platform::Twitch, Platform::Kick],
            ),
            (
                vec![
                    (Platform::Twitch, false),
                    (Platform::YouTube, true),
                    (Platform::Kick, false),
                ],
                vec![Platform::YouTube],
            ),
            (
                vec![
                    (Platform::Twitch, false),
                    (Platform::YouTube, false),
                    (Platform::Kick, false),
                ],
                vec![],
            ),
        ] {
            let selected = with_bar(cx, targets.clone(), |bar, _window, _cx| {
                bar.selected_targets()
            });
            assert_eq!(selected, expected, "selected_targets for {targets:?}");
        }
    }

    #[gpui::test]
    fn toggling_a_target_flips_it_in_the_bitset_and_round_trips(cx: &mut gpui::TestAppContext) {
        let (after_off, selected_off, after_on) = with_bar(
            cx,
            vec![
                (Platform::Twitch, true),
                (Platform::YouTube, true),
                (Platform::Kick, true),
            ],
            |bar, _window, cx| {
                bar.toggle_target(1, cx);
                let off_bits = bar.targets_bitset();
                let off_sel = bar.selected_targets();
                bar.toggle_target(1, cx);
                (off_bits, off_sel, bar.targets_bitset())
            },
        );
        assert_eq!(after_off, 0b101);
        assert_eq!(selected_off, vec![Platform::Twitch, Platform::Kick]);
        assert_eq!(after_on, 0b111);
    }
}
