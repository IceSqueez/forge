use forge_components::{ActivePresentation, ForgePalette, field_label, scaled, section_label, tr};
use forge_overlay::config::{
    DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX, DESIGN_WIDTH, MARGIN_SIDES, POSITION,
};
use forge_overlay::{ConfigSection, ContentMargins, DesignSize, OverlayConfig};
use gpui::{AnyElement, Context, Pixels, div, prelude::*, px};

use super::base_sections::{hinted, section_heading};
use super::property_panel::{FIELD_GAP, OverlayPropertyPanel, SECTION_GAP, SECTION_TOP_GAP};
use crate::shared::config_form::{ConfigField, InputFlavor, render_config_control};

const PAIR_GAP: Pixels = px(8.0);

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
                    flavor: InputFlavor::Integer(_),
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

    pub(super) fn render_placement_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let density = cx.density();
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
                        render_config_control(
                            field,
                            palette,
                            "overlays-panel",
                            &view,
                            &handlers,
                            density,
                            cx,
                        ),
                    )
                    .tone(palette.text_faint)
                })
        };

        let position =
            labelled(POSITION).map(|field| div().pb(scaled(FIELD_GAP, density)).child(field));
        let width = labelled(DESIGN_WIDTH);
        let height = labelled(DESIGN_HEIGHT);
        let margins: Vec<_> = MARGIN_SIDES
            .iter()
            .filter_map(|key| labelled(key))
            .collect();
        if position.is_none() && width.is_none() && height.is_none() && margins.is_empty() {
            return None;
        }

        let size_row = (width.is_some() || height.is_some()).then(|| {
            let pair = div()
                .w_full()
                .flex()
                .gap(scaled(PAIR_GAP, density))
                .children(width.map(|field| div().flex_1().min_w(px(0.0)).child(field)))
                .children(height.map(|field| div().flex_1().min_w(px(0.0)).child(field)));
            div().pb(scaled(FIELD_GAP, density)).child(hinted(
                pair,
                tr!("overlays_source_size_hint"),
                palette,
                density,
                cx,
            ))
        });

        let last = margins.len().saturating_sub(1);
        let margin_rows = margins.into_iter().enumerate().map(|(index, field)| {
            let row = if index == last {
                hinted(
                    field,
                    tr!("overlays_source_margin_hint"),
                    palette,
                    density,
                    cx,
                )
            } else {
                field.into_any_element()
            };
            div().pb(scaled(FIELD_GAP, density)).child(row)
        });

        Some(
            div()
                .flex()
                .flex_col()
                .pt(SECTION_TOP_GAP)
                .child(div().pb(SECTION_GAP).child(section_label(
                    section_heading(ConfigSection::Placement).to_uppercase(),
                    palette,
                    cx,
                )))
                .children(position)
                .children(size_row)
                .children(margin_rows)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use forge_overlay::config::{
        DESIGN_HEIGHT, DESIGN_WIDTH, DURATION, HEADLINE, MARGIN_BOTTOM, MARGIN_LEFT,
        MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT, MARGIN_RIGHT, MARGIN_TOP,
    };
    use forge_overlay::kinds::chat::ChatOverlayKind;
    use forge_types::Variant;

    use super::*;
    use crate::tools::overlays_screen::look_change::carried_config;

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
    fn opening_a_card_limit_seeds_its_maximum_from_the_matching_side_of_the_source_box() {
        let size = DesignSize {
            width: 1280,
            height: 720,
        };

        for (key, expected) in [
            (forge_overlay::config::CARD_MAX_WIDTH, Some(1280)),
            (forge_overlay::config::CARD_MAX_HEIGHT, Some(720)),
            (forge_overlay::config::CARD_MIN_WIDTH, None),
            (forge_overlay::config::CARD_MIN_HEIGHT, None),
            (DESIGN_WIDTH, None),
            (DURATION, None),
        ] {
            assert_eq!(source_extent_seed(key, size), expected, "{key}");
        }
    }
}
