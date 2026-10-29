use forge_registry::references::GLOBAL_OPTIONS_KEY;
use forge_registry::{
    ConfigProblem, DynamicSelectField, IntegerEditor, IntegerField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionConfig, strip_var_decoration};

pub const NAME_KEY: &str = "name";
const KEY_KEY: &str = "key";
const VARIABLE_NAME_LABEL: &str = "Variable Name";
pub const DEFAULT_AMOUNT: i64 = 1;
pub const AMOUNT_KEY: &str = "amount";

pub(super) const GLOBAL_BY_NAME: DynamicSelectField<String> =
    DynamicSelectField::new(NAME_KEY, VARIABLE_NAME_LABEL, GLOBAL_OPTIONS_KEY);

pub(super) const GLOBAL_BY_KEY: DynamicSelectField<String> =
    DynamicSelectField::new(KEY_KEY, VARIABLE_NAME_LABEL, GLOBAL_OPTIONS_KEY);

pub(super) const AMOUNT: IntegerField = IntegerField {
    key: AMOUNT_KEY,
    label: "Amount",
    editor: IntegerEditor::Number,
    default: DEFAULT_AMOUNT,
    bounds: ValueBounds::UNBOUNDED,
};

pub(super) fn global_name(
    field: &DynamicSelectField<String>,
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<String, ConfigProblem> {
    field
        .read(config, args)
        .map(|name| strip_var_decoration(&name))
}
