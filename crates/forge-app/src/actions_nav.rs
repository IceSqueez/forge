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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn step(kind: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind.to_owned(),
            config: SubActionConfig::new(),
            enabled: true,
            label: None,
        }
    }

    fn step_with_chain(kind: &str, key: &str, inner: &[SubActionStep]) -> SubActionStep {
        let mut s = step(kind);
        s.config.insert(key.to_owned(), encode_chain(inner));
        s
    }

    fn case_row(match_val: Variant, chain: &[SubActionStep]) -> Variant {
        let mut o = SubActionConfig::new();
        o.insert("match".to_owned(), match_val);
        o.insert("chain".to_owned(), encode_chain(chain));
        Variant::Object(o)
    }

    fn switch_step(cases: Vec<Variant>) -> SubActionStep {
        let mut s = step("core.logic.switch_case");
        s.config.insert("cases".to_owned(), Variant::Array(cases));
        s
    }

    // ---- encode / decode ------------------------------------------------

    #[test]
    fn encode_then_decode_round_trips_labels_enabled_and_nested_config() {
        let mut nested_cfg = SubActionConfig::new();
        nested_cfg.insert(
            "then_chain".to_owned(),
            encode_chain(&[step("core.log.write")]),
        );
        nested_cfg.insert("flag".to_owned(), Variant::Bool(false));
        let original = vec![
            step("core.log.write"),
            SubActionStep {
                kind_id: "core.logic.if_then_else".to_owned(),
                config: nested_cfg,
                enabled: false,
                label: Some("Branch".to_owned()),
            },
        ];

        let decoded = decode_chain_value(Some(&encode_chain(&original)));

        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_chain_produces_canonical_runtime_object_shape() {
        // Why: the runtime's own `decode_steps` reads these exact keys
        // (kind_id/config/enabled/label) off each `Object`. If this wire shape
        // drifts, the UI authors chains the runtime silently drops. `label` is
        // omitted entirely when `None` (not stored as null/empty).
        let labelled = SubActionStep {
            label: Some("L".to_owned()),
            ..step("core.log.write")
        };
        let encoded = encode_chain(&[step("core.log.write"), labelled]);

        let items = encoded.as_array().unwrap();
        let first = items[0].as_object().unwrap();
        assert_eq!(
            first.get("kind_id").and_then(Variant::as_str),
            Some("core.log.write")
        );
        assert!(matches!(first.get("config"), Some(Variant::Object(_))));
        assert!(matches!(first.get("enabled"), Some(Variant::Bool(true))));
        assert!(first.get("label").is_none(), "None label must be omitted");

        let second = items[1].as_object().unwrap();
        assert_eq!(second.get("label").and_then(Variant::as_str), Some("L"));
    }

    #[test]
    fn decode_chain_value_drops_elements_without_kind_id() {
        let valid = step("core.log.write");
        let mut malformed = SubActionConfig::new();
        malformed.insert("enabled".to_owned(), Variant::Bool(true));
        let blob = Variant::Array(vec![
            encode_chain(std::slice::from_ref(&valid))
                .as_array()
                .unwrap()[0]
                .clone(),
            Variant::Object(malformed),
        ]);

        let decoded = decode_chain_value(Some(&blob));

        assert_eq!(decoded, vec![valid]);
    }

    #[test]
    fn decode_chain_value_non_array_or_missing_yields_empty() {
        for value in [
            None,
            Some(&Variant::String("not a chain".to_owned())),
            Some(&Variant::Int(7)),
            Some(&Variant::Object(SubActionConfig::new())),
        ] {
            assert!(decode_chain_value(value).is_empty());
        }
    }

    #[test]
    fn decode_chain_value_applies_defaults_when_optional_fields_absent() {
        let mut obj = SubActionConfig::new();
        obj.insert(
            "kind_id".to_owned(),
            Variant::String("core.log.write".to_owned()),
        );
        let blob = Variant::Array(vec![Variant::Object(obj)]);

        let decoded = decode_chain_value(Some(&blob));

        assert_eq!(decoded.len(), 1);
        assert!(decoded[0].enabled, "absent `enabled` must default to true");
        assert_eq!(decoded[0].label, None);
        assert!(decoded[0].config.is_empty());
    }

    // ---- resolve / set at depth ----------------------------------------

    #[test]
    fn resolve_chain_descends_into_single_nested_chain() {
        let inner = vec![step("core.log.write"), step("core.logic.wait")];
        let root = vec![step_with_chain(
            "core.logic.if_then_else",
            "then_chain",
            &inner,
        )];

        let got = resolve_chain(&root, &[NavFrame::new(0, "then_chain", None)]);

        assert_eq!(got, inner);
    }

    #[test]
    fn resolve_chain_descends_two_levels() {
        let innermost = vec![step("core.log.write")];
        let mid = vec![step_with_chain("core.logic.loop", "body", &innermost)];
        let root = vec![step_with_chain(
            "core.logic.if_then_else",
            "then_chain",
            &mid,
        )];

        let got = resolve_chain(
            &root,
            &[
                NavFrame::new(0, "then_chain", None),
                NavFrame::new(0, "body", None),
            ],
        );

        assert_eq!(got, innermost);
    }

    #[test]
    fn resolve_chain_bad_step_index_yields_empty() {
        let root = vec![step_with_chain(
            "x",
            "then_chain",
            &[step("core.log.write")],
        )];

        let got = resolve_chain(&root, &[NavFrame::new(5, "then_chain", None)]);

        assert!(got.is_empty());
    }

    #[test]
    fn resolve_chain_malformed_blob_yields_empty() {
        let mut s = step("x");
        s.config
            .insert("then_chain".to_owned(), Variant::String("oops".to_owned()));

        let got = resolve_chain(&[s], &[NavFrame::new(0, "then_chain", None)]);

        assert!(got.is_empty());
    }

    #[test]
    fn set_chain_replaces_addressed_chain_preserving_siblings() {
        let mut parent = step_with_chain("core.logic.if_then_else", "then_chain", &[step("old")]);
        parent
            .config
            .insert("else_chain".to_owned(), encode_chain(&[step("keep_else")]));
        let sibling = step("sibling");
        let mut root = vec![parent, sibling.clone()];

        let replacement = vec![step("new_a"), step("new_b")];
        let ok = set_chain(
            &mut root,
            &[NavFrame::new(0, "then_chain", None)],
            &replacement,
        );

        assert!(ok);
        assert_eq!(
            resolve_chain(&root, &[NavFrame::new(0, "then_chain", None)]),
            replacement
        );
        // sibling step untouched, and the parent's other branch is not clobbered.
        assert_eq!(root[1], sibling);
        assert_eq!(
            resolve_chain(&root, &[NavFrame::new(0, "else_chain", None)]),
            vec![step("keep_else")]
        );
    }

    #[test]
    fn set_chain_at_depth_two_reserializes_through_parents() {
        let mid = vec![step_with_chain("core.logic.loop", "body", &[step("old")])];
        let mut root = vec![step_with_chain(
            "core.logic.if_then_else",
            "then_chain",
            &mid,
        )];
        let path = [
            NavFrame::new(0, "then_chain", None),
            NavFrame::new(0, "body", None),
        ];

        let replacement = vec![step("deep_new")];
        let ok = set_chain(&mut root, &path, &replacement);

        assert!(ok);
        assert_eq!(resolve_chain(&root, &path), replacement);
    }

    #[test]
    fn set_chain_unresolvable_frame_returns_false_leaving_root_untouched() {
        let mut root = vec![step_with_chain("x", "then_chain", &[step("old")])];
        let before = root.clone();

        let ok = set_chain(
            &mut root,
            &[NavFrame::new(9, "then_chain", None)],
            &[step("new")],
        );

        assert!(!ok);
        assert_eq!(root, before);
    }

    #[test]
    fn write_chain_value_for_case_preserves_match_field() {
        let mut config = SubActionConfig::new();
        config.insert(
            "cases".to_owned(),
            Variant::Array(vec![case_row(
                Variant::String("keep".to_owned()),
                &[step("old")],
            )]),
        );

        write_chain_value(&mut config, "cases", Some(0), &[step("a"), step("b")]);

        let s = SubActionStep {
            config,
            ..step("core.logic.switch_case")
        };
        assert_eq!(case_match_display(&s, 0), Some("keep".to_owned()));
        assert_eq!(branch_step_count(&s, "cases", Some(0)), 2);
    }

    // ---- switch case ops ------------------------------------------------

    #[test]
    fn append_empty_case_creates_list_when_absent() {
        let mut config = SubActionConfig::new();

        append_empty_case(&mut config);

        let s = switch_step_from(&config);
        assert_eq!(case_count(&s), 1);
        assert_eq!(case_match_display(&s, 0), Some(String::new()));
        assert_eq!(branch_step_count(&s, "cases", Some(0)), 0);
    }

    #[test]
    fn append_empty_case_appends_preserving_existing_rows() {
        let mut config = SubActionConfig::new();
        config.insert(
            "cases".to_owned(),
            Variant::Array(vec![case_row(
                Variant::String("first".to_owned()),
                &[step("x")],
            )]),
        );

        append_empty_case(&mut config);

        let s = switch_step_from(&config);
        assert_eq!(case_count(&s), 2);
        assert_eq!(case_match_display(&s, 0), Some("first".to_owned()));
        assert_eq!(branch_step_count(&s, "cases", Some(0)), 1);
    }

    #[test]
    fn remove_case_removes_addressed_row() {
        let mut config = SubActionConfig::new();
        config.insert(
            "cases".to_owned(),
            Variant::Array(vec![
                case_row(Variant::String("a".to_owned()), &[]),
                case_row(Variant::String("b".to_owned()), &[]),
            ]),
        );

        remove_case(&mut config, 0);

        let s = switch_step_from(&config);
        assert_eq!(case_count(&s), 1);
        assert_eq!(case_match_display(&s, 0), Some("b".to_owned()));
    }

    #[test]
    fn remove_case_out_of_range_is_noop() {
        let mut config = SubActionConfig::new();
        config.insert(
            "cases".to_owned(),
            Variant::Array(vec![case_row(Variant::String("a".to_owned()), &[])]),
        );

        remove_case(&mut config, 9);

        assert_eq!(case_count(&switch_step_from(&config)), 1);
    }

    #[test]
    fn move_case_swaps_with_neighbour() {
        for (from, up, expected_top) in [(1usize, true, "b"), (0usize, false, "b")] {
            let mut config = SubActionConfig::new();
            config.insert(
                "cases".to_owned(),
                Variant::Array(vec![
                    case_row(Variant::String("a".to_owned()), &[]),
                    case_row(Variant::String("b".to_owned()), &[]),
                ]),
            );

            move_case(&mut config, from, up);

            let s = switch_step_from(&config);
            assert_eq!(case_match_display(&s, 0).as_deref(), Some(expected_top));
        }
    }

    #[test]
    fn move_case_at_boundary_is_noop() {
        // up at top (checked_sub underflow) and down at bottom (filtered) both no-op.
        for (from, up) in [(0usize, true), (1usize, false)] {
            let mut config = SubActionConfig::new();
            config.insert(
                "cases".to_owned(),
                Variant::Array(vec![
                    case_row(Variant::String("a".to_owned()), &[]),
                    case_row(Variant::String("b".to_owned()), &[]),
                ]),
            );

            move_case(&mut config, from, up);

            let s = switch_step_from(&config);
            assert_eq!(case_match_display(&s, 0).as_deref(), Some("a"));
            assert_eq!(case_match_display(&s, 1).as_deref(), Some("b"));
        }
    }

    #[test]
    fn set_case_match_writes_single_value() {
        let mut config = SubActionConfig::new();
        config.insert(
            "cases".to_owned(),
            Variant::Array(vec![case_row(Variant::String(String::new()), &[])]),
        );

        set_case_match(&mut config, 0, "matched");

        assert_eq!(
            case_match_display(&switch_step_from(&config), 0),
            Some("matched".to_owned())
        );
    }

    // ---- single-value match contract (OQ-2) ----------------------------

    #[test]
    fn case_match_display_returns_single_value_and_empty_when_absent() {
        let with_value = switch_step(vec![case_row(Variant::String("v".to_owned()), &[])]);
        assert_eq!(case_match_display(&with_value, 0), Some("v".to_owned()));

        let mut no_match = SubActionConfig::new();
        no_match.insert("chain".to_owned(), encode_chain(&[]));
        let absent = switch_step(vec![Variant::Object(no_match)]);
        assert_eq!(case_match_display(&absent, 0), Some(String::new()));
    }

    #[test]
    fn case_match_display_returns_none_for_imported_multi_value_array() {
        let multi = switch_step(vec![case_row(
            Variant::Array(vec![
                Variant::String("a".to_owned()),
                Variant::String("b".to_owned()),
            ]),
            &[],
        )]);

        assert_eq!(case_match_display(&multi, 0), None);
    }

    #[test]
    fn case_match_is_multi_true_only_for_array() {
        let single = switch_step(vec![case_row(Variant::String("v".to_owned()), &[])]);
        let numeric = switch_step(vec![case_row(Variant::Int(3), &[])]);
        let multi = switch_step(vec![case_row(Variant::Array(vec![Variant::Int(1)]), &[])]);

        assert!(!case_match_is_multi(&single, 0));
        assert!(!case_match_is_multi(&numeric, 0));
        assert!(case_match_is_multi(&multi, 0));
    }

    #[test]
    fn case_match_helpers_are_safe_for_out_of_range_index() {
        let s = switch_step(vec![case_row(Variant::String("v".to_owned()), &[])]);

        assert_eq!(case_match_display(&s, 9), None);
        assert!(!case_match_is_multi(&s, 9));
    }

    // ---- depth cap (OQ-1) ----------------------------------------------

    #[test]
    fn branch_step_count_reports_existing_branch_length() {
        let empty = step_with_chain("x", "body", &[]);
        let filled = step_with_chain("x", "body", &[step("a"), step("b")]);
        let absent = step("x");

        assert_eq!(branch_step_count(&empty, "body", None), 0);
        assert_eq!(branch_step_count(&filled, "body", None), 2);
        assert_eq!(branch_step_count(&absent, "body", None), 0);

        let cased = switch_step(vec![case_row(
            Variant::String("m".to_owned()),
            &[step("a")],
        )]);
        assert_eq!(branch_step_count(&cased, "cases", Some(0)), 1);
    }

    #[test]
    fn ui_nesting_cap_stays_strictly_below_runtime_max() {
        // Why: the authoring cap MUST be under the runtime's `max_nesting_depth`
        // so the UI can never persist a chain the runtime would reject at execute.
        assert!(UI_MAX_NESTING_DEPTH < forge_runtime::Config::default().max_nesting_depth as usize);
    }

    fn switch_step_from(config: &SubActionConfig) -> SubActionStep {
        SubActionStep {
            config: config.clone(),
            ..step("core.logic.switch_case")
        }
    }
}
