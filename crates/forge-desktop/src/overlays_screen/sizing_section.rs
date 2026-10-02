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

#[cfg(test)]
mod tests {
    use forge_overlay::ConfigSection;
    use forge_overlay::config::{
        DESIGN_HEIGHT, DESIGN_WIDTH, DURATION, HEADLINE, MARGIN_BOTTOM, MARGIN_LEFT,
        MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT, MARGIN_RIGHT, MARGIN_TOP, SOUND,
    };
    use forge_overlay::kinds::chat::ChatOverlayKind;
    use forge_types::Variant;

    use super::*;
    use crate::overlays_screen::look_change::carried_config;

    const HELD: i64 = 1280;

    fn config(entries: &[(&str, i64)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::Int(*value)))
            .collect()
    }

    fn source_box_of(carried: &OverlayConfig) -> (DesignSize, ContentMargins) {
        (
            DesignSize::read(
                carried,
                DesignSize {
                    width: 0,
                    height: 0,
                },
            ),
            ContentMargins::read(carried, ContentMargins::even(u32::MAX)),
        )
    }

    #[test]
    fn a_typed_design_size_settles_inside_the_bounds_the_page_accepts() {
        for (typed, expected) in [
            ("1920", 1920),
            (" 1080 ", 1080),
            ("99999", DESIGN_SIZE_MAX_PX),
            ("7681", DESIGN_SIZE_MAX_PX),
            ("7680", DESIGN_SIZE_MAX_PX),
            ("40", DESIGN_SIZE_MIN_PX),
            ("39", DESIGN_SIZE_MIN_PX),
            ("0", DESIGN_SIZE_MIN_PX),
            ("-640", DESIGN_SIZE_MIN_PX),
        ] {
            assert_eq!(bounded_design_px(typed, HELD), expected, "typed {typed:?}");
        }
    }

    #[test]
    fn a_design_size_that_is_not_a_number_falls_back_to_the_size_already_held() {
        for typed in [
            "",
            "   ",
            "wide",
            "12.5",
            "1920px",
            "1 920",
            "99999999999999999999",
        ] {
            assert_eq!(bounded_design_px(typed, HELD), HELD, "typed {typed:?}");
        }
    }

    #[test]
    fn a_held_size_outside_the_bounds_is_pulled_back_when_the_typed_text_is_unusable() {
        assert_eq!(bounded_design_px("", 99_999), DESIGN_SIZE_MAX_PX);
    }

    #[test]
    fn a_look_change_from_untouched_sizing_carries_the_old_looks_default_box() {
        let defaults = config(&[
            (DESIGN_WIDTH, 1920),
            (DESIGN_HEIGHT, 120),
            (MARGIN_TOP, 10),
            (MARGIN_RIGHT, 5),
            (MARGIN_BOTTOM, 10),
            (MARGIN_LEFT, 5),
        ]);

        let carried = with_source_box(&defaults, OverlayConfig::new());

        assert_eq!(
            source_box_of(&carried),
            (
                DesignSize {
                    width: 1920,
                    height: 120
                },
                ContentMargins {
                    top: 10,
                    right: 5,
                    bottom: 10,
                    left: 5
                }
            ),
            "the new look must inherit the box the old look was showing, not its own default"
        );
    }

    #[test]
    fn a_look_change_carries_the_box_the_user_set_over_the_old_looks_default() {
        let defaults = config(&[(DESIGN_WIDTH, 800), (DESIGN_HEIGHT, 600), (MARGIN_TOP, 0)]);
        let sparse = config(&[(DESIGN_WIDTH, 1280), (MARGIN_TOP, 30)]);

        let carried = with_source_box(&defaults, sparse);

        assert_eq!(
            source_box_of(&carried),
            (
                DesignSize {
                    width: 1280,
                    height: 600
                },
                ContentMargins {
                    top: 30,
                    right: 0,
                    bottom: 0,
                    left: 0
                }
            )
        );
    }

    #[test]
    fn a_look_change_without_any_declared_box_carries_the_browser_source_default_and_no_margins() {
        let carried = with_source_box(&OverlayConfig::new(), OverlayConfig::new());

        assert_eq!(
            source_box_of(&carried),
            (DesignSize::BROWSER_SOURCE_DEFAULT, ContentMargins::NONE)
        );
    }

    #[test]
    fn a_look_change_carries_an_out_of_bounds_box_at_its_bounds() {
        let sparse = config(&[
            (DESIGN_WIDTH, 99_999),
            (DESIGN_HEIGHT, 1),
            (MARGIN_TOP, 99),
            (MARGIN_LEFT, -5),
        ]);

        let carried = with_source_box(&OverlayConfig::new(), sparse);

        assert_eq!(
            [DESIGN_WIDTH, DESIGN_HEIGHT, MARGIN_TOP, MARGIN_LEFT]
                .map(|key| carried.get(key).and_then(Variant::as_int)),
            [
                Some(DESIGN_SIZE_MAX_PX),
                Some(DESIGN_SIZE_MIN_PX),
                Some(MARGIN_MAX_PERCENT),
                Some(MARGIN_MIN_PERCENT)
            ]
        );
    }

    #[test]
    fn a_look_change_keeps_every_other_carried_value_untouched() {
        let mut sparse = config(&[(DESIGN_WIDTH, 1280)]);
        sparse.insert(HEADLINE.to_owned(), Variant::String("hello".into()));

        let carried = with_source_box(&OverlayConfig::new(), sparse);

        assert_eq!(
            carried.get(HEADLINE),
            Some(&Variant::String("hello".into()))
        );
    }

    #[test]
    fn the_carried_box_survives_the_new_looks_key_filter() {
        let sparse = config(&[(DESIGN_WIDTH, 1280), (MARGIN_BOTTOM, 20)]);
        let carried = with_source_box(&OverlayConfig::new(), sparse);

        let landed = carried_config(&ChatOverlayKind, &carried);

        assert_eq!(
            source_box_of(&landed),
            (
                DesignSize {
                    width: 1280,
                    height: DesignSize::BROWSER_SOURCE_DEFAULT.height
                },
                ContentMargins {
                    top: 0,
                    right: 0,
                    bottom: 20,
                    left: 0
                }
            ),
            "the target look dropped the box the panel carried into the change"
        );
    }

    #[test]
    fn a_pending_sizing_notice_survives_the_look_change_that_filters_undeclared_keys() {
        let mut from = config(&[(DESIGN_WIDTH, 1280)]);
        from.insert(MIGRATION_ACKNOWLEDGED.to_owned(), Variant::Bool(false));
        let mut into = carried_config(&ChatOverlayKind, &from);

        keep_sizing_notice(&from, &mut into);

        assert_eq!(
            into.get(MIGRATION_ACKNOWLEDGED),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn a_look_change_never_invents_a_sizing_notice_the_overlay_did_not_carry() {
        let from = config(&[(DESIGN_WIDTH, 1280)]);
        let mut into = carried_config(&ChatOverlayKind, &from);

        keep_sizing_notice(&from, &mut into);

        assert!(!into.contains_key(MIGRATION_ACKNOWLEDGED));
    }

    #[test]
    fn source_box_keys_move_to_the_source_section_whatever_section_the_look_declares() {
        for (key, declared, expected) in [
            (DESIGN_WIDTH, ConfigSection::Style, PanelSection::Source),
            (DESIGN_HEIGHT, ConfigSection::Content, PanelSection::Source),
            (MARGIN_TOP, ConfigSection::Style, PanelSection::Source),
            (MARGIN_RIGHT, ConfigSection::Behavior, PanelSection::Source),
            (MARGIN_BOTTOM, ConfigSection::Style, PanelSection::Source),
            (MARGIN_LEFT, ConfigSection::Style, PanelSection::Source),
            (HEADLINE, ConfigSection::Style, PanelSection::Style),
            (SOUND, ConfigSection::Behavior, PanelSection::Audio),
            (DURATION, ConfigSection::Behavior, PanelSection::Display),
        ] {
            assert_eq!(
                PanelSection::of(key, declared),
                expected,
                "{key} declared in {declared:?}"
            );
        }
    }
}
