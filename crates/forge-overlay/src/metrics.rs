use crate::kinds::{alert, chat, frame, goal, ticker};

pub const SURFACE_RGB: [u8; 3] = [20, 20, 28];
pub const PILL_RADIUS: f32 = 999.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertMetrics {
    pub body_padding: f32,
    pub gap: f32,
    pub padding_block: f32,
    pub padding_inline: f32,
    pub border: f32,
    pub radius: f32,
    pub icon_size: f32,
    pub headline_size: f32,
    pub subline_size: f32,
    pub surface_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatMetrics {
    pub body_padding: f32,
    pub column_width: f32,
    pub row_gap: f32,
    pub row_padding_block: f32,
    pub row_padding_inline: f32,
    pub row_radius: f32,
    pub badges_size: f32,
    pub author_size: f32,
    pub message_size: f32,
    pub surface_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameMetrics {
    pub inset: f32,
    pub border: f32,
    pub radius: f32,
    pub label_inset_inline: f32,
    /// How far the corner label hangs past the frame edge; the stylesheet states it as a negative offset.
    pub label_overhang: f32,
    pub label_gap: f32,
    pub label_padding_block: f32,
    pub label_padding_inline: f32,
    pub label_border: f32,
    pub label_radius: f32,
    pub headline_size: f32,
    pub subline_size: f32,
    pub surface_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoalMetrics {
    pub body_padding: f32,
    pub width: f32,
    pub gap: f32,
    pub padding_block: f32,
    pub padding_inline: f32,
    pub border: f32,
    pub radius: f32,
    pub label_size: f32,
    pub track_height: f32,
    pub track_radius: f32,
    pub figure_size: f32,
    pub separator_margin: f32,
    pub surface_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickerMetrics {
    pub gap: f32,
    pub padding_block: f32,
    pub padding_inline: f32,
    pub edge_border: f32,
    pub mark_width: f32,
    pub mark_height: f32,
    pub mark_radius: f32,
    pub headline_size: f32,
    pub subline_size: f32,
    pub surface_alpha: f32,
}

pub const ALERT: AlertMetrics = AlertMetrics {
    body_padding: 32.0,
    gap: 14.0,
    padding_block: 16.0,
    padding_inline: 22.0,
    border: 1.0,
    radius: 12.0,
    icon_size: 26.0,
    headline_size: 20.0,
    subline_size: 13.0,
    surface_alpha: 0.86,
};

pub const CHAT: ChatMetrics = ChatMetrics {
    body_padding: 24.0,
    column_width: 360.0,
    row_gap: 6.0,
    row_padding_block: 6.0,
    row_padding_inline: 10.0,
    row_radius: 8.0,
    badges_size: 11.0,
    author_size: 13.0,
    message_size: 13.0,
    surface_alpha: 0.78,
};

pub const FRAME: FrameMetrics = FrameMetrics {
    inset: 16.0,
    border: 3.0,
    radius: 14.0,
    label_inset_inline: 22.0,
    label_overhang: 13.0,
    label_gap: 8.0,
    label_padding_block: 3.0,
    label_padding_inline: 14.0,
    label_border: 1.0,
    label_radius: PILL_RADIUS,
    headline_size: 13.0,
    subline_size: 11.0,
    surface_alpha: 0.92,
};

pub const GOAL: GoalMetrics = GoalMetrics {
    body_padding: 32.0,
    width: 320.0,
    gap: 8.0,
    padding_block: 14.0,
    padding_inline: 18.0,
    border: 1.0,
    radius: 12.0,
    label_size: 14.0,
    track_height: 10.0,
    track_radius: PILL_RADIUS,
    figure_size: 12.0,
    separator_margin: 4.0,
    surface_alpha: 0.86,
};

pub const TICKER: TickerMetrics = TickerMetrics {
    gap: 14.0,
    padding_block: 10.0,
    padding_inline: 26.0,
    edge_border: 1.0,
    mark_width: 6.0,
    mark_height: 22.0,
    mark_radius: 3.0,
    headline_size: 19.0,
    subline_size: 13.0,
    surface_alpha: 0.9,
};

/// Every declaration must appear verbatim in the kind's stylesheet; that containment is what keeps
/// a drawn preview and its page on the same numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleGuard {
    pub kind_id: &'static str,
    pub declarations: Vec<String>,
}

pub fn style_guards() -> Vec<StyleGuard> {
    vec![
        StyleGuard {
            kind_id: alert::KIND_ID,
            declarations: ALERT.declarations(),
        },
        StyleGuard {
            kind_id: chat::KIND_ID,
            declarations: CHAT.declarations(),
        },
        StyleGuard {
            kind_id: frame::KIND_ID,
            declarations: FRAME.declarations(),
        },
        StyleGuard {
            kind_id: goal::KIND_ID,
            declarations: GOAL.declarations(),
        },
        StyleGuard {
            kind_id: ticker::KIND_ID,
            declarations: TICKER.declarations(),
        },
    ]
}

impl AlertMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("padding", self.body_padding),
            length("gap", self.gap),
            length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            length("border-radius", self.radius),
            length("font-size", self.icon_size),
            length("font-size", self.headline_size),
            length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

impl ChatMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("padding", self.body_padding),
            length("width", self.column_width),
            length("gap", self.row_gap),
            length_pair("padding", self.row_padding_block, self.row_padding_inline),
            length("border-radius", self.row_radius),
            length("font-size", self.badges_size),
            length("font-size", self.author_size),
            length("font-size", self.message_size),
            surface(self.surface_alpha),
        ]
    }
}

impl FrameMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("inset", self.inset),
            accent_border("border", self.border),
            length("border-radius", self.radius),
            length("left", self.label_inset_inline),
            negative_length("bottom", self.label_overhang),
            length("gap", self.label_gap),
            length_pair(
                "padding",
                self.label_padding_block,
                self.label_padding_inline,
            ),
            accent_border("border", self.label_border),
            length("border-radius", self.label_radius),
            length("font-size", self.headline_size),
            length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

impl GoalMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("padding", self.body_padding),
            length("width", self.width),
            length("gap", self.gap),
            length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            length("border-radius", self.radius),
            length("font-size", self.label_size),
            length("height", self.track_height),
            length("border-radius", self.track_radius),
            length("font-size", self.figure_size),
            format!("margin: 0 {}px;", self.separator_margin),
            surface(self.surface_alpha),
        ]
    }
}

impl TickerMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("gap", self.gap),
            length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border-top", self.edge_border),
            accent_border("border-bottom", self.edge_border),
            length("width", self.mark_width),
            length("height", self.mark_height),
            length("border-radius", self.mark_radius),
            length("font-size", self.headline_size),
            length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

fn length(property: &str, value: f32) -> String {
    format!("{property}: {value}px;")
}

fn negative_length(property: &str, value: f32) -> String {
    format!("{property}: -{value}px;")
}

fn length_pair(property: &str, block: f32, inline: f32) -> String {
    format!("{property}: {block}px {inline}px;")
}

fn accent_border(property: &str, width: f32) -> String {
    format!("{property}: {width}px solid var(--accent);")
}

fn surface(alpha: f32) -> String {
    let [red, green, blue] = SURFACE_RGB;
    format!("background: rgba({red}, {green}, {blue}, {alpha});")
}
