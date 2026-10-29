pub const COMBO_SEPARATOR: char = '+';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Meta,
}

impl Modifier {
    pub const ALL: [Modifier; 4] = [
        Modifier::Ctrl,
        Modifier::Shift,
        Modifier::Alt,
        Modifier::Meta,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Alt => "Alt",
            Modifier::Meta => "Meta",
        }
    }

    pub fn from_alias(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Modifier::Ctrl),
            "shift" => Some(Modifier::Shift),
            "alt" | "option" => Some(Modifier::Alt),
            "meta" | "cmd" | "super" | "win" => Some(Modifier::Meta),
            "cmdorctrl" | "commandorcontrol" => Some(Self::PRIMARY),
            _ => None,
        }
    }

    #[cfg(target_os = "macos")]
    const PRIMARY: Modifier = Modifier::Meta;
    #[cfg(not(target_os = "macos"))]
    const PRIMARY: Modifier = Modifier::Ctrl;

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);

    pub const fn with(self, modifier: Modifier) -> Self {
        Modifiers(self.0 | modifier.bit())
    }

    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL.into_iter().filter(move |m| self.contains(*m))
    }
}

impl FromIterator<Modifier> for Modifiers {
    fn from_iter<I: IntoIterator<Item = Modifier>>(iter: I) -> Self {
        iter.into_iter().fold(Modifiers::NONE, Modifiers::with)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Tab,
    Enter,
    Escape,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
}

impl Key {
    pub const ALL: [Key; 73] = [
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::Digit0,
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::Delete,
        Key::Insert,
        Key::Home,
        Key::End,
        Key::PageUp,
        Key::PageDown,
        Key::Backspace,
        Key::Tab,
        Key::Enter,
        Key::Escape,
        Key::Space,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::Num0,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Key::A => "A",
            Key::B => "B",
            Key::C => "C",
            Key::D => "D",
            Key::E => "E",
            Key::F => "F",
            Key::G => "G",
            Key::H => "H",
            Key::I => "I",
            Key::J => "J",
            Key::K => "K",
            Key::L => "L",
            Key::M => "M",
            Key::N => "N",
            Key::O => "O",
            Key::P => "P",
            Key::Q => "Q",
            Key::R => "R",
            Key::S => "S",
            Key::T => "T",
            Key::U => "U",
            Key::V => "V",
            Key::W => "W",
            Key::X => "X",
            Key::Y => "Y",
            Key::Z => "Z",
            Key::Digit0 => "0",
            Key::Digit1 => "1",
            Key::Digit2 => "2",
            Key::Digit3 => "3",
            Key::Digit4 => "4",
            Key::Digit5 => "5",
            Key::Digit6 => "6",
            Key::Digit7 => "7",
            Key::Digit8 => "8",
            Key::Digit9 => "9",
            Key::F1 => "F1",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            Key::Delete => "Delete",
            Key::Insert => "Insert",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "PageUp",
            Key::PageDown => "PageDown",
            Key::Backspace => "Backspace",
            Key::Tab => "Tab",
            Key::Enter => "Enter",
            Key::Escape => "Escape",
            Key::Space => "Space",
            Key::ArrowUp => "ArrowUp",
            Key::ArrowDown => "ArrowDown",
            Key::ArrowLeft => "ArrowLeft",
            Key::ArrowRight => "ArrowRight",
            Key::Num0 => "Num0",
            Key::Num1 => "Num1",
            Key::Num2 => "Num2",
            Key::Num3 => "Num3",
            Key::Num4 => "Num4",
            Key::Num5 => "Num5",
            Key::Num6 => "Num6",
            Key::Num7 => "Num7",
            Key::Num8 => "Num8",
            Key::Num9 => "Num9",
        }
    }

    pub fn from_alias(token: &str) -> Option<Self> {
        if let Some(key) = Self::ALL
            .into_iter()
            .find(|key| key.name().eq_ignore_ascii_case(token))
        {
            return Some(key);
        }
        match token.to_ascii_lowercase().as_str() {
            "del" => Some(Key::Delete),
            "ins" => Some(Key::Insert),
            "pgup" => Some(Key::PageUp),
            "pgdn" | "pgdown" => Some(Key::PageDown),
            "return" => Some(Key::Enter),
            "esc" => Some(Key::Escape),
            "up" => Some(Key::ArrowUp),
            "down" => Some(Key::ArrowDown),
            "left" => Some(Key::ArrowLeft),
            "right" => Some(Key::ArrowRight),
            "numpad0" => Some(Key::Num0),
            "numpad1" => Some(Key::Num1),
            "numpad2" => Some(Key::Num2),
            "numpad3" => Some(Key::Num3),
            "numpad4" => Some(Key::Num4),
            "numpad5" => Some(Key::Num5),
            "numpad6" => Some(Key::Num6),
            "numpad7" => Some(Key::Num7),
            "numpad8" => Some(Key::Num8),
            "numpad9" => Some(Key::Num9),
            _ => None,
        }
    }

    pub const fn types_text(self) -> bool {
        !matches!(
            self,
            Key::F1
                | Key::F2
                | Key::F3
                | Key::F4
                | Key::F5
                | Key::F6
                | Key::F7
                | Key::F8
                | Key::F9
                | Key::F10
                | Key::F11
                | Key::F12
                | Key::Delete
                | Key::Insert
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
                | Key::Escape
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::ArrowLeft
                | Key::ArrowRight
        )
    }
}
