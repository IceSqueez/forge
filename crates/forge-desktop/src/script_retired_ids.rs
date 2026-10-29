use std::sync::Arc;
use std::time::Duration;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::{ScriptRecord, ScriptRepo};
use gpui::{App, AsyncApp};
use tokio::runtime::Handle;

use crate::navigation::{Navigator, Screen};

const STICKY: Duration = Duration::ZERO;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdReplacement {
    pub retired: String,
    pub current: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetiredIdUse {
    pub script: String,
    pub replacements: Vec<IdReplacement>,
}

pub fn retired_successions(
    triggers: &TriggerRegistry,
    sub_actions: &SubActionRegistry,
) -> Vec<IdReplacement> {
    let mut replacements: Vec<IdReplacement> = triggers
        .retired_ids()
        .chain(sub_actions.retired_ids())
        .map(|(retired, current)| IdReplacement {
            retired: retired.to_owned(),
            current: current.to_owned(),
        })
        .collect();
    replacements.sort_by(|a, b| a.retired.cmp(&b.retired));
    replacements.dedup_by(|a, b| a.retired == b.retired);
    replacements
}

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn continues_id(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some('.') => chars.next().is_some_and(is_id_char),
        Some(c) => is_id_char(c),
        None => false,
    }
}

fn extends_id_backwards(before: &str) -> bool {
    before
        .chars()
        .next_back()
        .is_some_and(|c| is_id_char(c) || c == '.')
}

pub fn mentions_id(source: &str, id: &str) -> bool {
    source.match_indices(id).any(|(start, _)| {
        !extends_id_backwards(&source[..start]) && !continues_id(&source[start + id.len()..])
    })
}

pub fn retired_id_uses(scripts: &[ScriptRecord], retired: &[IdReplacement]) -> Vec<RetiredIdUse> {
    let mut uses: Vec<RetiredIdUse> = scripts
        .iter()
        .filter_map(|script| {
            let replacements: Vec<IdReplacement> = retired
                .iter()
                .filter(|replacement| mentions_id(&script.body, &replacement.retired))
                .cloned()
                .collect();
            (!replacements.is_empty()).then(|| RetiredIdUse {
                script: script.name.clone(),
                replacements,
            })
        })
        .collect();
    uses.sort_by(|a, b| a.script.cmp(&b.script));
    uses
}

pub fn notice_message(uses: &[RetiredIdUse]) -> String {
    let entries: Vec<String> = uses
        .iter()
        .map(|entry| {
            let pairs: Vec<String> = entry
                .replacements
                .iter()
                .map(|replacement| {
                    tr!(
                        "script_retired_ids_pair",
                        retired = replacement.retired.as_str(),
                        current = replacement.current.as_str()
                    )
                })
                .collect();
            tr!(
                "script_retired_ids_entry",
                script = entry.script.as_str(),
                ids = pairs.join(", ")
            )
        })
        .collect();
    tr!("script_retired_ids_notice", uses = entries.join("; "))
}

pub fn start_script_retired_id_notice(
    cx: &mut AsyncApp,
    scripts: Arc<dyn ScriptRepo>,
    retired: Vec<IdReplacement>,
    rt: &Handle,
    navigator: Navigator,
) {
    if retired.is_empty() {
        return;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let uses = match scripts.list().await {
            Ok(records) => retired_id_uses(&records, &retired),
            Err(error) => {
                tracing::warn!(error = %error, "scripts could not be checked for renamed ids");
                Vec::new()
            }
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the app may close before the result arrives"
        )]
        let _ = tx.send(uses);
    });
    cx.spawn(async move |cx| {
        let Ok(uses) = rx.await else {
            return;
        };
        if uses.is_empty() {
            return;
        }
        cx.update(|cx| raise_notice(&uses, navigator, cx));
    })
    .detach();
}

fn raise_notice(uses: &[RetiredIdUse], navigator: Navigator, cx: &mut App) {
    let action = ToastAction::new(tr!("script_retired_ids_open"), move |_window, cx| {
        navigator.open(Screen::Scripts, cx);
    });
    cx.push_toast_full(
        ToastKind::Warn,
        notice_message(uses),
        None,
        Some(action),
        STICKY,
    );
}

#[cfg(test)]
mod tests {
    use forge_registry::IdSuccession;
    use forge_storage::Language;
    use forge_types::{ScriptContract, ScriptId};
    use time::OffsetDateTime;

    use super::*;
    use crate::i18n::install_language;

    const RECORD_START: &str = "obs.record.start";

    fn replacement(retired: &str, current: &str) -> IdReplacement {
        IdReplacement {
            retired: retired.to_owned(),
            current: current.to_owned(),
        }
    }

    fn script(name: &str, body: &str) -> ScriptRecord {
        let now = OffsetDateTime::UNIX_EPOCH;
        ScriptRecord {
            id: ScriptId::new(),
            name: name.to_owned(),
            body: body.to_owned(),
            contract: ScriptContract::default(),
            enabled: true,
            created_at: now,
            last_modified: now,
        }
    }

    fn plain(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn mentions_id_matches_only_the_whole_id() {
        for (source, expected) in [
            ("obs.record.start", true),
            ("obs.record.start(); let x = 1;", true),
            ("let x = 1; obs.record.start", true),
            ("let kind = \"obs.record.start\";", true),
            ("ends the sentence with obs.record.start.", true),
            ("obs.record.started", false),
            ("obs.record.start_now", false),
            ("obs.record.start2", false),
            ("obs.record.start.paused", false),
            ("xobs.record.start", false),
            ("_obs.record.start", false),
            ("legacy.obs.record.start", false),
            ("obs.record.started then obs.record.start", true),
            ("", false),
            ("obs.record", false),
        ] {
            assert_eq!(mentions_id(source, RECORD_START), expected, "{source:?}");
        }
    }

    #[test]
    fn retired_successions_merges_both_registries_sorted_and_without_duplicates() {
        let mut triggers = TriggerRegistry::new();
        triggers
            .declare_retired(&IdSuccession {
                current: "obs.record.started",
                retired: &["obs_record_started", "obs.recording.started"],
            })
            .unwrap();
        let mut sub_actions = SubActionRegistry::new();
        sub_actions
            .declare_retired(&IdSuccession {
                current: "obs.record.start",
                retired: &["obs_record_start"],
            })
            .unwrap();
        sub_actions
            .declare_retired(&IdSuccession {
                current: "obs.record.started",
                retired: &["obs.recording.started"],
            })
            .unwrap();

        assert_eq!(
            retired_successions(&triggers, &sub_actions),
            vec![
                replacement("obs.recording.started", "obs.record.started"),
                replacement("obs_record_start", "obs.record.start"),
                replacement("obs_record_started", "obs.record.started"),
            ]
        );
    }

    #[test]
    fn retired_id_uses_lists_only_scripts_naming_a_retired_id_sorted_by_name() {
        let retired = [
            replacement("obs_record_start", "obs.record.start"),
            replacement("obs_scene_set", "obs.scene.set"),
        ];
        let scripts = [
            script("zeta", "forge.run(\"obs_scene_set\");"),
            script("clean", "forge.run(\"obs.record.start\");"),
            script("near_miss", "forge.run(\"obs_record_start_delayed\");"),
            script(
                "alpha",
                "forge.run(\"obs_scene_set\"); forge.run(\"obs_record_start\");",
            ),
        ];

        assert_eq!(
            retired_id_uses(&scripts, &retired),
            vec![
                RetiredIdUse {
                    script: "alpha".to_owned(),
                    replacements: retired.to_vec(),
                },
                RetiredIdUse {
                    script: "zeta".to_owned(),
                    replacements: vec![replacement("obs_scene_set", "obs.scene.set")],
                },
            ]
        );
    }

    #[test]
    fn retired_id_uses_is_empty_when_no_script_names_a_retired_id() {
        let retired = [replacement("obs_record_start", "obs.record.start")];
        for scripts in [
            vec![],
            vec![script("current", "forge.run(\"obs.record.start\");")],
            vec![script("longer", "forge.run(\"obs_record_start_now\");")],
        ] {
            assert!(
                retired_id_uses(&scripts, &retired).is_empty(),
                "{scripts:?}"
            );
        }
    }

    #[test]
    fn notice_message_lists_each_script_with_its_retired_to_current_pairs_per_language() {
        let uses = [
            RetiredIdUse {
                script: "alpha".to_owned(),
                replacements: vec![
                    replacement("obs_record_start", "obs.record.start"),
                    replacement("obs_scene_set", "obs.scene.set"),
                ],
            },
            RetiredIdUse {
                script: "zeta".to_owned(),
                replacements: vec![replacement("obs_scene_set", "obs.scene.set")],
            },
        ];
        let listed = "alpha (obs_record_start -> obs.record.start, obs_scene_set -> obs.scene.set); \
                      zeta (obs_scene_set -> obs.scene.set)";
        for (lang, expected) in [
            (
                Language::En,
                format!("Scripts still use renamed ids - update them: {listed}"),
            ),
            (
                Language::Uk,
                format!(
                    "Скрипти досі використовують перейменовані ідентифікатори - оновіть їх: {listed}"
                ),
            ),
        ] {
            install_language(lang);
            assert_eq!(plain(notice_message(&uses)), expected, "{lang:?}");
        }
    }
}
