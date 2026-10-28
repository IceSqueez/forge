use std::collections::BTreeSet;

use forge_registry::{SubActionCategory, SubActionRegistry, nested_chains};
use forge_types::{IntegrationId, SubActionStep};

use crate::documents::closure::ClosureRegistries;
use crate::documents::convert::action_of;
use crate::documents::model::ClipSourceDocument;
use crate::pack::import_plan::{ExecutableContent, PackWarning};
use crate::pack::reader::StagedPack;

pub(crate) struct PackReview {
    pub(crate) warnings: Vec<PackWarning>,
    pub(crate) executable: Vec<ExecutableContent>,
}

pub(crate) fn review_pack(staged: &StagedPack, registries: ClosureRegistries<'_>) -> PackReview {
    let manifest = staged.manifest();
    let documents = staged.documents();
    let catalog = registries.catalog();
    let mut warnings = BTreeSet::new();
    let mut executable = BTreeSet::new();

    for integration in &manifest.required_integrations {
        if !integration_known(registries, integration) {
            warnings.insert(PackWarning::MissingIntegration {
                integration: integration.clone(),
            });
        }
    }
    let mut unknown_kinds: BTreeSet<String> = manifest.unknown_kinds.iter().cloned().collect();
    for action in &documents.actions {
        let action = action_of(action);
        let listing = catalog.action_references(&action);
        unknown_kinds.extend(listing.unknown_kinds.into_iter().map(|kind| kind.kind_id));
        collect_executable(
            &action.name,
            &action.sub_actions,
            registries.sub_actions,
            &mut executable,
        );
    }
    for trigger in &documents.triggers {
        if registries.triggers.get(&trigger.kind_id).is_none() {
            unknown_kinds.insert(trigger.kind_id.clone());
        }
    }
    warnings.extend(
        unknown_kinds
            .into_iter()
            .map(|kind_id| PackWarning::UnknownKind { kind_id }),
    );
    warnings.extend(
        manifest
            .unresolved_references
            .iter()
            .cloned()
            .map(|reference| PackWarning::UnresolvedReference { reference }),
    );
    warnings.extend(
        manifest
            .external_references
            .iter()
            .cloned()
            .map(|reference| PackWarning::ExternalReference { reference }),
    );
    for clip in &documents.clips {
        if clip.source == ClipSourceDocument::Missing {
            warnings.insert(PackWarning::ClipWithoutSound {
                clip: clip.name.clone(),
            });
        }
    }

    executable.extend(
        documents
            .scripts
            .iter()
            .map(|script| ExecutableContent::Script {
                name: script.name.clone(),
            }),
    );
    for overlay in &documents.overlays {
        executable.extend(
            overlay
                .files
                .iter()
                .map(|file| ExecutableContent::OverlaySource {
                    overlay: overlay.display_name.clone(),
                    file: file.name.clone(),
                }),
        );
    }

    PackReview {
        warnings: warnings.into_iter().collect(),
        executable: executable.into_iter().collect(),
    }
}

fn integration_known(registries: ClosureRegistries<'_>, integration: &IntegrationId) -> bool {
    let owns = |owner: Option<&IntegrationId>| owner == Some(integration);
    registries
        .sub_actions
        .all()
        .any(|runner| owns(registries.sub_actions.owning_integration(runner.id())))
        || registries
            .triggers
            .all()
            .any(|descriptor| owns(registries.triggers.owning_integration(descriptor.id())))
}

fn collect_executable(
    action: &str,
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    out: &mut BTreeSet<ExecutableContent>,
) {
    for step in steps {
        let category = registry.get(&step.kind_id).map(|runner| runner.category());
        let kind_id = step.kind_id.clone();
        match category {
            Some(SubActionCategory::Http) => {
                out.insert(ExecutableContent::HttpStep {
                    action: action.to_owned(),
                    kind_id,
                });
            }
            Some(SubActionCategory::Scripts) => {
                out.insert(ExecutableContent::ScriptStep {
                    action: action.to_owned(),
                    kind_id,
                });
            }
            _ => {}
        }
        for chain in nested_chains(step, registry) {
            collect_executable(action, &chain, registry, out);
        }
    }
}
