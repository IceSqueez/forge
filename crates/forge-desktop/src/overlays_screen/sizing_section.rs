use forge_components::{
    BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, body_family, field_label,
    ghost_button_with_icon, icon, section_label, tr,
};
use forge_overlay::config::{
    DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX, DESIGN_WIDTH, MARGIN_SIDES,
    MIGRATION_ACKNOWLEDGED,
};
use forge_overlay::{
    ContentMargins, DesignSize, OverlayConfig, acknowledge_sizing_notice, sizing_notice_pending,
};
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Pixels, div, prelude::*, px};

use super::base_sections::{PanelSection, hinted};
use super::property_panel::{
    FIELD_GAP, NOTICE_LINE_H, NOTICE_PAD, NOTICE_RADIUS, OverlayPropertyPanel, PropertyPanelEvent,
    SECTION_GAP, SECTION_TOP_GAP,
};
use crate::config_form::{ConfigField, render_config_control};

const PAIR_GAP: Pixels = px(8.0);
const NOTICE_GAP: Pixels = px(8.0);
const NOTICE_GLYPH: Pixels = px(12.0);
const NOTICE_ACTION_TOP: Pixels = px(6.0);

pub(super) fn is_source_box_key(key: &str) -> bool {
    key == DESIGN_WIDTH || key == DESIGN_HEIGHT || MARGIN_SIDES.contains(&key)
}

pub(super) fn bounded_design_px(typed: &str, current: i64) -> i64 {
    typed
        .trim()
        .parse::<i64>()
        .unwrap_or(current)
        .clamp(DESIGN_SIZE_MIN_PX, DESIGN_SIZE_MAX_PX)
}

pub(super) fn with_source_box(defaults: &OverlayConfig, sparse: OverlayConfig) -> OverlayConfig {
    let mut effective = defaults.clone();
    effective.extend(
        sparse
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    let mut carried = sparse;
    DesignSize::read(&effective, DesignSize::BROWSER_SOURCE_DEFAULT).write_into(&mut carried);
    ContentMargins::read(&effective, ContentMargins::NONE).write_into(&mut carried);
    carried
}

pub(super) fn keep_sizing_notice(from: &OverlayConfig, into: &mut OverlayConfig) {
    if let Some(flag) = from.get(MIGRATION_ACKNOWLEDGED) {
        into.insert(MIGRATION_ACKNOWLEDGED.to_owned(), flag.clone());
    }
}

impl OverlayPropertyPanel {
    fn stored_design_size(&self) -> DesignSize {
        let fallback = DesignSize::read(&self.defaults, DesignSize::BROWSER_SOURCE_DEFAULT);
        DesignSize::read(&self.stored, fallback)
    }

    pub(super) fn bound_design_inputs(&self, cx: &mut Context<Self>) {
        let current = self.stored_design_size();
        let corrections: Vec<_> = self
            .fields
            .iter()
            .filter_map(|field| {
                let ConfigField::Input {
                    key,
                    integer: true,
                    input,
                    ..
                } = field
                else {
                    return None;
                };
                let held = match key.as_str() {
                    DESIGN_WIDTH => current.width,
                    DESIGN_HEIGHT => current.height,
                    _ => return None,
                };
                let typed = input.read(cx).content().to_owned();
                let settled = bounded_design_px(&typed, i64::from(held)).to_string();
                (typed.trim() != settled).then(|| (input.clone(), settled))
            })
            .collect();
        for (input, settled) in corrections {
            input.update(cx, |input, cx| input.set_content(settled, cx));
        }
    }

    fn acknowledge_sizing(&mut self, cx: &mut Context<Self>) {
        let mut next = self.pending_config(cx);
        acknowledge_sizing_notice(&mut next);
        if next == self.stored {
            return;
        }
        self.stored = next.clone();
        cx.emit(PropertyPanelEvent::Save(next));
        cx.notify();
    }

    pub(super) fn render_sizing_notice(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !sizing_notice_pending(&self.stored) {
            return None;
        }
        let size = self.stored_design_size();
        let width = size.width.to_string();
        let height = size.height.to_string();

        Some(
            div()
                .mb(SECTION_TOP_GAP)
                .flex()
                .items_start()
                .gap(NOTICE_GAP)
                .p(NOTICE_PAD)
                .rounded(NOTICE_RADIUS)
                .border(BORDER_THIN)
                .border_color(palette.warning)
                .bg(palette.base)
                .child(icon(Icon::AlertTriangle, NOTICE_GLYPH, palette.warning))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .font_family(body_family())
                        .line_height(NOTICE_LINE_H)
                        .child(
                            div()
                                .text_size(FONT_XS)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette.text_secondary)
                                .child(tr!("overlays_sizing_notice_title")),
                        )
                        .child(
                            div()
                                .text_size(FONT_XXS)
                                .text_color(palette.text_muted)
                                .child(tr!("overlays_sizing_notice_body")),
                        )
                        .child(
                            div()
                                .pt(NOTICE_ACTION_TOP)
                                .text_size(FONT_XXS)
                                .text_color(palette.text_secondary)
                                .child(tr!(
                                    "overlays_sizing_notice_size",
                                    width = width.as_str(),
                                    height = height.as_str()
                                )),
                        )
                        .child(
                            div().pt(NOTICE_ACTION_TOP).child(
                                ghost_button_with_icon(
                                    Icon::Check,
                                    tr!("overlays_sizing_notice_ack"),
                                    palette,
                                )
                                .on_click(
                                    "overlays-sizing-ack",
                                    cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.acknowledge_sizing(cx);
                                    }),
                                ),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_source_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = cx.entity();
        let handlers = self.handlers();
        let labelled = |key: &str| {
            self.fields
                .iter()
                .find(|field| field.key() == key)
                .map(|field| {
                    field_label(
                        palette,
                        self.label_of(key).to_uppercase(),
                        render_config_control(field, palette, "overlays-panel", &view, &handlers),
                    )
                    .tone(palette.text_faint)
                })
        };

        let width = labelled(DESIGN_WIDTH);
        let height = labelled(DESIGN_HEIGHT);
        let margins: Vec<_> = MARGIN_SIDES
            .iter()
            .filter_map(|key| labelled(key))
            .collect();
        if width.is_none() && height.is_none() && margins.is_empty() {
            return None;
        }

        let size_row = (width.is_some() || height.is_some()).then(|| {
            let pair = div()
                .w_full()
                .flex()
                .gap(PAIR_GAP)
                .children(width.map(|field| div().flex_1().min_w(px(0.0)).child(field)))
                .children(height.map(|field| div().flex_1().min_w(px(0.0)).child(field)));
            div()
                .pb(FIELD_GAP)
                .child(hinted(pair, tr!("overlays_source_size_hint"), palette))
        });

        let last = margins.len().saturating_sub(1);
        let margin_rows = margins.into_iter().enumerate().map(|(index, field)| {
            let row = if index == last {
                hinted(field, tr!("overlays_source_margin_hint"), palette)
            } else {
                field.into_any_element()
            };
            div().pb(FIELD_GAP).child(row)
        });

        Some(
            div()
                .flex()
                .flex_col()
                .pt(SECTION_TOP_GAP)
                .child(div().pb(SECTION_GAP).child(section_label(
                    PanelSection::Source.heading().to_uppercase(),
                    palette,
                )))
                .children(size_row)
                .children(margin_rows)
                .into_any_element(),
        )
    }
}
