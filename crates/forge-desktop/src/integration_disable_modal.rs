use forge_components::ActivePresentation;
use forge_components::Density;
use forge_components::FONT_META;
use forge_components::FONT_ROW;
use forge_components::FONT_SECTION_LABEL;
use forge_components::FONT_XS;
use forge_components::FONT_XXS;
use forge_components::Spacing;
use forge_components::spacing;
use std::ops::Range;

use forge_components::{
    ForgePalette, Icon, ModalSize, OverlayPosition, Radius, body_family, ghost_button, icon, modal,
    mono_family, overlay, radius, scaled, tr,
};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, HighlightStyle, Hsla, Pixels, Rgba, SharedString,
    StyledText, div, prelude::*, px,
};

use crate::integrations_hub::IntegrationsHubView;
use forge_registry::IntegrationReferences;

const BODY_PAD_V: Pixels = px(14.0);
const BODY_PAD_H: Pixels = px(18.0);
const BODY_GAP: Pixels = px(12.0);
const LEAD_SIZE: Pixels = FONT_ROW;
const LEAD_LINE: Pixels = px(19.0);
const LINES_GAP: Pixels = px(7.0);
const LINE_GAP: Pixels = px(8.0);
const LINE_SIZE: Pixels = FONT_XS;
const LINE_ICON: Pixels = px(13.0);
const LINE_ICON_NUDGE: Pixels = px(2.0);
const USED_BY_SIZE: Pixels = FONT_SECTION_LABEL;
const CHIP_SIZE: Pixels = FONT_XXS;
const CHIP_ICON: Pixels = px(10.0);
const CHIP_PAD_V: Pixels = px(2.0);
const CHIP_PAD_H: Pixels = px(7.0);
const SHOWN_CHIPS: usize = 4;
const FOOTER_HINT: Pixels = FONT_META;
const FOOTER_GAP: Pixels = px(8.0);
const CONFIRM_PAD_V: Pixels = px(5.0);
const CONFIRM_PAD_H: Pixels = px(12.0);
const CONFIRM_GAP: Pixels = px(5.0);
const CONFIRM_SIZE: Pixels = FONT_XS;
const CONFIRM_ICON: Pixels = px(12.0);

pub struct DisablePrompt {
    pub name: &'static str,
    pub references: IntegrationReferences,
    pub references_known: bool,
}

struct Emphasis {
    text: String,
    ranges: Vec<Range<usize>>,
}

impl Emphasis {
    fn new() -> Self {
        Self {
            text: String::new(),
            ranges: Vec::new(),
        }
    }

    fn plain(&mut self, part: &str) {
        self.text.push_str(part);
    }

    fn strong(&mut self, part: &str) {
        let start = self.text.len();
        self.text.push_str(part);
        self.ranges.push(start..self.text.len());
    }

    fn styled(self, style: HighlightStyle) -> StyledText {
        let highlights = self
            .ranges
            .into_iter()
            .map(|range| (range, style))
            .collect::<Vec<_>>();
        StyledText::new(SharedString::from(self.text)).with_highlights(highlights)
    }
}

fn lead(prompt: &DisablePrompt, palette: &ForgePalette) -> AnyElement {
    let actions = prompt.references.actions.len();
    let triggers = prompt.references.triggers;
    let mut text = Emphasis::new();
    if prompt.references_known {
        text.plain(&tr!("integration_disable_lead_prefix", name = prompt.name));
        match (actions > 0, triggers > 0) {
            (true, true) => {
                text.strong(&tr!("integration_refs_actions", count = actions as i64));
                text.plain(&tr!("integration_disable_lead_and"));
                text.strong(&tr!("integration_refs_triggers", count = triggers as i64));
            }
            (true, false) => text.strong(&tr!("integration_refs_actions", count = actions as i64)),
            _ => text.strong(&tr!("integration_refs_triggers", count = triggers as i64)),
        }
        text.plain(&tr!("integration_disable_lead_suffix"));
    } else {
        text.plain(&tr!("integration_disable_lead_unknown", name = prompt.name));
    }
    let style = HighlightStyle {
        color: Some(Hsla::from(palette.text_primary)),
        font_weight: Some(FontWeight::MEDIUM),
        ..HighlightStyle::default()
    };
    div()
        .font_family(body_family())
        .text_size(LEAD_SIZE)
        .line_height(LEAD_LINE)
        .text_color(palette.text_secondary)
        .child(text.styled(style))
        .into_any_element()
}

fn consequence(
    glyph: Icon,
    tint: Rgba,
    text: StyledText,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(scaled(LINE_GAP, density))
        .font_family(body_family())
        .text_size(LINE_SIZE)
        .text_color(palette.text_secondary)
        .child(
            div()
                .pt(LINE_ICON_NUDGE)
                .child(icon(glyph, LINE_ICON, tint)),
        )
        .child(div().flex_1().child(text))
        .into_any_element()
}

fn consequences(prompt: &DisablePrompt, palette: &ForgePalette, density: Density) -> AnyElement {
    let mut fails = Emphasis::new();
    fails.plain(&tr!("integration_disable_fails_prefix", name = prompt.name));
    fails.strong(&tr!("integration_disable_reason"));
    fails.plain(&tr!("integration_disable_fails_suffix"));
    let red = HighlightStyle {
        color: Some(Hsla::from(palette.danger)),
        ..HighlightStyle::default()
    };
    let mut lines = div()
        .flex()
        .flex_col()
        .gap(scaled(LINES_GAP, density))
        .child(consequence(
            Icon::AlertCircle,
            palette.danger,
            fails.styled(red),
            palette,
            density,
        ));
    if prompt.references.triggers > 0 {
        lines = lines.child(consequence(
            Icon::TargetArrow,
            palette.warning,
            StyledText::new(SharedString::from(tr!(
                "integration_disable_triggers_inactive"
            ))),
            palette,
            density,
        ));
    }
    lines
        .child(consequence(
            Icon::Key,
            palette.success,
            StyledText::new(SharedString::from(tr!("integration_disable_kept"))),
            palette,
            density,
        ))
        .into_any_element()
}

fn used_by(prompt: &DisablePrompt, palette: &ForgePalette, density: Density) -> Option<AnyElement> {
    let actions = &prompt.references.actions;
    if actions.is_empty() {
        return None;
    }
    let mut chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
    for action in actions.iter().take(SHOWN_CHIPS) {
        chips = chips.child(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .py(scaled(CHIP_PAD_V, density))
                .px(scaled(CHIP_PAD_H, density))
                .rounded(radius(Radius::Sm))
                .bg(palette.shell)
                .font_family(mono_family())
                .text_size(CHIP_SIZE)
                .text_color(palette.text_secondary)
                .child(icon(Icon::Bolt, CHIP_ICON, palette.brand))
                .child(SharedString::from(action.name.clone())),
        );
    }
    let more = actions.len().saturating_sub(SHOWN_CHIPS);
    if more > 0 {
        chips = chips.child(
            div()
                .py(scaled(CHIP_PAD_V, density))
                .px(spacing(Spacing::Xxs, density))
                .font_family(mono_family())
                .text_size(CHIP_SIZE)
                .text_color(palette.text_faint)
                .child(tr!("integration_disable_more", count = more as i64)),
        );
    }
    Some(
        div()
            .child(
                div()
                    .mb(spacing(Spacing::Xs, density))
                    .font_family(mono_family())
                    .text_size(USED_BY_SIZE)
                    .text_color(palette.text_faint)
                    .child(tr!("integration_disable_used_by").to_uppercase()),
            )
            .child(chips)
            .into_any_element(),
    )
}

pub fn render_disable_modal(
    prompt: &DisablePrompt,
    palette: &ForgePalette,
    cx: &mut Context<IntegrationsHubView>,
) -> AnyElement {
    let density = cx.density();
    let body = div()
        .py(scaled(BODY_PAD_V, density))
        .px(scaled(BODY_PAD_H, density))
        .flex()
        .flex_col()
        .gap(scaled(BODY_GAP, density))
        .child(lead(prompt, palette))
        .child(consequences(prompt, palette, density))
        .children(used_by(prompt, palette, density));

    let confirm = div()
        .id("integration-disable-confirm")
        .flex()
        .items_center()
        .gap(scaled(CONFIRM_GAP, density))
        .py(scaled(CONFIRM_PAD_V, density))
        .px(scaled(CONFIRM_PAD_H, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.warning)
        .cursor_pointer()
        .font_family(body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(CONFIRM_SIZE)
        .text_color(palette.shell)
        .child(icon(Icon::PlugOff, CONFIRM_ICON, palette.shell))
        .child(tr!("integration_disable_confirm", name = prompt.name))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disable(cx)));

    let footer = div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(body_family())
                .text_size(FOOTER_HINT)
                .text_color(palette.text_faint)
                .child(tr!("integration_disable_esc_hint")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(scaled(FOOTER_GAP, density))
                .child(ghost_button(tr!("common_cancel"), palette).on_click(
                    "integration-disable-cancel",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disable(cx)),
                ))
                .child(confirm),
        );

    let card = modal(
        tr!("integration_disable_title", name = prompt.name),
        body,
        palette,
    )
    .subtitle(tr!("integration_disable_subtitle"))
    .header_icon(Icon::PlugOff, palette.warning)
    .size(ModalSize::Sm)
    .flush_body()
    .footer(footer)
    .on_close(
        "integration-disable-close",
        cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disable(cx)),
    );

    let weak = cx.entity().downgrade();
    overlay(card, palette)
        .position(OverlayPosition::Center)
        .on_dismiss("integration-disable-dismiss", move |_window, cx| {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = weak.update(cx, |this, cx| this.cancel_disable(cx));
        })
        .into_any_element()
}
