use forge_registry::COMMAND_PHRASE_KEY;
use std::collections::{HashMap, HashSet};

use forge_registry::{
    CASE_MATCH_KEY, STEP_CONDITION_KEY, STEP_CONFIG_KEY, STEP_KIND_KEY, command_name_of,
};
use forge_types::{COMMAND_NAME_VARIABLE, Variant};

use crate::documents::model::{ActionDocument, DocumentConfig, PackDocuments};

const KICK_COMMAND_KIND: &str = "kick.chat.command";
const YOUTUBE_COMMAND_KIND: &str = "youtube.chat.command";
const SWITCH_KIND: &str = "core.logic.switch_case";

const KICK_DEFAULT_PHRASE: &str = "!command";
const WHILE_CONDITION_KEY: &str = "while_condition";
const SWITCH_VALUE_KEY: &str = "expression";
const SWITCH_CASES_KEY: &str = "cases";

const ARGUMENTS_VARIABLE: &str = "args";
const WRAPPED_ARGUMENTS_VARIABLE: &str = "%args%";

const VARIABLE_WRITERS: [(&str, &str); 33] = [
    ("core.action.cancel_scheduled", "into_var"),
    ("core.action.schedule", "due_at_into_var"),
    ("core.action.schedule", "id_into_var"),
    ("core.args.set", "name"),
    ("core.clipboard.read", "into_var"),
    ("core.file.list", "into_var"),
    ("core.file.read", "target_var"),
    ("core.globals.get", "into_arg"),
    ("core.latest.get", "into_var"),
    ("core.math.evaluate", "into_var"),
    ("core.random.bool", "into_var"),
    ("core.random.float", "into_var"),
    ("core.random.int", "target_var"),
    ("core.random.pick", "into_var"),
    ("core.string.concat", "into_var"),
    ("core.string.format", "into_var"),
    ("core.string.length", "into_var"),
    ("core.string.lowercase", "into_var"),
    ("core.string.regex_match", "captures_into_var"),
    ("core.string.regex_match", "into_var"),
    ("core.string.replace", "into_var"),
    ("core.string.split", "into_var"),
    ("core.string.substring", "into_var"),
    ("core.string.titlecase", "into_var"),
    ("core.string.trim", "into_var"),
    ("core.string.uppercase", "into_var"),
    ("core.time.add", "into_var"),
    ("core.time.diff", "into_var"),
    ("core.time.format", "into_var"),
    ("core.time.now", "into_var"),
    ("core.time.parse", "into_var"),
    ("core.users.get_var", "into_var"),
    ("script.run.named", "target_var"),
];

const BARE_ARRAY_SOURCE: (&str, &str) = (
    r#""array_source":{"type":"string","value":"args"}"#,
    r#""array_source":{"type":"string","value":"command.args"}"#,
);
const WRAPPED_ARRAY_SOURCE: (&str, &str) = (
    r#""array_source":{"type":"string","value":"%args%"}"#,
    r#""array_source":{"type":"string","value":"%command.args%"}"#,
);
const OLD_REFERENCE: &str = "%args%";
const OLD_PATH_PREFIX: &str = "%args.";
const PATH_PREFIX: &str = "%command.args.";
const TAIL_REFERENCE: &str = "%command.tail%";
const ARGS_REFERENCE: &str = "%command.args%";

struct ActionSteps {
    id: String,
    encoded: Option<String>,
    writes_arguments: bool,
}

struct CommandWord {
    typed: String,
    name: String,
}

pub(crate) fn upgrade_command_arguments(documents: &mut PackDocuments) {
    let actions: Vec<ActionSteps> = documents.actions.iter().map(ActionSteps::of).collect();
    let words = kick_command_words(documents, &actions);
    let kick = argument_readers(documents, &actions, KICK_COMMAND_KIND);
    let youtube = argument_readers(documents, &actions, YOUTUBE_COMMAND_KIND);
    let upgraded = documents.actions.iter_mut().zip(actions).enumerate();
    for (index, (action, steps)) in upgraded {
        if let Some(words) = words.get(&index) {
            for step in &mut action.steps {
                let renamed = step.condition.as_mut();
                rename_in_step(&step.kind_id, &mut step.config, renamed, words);
            }
        }
        let reference = if kick.contains(&index) {
            TAIL_REFERENCE
        } else if youtube.contains(&index) {
            ARGS_REFERENCE
        } else {
            continue;
        };
        if steps.writes_arguments {
            continue;
        }
        let rewritten = serde_json::to_string(&action.steps)
            .ok()
            .and_then(|encoded| serde_json::from_str(&rewrite(&encoded, reference)).ok());
        if let Some(rewritten) = rewritten {
            action.steps = rewritten;
        }
    }
}

impl ActionSteps {
    fn of(action: &ActionDocument) -> Self {
        Self {
            id: action.id.to_string(),
            encoded: serde_json::to_string(&action.steps).ok(),
            writes_arguments: action
                .steps
                .iter()
                .any(|step| step_writes_arguments(&step.kind_id, &step.config)),
        }
    }

    fn calls(&self, callee: &Self) -> bool {
        self.encoded
            .as_deref()
            .is_some_and(|encoded| encoded.contains(&callee.id))
    }
}

fn step_writes_arguments(kind_id: &str, config: &DocumentConfig) -> bool {
    VARIABLE_WRITERS.iter().any(|(writer, field)| {
        *writer == kind_id
            && matches!(
                config.get(*field),
                Some(Variant::String(name))
                    if name == ARGUMENTS_VARIABLE || name == WRAPPED_ARGUMENTS_VARIABLE
            )
    }) || config.values().any(value_writes_arguments)
}

fn value_writes_arguments(value: &Variant) -> bool {
    match value {
        Variant::Array(items) => items.iter().any(value_writes_arguments),
        Variant::Object(entries) => {
            match (entries.get(STEP_KIND_KEY), entries.get(STEP_CONFIG_KEY)) {
                (Some(Variant::String(kind_id)), Some(Variant::Object(config))) => {
                    step_writes_arguments(kind_id, config)
                }
                _ => entries.values().any(value_writes_arguments),
            }
        }
        _ => false,
    }
}

fn bound_actions(documents: &PackDocuments, kind_id: &str) -> Vec<(usize, Option<String>)> {
    let mut bound = Vec::new();
    for trigger in documents.triggers.iter().filter(|t| t.kind_id == kind_id) {
        let phrase = match trigger.overrides.get(COMMAND_PHRASE_KEY) {
            Some(Variant::String(phrase)) => Some(phrase.clone()),
            _ => None,
        };
        for (index, action) in documents.actions.iter().enumerate() {
            if action.trigger_ids.contains(&trigger.id) {
                bound.push((index, phrase.clone()));
            }
        }
    }
    bound
}

fn reached_from(
    seeds: impl IntoIterator<Item = usize>,
    actions: &[ActionSteps],
    passes_arguments_on: impl Fn(&ActionSteps) -> bool,
) -> HashSet<usize> {
    let mut reached: HashSet<usize> = seeds.into_iter().collect();
    let mut frontier: Vec<usize> = reached.iter().copied().collect();
    while let Some(caller) = frontier.pop() {
        if !passes_arguments_on(&actions[caller]) {
            continue;
        }
        for (index, callee) in actions.iter().enumerate() {
            if actions[caller].calls(callee) && reached.insert(index) {
                frontier.push(index);
            }
        }
    }
    reached
}

fn argument_readers(
    documents: &PackDocuments,
    actions: &[ActionSteps],
    kind_id: &str,
) -> HashSet<usize> {
    let seeds = bound_actions(documents, kind_id)
        .into_iter()
        .map(|(index, _)| index);
    reached_from(seeds, actions, |caller| !caller.writes_arguments)
}

fn kick_command_words(
    documents: &PackDocuments,
    actions: &[ActionSteps],
) -> HashMap<usize, Vec<CommandWord>> {
    let mut words: HashMap<usize, Vec<CommandWord>> = HashMap::new();
    for (bound, phrase) in bound_actions(documents, KICK_COMMAND_KIND) {
        let phrase = phrase.as_deref().unwrap_or(KICK_DEFAULT_PHRASE);
        let typed = phrase.split_whitespace().next().unwrap_or_default();
        let name = command_name_of(phrase);
        if typed == name {
            continue;
        }
        for reached in reached_from([bound], actions, |_| true) {
            words.entry(reached).or_default().push(CommandWord {
                typed: typed.to_owned(),
                name: name.clone(),
            });
        }
    }
    words
}

fn rename_in_condition(condition: &mut String, words: &[CommandWord]) {
    if !condition.contains(&format!("%{COMMAND_NAME_VARIABLE}%")) {
        return;
    }
    for word in words {
        for quote in ['"', '`'] {
            *condition = condition.replace(
                &format!("{quote}{}{quote}", word.typed),
                &format!("{quote}{}{quote}", word.name),
            );
        }
    }
}

fn rename_in_step(
    kind_id: &str,
    config: &mut DocumentConfig,
    condition: Option<&mut String>,
    words: &[CommandWord],
) {
    if let Some(condition) = condition {
        rename_in_condition(condition, words);
    }
    for key in [STEP_CONDITION_KEY, WHILE_CONDITION_KEY] {
        if let Some(Variant::String(condition)) = config.get_mut(key) {
            rename_in_condition(condition, words);
        }
    }
    if kind_id == SWITCH_KIND {
        rename_in_switch_cases(config, words);
    }
    for value in config.values_mut() {
        rename_in_value(value, words);
    }
}

fn rename_in_switch_cases(config: &mut DocumentConfig, words: &[CommandWord]) {
    let switches_on_the_name = matches!(
        config.get(SWITCH_VALUE_KEY),
        Some(Variant::String(value)) if value.contains(&format!("%{COMMAND_NAME_VARIABLE}%"))
    );
    let Some(Variant::Array(cases)) = config.get_mut(SWITCH_CASES_KEY) else {
        return;
    };
    if !switches_on_the_name {
        return;
    }
    for case in cases {
        if let Variant::Object(case) = case
            && let Some(Variant::String(matched)) = case.get_mut(CASE_MATCH_KEY)
            && let Some(word) = words.iter().find(|word| word.typed == *matched)
        {
            matched.clone_from(&word.name);
        }
    }
}

fn rename_in_value(value: &mut Variant, words: &[CommandWord]) {
    match value {
        Variant::Array(items) => {
            for item in items {
                rename_in_value(item, words);
            }
        }
        Variant::Object(entries) => {
            let kind_id = match entries.get(STEP_KIND_KEY) {
                Some(Variant::String(kind_id)) => Some(kind_id.clone()),
                _ => None,
            };
            if let Some(Variant::String(condition)) = entries.get_mut(STEP_CONDITION_KEY) {
                rename_in_condition(condition, words);
            }
            match (kind_id, entries.get_mut(STEP_CONFIG_KEY)) {
                (Some(kind_id), Some(Variant::Object(config))) => {
                    rename_in_step(&kind_id, config, None, words);
                }
                _ => {
                    for entry in entries.values_mut() {
                        rename_in_value(entry, words);
                    }
                }
            }
        }
        _ => {}
    }
}

fn rewrite(encoded: &str, reference: &str) -> String {
    encoded
        .replace(BARE_ARRAY_SOURCE.0, BARE_ARRAY_SOURCE.1)
        .replace(WRAPPED_ARRAY_SOURCE.0, WRAPPED_ARRAY_SOURCE.1)
        .replace(OLD_REFERENCE, reference)
        .replace(OLD_PATH_PREFIX, PATH_PREFIX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{
        ActionId, ExecutionMode, PermissionRung, PlatformScope, QueueId, TriggerInstanceId, Variant,
    };

    use super::*;
    use crate::documents::model::{StepDocument, TriggerDocument};

    const TWITCH_COMMAND_KIND: &str = "twitch.chat.command";
    const LOG_STEP: &str = "core.log.write";
    const RUN_STEP: &str = "core.action.run";
    const MESSAGE: &str = "message";
    const ARRAY_SOURCE: &str = "array_source";

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn step(kind_id: &str, key: &str, value: &str) -> StepDocument {
        StepDocument {
            kind_id: kind_id.to_owned(),
            config: DocumentConfig::from([(key.to_owned(), text(value))]),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn logging(value: &str) -> StepDocument {
        step(LOG_STEP, MESSAGE, value)
    }

    fn calling(action: &ActionDocument) -> StepDocument {
        step(RUN_STEP, "action_id", &action.id.to_string())
    }

    fn nested(inner: &StepDocument) -> StepDocument {
        let encoded = Variant::Object(BTreeMap::from([
            ("kind_id".to_owned(), text(&inner.kind_id)),
            ("config".to_owned(), Variant::Object(inner.config.clone())),
            ("enabled".to_owned(), Variant::Bool(true)),
        ]));
        StepDocument {
            config: DocumentConfig::from([(
                "then_chain".to_owned(),
                Variant::Array(vec![encoded]),
            )]),
            ..logging("")
        }
    }

    fn action(steps: Vec<StepDocument>) -> ActionDocument {
        ActionDocument {
            id: ActionId::new(),
            name: "action".to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::default(),
            description: None,
            steps,
            trigger_ids: Vec::new(),
        }
    }

    fn trigger(kind_id: &str) -> TriggerDocument {
        TriggerDocument {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: "command".to_owned(),
            overrides: DocumentConfig::new(),
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    fn pack(actions: Vec<ActionDocument>, triggers: Vec<TriggerDocument>) -> PackDocuments {
        PackDocuments {
            version: 1,
            actions,
            triggers,
            queues: Vec::new(),
            scripts: Vec::new(),
            globals: Vec::new(),
            clips: Vec::new(),
            overlays: Vec::new(),
            media: Vec::new(),
        }
    }

    fn bound_to(kinds: &[&str], steps: Vec<StepDocument>) -> PackDocuments {
        let triggers: Vec<TriggerDocument> = kinds.iter().map(|kind| trigger(kind)).collect();
        let mut bound = action(steps);
        bound.trigger_ids = triggers.iter().map(|trigger| trigger.id).collect();
        pack(vec![bound], triggers)
    }

    fn upgraded(mut documents: PackDocuments) -> PackDocuments {
        upgrade_command_arguments(&mut documents);
        documents
    }

    fn logged(documents: &PackDocuments, action: usize) -> &Variant {
        &documents.actions[action].steps[0].config[MESSAGE]
    }

    #[test]
    fn each_platform_reference_moves_to_the_variable_of_the_same_shape() {
        for (kind, old, new) in [
            (KICK_COMMAND_KIND, "got %args%!", "got %command.tail%!"),
            (KICK_COMMAND_KIND, "%args.0%", "%command.args.0%"),
            (YOUTUBE_COMMAND_KIND, "got %args%!", "got %command.args%!"),
            (YOUTUBE_COMMAND_KIND, "%args.12%", "%command.args.12%"),
        ] {
            let documents = upgraded(bound_to(&[kind], vec![logging(old)]));

            assert_eq!(logged(&documents, 0), &text(new), "{kind} {old}");
        }
    }

    #[test]
    fn names_that_only_start_like_the_old_variable_stay_as_they_are() {
        for name in ["%args_count%", "%argsy%", "%my.args%", "args"] {
            let documents = upgraded(bound_to(&[KICK_COMMAND_KIND], vec![logging(name)]));

            assert_eq!(logged(&documents, 0), &text(name), "{name}");
        }
    }

    #[test]
    fn an_array_source_of_either_spelling_moves_to_the_argument_array_on_both_platforms() {
        for kind in [KICK_COMMAND_KIND, YOUTUBE_COMMAND_KIND] {
            for (old, new) in [("args", "command.args"), ("%args%", "%command.args%")] {
                let looping = step("core.flow.foreach", ARRAY_SOURCE, old);

                let documents = upgraded(bound_to(&[kind], vec![looping]));

                assert_eq!(
                    documents.actions[0].steps[0].config[ARRAY_SOURCE],
                    text(new),
                    "{kind} {old}"
                );
            }
        }
    }

    #[test]
    fn an_action_that_writes_its_own_args_is_left_untouched() {
        for (kind_id, field) in VARIABLE_WRITERS {
            for spelling in ["args", "%args%"] {
                let writer = step(kind_id, field, spelling);
                let original = bound_to(
                    &[KICK_COMMAND_KIND],
                    vec![logging("%args%"), writer.clone()],
                );

                assert_eq!(
                    upgraded(original.clone()),
                    original,
                    "{kind_id} {field} {spelling}"
                );

                let in_a_branch = bound_to(
                    &[KICK_COMMAND_KIND],
                    vec![logging("%args%"), nested(&writer)],
                );
                assert_eq!(
                    upgraded(in_a_branch.clone()),
                    in_a_branch,
                    "{kind_id} {field} {spelling} nested"
                );
            }
        }
    }

    #[test]
    fn an_action_with_no_kick_or_youtube_command_is_left_untouched() {
        for kinds in [
            &[TWITCH_COMMAND_KIND][..],
            &[][..],
            &["kick.chat.message"][..],
        ] {
            let original = bound_to(kinds, vec![logging("%args% %args.0%")]);

            assert_eq!(upgraded(original.clone()), original, "{kinds:?}");
        }
    }

    #[test]
    fn an_action_on_both_command_triggers_takes_the_kick_text_reference() {
        let documents = upgraded(bound_to(
            &[YOUTUBE_COMMAND_KIND, KICK_COMMAND_KIND],
            vec![logging("%args% %args.0%")],
        ));

        assert_eq!(
            logged(&documents, 0),
            &text("%command.tail% %command.args.0%")
        );
    }

    #[test]
    fn every_action_down_a_call_chain_follows_the_command_that_reaches_it() {
        let last = action(vec![logging("%args.1%")]);
        let middle = action(vec![logging("%args%"), calling(&last)]);
        let bound = trigger(YOUTUBE_COMMAND_KIND);
        let mut first = action(vec![logging("%args%"), calling(&middle)]);
        first.trigger_ids = vec![bound.id];

        let documents = upgraded(pack(vec![last, middle, first], vec![bound]));

        assert_eq!(logged(&documents, 0), &text("%command.args.1%"));
        assert_eq!(logged(&documents, 1), &text("%command.args%"));
        assert_eq!(logged(&documents, 2), &text("%command.args%"));
    }

    #[test]
    fn a_call_cycle_finishes_and_upgrades_both_of_its_actions() {
        let mut first = action(vec![logging("%args%")]);
        let mut second = action(vec![logging("%args%"), calling(&first)]);
        first.steps.push(calling(&second));
        let bound = trigger(KICK_COMMAND_KIND);
        first.trigger_ids = vec![bound.id];
        second.trigger_ids = Vec::new();

        let documents = upgraded(pack(vec![first, second], vec![bound]));

        assert_eq!(logged(&documents, 0), &text("%command.tail%"));
        assert_eq!(logged(&documents, 1), &text("%command.tail%"));
    }

    #[test]
    fn a_condition_and_a_nested_step_move_with_the_rest() {
        let guarded = StepDocument {
            condition: Some(r#"%args% == "go""#.to_owned()),
            ..logging("plain")
        };
        let expected = bound_to(
            &[KICK_COMMAND_KIND],
            vec![
                StepDocument {
                    condition: Some(r#"%command.tail% == "go""#.to_owned()),
                    ..logging("plain")
                },
                nested(&logging("%command.args.0%")),
            ],
        );
        let mut original = expected.clone();
        original.actions[0].steps = vec![guarded, nested(&logging("%args.0%"))];

        assert_eq!(upgraded(original), expected);
    }

    #[test]
    fn upgrading_twice_changes_nothing_more() {
        let looping = step("core.flow.foreach", ARRAY_SOURCE, "args");
        for kind in [KICK_COMMAND_KIND, YOUTUBE_COMMAND_KIND] {
            let once = upgraded(bound_to(
                &[kind],
                vec![logging("%args% %args.0%"), looping.clone()],
            ));

            assert_eq!(upgraded(once.clone()), once, "{kind}");
        }
    }

    #[test]
    fn repro_a_callee_inheriting_a_callers_own_args_keeps_its_reference() {
        let callee = action(vec![logging("%args%")]);
        let bound = trigger(KICK_COMMAND_KIND);
        let mut caller = action(vec![
            step("core.args.set", "name", "args"),
            calling(&callee),
        ]);
        caller.trigger_ids = vec![bound.id];

        let documents = upgraded(pack(vec![callee, caller], vec![bound]));

        assert_eq!(logged(&documents, 0), &text("%args%"));
    }

    #[test]
    fn repro_a_global_named_args_does_not_stop_the_upgrade() {
        let documents = upgraded(bound_to(
            &[KICK_COMMAND_KIND],
            vec![logging("%args%"), step("core.globals.set", "name", "args")],
        ));

        assert_eq!(logged(&documents, 0), &text("%command.tail%"));
    }

    fn phrased(phrase: &str) -> TriggerDocument {
        let mut bound = trigger(KICK_COMMAND_KIND);
        bound
            .overrides
            .insert(COMMAND_PHRASE_KEY.to_owned(), text(phrase));
        bound
    }

    fn bound_to_phrase(phrase: &str, steps: Vec<StepDocument>) -> PackDocuments {
        let bound = phrased(phrase);
        let mut only = action(steps);
        only.trigger_ids = vec![bound.id];
        pack(vec![only], vec![bound])
    }

    fn guarded(condition: &str) -> StepDocument {
        StepDocument {
            condition: Some(condition.to_owned()),
            ..logging("plain")
        }
    }

    fn first_condition(documents: &PackDocuments) -> Option<&str> {
        documents.actions[0].steps[0].condition.as_deref()
    }

    #[test]
    fn a_kick_condition_on_the_command_name_loses_the_marker_of_its_phrase() {
        for (phrase, old, new) in [
            (
                "!so",
                r#"%command_name% == "!so""#,
                r#"%command_name% == "so""#,
            ),
            (
                "~so",
                r#""~so" != %command_name%"#,
                r#""so" != %command_name%"#,
            ),
            ("!!so", "%command_name% == `!!so`", "%command_name% == `so`"),
            (
                "!shout out",
                r#"%command_name% == "!shout""#,
                r#"%command_name% == "shout""#,
            ),
        ] {
            let documents = upgraded(bound_to_phrase(phrase, vec![guarded(old)]));

            assert_eq!(first_condition(&documents), Some(new), "{phrase}");
        }
    }

    #[test]
    fn the_default_phrase_stands_in_for_a_trigger_without_an_override() {
        let documents = upgraded(bound_to(
            &[KICK_COMMAND_KIND],
            vec![guarded(r#"%command_name% == "!command""#)],
        ));

        assert_eq!(
            first_condition(&documents),
            Some(r#"%command_name% == "command""#)
        );
    }

    #[test]
    fn a_condition_outside_the_command_name_or_the_phrase_stays_as_it_is() {
        for (phrase, condition) in [
            ("!so", r#"%message% == "!so""#),
            ("!so", r#"%command_name% == "!other""#),
            ("!so", r#"%command_name% == "!so now""#),
            ("so", r#"%command_name% == "so""#),
            ("!!!", r#"%command_name% == "!!!""#),
        ] {
            let documents = upgraded(bound_to_phrase(phrase, vec![guarded(condition)]));

            assert_eq!(first_condition(&documents), Some(condition), "{phrase}");
        }
    }

    #[test]
    fn a_youtube_condition_on_the_command_name_stays_as_it_is() {
        let condition = r#"%command_name% == "!so""#;

        let documents = upgraded(bound_to(&[YOUTUBE_COMMAND_KIND], vec![guarded(condition)]));

        assert_eq!(first_condition(&documents), Some(condition));
    }

    #[test]
    fn every_phrase_bound_to_an_action_is_rewritten_down_its_call_chain() {
        let callee = action(vec![guarded(r#"%command_name% == "!one""#)]);
        let (first, second) = (phrased("!one"), phrased("!two"));
        let mut caller = action(vec![
            guarded(r#"%command_name% == "!one" || %command_name% == "!two""#),
            calling(&callee),
        ]);
        caller.trigger_ids = vec![first.id, second.id];

        let documents = upgraded(pack(vec![callee, caller], vec![first, second]));

        assert_eq!(
            first_condition(&documents),
            Some(r#"%command_name% == "one""#)
        );
        assert_eq!(
            documents.actions[1].steps[0].condition.as_deref(),
            Some(r#"%command_name% == "one" || %command_name% == "two""#)
        );
    }

    #[test]
    fn a_caller_writing_its_own_args_still_gets_its_name_condition_rewritten() {
        let documents = upgraded(bound_to_phrase(
            "!so",
            vec![
                guarded(r#"%command_name% == "!so""#),
                step("core.args.set", "name", "args"),
            ],
        ));

        assert_eq!(
            first_condition(&documents),
            Some(r#"%command_name% == "so""#)
        );
    }

    #[test]
    fn branch_loop_and_nested_conditions_follow_the_step_condition() {
        let condition = r#"%command_name% == "!so""#;
        let expected = r#"%command_name% == "so""#;
        for key in ["condition", "while_condition"] {
            let flow = step("core.logic.if_then_else", key, condition);
            let rewritten = step("core.logic.if_then_else", key, expected);

            let documents = upgraded(bound_to_phrase("!so", vec![flow.clone(), nested(&flow)]));

            assert_eq!(
                documents.actions[0].steps,
                vec![rewritten.clone(), nested(&rewritten)],
                "{key}"
            );
        }
    }

    fn switch_on(value: &str, cases: &[&str]) -> StepDocument {
        let cases = cases
            .iter()
            .map(|matched| {
                Variant::Object(BTreeMap::from([
                    (CASE_MATCH_KEY.to_owned(), text(matched)),
                    ("chain".to_owned(), Variant::Array(Vec::new())),
                ]))
            })
            .collect();
        let mut switch = step(SWITCH_KIND, SWITCH_VALUE_KEY, value);
        switch
            .config
            .insert(SWITCH_CASES_KEY.to_owned(), Variant::Array(cases));
        switch
    }

    #[test]
    fn a_switch_on_the_command_name_renames_its_matching_cases_only() {
        let documents = upgraded(bound_to_phrase(
            "!so",
            vec![
                switch_on("%command_name%", &["!so", "!other"]),
                switch_on("%message%", &["!so"]),
            ],
        ));

        assert_eq!(
            documents.actions[0].steps,
            vec![
                switch_on("%command_name%", &["so", "!other"]),
                switch_on("%message%", &["!so"]),
            ]
        );
    }

    #[test]
    fn a_caller_that_does_not_write_args_still_upgrades_its_callee() {
        let callee = action(vec![logging("%args%")]);
        let bound = trigger(KICK_COMMAND_KIND);
        let mut caller = action(vec![logging("hi"), calling(&callee)]);
        caller.trigger_ids = vec![bound.id];

        let documents = upgraded(pack(vec![callee, caller], vec![bound]));

        assert_eq!(logged(&documents, 0), &text("%command.tail%"));
    }

    #[test]
    fn a_callee_also_reached_by_a_plain_chain_follows_that_chain() {
        let callee = action(vec![logging("%args%")]);
        let (shadowed, direct) = (trigger(KICK_COMMAND_KIND), trigger(KICK_COMMAND_KIND));
        let mut writer = action(vec![
            step("core.args.set", "name", "args"),
            calling(&callee),
        ]);
        writer.trigger_ids = vec![shadowed.id];
        let mut plain = action(vec![logging("hi"), calling(&callee)]);
        plain.trigger_ids = vec![direct.id];

        let documents = upgraded(pack(vec![callee, writer, plain], vec![shadowed, direct]));

        assert_eq!(logged(&documents, 0), &text("%command.tail%"));
    }
}
