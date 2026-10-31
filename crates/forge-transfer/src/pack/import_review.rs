use std::collections::BTreeSet;

use forge_registry::{StepNode, StepReach, SubActionCategory, SubActionRegistry, walk_steps};
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
    walk_steps(steps, registry, StepReach::All, &mut |node| {
        let StepNode::Step(step) = node else {
            return;
        };
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
    });
}

#[cfg(test)]
mod tests {
    use forge_types::QueueId;

    use super::*;
    use crate::documents::model::{
        ActionDocument, ClipDocument, OverlayDocument, OverlayFileDocument, ScriptDocument,
        TriggerDocument,
    };
    use crate::documents::reference::PackReference;
    use crate::test_support::{
        FixtureRegistries, HTTP_STEP, OWNED_TRIGGER, PLAIN_STEP, PLAIN_TRIGGER, SCRIPT_STEP,
        STEP_OWNER, TRIGGER_OWNER, action, blank_manifest, clip, no_documents, overlay, script,
        staged, step, trigger,
    };

    const UNKNOWN_KIND: &str = "fixture.unknown";

    fn action_document(name: &str, steps: Vec<SubActionStep>) -> ActionDocument {
        ActionDocument::of(&action(name, QueueId::new(), steps), Vec::new())
    }

    #[test]
    fn only_integrations_no_registered_kind_belongs_to_are_missing() {
        let mut manifest = blank_manifest();
        manifest.required_integrations = vec![
            IntegrationId::new(STEP_OWNER),
            IntegrationId::new(TRIGGER_OWNER),
            IntegrationId::new("absent"),
        ];
        let registries = FixtureRegistries::new();
        let review = review_pack(&staged(manifest, no_documents()), registries.closure());
        assert_eq!(
            review.warnings,
            [PackWarning::MissingIntegration {
                integration: IntegrationId::new("absent")
            }]
        );
    }

    #[test]
    fn unknown_kinds_from_the_manifest_steps_and_triggers_merge_into_one_warning_each() {
        let mut manifest = blank_manifest();
        manifest.unknown_kinds = vec![UNKNOWN_KIND.to_owned(), "fixture.manifest_only".to_owned()];
        let mut documents = no_documents();
        documents.actions.push(action_document(
            "Acts",
            vec![step(UNKNOWN_KIND, &[]), step(PLAIN_STEP, &[])],
        ));
        documents.triggers.push(TriggerDocument::of(&trigger(
            "fixture.unknown_trigger",
            "Trigger",
            false,
        )));
        documents
            .triggers
            .push(TriggerDocument::of(&trigger(OWNED_TRIGGER, "Known", false)));
        documents
            .triggers
            .push(TriggerDocument::of(&trigger(PLAIN_TRIGGER, "Known", false)));
        let registries = FixtureRegistries::new();
        let review = review_pack(&staged(manifest, documents), registries.closure());
        assert_eq!(
            review.warnings,
            [
                "fixture.manifest_only",
                UNKNOWN_KIND,
                "fixture.unknown_trigger"
            ]
            .map(|kind_id| PackWarning::UnknownKind {
                kind_id: kind_id.to_owned()
            })
        );
    }

    #[test]
    fn manifest_references_and_soundless_clips_become_warnings() {
        let unresolved = PackReference {
            namespace: "script".to_owned(),
            value: "gone".to_owned(),
        };
        let external = PackReference {
            namespace: "scene".to_owned(),
            value: "Live".to_owned(),
        };
        let mut manifest = blank_manifest();
        manifest.unresolved_references = vec![unresolved.clone()];
        manifest.external_references = vec![external.clone()];
        let mut documents = no_documents();
        documents.clips.push(ClipDocument::of(
            &clip("Silent"),
            ClipSourceDocument::Missing,
        ));
        documents.clips.push(ClipDocument::of(
            &clip("Sounding"),
            ClipSourceDocument::Media {
                media_id: "blob".to_owned(),
            },
        ));
        let registries = FixtureRegistries::new();
        let review = review_pack(&staged(manifest, documents), registries.closure());
        assert_eq!(
            review.warnings,
            [
                PackWarning::UnresolvedReference {
                    reference: unresolved
                },
                PackWarning::ExternalReference {
                    reference: external
                },
                PackWarning::ClipWithoutSound {
                    clip: "Silent".to_owned()
                },
            ]
        );
    }

    #[test]
    fn the_review_lists_everything_that_would_run_and_only_that() {
        let mut documents = no_documents();
        documents.actions.push(action_document(
            "Acts",
            vec![
                step(HTTP_STEP, &[]),
                step(SCRIPT_STEP, &[]),
                step(PLAIN_STEP, &[]),
                step(UNKNOWN_KIND, &[]),
            ],
        ));
        documents.scripts.push(ScriptDocument::of(&script("greet")));
        documents.overlays.push(OverlayDocument::of(
            &overlay("pack-overlay", "Alert box"),
            vec![OverlayFileDocument {
                name: "custom.css".to_owned(),
                file: "overlays/custom.css".to_owned(),
            }],
        ));
        let registries = FixtureRegistries::new();
        let review = review_pack(&staged(blank_manifest(), documents), registries.closure());
        assert_eq!(
            review.executable,
            [
                ExecutableContent::Script {
                    name: "greet".to_owned()
                },
                ExecutableContent::HttpStep {
                    action: "Acts".to_owned(),
                    kind_id: HTTP_STEP.to_owned()
                },
                ExecutableContent::ScriptStep {
                    action: "Acts".to_owned(),
                    kind_id: SCRIPT_STEP.to_owned()
                },
                ExecutableContent::OverlaySource {
                    overlay: "Alert box".to_owned(),
                    file: "custom.css".to_owned()
                },
            ]
        );
    }

    #[test]
    fn a_clean_pack_reviews_to_nothing() {
        let registries = FixtureRegistries::new();
        let review = review_pack(
            &staged(blank_manifest(), no_documents()),
            registries.closure(),
        );
        assert_eq!((review.warnings, review.executable), (vec![], vec![]));
    }
}
