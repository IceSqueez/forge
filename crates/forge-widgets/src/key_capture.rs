use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::{
    Background, Border, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow, Size,
    alignment, keyboard, keyboard::key::Named,
};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_SM, FontRole, Radius, font, radius};

const WIDGET_H: f32 = 38.0;
const PAD_H: f32 = 12.0;
const BORDER_W: f32 = 1.0;

#[derive(Default, Clone)]
struct KeyCaptureState {
    focused: bool,
    current_modifiers: keyboard::Modifiers,
    partial_key: Option<String>,
    locked: bool,
}

pub struct KeyCapture<'a, Msg> {
    placeholder: String,
    value: Option<String>,
    on_captured: Option<Box<dyn Fn(String) -> Msg + 'a>>,
    on_reset: Option<Box<dyn Fn() -> Msg + 'a>>,
    palette: &'a ForgePalette,
}

pub fn key_capture<'a, Msg>(palette: &'a ForgePalette) -> KeyCapture<'a, Msg> {
    KeyCapture {
        placeholder: crate::tr!("widget.key_capture.placeholder"),
        value: None,
        on_captured: None,
        on_reset: None,
        palette,
    }
}

impl<'a, Msg> KeyCapture<'a, Msg> {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn value(mut self, combo: Option<impl Into<String>>) -> Self {
        self.value = combo.map(Into::into);
        self
    }

    pub fn on_captured(mut self, f: impl Fn(String) -> Msg + 'a) -> Self {
        self.on_captured = Some(Box::new(f));
        self
    }

    pub fn on_reset(mut self, f: impl Fn() -> Msg + 'a) -> Self {
        self.on_reset = Some(Box::new(f));
        self
    }

    fn display_str(&self, state: &KeyCaptureState) -> String {
        if !state.focused {
            return self
                .value
                .clone()
                .unwrap_or_else(|| self.placeholder.clone());
        }
        if state.locked {
            return build_combo_string(
                state.current_modifiers,
                state.partial_key.as_deref().unwrap_or(""),
            );
        }
        let mods = format_modifiers(state.current_modifiers);
        if mods.is_empty() {
            self.placeholder.clone()
        } else {
            format!("{mods}+\u{2026}")
        }
    }
}

impl<'a, Msg, Theme, Renderer> Widget<Msg, Theme, Renderer> for KeyCapture<'a, Msg>
where
    Renderer: iced::advanced::Renderer + text::Renderer<Font = Font>,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fixed(WIDGET_H),
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<KeyCaptureState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(KeyCaptureState::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let w = limits.max().width;
        layout::Node::new(Size::new(w, WIDGET_H))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<KeyCaptureState>();
        let bounds = layout.bounds();
        let p = self.palette;

        let (bg_color, border_color) = if state.focused {
            (p.elevated, p.brand)
        } else {
            (p.surface_overlay, p.border_input)
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    color: border_color,
                    width: BORDER_W,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(bg_color),
        );

        let text_color = if state.focused || self.value.is_some() {
            p.text_primary
        } else {
            p.text_faint
        };

        renderer.fill_text(
            text::Text {
                content: self.display_str(state),
                bounds: Size::new(bounds.width - PAD_H * 2.0, WIDGET_H),
                size: Pixels(FONT_SM),
                line_height: text::LineHeight::default(),
                font: font(FontRole::Monospace),
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::default(),
                wrapping: text::Wrapping::None,
            },
            Point::new(bounds.x + PAD_H, bounds.y),
            text_color,
            bounds,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<KeyCaptureState>();
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let is_over = cursor.is_over(bounds);
                if is_over && !state.focused {
                    state.focused = true;
                    state.current_modifiers = keyboard::Modifiers::empty();
                    state.partial_key = None;
                    state.locked = false;
                    shell.request_redraw();
                } else if !is_over && state.focused {
                    state.focused = false;
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(mods))
                if state.focused && !state.locked =>
            {
                state.current_modifiers = *mods;
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused =>
            {
                if matches!(key, keyboard::Key::Named(Named::Escape)) {
                    state.focused = false;
                    state.current_modifiers = keyboard::Modifiers::empty();
                    state.partial_key = None;
                    state.locked = false;
                    if let Some(cb) = &self.on_reset {
                        shell.publish((cb)());
                    }
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }

                if state.locked || is_modifier_key(key) {
                    return;
                }

                if let Some(key_str) = key_to_combo_segment(key) {
                    let combo = build_combo_string(*modifiers, &key_str);
                    state.partial_key = Some(key_str);
                    state.locked = true;
                    if let Some(cb) = &self.on_captured {
                        shell.publish((cb)(combo));
                    }
                    // Consume the keystroke so app-level keyboard subscriptions
                    // never dispatch a shortcut for the chord being recorded.
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, Msg, Theme, Renderer> From<KeyCapture<'a, Msg>> for Element<'a, Msg, Theme, Renderer>
where
    Msg: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + text::Renderer<Font = Font> + 'a,
{
    fn from(widget: KeyCapture<'a, Msg>) -> Self {
        Element::new(widget)
    }
}

/// Canonical chord form shared by capture, display, persistence and dispatch
/// (modifiers in Ctrl/Shift/Alt/Meta order, `+`-joined). None for keys that
/// cannot anchor a chord (modifiers alone, Escape, punctuation).
pub fn chord_from_key(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<String> {
    if is_modifier_key(key) {
        return None;
    }
    key_to_combo_segment(key).map(|segment| build_combo_string(modifiers, &segment))
}

fn is_modifier_key(key: &keyboard::Key) -> bool {
    matches!(
        key,
        keyboard::Key::Named(
            Named::Shift
                | Named::Control
                | Named::Alt
                | Named::AltGraph
                | Named::Super
                | Named::Meta
                | Named::Hyper
                | Named::CapsLock
                | Named::NumLock
                | Named::ScrollLock
                | Named::Fn
                | Named::FnLock
        )
    )
}

fn key_to_combo_segment(key: &keyboard::Key) -> Option<String> {
    match key {
        keyboard::Key::Character(s) => {
            let c = s.as_str();
            if c.len() != 1 {
                return None;
            }
            let ch = c.chars().next()?;
            if ch.is_ascii_alphabetic() {
                Some(ch.to_ascii_uppercase().to_string())
            } else if ch.is_ascii_digit() {
                Some(c.to_owned())
            } else {
                None
            }
        }
        keyboard::Key::Named(named) => named_to_segment(named),
        keyboard::Key::Unidentified => None,
    }
}

fn named_to_segment(named: &Named) -> Option<String> {
    match named {
        Named::F1 => Some("F1".into()),
        Named::F2 => Some("F2".into()),
        Named::F3 => Some("F3".into()),
        Named::F4 => Some("F4".into()),
        Named::F5 => Some("F5".into()),
        Named::F6 => Some("F6".into()),
        Named::F7 => Some("F7".into()),
        Named::F8 => Some("F8".into()),
        Named::F9 => Some("F9".into()),
        Named::F10 => Some("F10".into()),
        Named::F11 => Some("F11".into()),
        Named::F12 => Some("F12".into()),
        Named::Delete => Some("Delete".into()),
        Named::Insert => Some("Insert".into()),
        Named::Home => Some("Home".into()),
        Named::End => Some("End".into()),
        Named::PageUp => Some("PageUp".into()),
        Named::PageDown => Some("PageDown".into()),
        Named::Backspace => Some("Backspace".into()),
        Named::Tab => Some("Tab".into()),
        Named::Enter => Some("Enter".into()),
        Named::Space => Some("Space".into()),
        Named::ArrowUp => Some("ArrowUp".into()),
        Named::ArrowDown => Some("ArrowDown".into()),
        Named::ArrowLeft => Some("ArrowLeft".into()),
        Named::ArrowRight => Some("ArrowRight".into()),
        _ => None,
    }
}

fn format_modifiers(mods: keyboard::Modifiers) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mods.control() {
        parts.push("Ctrl");
    }
    if mods.shift() {
        parts.push("Shift");
    }
    if mods.alt() {
        parts.push("Alt");
    }
    if mods.logo() {
        parts.push("Meta");
    }
    parts.join("+")
}

fn build_combo_string(mods: keyboard::Modifiers, key: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mods.control() {
        parts.push("Ctrl");
    }
    if mods.shift() {
        parts.push("Shift");
    }
    if mods.alt() {
        parts.push("Alt");
    }
    if mods.logo() {
        parts.push("Meta");
    }
    parts.push(key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_modifiers_orders_modifiers_canonically() {
        for (mods, expected) in [
            (keyboard::Modifiers::empty(), ""),
            (keyboard::Modifiers::CTRL, "Ctrl"),
            (
                keyboard::Modifiers::SHIFT | keyboard::Modifiers::CTRL,
                "Ctrl+Shift",
            ),
            (
                keyboard::Modifiers::SHIFT
                    | keyboard::Modifiers::CTRL
                    | keyboard::Modifiers::ALT
                    | keyboard::Modifiers::LOGO,
                "Ctrl+Shift+Alt+Meta",
            ),
        ] {
            assert_eq!(format_modifiers(mods), expected);
        }
    }

    #[test]
    fn key_to_combo_segment_maps_letters_digits_and_named_keys() {
        for (key, expected) in [
            (keyboard::Key::Character("a".into()), "A"), // letters uppercase
            (keyboard::Key::Character("A".into()), "A"),
            (keyboard::Key::Character("5".into()), "5"),
            (keyboard::Key::Named(Named::F7), "F7"),
            (keyboard::Key::Named(Named::Delete), "Delete"),
            (keyboard::Key::Named(Named::ArrowUp), "ArrowUp"),
            (keyboard::Key::Named(Named::Space), "Space"),
        ] {
            assert_eq!(key_to_combo_segment(&key), Some(expected.to_owned()));
        }
    }

    #[test]
    fn key_to_combo_segment_rejects_keys_that_cannot_anchor_a_chord() {
        for key in [
            keyboard::Key::Named(Named::Shift),
            keyboard::Key::Named(Named::Control),
            keyboard::Key::Named(Named::Escape),
            keyboard::Key::Character(";".into()), // punctuation
            keyboard::Key::Unidentified,
        ] {
            assert_eq!(key_to_combo_segment(&key), None, "{key:?}");
        }
    }

    #[test]
    fn chord_from_key_canonicalizes_modifiers_and_key() {
        for (key, mods, expected) in [
            (
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::SHIFT | keyboard::Modifiers::CTRL,
                Some("Ctrl+Shift+A"),
            ),
            (
                keyboard::Key::Named(Named::F5),
                keyboard::Modifiers::ALT,
                Some("Alt+F5"),
            ),
            // Bindability is enforced later — a bare digit still canonicalizes.
            (
                keyboard::Key::Character("5".into()),
                keyboard::Modifiers::empty(),
                Some("5"),
            ),
            (
                keyboard::Key::Named(Named::Shift),
                keyboard::Modifiers::CTRL,
                None,
            ),
            (
                keyboard::Key::Named(Named::Escape),
                keyboard::Modifiers::empty(),
                None,
            ),
        ] {
            assert_eq!(
                chord_from_key(&key, mods),
                expected.map(str::to_owned),
                "{key:?}"
            );
        }
    }

    #[test]
    fn is_modifier_key_returns_true_for_modifiers() {
        assert!(is_modifier_key(&keyboard::Key::Named(Named::Shift)));
        assert!(is_modifier_key(&keyboard::Key::Named(Named::Control)));
        assert!(is_modifier_key(&keyboard::Key::Named(Named::Alt)));
        assert!(is_modifier_key(&keyboard::Key::Named(Named::Super)));
    }

    #[test]
    fn is_modifier_key_returns_false_for_regular_keys() {
        assert!(!is_modifier_key(&keyboard::Key::Named(Named::Enter)));
        assert!(!is_modifier_key(&keyboard::Key::Character("a".into())));
        assert!(!is_modifier_key(&keyboard::Key::Named(Named::F1)));
    }

    #[test]
    fn display_str_unfocused_shows_placeholder_when_no_value() {
        let palette = crate::palette::CATPPUCCIN_MOCHA;
        let widget = key_capture::<()>(&palette);
        let state = KeyCaptureState::default();
        assert_eq!(widget.display_str(&state), "widget.key_capture.placeholder");
    }

    #[test]
    fn display_str_unfocused_shows_value_when_set() {
        let palette = crate::palette::CATPPUCCIN_MOCHA;
        let widget = key_capture::<()>(&palette).value(Some("Ctrl+Shift+A"));
        let state = KeyCaptureState::default();
        assert_eq!(widget.display_str(&state), "Ctrl+Shift+A");
    }

    #[test]
    fn display_str_focused_no_modifiers_shows_placeholder() {
        let palette = crate::palette::CATPPUCCIN_MOCHA;
        let widget = key_capture::<()>(&palette);
        let state = KeyCaptureState {
            focused: true,
            current_modifiers: keyboard::Modifiers::empty(),
            partial_key: None,
            locked: false,
        };
        assert_eq!(widget.display_str(&state), "widget.key_capture.placeholder");
    }

    #[test]
    fn display_str_focused_with_modifiers_shows_partial() {
        let palette = crate::palette::CATPPUCCIN_MOCHA;
        let widget = key_capture::<()>(&palette);
        let state = KeyCaptureState {
            focused: true,
            current_modifiers: keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
            partial_key: None,
            locked: false,
        };
        assert_eq!(widget.display_str(&state), "Ctrl+Shift+\u{2026}");
    }

    #[test]
    fn display_str_locked_shows_full_combo() {
        let palette = crate::palette::CATPPUCCIN_MOCHA;
        let widget = key_capture::<()>(&palette);
        let state = KeyCaptureState {
            focused: true,
            current_modifiers: keyboard::Modifiers::CTRL,
            partial_key: Some("A".to_owned()),
            locked: true,
        };
        assert_eq!(widget.display_str(&state), "Ctrl+A");
    }
}
