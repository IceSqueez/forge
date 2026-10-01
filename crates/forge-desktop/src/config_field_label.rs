use forge_components::tr;
use forge_registry::FormField;
use gpui::SharedString;

use crate::config_form::ConfigField;

pub(crate) fn config_field_labels(spec: &FormField, out: &mut Vec<SharedString>) {
    out.push(localized_label(descriptor_label(spec)).into());
    if let FormField::Optional { inner, .. } = spec {
        config_field_labels(inner, out);
    }
}

pub(crate) fn row_label(
    labels: &[SharedString],
    index: usize,
    field: &ConfigField,
) -> SharedString {
    labels
        .get(index)
        .cloned()
        .unwrap_or_else(|| SharedString::from(field.key().to_owned()))
}

fn descriptor_label(spec: &FormField) -> &'static str {
    match spec {
        FormField::Text { label, .. }
        | FormField::TextArea { label, .. }
        | FormField::Code { label, .. }
        | FormField::Integer { label, .. }
        | FormField::Slider { label, .. }
        | FormField::Toggle { label, .. }
        | FormField::FilePicker { label, .. }
        | FormField::DateTime { label, .. }
        | FormField::Select { label, .. }
        | FormField::DynamicSelect { label, .. }
        | FormField::DependentSelect { label, .. }
        | FormField::Swatch { label, .. }
        | FormField::Optional { label, .. }
        | FormField::SubChain { label, .. }
        | FormField::CaseList { label, .. } => label,
    }
}

fn localized_label(english: &'static str) -> String {
    match english {
        "Ban type" => tr!("config_field_label_ban_type"),
        "Case sensitive" => tr!("config_field_label_case_sensitive"),
        "Channel" => tr!("config_field_label_channel"),
        "Channel (leave empty for any)" => tr!("config_field_label_channel_filter"),
        "Command phrase" => tr!("config_field_label_command_phrase"),
        "Controller" => tr!("config_field_label_controller"),
        "Controller (leave empty for any)" => tr!("config_field_label_controller_filter"),
        "Decision status" => tr!("config_field_label_decision_status"),
        "Device" => tr!("config_field_label_device"),
        "Device (leave empty for any)" => tr!("config_field_label_device_filter"),
        "Direction" => tr!("config_field_label_direction"),
        "Direction (leave empty for any)" => tr!("config_field_label_direction_filter"),
        "Event Name" => tr!("config_field_label_event_name"),
        "From user (login)" => tr!("config_field_label_from_user"),
        "Guest state (empty = any)" => tr!("config_field_label_guest_state"),
        "Hotkey combo" => tr!("config_field_label_hotkey_combo"),
        "Interval (minutes)" => tr!("config_field_label_interval_minutes"),
        "Match text" => tr!("config_field_label_match_text"),
        "Maximum bits (-1 = unlimited)" => tr!("config_field_label_max_bits"),
        "Minimum bits" => tr!("config_field_label_min_bits"),
        "Minimum chat messages since last fire" => tr!("config_field_label_min_chat_messages"),
        "Minimum donation (cents)" => tr!("config_field_label_min_donation_cents"),
        "Minimum level (1-5)" => tr!("config_field_label_min_level"),
        "Note" => tr!("config_field_label_note"),
        "Note (leave empty for any)" => tr!("config_field_label_note_filter"),
        "Only while live" => tr!("config_field_label_only_while_live"),
        "Program" => tr!("config_field_label_program"),
        "Program (leave empty for any)" => tr!("config_field_label_program_filter"),
        "Redemption status" => tr!("config_field_label_redemption_status"),
        "Reward ID (leave blank to match any reward)" => tr!("config_field_label_reward_id"),
        "Reward title (used only when Reward ID is blank; leave blank to match any)" => {
            tr!("config_field_label_reward_title")
        }
        "Scene" => tr!("config_field_label_scene"),
        "Scene name (leave empty to match any)" => tr!("config_field_label_scene_filter"),
        "Source name" => tr!("config_field_label_source_name"),
        "Source name (leave empty to match any)" => tr!("config_field_label_source_filter"),
        "Use regex" => tr!("config_field_label_use_regex"),
        other => other.to_owned(),
    }
}
