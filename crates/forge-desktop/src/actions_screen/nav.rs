use forge_types::{SubActionConfig, SubActionStep, Variant};

/// Must stay strictly below the runtime's `max_nesting_depth` so the UI can never author a chain the runtime rejects.
pub(super) const UI_MAX_NESTING_DEPTH: usize = 8;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct NavFrame {
    pub step_index: usize,
    pub chain_key: String,
    pub case_index: Option<usize>,
}

pub(super) fn variant_to_display_str(v: &Variant) -> String {
    match v {
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Bool(b) => b.to_string(),
        Variant::String(s) => s.clone(),
        Variant::Datetime(dt) => dt.to_string(),
        Variant::Array(_) | Variant::Object(_) => String::new(),
    }
}

pub(super) fn decode_chain_value(value: Option<&Variant>) -> Vec<SubActionStep> {
    let Some(steps) = value.and_then(Variant::as_array) else {
        return Vec::new();
    };
    steps
        .iter()
        .filter_map(|step| {
            let obj = step.as_object()?;
            let kind_id = obj.get("kind_id").and_then(Variant::as_str)?.to_owned();
            let config = match obj.get("config") {
                Some(Variant::Object(map)) => map.clone(),
                _ => SubActionConfig::new(),
            };
            let enabled = obj
                .get("enabled")
                .and_then(Variant::as_bool)
                .unwrap_or(true);
            let continue_on_error = obj
                .get("continue_on_error")
                .and_then(Variant::as_bool)
                .unwrap_or(false);
            let condition = obj
                .get("condition")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            let label = obj
                .get("label")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            Some(SubActionStep {
                kind_id,
                config,
                enabled,
                continue_on_error,
                condition,
                label,
            })
        })
        .collect()
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
            .and_then(|case| case.get("chain")),
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
                case.insert("chain".to_owned(), encode_chain(steps));
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
        current = decode_chain_value(chain_value_at(
            &step.config,
            &frame.chain_key,
            frame.case_index,
        ));
    }
    current
}

/// Returns `false` (leaving `root` untouched) if any frame fails to resolve.
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
    let mut sub = decode_chain_value(chain_value_at(
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

/// `None` when the case `match` is a multi-value array (kept read-only).
pub(super) fn case_match_display(step: &SubActionStep, case_index: usize) -> Option<String> {
    let case = step
        .config
        .get("cases")
        .and_then(Variant::as_array)
        .and_then(|cases| cases.get(case_index))
        .and_then(Variant::as_object)?;
    match case.get("match") {
        Some(Variant::Array(_)) => None,
        Some(other) => Some(variant_to_display_str(other)),
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
    case.insert("chain".to_owned(), Variant::Array(Vec::new()));
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
