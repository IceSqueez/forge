use forge_registry::{
    CASE_CHAIN_KEY, CASE_MATCH_KEY, ChainEntry, FormField, SubActionRegistry, UndecodableStep,
    decode_chain, decode_step, encode_step, encode_step_onto,
};
use forge_types::{SubActionConfig, SubActionStep, Variant};

pub(super) const UI_MAX_NESTING_DEPTH: usize = 8;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct NavFrame {
    pub step_index: usize,
    pub chain_key: String,
    pub case_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ChainItem {
    Step {
        step: SubActionStep,
        stored: Option<Variant>,
    },
    Unreadable(UndecodableStep),
}

impl ChainItem {
    pub(super) fn fresh(step: SubActionStep) -> Self {
        Self::Step { step, stored: None }
    }

    pub(super) fn step(&self) -> Option<&SubActionStep> {
        match self {
            Self::Step { step, .. } => Some(step),
            Self::Unreadable(_) => None,
        }
    }

    pub(super) fn step_mut(&mut self) -> Option<&mut SubActionStep> {
        match self {
            Self::Step { step, .. } => Some(step),
            Self::Unreadable(_) => None,
        }
    }

    fn encode(&self) -> Variant {
        match self {
            Self::Step {
                step,
                stored: Some(stored),
            } => encode_step_onto(step, stored),
            Self::Step { step, stored: None } => encode_step(step),
            Self::Unreadable(unreadable) => unreadable.raw.clone(),
        }
    }
}

pub(super) fn chain_steps(items: &[ChainItem]) -> Vec<SubActionStep> {
    items.iter().filter_map(ChainItem::step).cloned().collect()
}

pub(super) fn steps_before(items: &[ChainItem], index: usize) -> usize {
    items
        .iter()
        .take(index)
        .filter(|item| item.step().is_some())
        .count()
}

fn decode_items(value: Option<&Variant>) -> Vec<ChainItem> {
    let stored = value.and_then(Variant::as_array).unwrap_or_default();
    decode_chain(value)
        .into_entries()
        .into_iter()
        .zip(stored)
        .map(|(entry, raw)| match entry {
            ChainEntry::Step(step) => ChainItem::Step {
                step,
                stored: Some(raw.clone()),
            },
            ChainEntry::Undecodable(unreadable) => ChainItem::Unreadable(unreadable),
        })
        .collect()
}

fn encode_items(items: &[ChainItem]) -> Variant {
    Variant::Array(items.iter().map(ChainItem::encode).collect())
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

fn chain_slot_mut<'a>(
    config: &'a mut SubActionConfig,
    chain_key: &str,
    case_index: Option<usize>,
) -> Option<&'a mut Variant> {
    let empty = || Variant::Array(Vec::new());
    match case_index {
        None => Some(config.entry(chain_key.to_owned()).or_insert_with(empty)),
        Some(ci) => match config.get_mut(chain_key)? {
            Variant::Array(cases) => match cases.get_mut(ci)? {
                Variant::Object(case) => {
                    Some(case.entry(CASE_CHAIN_KEY.to_owned()).or_insert_with(empty))
                }
                _ => None,
            },
            _ => None,
        },
    }
}

pub(super) fn resolve_chain(root: &[SubActionStep], path: &[NavFrame]) -> Vec<ChainItem> {
    let Some((first, rest)) = path.split_first() else {
        return root.iter().cloned().map(ChainItem::fresh).collect();
    };
    let Some(step) = root.get(first.step_index) else {
        return Vec::new();
    };
    let mut value = chain_value_at(&step.config, &first.chain_key, first.case_index).cloned();
    for frame in rest {
        value = value
            .as_ref()
            .and_then(Variant::as_array)
            .and_then(|entries| entries.get(frame.step_index))
            .and_then(|raw| decode_step(raw).ok())
            .and_then(|step| {
                chain_value_at(&step.config, &frame.chain_key, frame.case_index).cloned()
            });
    }
    decode_items(value.as_ref())
}

pub(super) fn set_chain(
    root: &mut Vec<SubActionStep>,
    path: &[NavFrame],
    new_chain: &[ChainItem],
) -> bool {
    let Some((first, rest)) = path.split_first() else {
        if new_chain.iter().any(|item| item.step().is_none()) {
            return false;
        }
        *root = chain_steps(new_chain);
        return true;
    };
    let Some(step) = root.get_mut(first.step_index) else {
        return false;
    };
    let Some(slot) = chain_slot_mut(&mut step.config, &first.chain_key, first.case_index) else {
        return false;
    };
    write_nested(slot, rest, new_chain)
}

pub(super) fn owner_kinds(root: &[SubActionStep], path: &[NavFrame]) -> Option<Vec<String>> {
    let Some((first, rest)) = path.split_first() else {
        return Some(Vec::new());
    };
    let step = root.get(first.step_index)?;
    let mut kinds = vec![step.kind_id.clone()];
    let mut value = chain_value_at(&step.config, &first.chain_key, first.case_index).cloned();
    for frame in rest {
        let raw = value.as_ref()?.as_array()?.get(frame.step_index)?;
        let owner = decode_step(raw).ok()?;
        kinds.push(owner.kind_id.clone());
        value = chain_value_at(&owner.config, &frame.chain_key, frame.case_index).cloned();
    }
    Some(kinds)
}

pub(super) fn insert_item(
    root: &mut Vec<SubActionStep>,
    path: &[NavFrame],
    owners: &[String],
    index: usize,
    item: &ChainItem,
) -> bool {
    if owner_kinds(root, path).as_deref() != Some(owners) {
        return false;
    }
    let mut chain = resolve_chain(root, path);
    chain.insert(index.min(chain.len()), item.clone());
    set_chain(root, path, &chain)
}

fn write_nested(slot: &mut Variant, path: &[NavFrame], new_chain: &[ChainItem]) -> bool {
    let Some((frame, rest)) = path.split_first() else {
        *slot = encode_items(new_chain);
        return true;
    };
    let Variant::Array(entries) = slot else {
        return false;
    };
    let Some(raw) = entries.get_mut(frame.step_index) else {
        return false;
    };
    let Ok(mut step) = decode_step(raw) else {
        return false;
    };
    let Some(child) = chain_slot_mut(&mut step.config, &frame.chain_key, frame.case_index) else {
        return false;
    };
    if !write_nested(child, rest, new_chain) {
        return false;
    }
    *raw = encode_step_onto(&step, raw);
    true
}

pub(super) fn branch_step_count(
    step: &SubActionStep,
    chain_key: &str,
    case_index: Option<usize>,
) -> usize {
    decode_chain(chain_value_at(&step.config, chain_key, case_index))
        .entries()
        .len()
}

pub(super) fn case_list_keys(
    step: &SubActionStep,
    registry: &SubActionRegistry,
) -> Vec<&'static str> {
    fn push_case_list(field: &FormField, keys: &mut Vec<&'static str>) {
        match field {
            FormField::CaseList { key, .. } => keys.push(key),
            FormField::Optional { inner, .. } => push_case_list(inner, keys),
            _ => {}
        }
    }
    let mut keys = Vec::new();
    if let Some(runner) = registry.get(&step.kind_id) {
        for field in runner.config_fields() {
            push_case_list(&field, &mut keys);
        }
    }
    keys
}

fn case_at<'a>(
    step: &'a SubActionStep,
    cases_key: &str,
    case_index: usize,
) -> Option<&'a SubActionConfig> {
    step.config
        .get(cases_key)
        .and_then(Variant::as_array)
        .and_then(|cases| cases.get(case_index))
        .and_then(Variant::as_object)
}

pub(super) fn case_match_display(
    step: &SubActionStep,
    cases_key: &str,
    case_index: usize,
) -> Option<String> {
    match case_at(step, cases_key, case_index)?.get(CASE_MATCH_KEY) {
        Some(Variant::Array(_)) => None,
        Some(other) => Some(crate::config_text::editable_line(other)),
        None => Some(String::new()),
    }
}

pub(super) fn case_match_is_multi(
    step: &SubActionStep,
    cases_key: &str,
    case_index: usize,
) -> bool {
    case_at(step, cases_key, case_index)
        .and_then(|case| case.get(CASE_MATCH_KEY))
        .is_some_and(|m| matches!(m, Variant::Array(_)))
}

pub(super) fn case_count(step: &SubActionStep, cases_key: &str) -> usize {
    step.config
        .get(cases_key)
        .and_then(Variant::as_array)
        .map(<[Variant]>::len)
        .unwrap_or(0)
}

pub(super) fn append_empty_case(config: &mut SubActionConfig, cases_key: &str) {
    let mut cases = match config.get(cases_key) {
        Some(Variant::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let mut case = SubActionConfig::new();
    case.insert(CASE_MATCH_KEY.to_owned(), Variant::String(String::new()));
    case.insert(CASE_CHAIN_KEY.to_owned(), Variant::Array(Vec::new()));
    cases.push(Variant::Object(case));
    config.insert(cases_key.to_owned(), Variant::Array(cases));
}

pub(super) fn remove_case(config: &mut SubActionConfig, cases_key: &str, case_index: usize) {
    if let Some(Variant::Array(items)) = config.get(cases_key) {
        let mut cases = items.clone();
        if case_index < cases.len() {
            cases.remove(case_index);
            config.insert(cases_key.to_owned(), Variant::Array(cases));
        }
    }
}

pub(super) fn move_case(
    config: &mut SubActionConfig,
    cases_key: &str,
    case_index: usize,
    up: bool,
) {
    if let Some(Variant::Array(items)) = config.get(cases_key) {
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
            config.insert(cases_key.to_owned(), Variant::Array(cases));
        }
    }
}

pub(super) fn set_case_match(
    config: &mut SubActionConfig,
    cases_key: &str,
    case_index: usize,
    value: &str,
) {
    if let Some(Variant::Array(items)) = config.get(cases_key) {
        let mut cases = items.clone();
        if let Some(Variant::Object(case)) = cases.get_mut(case_index) {
            case.insert(CASE_MATCH_KEY.to_owned(), Variant::String(value.to_owned()));
            config.insert(cases_key.to_owned(), Variant::Array(cases));
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{SubActionConfig, SubActionStep, Variant};

    use forge_registry::{
        FormField, RegistryError, RunContext, SubActionCategory, SubActionRegistry,
        SubActionRunner, UndecodableReason, UndecodableStep,
    };
    use forge_types::{ArgStack, SubActionTelemetry};

    use super::{
        ChainItem, NavFrame, append_empty_case, branch_step_count, case_list_keys, chain_steps,
        resolve_chain, set_chain,
    };

    const LOG: &str = "core.log";
    const BRANCH: &str = "core.if";
    const THEN: &str = "then";
    const SWITCH: &str = "fake.switch";
    const OPTIONAL_CASES_KEY: &str = "branches";

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
        append_empty_case(&mut switch.config, "cases");
        append_empty_case(&mut switch.config, "cases");
        switch
    }

    fn frame(step_index: usize, chain_key: &str, case_index: Option<usize>) -> NavFrame {
        NavFrame {
            step_index,
            chain_key: chain_key.to_owned(),
            case_index,
        }
    }

    fn edited_items() -> Vec<ChainItem> {
        edited_chain().into_iter().map(ChainItem::fresh).collect()
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
            &[ChainItem::fresh(nested_if)]
        ));
        let path = [frame(1, "cases", Some(1)), frame(0, "then", None)];

        assert!(set_chain(&mut root, &path, &edited_items()));

        assert_eq!(chain_steps(&resolve_chain(&root, &path)), edited_chain());
    }

    #[test]
    fn writing_into_one_case_leaves_the_sibling_case_empty() {
        let mut root = vec![switch_with_two_cases()];

        assert!(set_chain(
            &mut root,
            &[frame(0, "cases", Some(1))],
            &edited_items()
        ));

        assert_eq!(
            chain_steps(&resolve_chain(&root, &[frame(0, "cases", Some(0))])),
            Vec::new()
        );
    }

    #[test]
    fn a_path_through_a_step_that_does_not_exist_is_refused_without_touching_the_chain() {
        let mut root = vec![step("core.log")];

        let written = set_chain(&mut root, &[frame(3, "then", None)], &edited_items());

        assert_eq!((written, root), (false, vec![step("core.log")]));
    }

    fn s(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn obj<const N: usize>(fields: [(&str, Variant); N]) -> Variant {
        Variant::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn with(entry: Variant, key: &str, value: Variant) -> Variant {
        match entry {
            Variant::Object(mut fields) => {
                fields.insert(key.to_owned(), value);
                Variant::Object(fields)
            }
            other => other,
        }
    }

    fn readable(text: &str) -> Variant {
        obj([
            ("kind_id", s(LOG)),
            ("config", obj([("text", s(text))])),
            ("enabled", Variant::Bool(true)),
            ("continue_on_error", Variant::Bool(false)),
            ("x_extra", s("kept")),
        ])
    }

    fn not_an_object() -> Variant {
        s("not a step")
    }

    fn kindless() -> Variant {
        obj([("config", obj([])), ("x_extra", s("kept"))])
    }

    fn stored_branch(body: Vec<Variant>) -> Variant {
        obj([
            ("kind_id", s(BRANCH)),
            ("config", obj([(THEN, Variant::Array(body))])),
            ("enabled", Variant::Bool(true)),
            ("continue_on_error", Variant::Bool(false)),
            ("x_extra", s("kept")),
        ])
    }

    fn branch_holding(body: Vec<Variant>) -> SubActionStep {
        SubActionStep {
            config: SubActionConfig::from([(THEN.to_owned(), Variant::Array(body))]),
            ..step(BRANCH)
        }
    }

    fn mixed_chain() -> Vec<Variant> {
        vec![not_an_object(), readable("a"), kindless(), readable("b")]
    }

    type Edit = fn(&mut Vec<ChainItem>);

    fn edits_of_a_mixed_chain() -> Vec<(&'static str, Edit, Vec<Variant>)> {
        vec![
            (
                "toggle",
                |items| {
                    if let Some(step) = items[1].step_mut() {
                        step.enabled = false;
                    }
                },
                vec![
                    not_an_object(),
                    with(readable("a"), "enabled", Variant::Bool(false)),
                    kindless(),
                    readable("b"),
                ],
            ),
            (
                "edit",
                |items| {
                    if let Some(step) = items[3].step_mut() {
                        step.config.insert("text".to_owned(), s("edited"));
                        step.label = Some("renamed".to_owned());
                    }
                },
                vec![
                    not_an_object(),
                    readable("a"),
                    kindless(),
                    with(readable("edited"), "label", s("renamed")),
                ],
            ),
            (
                "move up past an unreadable entry",
                |items| {
                    let moved = items.remove(3);
                    items.insert(2, moved);
                },
                vec![not_an_object(), readable("a"), readable("b"), kindless()],
            ),
            (
                "remove",
                |items| {
                    items.remove(1);
                },
                vec![not_an_object(), kindless(), readable("b")],
            ),
        ]
    }

    fn apply(root: &mut Vec<SubActionStep>, path: &[NavFrame], edit: Edit) -> bool {
        let mut items = resolve_chain(root, path);
        edit(&mut items);
        set_chain(root, path, &items)
    }

    #[test]
    fn editing_a_chain_one_level_deep_keeps_unreadable_entries_and_unknown_keys_verbatim() {
        for (case, edit, expected) in edits_of_a_mixed_chain() {
            let mut root = vec![branch_holding(mixed_chain())];

            let written = apply(&mut root, &[frame(0, THEN, None)], edit);

            assert_eq!(
                (written, root[0].config.get(THEN)),
                (true, Some(&Variant::Array(expected))),
                "{case}"
            );
        }
    }

    #[test]
    fn editing_a_chain_two_levels_deep_keeps_unreadable_entries_and_unknown_keys_at_both_levels() {
        let outer = |inner: Vec<Variant>| vec![not_an_object(), stored_branch(inner), kindless()];
        for (case, edit, expected) in edits_of_a_mixed_chain() {
            let mut root = vec![branch_holding(outer(mixed_chain()))];
            let path = [frame(0, THEN, None), frame(1, THEN, None)];

            let written = apply(&mut root, &path, edit);

            assert_eq!(
                (written, root[0].config.get(THEN)),
                (true, Some(&Variant::Array(outer(expected)))),
                "{case}"
            );
        }
    }

    #[test]
    fn a_nested_chain_saved_without_edits_keeps_every_stored_entry_verbatim() {
        let stored = vec![
            not_an_object(),
            stored_branch(mixed_chain()),
            obj([("kind_id", s(LOG)), ("x_extra", s("kept"))]),
        ];
        let mut root = vec![branch_holding(stored.clone())];

        let written = apply(&mut root, &[frame(0, THEN, None)], |_| {});

        assert_eq!(
            (written, root[0].config.get(THEN)),
            (true, Some(&Variant::Array(stored)))
        );
    }

    #[test]
    fn a_path_through_an_unreadable_entry_resolves_empty_and_refuses_writes() {
        let original = vec![branch_holding(vec![not_an_object(), readable("a")])];
        let mut root = original.clone();
        let path = [frame(0, THEN, None), frame(0, THEN, None)];

        let resolved = resolve_chain(&root, &path);
        let written = set_chain(&mut root, &path, &edited_items());

        assert_eq!((resolved, written, root), (Vec::new(), false, original));
    }

    #[test]
    fn the_root_chain_refuses_an_unreadable_entry() {
        let mut root = vec![step(LOG)];
        let unreadable = ChainItem::Unreadable(UndecodableStep {
            position: 0,
            reason: UndecodableReason::NotAnObject,
            raw: not_an_object(),
        });

        let written = set_chain(
            &mut root,
            &[],
            &[ChainItem::fresh(step(BRANCH)), unreadable],
        );

        assert_eq!((written, root), (false, vec![step(LOG)]));
    }

    #[test]
    fn branch_step_count_counts_unreadable_entries_like_the_resolved_chain() {
        let mut switch = switch_with_two_cases();
        switch.config.insert(
            "cases".to_owned(),
            Variant::Array(vec![obj([
                ("match", s("x")),
                ("chain", Variant::Array(mixed_chain())),
            ])]),
        );
        let root = vec![branch_holding(mixed_chain()), switch];

        for (step_index, key, case_index) in [(0, THEN, None), (1, "cases", Some(0))] {
            let resolved = resolve_chain(&root, &[frame(step_index, key, case_index)]).len();

            assert_eq!(
                (
                    branch_step_count(&root[step_index], key, case_index),
                    resolved
                ),
                (4, 4),
                "{key}"
            );
        }
    }

    struct CaseListRunner;

    #[async_trait::async_trait]
    impl SubActionRunner for CaseListRunner {
        fn id(&self) -> &str {
            SWITCH
        }

        fn category(&self) -> SubActionCategory {
            SubActionCategory::Logic
        }

        fn label(&self) -> &str {
            SWITCH
        }

        fn summary(&self) -> &str {
            SWITCH
        }

        fn search_text(&self) -> &str {
            SWITCH
        }

        fn icon_name(&self) -> &str {
            "bolt"
        }

        fn default_config(&self) -> SubActionConfig {
            SubActionConfig::new()
        }

        fn config_fields(&self) -> Vec<FormField> {
            vec![
                FormField::SubChain {
                    key: THEN,
                    label: THEN,
                },
                FormField::Optional {
                    key: "use_branches",
                    label: "use_branches",
                    inner: Box::new(FormField::CaseList {
                        key: OPTIONAL_CASES_KEY,
                        label: OPTIONAL_CASES_KEY,
                    }),
                },
            ]
        }

        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            Ok(())
        }

        async fn execute(
            &self,
            _: &SubActionConfig,
            _: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            unreachable!("case lookup never runs a step")
        }
    }

    #[test]
    fn case_list_keys_come_from_the_registered_form_including_behind_an_optional_field() {
        let mut registry = SubActionRegistry::new();
        assert!(registry.register(Box::new(CaseListRunner)).is_ok());

        let keys = [step(SWITCH), step(LOG)].map(|step| case_list_keys(&step, &registry));

        assert_eq!(keys, [vec![OPTIONAL_CASES_KEY], Vec::new()]);
    }
}
