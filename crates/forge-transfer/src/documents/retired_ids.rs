use std::collections::BTreeMap;

use forge_registry::rename_step_kinds;
use forge_storage::{rename_favorite_ids, reserved_keys};

use crate::documents::closure::ClosureRegistries;
use crate::documents::model::PackDocuments;

pub(crate) fn upgrade_pack_documents(
    documents: &mut PackDocuments,
    registries: ClosureRegistries<'_>,
) {
    for trigger in &mut documents.triggers {
        if let Some(current) = registries.triggers.current_id(&trigger.kind_id) {
            trigger.kind_id = current.to_owned();
        }
    }
    let current_of = |id: &str| registries.sub_actions.current_id(id).map(str::to_owned);
    for action in &mut documents.actions {
        for step in &mut action.steps {
            rename_step_kinds(&mut step.kind_id, &mut step.config, &current_of);
        }
    }
}

pub(crate) fn upgrade_favorite_settings(
    settings: &mut BTreeMap<String, String>,
    registries: ClosureRegistries<'_>,
) {
    rename_favorites(
        settings,
        reserved_keys::PICKER_FAVORITES_SUB_ACTIONS,
        &|id| registries.sub_actions.current_id(id).map(str::to_owned),
    );
    rename_favorites(settings, reserved_keys::PICKER_FAVORITES_TRIGGERS, &|id| {
        registries.triggers.current_id(id).map(str::to_owned)
    });
}

fn rename_favorites(
    settings: &mut BTreeMap<String, String>,
    key: &str,
    current_of: &dyn Fn(&str) -> Option<String>,
) {
    if let Some(stored) = settings.get_mut(key)
        && let Some(renamed) = rename_favorite_ids(stored, current_of)
    {
        *stored = renamed;
    }
}
