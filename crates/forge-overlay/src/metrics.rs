use crate::kinds::{alert, chat, frame, goal, ticker};

pub const SURFACE_RGB: [u8; 3] = [20, 20, 28];
pub const PILL_RADIUS: f32 = 999.0;

pub const TEXT_SIZE_PROPERTY: &str = "--text-size";
pub const TEXT_SCALE_PROPERTY: &str = "--text-scale";
pub const ELEMENT_WIDTH_PROPERTY: &str = "--element-width";
pub const ELEMENT_HEIGHT_PROPERTY: &str = "--element-height";

const WIDTH_AXIS: &str = "width";
const HEIGHT_AXIS: &str = "height";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisBound {
    Exact,
    /// The element grows past the value and centers its content inside whatever is left over.
    AtLeast,
    /// The element shows as much of its content as the value holds and hides the rest.
    AtMost,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisFallback {
    Content,
    Canvas,
    Pixels(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementAxis {
    pub bound: AxisBound,
    pub fallback: AxisFallback,
}

/// An axis left unset spans the canvas and is offered as no field; every size is a pixel of the
/// reference canvas, and `base_text_size` is the divisor of the one ratio a kind scales by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementSizing {
    pub width: Option<ElementAxis>,
    pub height: Option<ElementAxis>,
    pub base_text_size: f32,
}

pub const ALERT_SIZING: ElementSizing = ElementSizing {
    width: Some(ElementAxis {
        bound: AxisBound::Exact,
        fallback: AxisFallback::Content,
    }),
    height: Some(ElementAxis {
        bound: AxisBound::AtLeast,
        fallback: AxisFallback::Content,
    }),
    base_text_size: ALERT.headline_size,
};

pub const CHAT_SIZING: ElementSizing = ElementSizing {
    width: Some(ElementAxis {
        bound: AxisBound::Exact,
        fallback: AxisFallback::Pixels(CHAT.column_width),
    }),
    height: Some(ElementAxis {
        bound: AxisBound::AtMost,
        fallback: AxisFallback::Canvas,
    }),
    base_text_size: CHAT.message_size,
};

pub const FRAME_SIZING: ElementSizing = ElementSizing {
    width: None,
    height: None,
    base_text_size: FRAME.headline_size,
};

pub const GOAL_SIZING: ElementSizing = ElementSizing {
    width: Some(ElementAxis {
        bound: AxisBound::Exact,
        fallback: AxisFallback::Pixels(GOAL.width),
    }),
    height: Some(ElementAxis {
        bound: AxisBound::AtLeast,
        fallback: AxisFallback::Content,
    }),
    base_text_size: GOAL.label_size,
};

pub const TICKER_SIZING: ElementSizing = ElementSizing {
    width: None,
    height: Some(ElementAxis {
        bound: AxisBound::AtLeast,
        fallback: AxisFallback::Content,
    }),
    base_text_size: TICKER.headline_size,
};

pub fn element_sizing(kind_id: &str) -> Option<ElementSizing> {
    match kind_id {
        alert::KIND_ID => Some(ALERT_SIZING),
        chat::KIND_ID => Some(CHAT_SIZING),
        frame::KIND_ID => Some(FRAME_SIZING),
        goal::KIND_ID => Some(GOAL_SIZING),
        ticker::KIND_ID => Some(TICKER_SIZING),
        _ => None,
    }
}

impl ElementSizing {
    pub fn default_text_size_px(self) -> u32 {
        self.base_text_size as u32
    }

    pub fn text_scale(self, text_size_px: u32) -> f32 {
        text_size_px as f32 / self.base_text_size
    }

    pub fn declarations(self) -> Vec<String> {
        let mut out = vec![text_scale(self.base_text_size)];
        if let Some(axis) = self.width {
            out.push(axis_extent(WIDTH_AXIS, ELEMENT_WIDTH_PROPERTY, axis));
        }
        if let Some(axis) = self.height {
            out.push(axis_extent(HEIGHT_AXIS, ELEMENT_HEIGHT_PROPERTY, axis));
        }
        out
    }
}

/// Every declaration must appear verbatim in the kind's stylesheet; that containment is what keeps
/// a drawn preview and its page on the same numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleGuard {
    pub kind_id: &'static str,
    pub declarations: Vec<String>,
}

pub fn style_guards() -> Vec<StyleGuard> {
    vec![
        guard(alert::KIND_ID, ALERT.declarations(), ALERT_SIZING),
        guard(chat::KIND_ID, CHAT.declarations(), CHAT_SIZING),
        guard(frame::KIND_ID, FRAME.declarations(), FRAME_SIZING),
        guard(goal::KIND_ID, GOAL.declarations(), GOAL_SIZING),
        guard(ticker::KIND_ID, TICKER.declarations(), TICKER_SIZING),
    ]
}

fn guard(
    kind_id: &'static str,
    mut declarations: Vec<String>,
    sizing: ElementSizing,
) -> StyleGuard {
    declarations.extend(sizing.declarations());
    StyleGuard {
        kind_id,
        declarations,
    }
}

impl AlertMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("padding", self.body_padding),
            scaled_length("gap", self.gap),
            scaled_length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            scaled_length("border-radius", self.radius),
            scaled_length("height", self.icon_size),
            accent_fill(),
            scaled_length("font-size", self.headline_size),
            scaled_length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

impl ChatMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("padding", self.body_padding),
            scaled_length("gap", self.row_gap),
            scaled_length_pair("padding", self.row_padding_block, self.row_padding_inline),
            scaled_length("border-radius", self.row_radius),
            scaled_length("font-size", self.badges_size),
            scaled_length("font-size", self.author_size),
            scaled_length("font-size", self.message_size),
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
            scaled_negative_length("bottom", self.label_overhang),
            scaled_length("gap", self.label_gap),
            scaled_length_pair(
                "padding",
                self.label_padding_block,
                self.label_padding_inline,
            ),
            accent_border("border", self.label_border),
            length("border-radius", self.label_radius),
            scaled_length("font-size", self.headline_size),
            scaled_length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

impl GoalMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            length("padding", self.body_padding),
            scaled_length("gap", self.gap),
            scaled_length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            scaled_length("border-radius", self.radius),
            scaled_length("font-size", self.label_size),
            scaled_length("height", self.track_height),
            length("border-radius", self.track_radius),
            scaled_length("font-size", self.figure_size),
            format!("margin: 0 {};", scaled(self.separator_margin)),
            surface(self.surface_alpha),
        ]
    }
}

impl TickerMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            scaled_length("gap", self.gap),
            scaled_length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border-top", self.edge_border),
            accent_border("border-bottom", self.edge_border),
            scaled_length("width", self.mark_width),
            scaled_length("height", self.mark_height),
            scaled_length("border-radius", self.mark_radius),
            scaled_length("font-size", self.headline_size),
            scaled_length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

fn length(property: &str, value: f32) -> String {
    format!("{property}: {value}px;")
}

fn scaled(value: f32) -> String {
    format!("calc({value}px * var({TEXT_SCALE_PROPERTY}))")
}

fn scaled_length(property: &str, value: f32) -> String {
    format!("{property}: {};", scaled(value))
}

fn scaled_negative_length(property: &str, value: f32) -> String {
    format!("{property}: calc(-{value}px * var({TEXT_SCALE_PROPERTY}));")
}

fn scaled_length_pair(property: &str, block: f32, inline: f32) -> String {
    format!("{property}: {} {};", scaled(block), scaled(inline))
}

fn text_scale(base: f32) -> String {
    format!("{TEXT_SCALE_PROPERTY}: calc(var({TEXT_SIZE_PROPERTY}, {base}) / {base});")
}

fn axis_extent(axis: &str, property: &str, rule: ElementAxis) -> String {
    let bounded = match rule.bound {
        AxisBound::Exact => axis.to_owned(),
        AxisBound::AtLeast => format!("min-{axis}"),
        AxisBound::AtMost => format!("max-{axis}"),
    };
    let fallback = match rule.fallback {
        AxisFallback::Content => "auto".to_owned(),
        AxisFallback::Canvas => "100%".to_owned(),
        AxisFallback::Pixels(value) => format!("{value}px"),
    };
    format!("{bounded}: var({property}, {fallback});")
}

fn accent_border(property: &str, width: f32) -> String {
    format!("{property}: {width}px solid var(--accent);")
}

fn accent_fill() -> String {
    "background-color: var(--accent);".to_owned()
}

fn surface(alpha: f32) -> String {
    let [red, green, blue] = SURFACE_RGB;
    format!("background: rgba({red}, {green}, {blue}, {alpha});")
}
