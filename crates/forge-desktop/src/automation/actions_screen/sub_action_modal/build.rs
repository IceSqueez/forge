use super::*;
use crate::shared::config_field_label::config_field_labels;
use crate::shared::config_form::{ConfigField, FieldCapabilities, FoldContext, fold_config_field};
use crate::shared::option_choices::ChoiceOption;
use forge_components::{ActivePresentation, FONT_SM, InputEvent};
use forge_registry::FormField;
use forge_types::SubActionConfig;

pub(super) struct FieldBuildContext<'a> {
    pub(super) kind_id: &'a str,
    pub(super) output_name_keys: &'a [String],
    pub(super) options_map: &'a HashMap<String, Vec<ChoiceOption>>,
}

fn nothing_commits_before_submit(
    _: &mut EditSubActionForm,
    _: &InputEvent,
    _: &mut Context<EditSubActionForm>,
) {
}

pub(super) fn push_form_fields(
    specs: &[FormField],
    config: &SubActionConfig,
    copy: &FieldBuildContext<'_>,
    fields: &mut Vec<ConfigField>,
    labels: &mut Vec<SharedString>,
    cx: &mut Context<EditSubActionForm>,
) {
    let palette = cx.palette();
    let defaults = SubActionConfig::new();
    let fold = FoldContext {
        kind_id: copy.kind_id,
        config,
        defaults: &defaults,
        palette: &palette,
        capabilities: FieldCapabilities::on_submit(copy.options_map, copy.output_name_keys),
        on_committed: nothing_commits_before_submit,
    };
    for spec in specs {
        config_field_labels(copy.kind_id, spec, labels);
        fold_config_field(spec, None, &fold, fields, cx);
    }
}

pub(super) fn build_step_meta_inputs(
    kind_label: &str,
    name_value: &str,
    condition_value: &str,
    cx: &mut Context<EditSubActionForm>,
) -> (Entity<TextInput>, Entity<TextInput>) {
    let placeholder = kind_label.to_owned();
    let name_value = name_value.to_owned();
    let condition_value = condition_value.to_owned();
    let name_input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx)
            .plain()
            .with_font_size(FONT_SM);
        if !name_value.is_empty() {
            input.set_content(name_value, cx);
        }
        input
    });
    let condition_input = cx.new(|cx| {
        let mut input = TextInput::new("%user.isMod% == true", cx)
            .mono()
            .prefix(tr!("sub_action_condition_prefix"));
        if !condition_value.is_empty() {
            input.set_content(condition_value, cx);
        }
        input
    });
    (name_input, condition_input)
}
