use forge_registry::{ConfigProblem, TextEditor, TextField, TypedField, VariableField};
use forge_types::{ArgStack, SubActionConfig};

pub(super) const INTO_VAR_KEY: &str = "into_var";

const STRING_RESULT_VARIABLE: &str = "string.result";

pub(super) const INTO_RESULT: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: STRING_RESULT_VARIABLE,
    default: STRING_RESULT_VARIABLE,
};

pub(super) const fn source_field(placeholder: &'static str) -> TextField {
    TextField {
        key: "source",
        label: "Source",
        editor: TextEditor::Line { placeholder },
        default: "",
        required: false,
    }
}

pub(super) fn read_source_into(
    source: &TextField,
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((source.read(config, args)?, INTO_RESULT.read(config, args)?))
}
