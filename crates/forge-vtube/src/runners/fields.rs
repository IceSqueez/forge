use forge_registry::{
    ConfigProblem, ConfigReason, IntegerEditor, IntegerField, NumberField, OptionalField,
    StoredNumber, SubActionConfig, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::ArgStack;

use crate::vocabulary::config_keys;

pub(crate) const IMMEDIATE_SECONDS: f64 = 0.0;

pub(crate) const fn optional_number(
    key: &'static str,
    label: &'static str,
    placeholder: &'static str,
) -> OptionalField<NumberField> {
    OptionalField::shared(NumberField {
        key,
        label,
        placeholder,
        default: StoredNumber::Text("0"),
        bounds: ValueBounds::UNBOUNDED,
    })
}

pub(crate) const fn optional_whole_number(
    key: &'static str,
    label: &'static str,
    bounds: ValueBounds,
) -> OptionalField<IntegerField> {
    OptionalField::shared(IntegerField {
        key,
        label,
        editor: IntegerEditor::Number,
        default: 0,
        bounds,
    })
}

pub(crate) const fn optional_text(
    key: &'static str,
    label: &'static str,
    placeholder: &'static str,
) -> OptionalField<TextField> {
    OptionalField::shared(TextField {
        key,
        label,
        editor: TextEditor::Line { placeholder },
        default: "",
        required: false,
    })
}

pub(crate) const X: OptionalField<NumberField> =
    optional_number(config_keys::X, "X Position", "-1.0 to 1.0");

pub(crate) const Y: OptionalField<NumberField> =
    optional_number(config_keys::Y, "Y Position", "-1.0 to 1.0");

pub(crate) const ROTATION: OptionalField<NumberField> =
    optional_number(config_keys::ROTATION, "Rotation", "-360 to 360");

pub(crate) const ITEM_SIZE: OptionalField<NumberField> =
    optional_number(config_keys::SIZE, "Size", "0.0 to 1.0");

pub(crate) const DURATION: OptionalField<NumberField> =
    optional_number(config_keys::DURATION, "Duration (s)", "0.5");

pub(crate) const ITEM_INSTANCE_ID: TextField = TextField {
    key: config_keys::ITEM_INSTANCE_ID,
    label: "Item Instance ID",
    editor: TextEditor::Line {
        placeholder: "item instance id from VTS",
    },
    default: "",
    required: true,
};

pub(crate) const ITEM_FILE_NAME: TextField = TextField {
    key: config_keys::FILE_NAME,
    label: "Item File Name",
    editor: TextEditor::Line {
        placeholder: "my_item.png",
    },
    default: "",
    required: true,
};

pub(crate) fn read_non_empty(
    field: &TextField,
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<String, ConfigProblem> {
    let value = field.read(config, args)?;
    if value.is_empty() {
        Err(ConfigProblem::field(field.key, ConfigReason::Required))
    } else {
        Ok(value)
    }
}
