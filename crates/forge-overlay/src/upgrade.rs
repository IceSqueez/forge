use forge_registry::FormField;
use forge_types::Variant;

use crate::config::{
    ANIMATION, ANIMATION_OPTIONS, DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX,
    DESIGN_WIDTH, ELEMENT_HEIGHT, ELEMENT_WIDTH, MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT,
    MARGIN_SIDES, MIGRATION_ACKNOWLEDGED, POSITION, POSITION_BOTTOM, POSITION_OPTIONS,
    RETIRED_KEYS,
};
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::kinds::{alert, chat, frame, goal, ticker};
use crate::motion::{
    ENTRANCE, ENTRANCE_MS, EXIT, EXIT_MS, FADE, NO_MOTION, POP, SLIDE_DOWN, SLIDE_LEFT,
    SLIDE_RIGHT, SLIDE_UP, TEXT_EFFECT, WIPE,
};
use crate::source_box::DesignSize;

const PERCENT: i64 = 100;

const ALERT_BODY_PADDING_PX: i64 = 32;
const CHAT_BODY_PADDING_PX: i64 = 24;
const GOAL_BODY_PADDING_PX: i64 = 32;
const NO_BODY_PADDING_PX: i64 = 0;

struct BoxContractStep {
    kind_id: &'static str,
    schema_version: u32,
    retired_body_padding_px: i64,
}

const BOX_CONTRACT_STEPS: &[BoxContractStep] = &[
    BoxContractStep {
        kind_id: alert::KIND_ID,
        schema_version: alert::BOX_CONTRACT_SCHEMA_VERSION,
        retired_body_padding_px: ALERT_BODY_PADDING_PX,
    },
    BoxContractStep {
        kind_id: chat::KIND_ID,
        schema_version: chat::BOX_CONTRACT_SCHEMA_VERSION,
        retired_body_padding_px: CHAT_BODY_PADDING_PX,
    },
    BoxContractStep {
        kind_id: frame::KIND_ID,
        schema_version: frame::BOX_CONTRACT_SCHEMA_VERSION,
        retired_body_padding_px: NO_BODY_PADDING_PX,
    },
    BoxContractStep {
        kind_id: goal::KIND_ID,
        schema_version: goal::BOX_CONTRACT_SCHEMA_VERSION,
        retired_body_padding_px: GOAL_BODY_PADDING_PX,
    },
    BoxContractStep {
        kind_id: ticker::KIND_ID,
        schema_version: ticker::BOX_CONTRACT_SCHEMA_VERSION,
        retired_body_padding_px: NO_BODY_PADDING_PX,
    },
];

const LEGACY_TRANSITION_MS: i64 = 400;
const LEGACY_ROW_ENTRANCE_MS: i64 = 200;

struct LegacyPreset {
    entrance: &'static str,
    row_entrance: &'static str,
    exit: &'static str,
}

const LEGACY_PRESETS: [LegacyPreset; 6] = [
    LegacyPreset {
        entrance: FADE,
        row_entrance: FADE,
        exit: FADE,
    },
    LegacyPreset {
        entrance: SLIDE_UP,
        row_entrance: SLIDE_UP,
        exit: SLIDE_DOWN,
    },
    LegacyPreset {
        entrance: SLIDE_DOWN,
        row_entrance: SLIDE_DOWN,
        exit: SLIDE_UP,
    },
    LegacyPreset {
        entrance: SLIDE_LEFT,
        row_entrance: SLIDE_LEFT,
        exit: SLIDE_RIGHT,
    },
    LegacyPreset {
        entrance: POP,
        row_entrance: POP,
        exit: POP,
    },
    LegacyPreset {
        entrance: WIPE,
        row_entrance: SLIDE_LEFT,
        exit: FADE,
    },
];

struct MotionStep {
    kind_id: &'static str,
    schema_version: u32,
    legacy_entrance_ms: i64,
    rows_enter: bool,
}

const MOTION_STEPS: &[MotionStep] = &[
    MotionStep {
        kind_id: alert::KIND_ID,
        schema_version: alert::MOTION_SCHEMA_VERSION,
        legacy_entrance_ms: LEGACY_TRANSITION_MS,
        rows_enter: false,
    },
    MotionStep {
        kind_id: chat::KIND_ID,
        schema_version: chat::MOTION_SCHEMA_VERSION,
        legacy_entrance_ms: LEGACY_ROW_ENTRANCE_MS,
        rows_enter: true,
    },
    MotionStep {
        kind_id: frame::KIND_ID,
        schema_version: frame::MOTION_SCHEMA_VERSION,
        legacy_entrance_ms: LEGACY_TRANSITION_MS,
        rows_enter: false,
    },
    MotionStep {
        kind_id: goal::KIND_ID,
        schema_version: goal::MOTION_SCHEMA_VERSION,
        legacy_entrance_ms: LEGACY_TRANSITION_MS,
        rows_enter: false,
    },
    MotionStep {
        kind_id: ticker::KIND_ID,
        schema_version: ticker::MOTION_SCHEMA_VERSION,
        legacy_entrance_ms: LEGACY_TRANSITION_MS,
        rows_enter: false,
    },
];

pub fn upgrade_config(
    descriptor: &dyn OverlayKindDescriptor,
    stored_version: u32,
    config: &OverlayConfig,
) -> Option<OverlayConfig> {
    if stored_version >= descriptor.config_schema_version() {
        return None;
    }
    let mut upgraded = config.clone();
    if let Some(step) = MOTION_STEPS
        .iter()
        .find(|step| step.kind_id == descriptor.id() && stored_version < step.schema_version)
    {
        adopt_motion_presets(descriptor, step, &mut upgraded);
    }
    if let Some(step) = BOX_CONTRACT_STEPS
        .iter()
        .find(|step| step.kind_id == descriptor.id() && stored_version < step.schema_version)
    {
        adopt_box_contract(descriptor, step, &mut upgraded);
    }
    Some(upgraded)
}

pub fn sizing_notice_pending(config: &OverlayConfig) -> bool {
    matches!(
        config.get(MIGRATION_ACKNOWLEDGED),
        Some(Variant::Bool(false))
    )
}

pub fn acknowledge_sizing_notice(config: &mut OverlayConfig) {
    if config.contains_key(MIGRATION_ACKNOWLEDGED) {
        config.insert(MIGRATION_ACKNOWLEDGED.to_owned(), Variant::Bool(true));
    }
}

fn adopt_box_contract(
    descriptor: &dyn OverlayKindDescriptor,
    step: &BoxContractStep,
    config: &mut OverlayConfig,
) {
    let fallback = DesignSize::read(config, descriptor.default_design_size());
    let design = DesignSize {
        width: seeded(config, DESIGN_WIDTH, ELEMENT_WIDTH, fallback.width),
        height: seeded(config, DESIGN_HEIGHT, ELEMENT_HEIGHT, fallback.height),
    };
    design.write_into(config);

    if !MARGIN_SIDES.iter().any(|side| config.contains_key(*side)) {
        seed_margins(config, design, step.retired_body_padding_px);
    }

    for key in RETIRED_KEYS {
        config.remove(*key);
    }
    fold_position(descriptor, config);
    config.insert(MIGRATION_ACKNOWLEDGED.to_owned(), Variant::Bool(false));
}

fn adopt_motion_presets(
    descriptor: &dyn OverlayKindDescriptor,
    step: &MotionStep,
    config: &mut OverlayConfig,
) {
    let axes = descriptor.motion().axes;
    let legacy = config
        .remove(ANIMATION)
        .and_then(|value| value.as_str().map(str::to_owned))
        .and_then(|value| legacy_preset(&value));

    if axes.entrance {
        if let Some(preset) = legacy {
            let entrance = if step.rows_enter {
                preset.row_entrance
            } else {
                preset.entrance
            };
            config
                .entry(ENTRANCE.to_owned())
                .or_insert_with(|| Variant::String(entrance.to_owned()));
        }
        config
            .entry(ENTRANCE_MS.to_owned())
            .or_insert(Variant::Int(step.legacy_entrance_ms));
    }
    if axes.text_effect {
        config
            .entry(TEXT_EFFECT.to_owned())
            .or_insert_with(|| Variant::String(NO_MOTION.to_owned()));
    }
    if axes.exit {
        if let Some(preset) = legacy {
            config
                .entry(EXIT.to_owned())
                .or_insert_with(|| Variant::String(preset.exit.to_owned()));
        }
        config
            .entry(EXIT_MS.to_owned())
            .or_insert(Variant::Int(step.legacy_entrance_ms));
    }
}

fn legacy_preset(value: &str) -> Option<&'static LegacyPreset> {
    ANIMATION_OPTIONS
        .iter()
        .zip(LEGACY_PRESETS.iter())
        .find_map(|(legacy, preset)| (*legacy == value).then_some(preset))
}

fn seeded(config: &OverlayConfig, design_key: &str, element_key: &str, fallback: u32) -> u32 {
    if config.contains_key(design_key) {
        return fallback;
    }
    config
        .get(element_key)
        .and_then(Variant::as_int)
        .map(|px| px.clamp(DESIGN_SIZE_MIN_PX, DESIGN_SIZE_MAX_PX))
        .and_then(|px| u32::try_from(px).ok())
        .unwrap_or(fallback)
}

fn seed_margins(config: &mut OverlayConfig, design: DesignSize, padding_px: i64) {
    let vertical = share_of(padding_px, design.height);
    let horizontal = share_of(padding_px, design.width);
    for (side, percent) in MARGIN_SIDES
        .iter()
        .zip([vertical, horizontal, vertical, horizontal])
    {
        config.insert((*side).to_owned(), Variant::Int(percent));
    }
}

fn share_of(padding_px: i64, extent_px: u32) -> i64 {
    if extent_px == 0 {
        return MARGIN_MIN_PERCENT;
    }
    let share = (padding_px * PERCENT) as f64 / f64::from(extent_px);
    (share.round() as i64).clamp(MARGIN_MIN_PERCENT, MARGIN_MAX_PERCENT)
}

fn fold_position(descriptor: &dyn OverlayKindDescriptor, config: &mut OverlayConfig) {
    if !declares(descriptor, POSITION) {
        config.remove(POSITION);
        return;
    }
    let known = config
        .get(POSITION)
        .and_then(Variant::as_str)
        .is_some_and(|value| POSITION_OPTIONS.contains(&value));
    if config.contains_key(POSITION) && !known {
        config.insert(
            POSITION.to_owned(),
            Variant::String(POSITION_BOTTOM.to_owned()),
        );
    }
}

fn declares(descriptor: &dyn OverlayKindDescriptor, key: &str) -> bool {
    descriptor
        .config_fields()
        .iter()
        .any(|sectioned| field_declares(&sectioned.field, key))
}

fn field_declares(field: &FormField, key: &str) -> bool {
    if field.key() == key {
        return true;
    }
    match field {
        FormField::Optional { inner, .. } => field_declares(inner, key),
        _ => false,
    }
}
