use forge_storage::{SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND};
use forge_types::{SubActionConfig, SubActionStep, Variant, variable_references};

const NESTED_STEP_KIND_KEY: &str = "kind_id";
const NESTED_STEP_CONFIG_KEY: &str = "config";

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ScriptNameUses {
    pub literal: bool,
    pub templated: bool,
}

pub(crate) struct ScriptNameEdit<'a> {
    pub old_name: &'a str,
    pub new_name: Option<&'a str>,
}

pub(crate) fn apply_script_name_edit(
    steps: &mut [SubActionStep],
    edit: &ScriptNameEdit<'_>,
) -> ScriptNameUses {
    let mut uses = ScriptNameUses::default();
    for step in steps {
        visit_step(&step.kind_id, &mut step.config, edit, &mut uses);
    }
    uses
}

fn visit_step(
    kind_id: &str,
    config: &mut SubActionConfig,
    edit: &ScriptNameEdit<'_>,
    uses: &mut ScriptNameUses,
) {
    if kind_id == SCRIPT_RUN_NAMED_KIND
        && let Some(Variant::String(stored)) = config.get_mut(SCRIPT_NAME_KEY)
    {
        if variable_references(stored).next().is_some() {
            uses.templated = true;
        } else if stored == edit.old_name {
            uses.literal = true;
            if let Some(new_name) = edit.new_name {
                new_name.clone_into(stored);
            }
        }
    }
    for value in config.values_mut() {
        visit_value(value, edit, uses);
    }
}

fn visit_value(value: &mut Variant, edit: &ScriptNameEdit<'_>, uses: &mut ScriptNameUses) {
    match value {
        Variant::Array(items) => {
            for item in items {
                visit_value(item, edit, uses);
            }
        }
        Variant::Object(map) => {
            let nested_kind = map
                .get(NESTED_STEP_KIND_KEY)
                .and_then(Variant::as_str)
                .map(str::to_owned);
            match (nested_kind, map.get_mut(NESTED_STEP_CONFIG_KEY)) {
                (Some(kind_id), Some(Variant::Object(config))) => {
                    visit_step(&kind_id, config, edit, uses);
                }
                _ => {
                    for nested in map.values_mut() {
                        visit_value(nested, edit, uses);
                    }
                }
            }
        }
        Variant::Int(_)
        | Variant::Float(_)
        | Variant::Bool(_)
        | Variant::String(_)
        | Variant::Datetime(_) => {}
    }
}
