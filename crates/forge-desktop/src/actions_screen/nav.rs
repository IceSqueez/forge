use forge_runtime::{CASE_CHAIN_KEY, decode_steps};
use forge_types::{SubActionConfig, SubActionStep, Variant};

pub(super) const UI_MAX_NESTING_DEPTH: usize = 8;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct NavFrame {
    pub step_index: usize,
    pub chain_key: String,
    pub case_index: Option<usize>,
}

pub(super) fn encode_chain(steps: &[SubActionStep]) -> Variant {
    let items = steps
        .iter()
        .map(|step| {
            let mut obj = SubActionConfig::new();
            obj.insert("kind_id".to_owned(), Variant::String(step.kind_id.clone()));
            obj.insert("config".to_owned(), Variant::Object(step.config.clone()));
            obj.insert("enabled".to_owned(), Variant::Bool(step.enabled));
            obj.insert(
                "continue_on_error".to_owned(),
                Variant::Bool(step.continue_on_error),
            );
            if let Some(condition) = &step.condition {
                obj.insert("condition".to_owned(), Variant::String(condition.clone()));
            }
            if let Some(label) = &step.label {
                obj.insert("label".to_owned(), Variant::String(label.clone()));
            }
            Variant::Object(obj)
        })
        .collect();
    Variant::Array(items)
}

pub(super) fn chain_value_at<'a>(
    config: &'a SubActionConfig,
    chain_key: &str,
    case_index: Option<usize>,
) -> Option<&'a Variant> {
    match case_index {
        None => config.get(chain_key),
        Some(ci) => config
            .get(chain_key)
            .and_then(Variant::as_array)
            .and_then(|cases| cases.get(ci))
            .and_then(Variant::as_object)
            .and_then(|case| case.get(CASE_CHAIN_KEY)),
    }
}

fn write_chain_value(
    config: &mut SubActionConfig,
    chain_key: &str,
    case_index: Option<usize>,
    steps: &[SubActionStep],
) {
    match case_index {
        None => {
            config.insert(chain_key.to_owned(), encode_chain(steps));
        }
        Some(ci) => {
            let mut cases = match config.get(chain_key) {
                Some(Variant::Array(items)) => items.clone(),
                _ => Vec::new(),
            };
            if let Some(Variant::Object(case)) = cases.get_mut(ci) {
                case.insert(CASE_CHAIN_KEY.to_owned(), encode_chain(steps));
                config.insert(chain_key.to_owned(), Variant::Array(cases));
            }
        }
    }
}

pub(super) fn resolve_chain(root: &[SubActionStep], path: &[NavFrame]) -> Vec<SubActionStep> {
    let mut current = root.to_vec();
    for frame in path {
        let Some(step) = current.get(frame.step_index) else {
            return Vec::new();
        };
        current = decode_steps(chain_value_at(
            &step.config,
            &frame.chain_key,
            frame.case_index,
        ));
    }
    current
}

pub(super) fn set_chain(
    root: &mut Vec<SubActionStep>,
    path: &[NavFrame],
    new_chain: &[SubActionStep],
) -> bool {
    let Some((frame, rest)) = path.split_first() else {
        *root = new_chain.to_vec();
        return true;
    };
    let Some(step) = root.get_mut(frame.step_index) else {
        return false;
    };
    let mut sub = decode_steps(chain_value_at(
        &step.config,
        &frame.chain_key,
        frame.case_index,
    ));
    if !set_chain(&mut sub, rest, new_chain) {
        return false;
    }
    write_chain_value(&mut step.config, &frame.chain_key, frame.case_index, &sub);
    true
}

pub(super) fn branch_step_count(
    step: &SubActionStep,
    chain_key: &str,
    case_index: Option<usize>,
) -> usize {
    chain_value_at(&step.config, chain_key, case_index)
        .and_then(Variant::as_array)
        .map(<[Variant]>::len)
        .unwrap_or(0)
}

pub(super) fn case_match_display(step: &SubActionStep, case_index: usize) -> Option<String> {
    let case = step
        .config
        .get("cases")
        .and_then(Variant::as_array)
        .and_then(|cases| cases.get(case_index))
        .and_then(Variant::as_object)?;
    match case.get("match") {
        Some(Variant::Array(_)) => None,
        Some(other) => Some(forge_types::display_scalar(other)),
        None => Some(String::new()),
    }
}

pub(super) fn case_match_is_multi(step: &SubActionStep, case_index: usize) -> bool {
    step.config
        .get("cases")
        .and_then(Variant::as_array)
        .and_then(|cases| cases.get(case_index))
        .and_then(Variant::as_object)
        .and_then(|case| case.get("match"))
        .is_some_and(|m| matches!(m, Variant::Array(_)))
}

pub(super) fn case_count(step: &SubActionStep) -> usize {
    step.config
        .get("cases")
        .and_then(Variant::as_array)
        .map(<[Variant]>::len)
        .unwrap_or(0)
}

pub(super) fn append_empty_case(config: &mut SubActionConfig) {
    let mut cases = match config.get("cases") {
        Some(Variant::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let mut case = SubActionConfig::new();
    case.insert("match".to_owned(), Variant::String(String::new()));
    case.insert(CASE_CHAIN_KEY.to_owned(), Variant::Array(Vec::new()));
    cases.push(Variant::Object(case));
    config.insert("cases".to_owned(), Variant::Array(cases));
}

pub(super) fn remove_case(config: &mut SubActionConfig, case_index: usize) {
    if let Some(Variant::Array(items)) = config.get("cases") {
        let mut cases = items.clone();
        if case_index < cases.len() {
            cases.remove(case_index);
            config.insert("cases".to_owned(), Variant::Array(cases));
        }
    }
}

pub(super) fn move_case(config: &mut SubActionConfig, case_index: usize, up: bool) {
    if let Some(Variant::Array(items)) = config.get("cases") {
        let mut cases = items.clone();
        let target = if up {
            case_index.checked_sub(1)
        } else {
            case_index.checked_add(1).filter(|&t| t < cases.len())
        };
        if let Some(t) = target
            && case_index < cases.len()
        {
            cases.swap(case_index, t);
            config.insert("cases".to_owned(), Variant::Array(cases));
        }
    }
}

pub(super) fn set_case_match(config: &mut SubActionConfig, case_index: usize, value: &str) {
    if let Some(Variant::Array(items)) = config.get("cases") {
        let mut cases = items.clone();
        if let Some(Variant::Object(case)) = cases.get_mut(case_index) {
            case.insert("match".to_owned(), Variant::String(value.to_owned()));
            config.insert("cases".to_owned(), Variant::Array(cases));
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{SubActionConfig, SubActionStep, Variant};

    use super::{NavFrame, append_empty_case, resolve_chain, set_chain};

    fn step(kind_id: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config: SubActionConfig::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn switch_with_two_cases() -> SubActionStep {
        let mut switch = step("core.switch");
        append_empty_case(&mut switch.config);
        append_empty_case(&mut switch.config);
        switch
    }

    fn frame(step_index: usize, chain_key: &str, case_index: Option<usize>) -> NavFrame {
        NavFrame {
            step_index,
            chain_key: chain_key.to_owned(),
            case_index,
        }
    }

    fn edited_chain() -> Vec<SubActionStep> {
        vec![
            SubActionStep {
                config: SubActionConfig::from([(
                    "text".to_owned(),
                    Variant::String("hi".to_owned()),
                )]),
                enabled: false,
                continue_on_error: true,
                condition: Some("%user% == \"bob\"".to_owned()),
                label: Some("greet".to_owned()),
                ..step("core.chat_send")
            },
            step("core.delay"),
        ]
    }

    #[test]
    fn a_chain_written_through_a_nested_path_reads_back_with_every_step_field_intact() {
        let mut nested_if = step("core.if");
        nested_if
            .config
            .insert("then".to_owned(), Variant::Array(Vec::new()));
        let mut root = vec![step("core.log"), switch_with_two_cases()];
        assert!(set_chain(
            &mut root,
            &[frame(1, "cases", Some(1))],
            &[nested_if]
        ));
        let path = [frame(1, "cases", Some(1)), frame(0, "then", None)];

        assert!(set_chain(&mut root, &path, &edited_chain()));

        assert_eq!(resolve_chain(&root, &path), edited_chain());
    }

    #[test]
    fn writing_into_one_case_leaves_the_sibling_case_empty() {
        let mut root = vec![switch_with_two_cases()];

        assert!(set_chain(
            &mut root,
            &[frame(0, "cases", Some(1))],
            &edited_chain()
        ));

        assert_eq!(
            resolve_chain(&root, &[frame(0, "cases", Some(0))]),
            Vec::new()
        );
    }

    #[test]
    fn a_path_through_a_step_that_does_not_exist_is_refused_without_touching_the_chain() {
        let mut root = vec![step("core.log")];

        let written = set_chain(&mut root, &[frame(3, "then", None)], &edited_chain());

        assert_eq!((written, root), (false, vec![step("core.log")]));
    }
}
