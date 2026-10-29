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
