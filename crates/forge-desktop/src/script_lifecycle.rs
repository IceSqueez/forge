use std::sync::Arc;

use forge_components::tr;
use forge_runtime::{EventBus, ScriptRegistry};
use forge_storage::{ScriptReferences, ScriptReferrer, ScriptRepo};
use forge_types::ScriptId;

use crate::repo_failure::RepoFailure;

#[derive(Clone)]
pub(crate) struct ScriptLifecycle {
    repo: Arc<dyn ScriptRepo>,
    registry: Arc<ScriptRegistry>,
    bus: Arc<EventBus>,
}

pub(crate) struct ScriptRenamed {
    pub id: ScriptId,
    pub name: String,
    pub references: ScriptReferences,
}

impl ScriptLifecycle {
    pub(crate) fn new(
        repo: Arc<dyn ScriptRepo>,
        registry: Arc<ScriptRegistry>,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            repo,
            registry,
            bus,
        }
    }

    pub(crate) async fn referrers(&self, name: String) -> Result<ScriptReferences, RepoFailure> {
        self.repo.referrers(&name).await.map_err(RepoFailure::from)
    }

    pub(crate) async fn rename(
        &self,
        id: ScriptId,
        name: String,
    ) -> Result<ScriptRenamed, RepoFailure> {
        let references = self
            .repo
            .rename_cascading(id, &name)
            .await
            .map_err(RepoFailure::from)?;
        if let Err(error) = self.reload(id).await {
            tracing::warn!(%error, "renamed script did not reload into the registry");
        }
        Ok(ScriptRenamed {
            id,
            name,
            references,
        })
    }

    pub(crate) async fn archive(&self, id: ScriptId) -> Result<bool, RepoFailure> {
        let archived = self
            .repo
            .archive_script(id)
            .await
            .map_err(RepoFailure::from)?;
        self.registry.remove(id).await;
        Ok(archived)
    }

    pub(crate) async fn restore(&self, id: ScriptId) -> Result<bool, RepoFailure> {
        let restored = self
            .repo
            .restore_script(id)
            .await
            .map_err(RepoFailure::from)?;
        if restored && let Err(error) = self.reload(id).await {
            tracing::warn!(%error, "restored script did not reload into the registry");
        }
        Ok(restored)
    }

    async fn reload(&self, id: ScriptId) -> Result<(), RepoFailure> {
        let Some(record) = self.repo.get(id).await.map_err(RepoFailure::from)? else {
            return Ok(());
        };
        self.registry
            .reload(record, &self.bus)
            .await
            .map_err(RepoFailure::from)
    }
}

pub(crate) fn has_referrers(references: &ScriptReferences) -> bool {
    !references.literal.is_empty() || !references.templated.is_empty()
}

pub(crate) fn referrer_count(referrers: &[ScriptReferrer]) -> i64 {
    i64::try_from(referrers.len()).unwrap_or(i64::MAX)
}

pub(crate) fn referrer_names(referrers: &[ScriptReferrer]) -> String {
    referrers
        .iter()
        .map(|r| r.action_name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn delete_hint(references: Option<&ScriptReferences>) -> String {
    let Some(references) = references else {
        return tr!("script_editor_delete_checking");
    };
    if !has_referrers(references) {
        return tr!("script_editor_delete_unused");
    }
    let mut parts = Vec::new();
    if !references.literal.is_empty() {
        parts.push(counted_names(
            "script_editor_delete_used_by",
            &references.literal,
        ));
    }
    if !references.templated.is_empty() {
        parts.push(counted_names(
            "script_editor_delete_maybe_used_by",
            &references.templated,
        ));
    }
    parts.push(tr!("script_editor_delete_archive_note"));
    parts.join(" ")
}

pub(crate) fn rename_hint(references: &ScriptReferences) -> String {
    let mut parts = Vec::new();
    if references.literal.is_empty() {
        parts.push(tr!("script_editor_rename_updates_none"));
    } else {
        parts.push(counted_names(
            "script_editor_rename_updates",
            &references.literal,
        ));
    }
    if !references.templated.is_empty() {
        parts.push(counted_names(
            "script_editor_rename_templated",
            &references.templated,
        ));
    }
    parts.join(" ")
}

pub(crate) fn counted_names(key: &str, referrers: &[ScriptReferrer]) -> String {
    let names = referrer_names(referrers);
    tr!(
        key,
        count = referrer_count(referrers),
        names = names.as_str()
    )
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;
    use forge_types::ActionId;

    use super::*;
    use crate::i18n::install_language;

    fn uses(literal: &[&str], templated: &[&str]) -> ScriptReferences {
        let referrers = |names: &[&str]| {
            names
                .iter()
                .map(|name| ScriptReferrer {
                    action_id: ActionId::new(),
                    action_name: (*name).to_owned(),
                })
                .collect()
        };
        ScriptReferences {
            literal: referrers(literal),
            templated: referrers(templated),
        }
    }

    fn plain(text: String) -> String {
        text.replace(crate::test_support::FORMAT_ISOLATES, "")
    }

    #[test]
    fn the_delete_hint_lists_each_kind_of_referrer_with_its_count() {
        install_language(Language::En);
        let archive_note =
            "Their steps fail until the script is restored. You can undo right after deleting.";
        for (references, expected) in [
            (None, "Checking which actions use this script...".to_owned()),
            (
                Some(uses(&[], &[])),
                "No action uses this script. It moves to the archive, and you can undo right after deleting."
                    .to_owned(),
            ),
            (
                Some(uses(&["Greeter"], &[])),
                format!("Used by 1 action: Greeter. {archive_note}"),
            ),
            (
                Some(uses(&["Greeter", "Welcomer"], &["Picker"])),
                format!(
                    "Used by 2 actions: Greeter, Welcomer. 1 action may pick it through a variable: Picker. {archive_note}"
                ),
            ),
            (
                Some(uses(&[], &["Picker", "Chooser"])),
                format!(
                    "2 actions may pick it through a variable: Picker, Chooser. {archive_note}"
                ),
            ),
        ] {
            assert_eq!(plain(delete_hint(references.as_ref())), expected);
        }
    }

    #[test]
    fn the_rename_hint_says_which_actions_follow_the_new_name_and_which_need_a_hand_check() {
        install_language(Language::En);
        for (references, expected) in [
            (uses(&[], &[]), "No action names this script directly.".to_owned()),
            (
                uses(&["Greeter", "Welcomer"], &[]),
                "2 actions will be updated to the new name: Greeter, Welcomer.".to_owned(),
            ),
            (
                uses(&[], &["Picker"]),
                "No action names this script directly. Check 1 action by hand - it picks the script through a variable and will not be updated: Picker."
                    .to_owned(),
            ),
            (
                uses(&["Greeter"], &["Picker", "Chooser"]),
                "1 action will be updated to the new name: Greeter. Check 2 actions by hand - they pick the script through a variable and will not be updated: Picker, Chooser."
                    .to_owned(),
            ),
        ] {
            assert_eq!(plain(rename_hint(&references)), expected);
        }
    }
}
