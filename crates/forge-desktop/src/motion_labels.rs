use forge_components::tr;
use forge_overlay::motion::{ENTRANCE, EXIT, INTENSITY, TEXT_EFFECT, TEXT_UNIT};

use crate::latest_labels::latest_option_label;

pub(crate) fn motion_option_label(key: &str, option: &str) -> Option<String> {
    match key {
        ENTRANCE | EXIT => transition_label(option),
        TEXT_EFFECT => text_effect_label(option),
        TEXT_UNIT => text_unit_label(option),
        INTENSITY => intensity_label(option),
        _ => None,
    }
}

pub(crate) fn preset_label(key: &str, option: &str) -> String {
    motion_option_label(key, option)
        .or_else(|| latest_option_label(key, option))
        .unwrap_or_else(|| option.to_owned())
}

fn transition_label(option: &str) -> Option<String> {
    let label = match option {
        "none" => tr!("overlays_motion_opt_none"),
        "fade" => tr!("overlays_motion_opt_fade"),
        "slide-up" => tr!("overlays_motion_opt_slide_up"),
        "slide-down" => tr!("overlays_motion_opt_slide_down"),
        "slide-left" => tr!("overlays_motion_opt_slide_left"),
        "slide-right" => tr!("overlays_motion_opt_slide_right"),
        "pop" => tr!("overlays_motion_opt_pop"),
        "wipe" => tr!("overlays_motion_opt_wipe"),
        "sparks" => tr!("overlays_motion_opt_sparks"),
        "assemble" => tr!("overlays_motion_opt_assemble"),
        "glow-burst" => tr!("overlays_motion_opt_glow_burst"),
        "dissolve" => tr!("overlays_motion_opt_dissolve"),
        "smoke" => tr!("overlays_motion_opt_smoke"),
        "shatter" => tr!("overlays_motion_opt_shatter"),
        "dust" => tr!("overlays_motion_opt_dust"),
        "burst" => tr!("overlays_motion_opt_burst"),
        _ => return None,
    };
    Some(label)
}

fn text_effect_label(option: &str) -> Option<String> {
    let label = match option {
        "none" => tr!("overlays_motion_opt_none"),
        "typewriter" => tr!("overlays_motion_opt_typewriter"),
        "fly-in" => tr!("overlays_motion_opt_fly_in"),
        "spin" => tr!("overlays_motion_opt_spin"),
        "wave" => tr!("overlays_motion_opt_wave"),
        "bounce" => tr!("overlays_motion_opt_bounce"),
        _ => return None,
    };
    Some(label)
}

fn text_unit_label(option: &str) -> Option<String> {
    let label = match option {
        "letter" => tr!("overlays_motion_opt_letter"),
        "word" => tr!("overlays_motion_opt_word"),
        _ => return None,
    };
    Some(label)
}

fn intensity_label(option: &str) -> Option<String> {
    let label = match option {
        "low" => tr!("overlays_motion_opt_low"),
        "medium" => tr!("overlays_motion_opt_medium"),
        "high" => tr!("overlays_motion_opt_high"),
        _ => return None,
    };
    Some(label)
}
