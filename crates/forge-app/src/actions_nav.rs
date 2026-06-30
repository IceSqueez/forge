use std::sync::Arc;

use forge_types::{Action, SubActionConfig, SubActionStep, Variant};
use iced::Task;

use crate::message::{ActionsMsg, Message};
use crate::runtime_view::RuntimeView;

/// Authoring depth ceiling for nested sub-chains. Strictly below the runtime's
/// own `max_nesting_depth` (32) so it can never author a chain the runtime would
/// reject; deliberately small to keep breadcrumbs legible. Drilling past this
/// depth into an *empty* branch is disabled (no new depth is created), but an
/// already-deeper imported chain stays fully editable at its existing depth.
pub const UI_MAX_NESTING_DEPTH: usize = 8;

/// One drill-in step: the index of the parent step in the chain we descended
/// from, the config key on that step that holds the sub-chain, and — for a
/// switch case — which case row's chain we entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavFrame {
    pub step_index: usize,
    pub chain_key: String,
    pub case_index: Option<usize>,
}

impl NavFrame {
    pub fn new(step_index: usize, chain_key: impl Into<String>, case_index: Option<usize>) -> Self {
        Self {
            step_index,
            chain_key: chain_key.into(),
            case_index,
        }
    }
}

/// Decodes the canonical stored chain form — `Array` of per-step `Object`s — into
/// a step list. Mirrors the runtime decode: a non-array value, or any element
/// lacking a `kind_id`, is dropped so malformed or partially-authored data
/// degrades to an empty chain instead of panicking.
pub fn decode_chain_value(value: Option<&Variant>) -> Vec<SubActionStep> {
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
pub fn encode_chain(steps: &[SubActionStep]) -> Variant {
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

/// Reads the raw stored sub-chain value addressed by a single frame on `config`.
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

/// Writes a sub-chain value back into `config` for a single frame, preserving any
/// sibling case fields (e.g. `match`). A case index outside the current list is a
/// no-op so stale navigation never corrupts the blob.
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

/// Resolves a navigation path to the step list it currently points at, starting
/// from the action's own top-level steps. An unresolvable frame (missing step or
/// malformed blob) yields an empty chain — never a panic.
pub fn resolve_chain(root: &[SubActionStep], path: &[NavFrame]) -> Vec<SubActionStep> {
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

/// Replaces the sub-chain addressed by `path` with `new_chain`, re-serializing up
/// through every parent step's config. Returns `false` if any frame fails to
/// resolve, leaving `root` untouched in that case.
fn set_chain(
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

/// Number of steps in the sub-chain a drill-in frame would enter, read from
/// `step`'s config. Used to decide whether descending past the depth cap is
/// permitted (an empty branch beyond the cap would only deepen nesting).
pub fn branch_step_count(
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
/// (imported multi-value) returns `None` so the renderer keeps it read-only and
/// never clobbers it with a single value.
pub fn case_match_display(step: &SubActionStep, case_index: usize) -> Option<String> {
    let case = step
        .config
        .get("cases")
        .and_then(Variant::as_array)
        .and_then(|cases| cases.get(case_index))
        .and_then(Variant::as_object)?;
    match case.get("match") {
        Some(Variant::Array(_)) => None,
        Some(other) => Some(super::actions_field_form::variant_to_display_str(other)),
        None => Some(String::new()),
    }
}

/// True when the case row's `match` is an imported multi-value array (kept
/// read-only per the single-value authoring contract).
pub fn case_match_is_multi(step: &SubActionStep, case_index: usize) -> bool {
    step.config
        .get("cases")
        .and_then(Variant::as_array)
        .and_then(|cases| cases.get(case_index))
        .and_then(Variant::as_object)
        .and_then(|case| case.get("match"))
        .is_some_and(|m| matches!(m, Variant::Array(_)))
}

/// Number of case rows on a switch step.
pub fn case_count(step: &SubActionStep) -> usize {
    step.config
        .get("cases")
        .and_then(Variant::as_array)
        .map(<[Variant]>::len)
        .unwrap_or(0)
}

/// Appends an empty case row (`{ match: "", chain: [] }`) to a switch step's
/// case list, creating the list if absent.
pub fn append_empty_case(config: &mut SubActionConfig) {
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

/// Removes the case row at `case_index` from a switch step's case list.
pub fn remove_case(config: &mut SubActionConfig, case_index: usize) {
    if let Some(Variant::Array(items)) = config.get("cases") {
        let mut cases = items.clone();
        if case_index < cases.len() {
            cases.remove(case_index);
            config.insert("cases".to_owned(), Variant::Array(cases));
        }
    }
}

/// Swaps a case row with its neighbour above (`up`) or below.
pub fn move_case(config: &mut SubActionConfig, case_index: usize, up: bool) {
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

/// Sets the single-value `match` of a switch case row to `value`.
pub fn set_case_match(config: &mut SubActionConfig, case_index: usize, value: &str) {
    if let Some(Variant::Array(items)) = config.get("cases") {
        let mut cases = items.clone();
        if let Some(Variant::Object(case)) = cases.get_mut(case_index) {
            case.insert("match".to_owned(), Variant::String(value.to_owned()));
            config.insert("cases".to_owned(), Variant::Array(cases));
        }
    }
}

/// Applies `mutate` to the sub-chain addressed by `path` inside a clone of
/// `action`, then persists the whole action and reloads the editor detail. An
/// empty path edits the top-level chain. Returns `Task::none()` if the path no
/// longer resolves, so stale navigation degrades to a no-op rather than a panic.
pub fn persist_chain_mutation(
    rt: &RuntimeView,
    action: &Action,
    path: &[NavFrame],
    mutate: impl FnOnce(&mut Vec<SubActionStep>),
) -> Task<Message> {
    let mut updated = action.clone();
    let mut chain = resolve_chain(&updated.sub_actions, path);
    mutate(&mut chain);
    if !set_chain(&mut updated.sub_actions, path, &chain) {
        return Task::none();
    }
    let action_id = updated.id;
    let dp = Arc::clone(&rt.backend);
    Task::perform(
        async move {
            dp.action_repo()
                .save(&updated)
                .await
                .map_err(|e| e.to_string())
        },
        move |r| match r {
            Ok(()) => Message::Actions(ActionsMsg::BranchReload(action_id)),
            Err(e) => {
                tracing::warn!(error = %e, "nested chain persist failed");
                Message::Noop
            }
        },
    )
}
