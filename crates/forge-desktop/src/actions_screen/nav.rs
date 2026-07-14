//! Actions screen — nested sub-chain navigation over the canonical stored form:
//! decode/encode of the `Array`-of-`Object` chain blob, path resolution and
//! rewrite, and switch-case operations, all reading/writing `SubActionConfig`.

use forge_types::{SubActionConfig, SubActionStep, Variant};

/// Authoring depth ceiling for nested sub-chains. Strictly below the runtime's
/// own `max_nesting_depth` so it can never author a chain the runtime would
/// reject; deliberately small to keep breadcrumbs legible. Drilling past this
/// depth into an *empty* branch is disabled, but an already-deeper imported
/// chain stays editable at its existing depth.
pub(super) const UI_MAX_NESTING_DEPTH: usize = 8;

/// One drill-in frame: the parent step's index in the chain descended from, the
/// config key on that step holding the sub-chain, and — for a switch case —
/// which case row's chain was entered.
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

/// Decodes the canonical stored chain form — `Array` of per-step `Object`s —
/// into a step list. A non-array value, or any element lacking a `kind_id`, is
/// dropped so malformed data degrades to an empty chain instead of panicking.
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
            let label = obj
                .get("label")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            Some(SubActionStep {
                kind_id,
                config,
                enabled,
                label,
            })
        })
        .collect()
}

/// Re-encodes a step list into the canonical stored chain form the runtime
/// decodes and storage persists.
pub(super) fn encode_chain(steps: &[SubActionStep]) -> Variant {
    let items = steps
        .iter()
        .map(|step| {
            let mut obj = SubActionConfig::new();
            obj.insert("kind_id".to_owned(), Variant::String(step.kind_id.clone()));
            obj.insert("config".to_owned(), Variant::Object(step.config.clone()));
            obj.insert("enabled".to_owned(), Variant::Bool(step.enabled));
            if let Some(label) = &step.label {
                obj.insert("label".to_owned(), Variant::String(label.clone()));
            }
            Variant::Object(obj)
        })
        .collect();
    Variant::Array(items)
}

fn chain_value_at<'a>(
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

/// Writes a sub-chain value back into `config` for a single frame, preserving
/// any sibling case fields (e.g. `match`). A case index outside the current
/// list is a no-op so stale navigation never corrupts the blob.
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

/// Resolves a navigation path to the step list it currently points at, from the
/// action's top-level steps. An unresolvable frame yields an empty chain.
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

/// Replaces the sub-chain addressed by `path` with `new_chain`, re-serializing
/// up through every parent step's config. Returns `false` if any frame fails to
/// resolve, leaving `root` untouched in that case.
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

/// Number of steps in the sub-chain a drill-in frame would enter, used to gate
/// descending past the depth cap into an empty branch.
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

/// Reads a switch case row's single-value `match` as display text. An array
/// (imported multi-value) returns `None` so the renderer keeps it read-only.
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

/// True when the case row's `match` is an imported multi-value array (kept
/// read-only per the single-value authoring contract).
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
