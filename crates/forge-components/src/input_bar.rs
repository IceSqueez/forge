use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, px,
};

use crate::chat_row::PlatformMark;
use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XXS, Radius, Spacing, body_family, mono_family, radius,
    spacing,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SendTarget {
    pub id: SharedString,
    pub mark: PlatformMark,
}

#[derive(Debug, Clone)]
struct TargetSlot {
    target: SendTarget,
    active: bool,
    available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputBarSwitch {
    pub options: Vec<SharedString>,
    pub selected: usize,
}

#[derive(Clone, Debug)]
pub enum InputBarEvent {
    Send {
        text: SharedString,
        targets: Vec<SharedString>,
    },
    EmojiToggled,
    TargetsChanged,
    SwitchChanged(usize),
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
const SWITCH_PAD_X: gpui::Pixels = px(6.0);

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

pub struct InputBar {
    field: Entity<TextInput>,
    targets: Vec<TargetSlot>,
    switch: Option<InputBarSwitch>,
    max_chars: Option<usize>,
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
            targets: Vec::new(),
            switch: None,
            max_chars: None,
            emoji_open: false,
            palette,
            density: Density::default(),
            emoji_scroll: ScrollHandle::new(),
            _field_sub: field_sub,
        }
    }

    pub fn with_targets(mut self, targets: Vec<(SendTarget, bool)>) -> Self {
        self.targets = targets
            .into_iter()
            .map(|(target, active)| TargetSlot {
                target,
                active,
                available: true,
            })
            .collect();
        self
    }

    pub fn selected_targets(&self) -> Vec<SharedString> {
        self.targets
            .iter()
            .filter(|slot| slot.active)
            .map(|slot| slot.target.id.clone())
            .collect()
    }

    pub fn any_target_selected(&self) -> bool {
        self.targets.iter().any(|slot| slot.active)
    }

    pub fn is_available(&self, id: &str) -> bool {
        self.targets
            .iter()
            .any(|slot| slot.available && slot.target.id == id)
    }

    pub fn effective_targets(&self) -> Vec<SharedString> {
        self.targets
            .iter()
            .filter(|slot| slot.active && slot.available)
            .map(|slot| slot.target.id.clone())
            .collect()
    }

    pub fn set_available_targets(&mut self, available: &[SharedString], cx: &mut Context<Self>) {
        let mut changed = false;
        for slot in &mut self.targets {
            let next = available.contains(&slot.target.id);
            changed |= slot.available != next;
            slot.available = next;
        }
        if changed {
            cx.notify();
        }
    }

    pub fn set_switch(&mut self, switch: Option<InputBarSwitch>, cx: &mut Context<Self>) {
        if self.switch != switch {
            self.switch = switch;
            cx.notify();
        }
    }

    pub fn set_max_chars(&mut self, max_chars: Option<usize>, cx: &mut Context<Self>) {
        if self.max_chars != max_chars {
            self.max_chars = max_chars;
            cx.notify();
        }
    }

    pub fn char_count(&self, cx: &App) -> usize {
        self.field.read(cx).content().trim().chars().count()
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
        match event {
            InputEvent::Submitted(_) => self.emit_send(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) | InputEvent::Cancelled => {}
        }
    }

    fn emit_send(&mut self, cx: &mut Context<Self>) {
        let text = self.field.read(cx).content().to_string();
        let targets = self.effective_targets();
        cx.emit(InputBarEvent::Send {
            text: text.into(),
            targets,
        });
    }

    fn toggle_target(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(slot) = self.targets.get_mut(idx).filter(|slot| slot.available) {
            slot.active = !slot.active;
        }
        cx.emit(InputBarEvent::TargetsChanged);
        cx.notify();
    }

    fn pick_switch(&mut self, idx: usize, cx: &mut Context<Self>) {
        let Some(switch) = self.switch.as_mut() else {
            return;
        };
        if idx >= switch.options.len() || switch.selected == idx {
            return;
        }
        switch.selected = idx;
        cx.emit(InputBarEvent::SwitchChanged(idx));
        cx.notify();
    }

    fn render_switch(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let switch = self.switch.as_ref()?;
        let p = self.palette;
        let mut group = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(TARGET_GROUP_GAP);
        for (idx, label) in switch.options.iter().enumerate() {
            let lit = idx == switch.selected;
            let mut tile = div()
                .id(("forge-inputbar-switch", idx))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .h(TARGET_SIZE)
                .px(SWITCH_PAD_X)
                .rounded(TARGET_RADIUS)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| this.pick_switch(idx, cx)))
                .child(
                    div()
                        .font_family(body_family())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(FONT_XXS)
                        .text_color(if lit { p.text_primary } else { p.text_faint })
                        .child(label.clone()),
                );
            if lit {
                tile = tile.bg(p.surface_overlay);
            }
            group = group.child(tile);
        }
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(COMPOSER_GAP)
                .child(group)
                .child(
                    div()
                        .flex_none()
                        .w(DIVIDER_WIDTH)
                        .h(DIVIDER_HEIGHT)
                        .bg(p.border_regular),
                ),
        )
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
        slot: TargetSlot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette;
        let available = slot.available;
        let lit = slot.active && available;
        let letter_color = match (available, lit) {
            (false, _) => p.text_extreme_faint,
            (true, true) => slot.target.mark.color,
            (true, false) => p.text_faint,
        };

        let mut tile = div()
            .id(("forge-inputbar-target", idx))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .w(TARGET_SIZE)
            .h(TARGET_SIZE)
            .rounded(TARGET_RADIUS);
        if available {
            tile = tile.cursor_pointer().on_click(
                cx.listener(move |this, _event, _window, cx| this.toggle_target(idx, cx)),
            );
        }
        tile = tile.child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(TARGET_LETTER_SIZE)
                .text_color(letter_color)
                .child(slot.target.mark.letter),
        );
        if lit {
            tile = tile.bg(p.surface_overlay);
        }
        tile
    }

    fn render_char_counter(&self, cx: &App) -> Option<impl IntoElement> {
        let limit = self.max_chars?;
        let count = self.char_count(cx);
        if count == 0 {
            return None;
        }
        let color = if count > limit {
            self.palette.danger
        } else {
            self.palette.text_faint
        };
        Some(
            div()
                .ml_auto()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(color)
                .child(format!("{count}/{limit}")),
        )
    }

    fn render_hint(&self, glyph: &'static str, label: &'static str) -> impl IntoElement {
        let color = self.palette.text_faint;
        div()
            .flex()
            .items_center()
            .gap(HINT_GAP)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(color)
                    .child(glyph),
            )
            .child(
                div()
                    .font_family(body_family())
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
                    .font_family(body_family())
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
        for (idx, slot) in strip.into_iter().enumerate() {
            targets_group = targets_group.child(self.render_target(idx, slot, cx));
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
            .children(self.render_switch(cx))
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
                    .child(self.render_hint("!", "trigger action"))
                    .children(self.render_char_counter(cx)),
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

    const TWITCH: &str = "twitch";
    const YOUTUBE: &str = "youtube";
    const KICK: &str = "kick";

    fn target(id: &'static str) -> SendTarget {
        SendTarget {
            id: SharedString::new_static(id),
            mark: PlatformMark {
                letter: SharedString::new_static("X"),
                color: FORGE_DEFAULT.brand,
            },
        }
    }

    fn strip(twitch: bool, youtube: bool, kick: bool) -> Vec<(SendTarget, bool)> {
        vec![
            (target(TWITCH), twitch),
            (target(YOUTUBE), youtube),
            (target(KICK), kick),
        ]
    }

    fn ids(raw: &[&'static str]) -> Vec<SharedString> {
        raw.iter().map(|id| SharedString::new_static(id)).collect()
    }

    fn with_bar<R>(
        cx: &mut gpui::TestAppContext,
        targets: Vec<(SendTarget, bool)>,
        f: impl FnOnce(&mut InputBar, &mut Window, &mut Context<InputBar>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| {
            InputBar::new("placeholder", FORGE_DEFAULT, cx).with_targets(targets)
        });
        window
            .update(cx, |bar, window, cx| f(bar, window, cx))
            .unwrap()
    }

    #[gpui::test]
    fn any_target_selected_is_true_only_when_a_target_is_on(cx: &mut gpui::TestAppContext) {
        for (targets, expected) in [
            (strip(true, true, true), true),
            (strip(false, true, false), true),
            (strip(false, false, false), false),
            (Vec::new(), false),
        ] {
            let any = with_bar(cx, targets.clone(), |bar, _window, _cx| {
                bar.any_target_selected()
            });
            assert_eq!(any, expected, "any_target_selected for {targets:?}");
        }
    }

    #[gpui::test]
    fn selected_targets_yields_the_on_target_ids_in_strip_order(cx: &mut gpui::TestAppContext) {
        for (targets, expected) in [
            (strip(true, false, true), ids(&[TWITCH, KICK])),
            (strip(false, true, false), ids(&[YOUTUBE])),
            (strip(false, false, false), ids(&[])),
        ] {
            let selected = with_bar(cx, targets.clone(), |bar, _window, _cx| {
                bar.selected_targets()
            });
            assert_eq!(selected, expected, "selected_targets for {targets:?}");
        }
    }

    #[gpui::test]
    fn toggling_a_target_flips_its_selection_and_round_trips(cx: &mut gpui::TestAppContext) {
        let (after_off, after_on) = with_bar(cx, strip(true, true, true), |bar, _window, cx| {
            bar.toggle_target(1, cx);
            let off = bar.selected_targets();
            bar.toggle_target(1, cx);
            (off, bar.selected_targets())
        });
        assert_eq!(after_off, ids(&[TWITCH, KICK]));
        assert_eq!(after_on, ids(&[TWITCH, YOUTUBE, KICK]));
    }

    #[gpui::test]
    fn toggling_an_index_past_the_strip_changes_nothing(cx: &mut gpui::TestAppContext) {
        let selected = with_bar(cx, strip(true, false, true), |bar, _window, cx| {
            bar.toggle_target(3, cx);
            bar.selected_targets()
        });
        assert_eq!(selected, ids(&[TWITCH, KICK]));
    }

    #[gpui::test]
    fn unavailable_targets_drop_out_of_the_effective_targets_but_stay_selected(
        cx: &mut gpui::TestAppContext,
    ) {
        let (effective, selected, youtube_available) =
            with_bar(cx, strip(true, true, true), |bar, _window, cx| {
                bar.set_available_targets(&ids(&[TWITCH, KICK]), cx);
                (
                    bar.effective_targets(),
                    bar.selected_targets(),
                    bar.is_available(YOUTUBE),
                )
            });

        assert_eq!(effective, ids(&[TWITCH, KICK]));
        assert_eq!(selected, ids(&[TWITCH, YOUTUBE, KICK]));
        assert!(!youtube_available);
    }

    #[gpui::test]
    fn a_target_coming_back_rejoins_the_effective_targets(cx: &mut gpui::TestAppContext) {
        let effective = with_bar(cx, strip(true, true, true), |bar, _window, cx| {
            bar.set_available_targets(&ids(&[TWITCH]), cx);
            bar.set_available_targets(&ids(&[TWITCH, YOUTUBE]), cx);
            bar.effective_targets()
        });

        assert_eq!(effective, ids(&[TWITCH, YOUTUBE]));
    }

    #[gpui::test]
    fn an_id_outside_the_strip_is_never_available(cx: &mut gpui::TestAppContext) {
        let (available, effective) = with_bar(cx, strip(true, false, false), |bar, _window, cx| {
            bar.set_available_targets(&ids(&[TWITCH, "trovo"]), cx);
            (bar.is_available("trovo"), bar.effective_targets())
        });

        assert!(!available);
        assert_eq!(effective, ids(&[TWITCH]));
    }

    #[gpui::test]
    fn toggling_an_unavailable_target_leaves_the_selection_unchanged(
        cx: &mut gpui::TestAppContext,
    ) {
        let selected = with_bar(cx, strip(true, true, true), |bar, _window, cx| {
            bar.set_available_targets(&ids(&[TWITCH, KICK]), cx);
            bar.toggle_target(1, cx);
            bar.selected_targets()
        });

        assert_eq!(selected, ids(&[TWITCH, YOUTUBE, KICK]));
    }

    #[gpui::test]
    fn send_carries_only_the_available_selected_targets(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_window, cx| {
            InputBar::new("placeholder", FORGE_DEFAULT, cx).with_targets(strip(true, true, true))
        });
        let bar = window.root(cx).unwrap();
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = std::rc::Rc::clone(&sent);
        let _subscription = cx.update(|cx| {
            cx.subscribe(&bar, move |_, event: &InputBarEvent, _| {
                if let InputBarEvent::Send { targets, .. } = event {
                    sink.borrow_mut().push(targets.clone());
                }
            })
        });

        bar.update(cx, |bar, cx| {
            bar.set_available_targets(&ids(&[YOUTUBE]), cx);
            bar.emit_send(cx);
        });

        assert_eq!(*sent.borrow(), vec![ids(&[YOUTUBE])]);
    }

    #[gpui::test]
    fn char_count_counts_characters_of_the_trimmed_text(cx: &mut gpui::TestAppContext) {
        for (text, expected) in [
            ("  hello  ", 5),
            ("привіт", 6),
            ("👋 hi", 4),
            (" \t ", 0),
            ("", 0),
        ] {
            let count = with_bar(cx, strip(true, true, true), |bar, _window, cx| {
                bar.field
                    .update(cx, |field, cx| field.set_content(text, cx));
                bar.char_count(cx)
            });
            assert_eq!(count, expected, "text {text:?}");
        }
    }

    fn me_bot(selected: usize) -> Option<InputBarSwitch> {
        Some(InputBarSwitch {
            options: vec!["Me".into(), "Bot".into()],
            selected,
        })
    }

    fn pick_on_switch(
        cx: &mut gpui::TestAppContext,
        switch: Option<InputBarSwitch>,
        picks: &[usize],
    ) -> (Vec<usize>, Option<InputBarSwitch>) {
        let window = cx.add_window(|_window, cx| {
            InputBar::new("placeholder", FORGE_DEFAULT, cx).with_targets(strip(true, true, true))
        });
        let bar = window.root(cx).unwrap();
        let emitted = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = std::rc::Rc::clone(&emitted);
        let _subscription = cx.update(|cx| {
            cx.subscribe(&bar, move |_, event: &InputBarEvent, _| {
                if let InputBarEvent::SwitchChanged(idx) = event {
                    sink.borrow_mut().push(*idx);
                }
            })
        });

        let state = bar.update(cx, |bar, cx| {
            bar.set_switch(switch, cx);
            for idx in picks {
                bar.pick_switch(*idx, cx);
            }
            bar.switch.clone()
        });

        let emitted = emitted.borrow().clone();
        (emitted, state)
    }

    #[gpui::test]
    fn picking_another_switch_option_selects_it_and_reports_its_index(
        cx: &mut gpui::TestAppContext,
    ) {
        let (emitted, state) = pick_on_switch(cx, me_bot(0), &[1, 0]);

        assert_eq!(emitted, vec![1, 0]);
        assert_eq!(state, me_bot(0));
    }

    #[gpui::test]
    fn picking_the_selected_option_or_one_past_the_end_reports_nothing(
        cx: &mut gpui::TestAppContext,
    ) {
        let (emitted, state) = pick_on_switch(cx, me_bot(1), &[1, 2, usize::MAX]);

        assert!(emitted.is_empty(), "{emitted:?}");
        assert_eq!(state, me_bot(1));
    }

    #[gpui::test]
    fn picking_without_a_switch_reports_nothing(cx: &mut gpui::TestAppContext) {
        let (emitted, state) = pick_on_switch(cx, None, &[0, 1]);

        assert!(emitted.is_empty());
        assert_eq!(state, None);
    }
}
