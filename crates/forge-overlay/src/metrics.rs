pub const SURFACE_RGB: [u8; 3] = [20, 20, 28];
pub const BRIGHT_INK_RGB: [u8; 3] = [255, 255, 255];
pub const MESSAGE_INK_RGB: [u8; 3] = [238, 238, 238];
pub const PILL_RADIUS: f32 = 999.0;

pub const ACCENT_PROPERTY: &str = "--accent";
pub const FONT_PROPERTY: &str = "--font";
pub const TEXT_SIZE_PROPERTY: &str = "--text-size";
pub const TEXT_SCALE_PROPERTY: &str = "--text-scale";
pub const MARGIN_TOP_PROPERTY: &str = "--margin-top";
pub const MARGIN_RIGHT_PROPERTY: &str = "--margin-right";
pub const MARGIN_BOTTOM_PROPERTY: &str = "--margin-bottom";
pub const MARGIN_LEFT_PROPERTY: &str = "--margin-left";

const WIDTH_AXIS: &str = "width";
const HEIGHT_AXIS: &str = "height";
const WHOLE_BOX: &str = "100%";
const NO_MARGIN: &str = "0px";
const HEX_DIGIT_BITS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertMetrics {
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
    pub separator_alpha: f32,
    pub track_wash_alpha: f32,
    pub surface_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatestMetrics {
    pub gap: f32,
    pub padding_block: f32,
    pub padding_inline: f32,
    pub border: f32,
    pub radius: f32,
    pub icon_size: f32,
    pub line_gap: f32,
    pub label_size: f32,
    pub headline_size: f32,
    pub subline_size: f32,
    pub subline_margin: f32,
    pub subline_alpha: f32,
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
    separator_alpha: 0.5,
    track_wash_alpha: 0.14,
    surface_alpha: 0.86,
};

pub const LATEST: LatestMetrics = LatestMetrics {
    gap: 14.0,
    padding_block: 14.0,
    padding_inline: 20.0,
    border: 1.0,
    radius: 12.0,
    icon_size: 26.0,
    line_gap: 4.0,
    label_size: 13.0,
    headline_size: 20.0,
    subline_size: 15.0,
    subline_margin: 10.0,
    subline_alpha: 0.7,
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
    AtMost,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementSizing {
    pub width: Option<AxisBound>,
    pub height: Option<AxisBound>,
    pub base_text_size: f32,
}

pub const ALERT_SIZING: ElementSizing = ElementSizing {
    width: Some(AxisBound::AtMost),
    height: Some(AxisBound::AtMost),
    base_text_size: ALERT.headline_size,
};

pub const CHAT_SIZING: ElementSizing = ElementSizing {
    width: Some(AxisBound::Exact),
    height: Some(AxisBound::Exact),
    base_text_size: CHAT.message_size,
};

pub const FRAME_SIZING: ElementSizing = ElementSizing {
    width: None,
    height: None,
    base_text_size: FRAME.headline_size,
};

pub const GOAL_SIZING: ElementSizing = ElementSizing {
    width: Some(AxisBound::Exact),
    height: Some(AxisBound::AtMost),
    base_text_size: GOAL.label_size,
};

pub const LATEST_SIZING: ElementSizing = ElementSizing {
    width: Some(AxisBound::Exact),
    height: Some(AxisBound::AtMost),
    base_text_size: LATEST.headline_size,
};

pub const TICKER_SIZING: ElementSizing = ElementSizing {
    width: Some(AxisBound::Exact),
    height: Some(AxisBound::AtMost),
    base_text_size: TICKER.headline_size,
};

impl ElementSizing {
    pub fn default_text_size_px(self) -> u32 {
        self.base_text_size.round() as u32
    }

    pub fn text_scale(self, text_size_px: u32) -> f32 {
        text_size_px as f32 / self.base_text_size
    }

    pub fn declarations(self) -> Vec<String> {
        let mut out = vec![text_scale(self.base_text_size), content_margins()];
        if let Some(bound) = self.width {
            out.push(axis_extent(WIDTH_AXIS, bound));
        }
        if let Some(bound) = self.height {
            out.push(axis_extent(HEIGHT_AXIS, bound));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleGuard {
    pub kind_id: String,
    pub declarations: Vec<String>,
}

impl StyleGuard {
    pub(crate) fn of(kind_id: &str, mut declarations: Vec<String>, sizing: ElementSizing) -> Self {
        declarations.extend(sizing.declarations());
        Self {
            kind_id: kind_id.to_owned(),
            declarations,
        }
    }
}

impl AlertMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            scaled_length("gap", self.gap),
            scaled_length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            scaled_length("border-radius", self.radius),
            scaled_length("height", self.icon_size),
            accent_fill(),
            opaque_ink(BRIGHT_INK_RGB),
            scaled_length("font-size", self.headline_size),
            scaled_length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

impl ChatMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            scaled_length("gap", self.row_gap),
            scaled_length_pair("padding", self.row_padding_block, self.row_padding_inline),
            scaled_length("border-radius", self.row_radius),
            scaled_length("font-size", self.badges_size),
            scaled_length("font-size", self.author_size),
            scaled_length("font-size", self.message_size),
            opaque_ink(BRIGHT_INK_RGB),
            opaque_ink(MESSAGE_INK_RGB),
            surface(self.surface_alpha),
        ]
    }
}

impl FrameMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            scaled_length("margin", self.inset),
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
            opaque_ink(BRIGHT_INK_RGB),
            scaled_length("font-size", self.headline_size),
            scaled_length("font-size", self.subline_size),
            surface(self.surface_alpha),
        ]
    }
}

impl GoalMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            scaled_length("gap", self.gap),
            scaled_length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            scaled_length("border-radius", self.radius),
            scaled_length("font-size", self.label_size),
            scaled_length("height", self.track_height),
            length("border-radius", self.track_radius),
            scaled_length("font-size", self.figure_size),
            format!("margin: 0 {};", scaled(self.separator_margin)),
            opaque_ink(BRIGHT_INK_RGB),
            translucent_ink("color", self.separator_alpha),
            translucent_ink("background", self.track_wash_alpha),
            surface(self.surface_alpha),
        ]
    }
}

impl LatestMetrics {
    pub fn declarations(&self) -> Vec<String> {
        vec![
            scaled_length("gap", self.gap),
            scaled_length_pair("padding", self.padding_block, self.padding_inline),
            accent_border("border", self.border),
            scaled_length("border-radius", self.radius),
            scaled_length("width", self.icon_size),
            scaled_length("height", self.icon_size),
            scaled_length("gap", self.line_gap),
            scaled_length("font-size", self.label_size),
            scaled_length("font-size", self.headline_size),
            scaled_length("font-size", self.subline_size),
            scaled_length("margin-left", self.subline_margin),
            opaque_ink(BRIGHT_INK_RGB),
            translucent_ink("color", self.subline_alpha),
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
            opaque_ink(BRIGHT_INK_RGB),
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

fn axis_extent(axis: &str, bound: AxisBound) -> String {
    match bound {
        AxisBound::Exact => format!("{axis}: {WHOLE_BOX};"),
        AxisBound::AtMost => format!("max-{axis}: {WHOLE_BOX};"),
    }
}

fn content_margins() -> String {
    let sides = [
        MARGIN_TOP_PROPERTY,
        MARGIN_RIGHT_PROPERTY,
        MARGIN_BOTTOM_PROPERTY,
        MARGIN_LEFT_PROPERTY,
    ]
    .map(|property| format!("var({property}, {NO_MARGIN})"))
    .join(" ");
    format!("padding: {sides};")
}

fn accent_border(property: &str, width: f32) -> String {
    format!("{property}: {width}px solid var({ACCENT_PROPERTY});")
}

fn accent_fill() -> String {
    format!("background-color: var({ACCENT_PROPERTY});")
}

fn opaque_ink([red, green, blue]: [u8; 3]) -> String {
    format!(
        "color: #{:x}{:x}{:x};",
        red >> HEX_DIGIT_BITS,
        green >> HEX_DIGIT_BITS,
        blue >> HEX_DIGIT_BITS
    )
}

fn translucent_ink(property: &str, alpha: f32) -> String {
    let [red, green, blue] = BRIGHT_INK_RGB;
    format!("{property}: rgba({red}, {green}, {blue}, {alpha});")
}

fn surface(alpha: f32) -> String {
    let [red, green, blue] = SURFACE_RGB;
    format!("background: rgba({red}, {green}, {blue}, {alpha});")
}
