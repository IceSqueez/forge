use std::collections::{BTreeMap, BTreeSet};

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_registry::references::{
    LOCAL_FILE_NAMESPACE, OVERLAY_ICON_OPTIONS_KEY, OVERLAY_SOUND_OPTIONS_KEY,
};
use forge_registry::{
    ConfigReference, FormField, FormSchemaSource, InternalNamespace, ReferenceCatalog,
    ReferenceListing, ReferenceNamespace, ReferenceRemap, ReferenceScope, RegistryError,
    SubActionRegistry, TriggerRegistry,
};
use forge_types::{Action, ActionId, ExecutionMode, QueueId, SubActionStep, Variant};

use crate::integrations::tests::every_registry_entry;

type Config = BTreeMap<String, Variant>;

const EXTERNAL_OPTIONS_KEYS: &[&str] = &[
    "collections.twitch.rewards",
    "collections.twitch.rewards.manageable",
    "donations.provider_ids",
    "obs.audio_input_names",
    "obs.audio_inputs",
    "obs.input_names",
    "obs.scene_names",
    "obs.source_names",
    "tts.engine_ids",
    "tts.engine_voices",
    "tts.voices",
    "vtube.expression_files",
    "vtube.hotkey_ids",
    "vtube.model_ids",
];

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn config(pairs: &[(&str, Variant)]) -> Config {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn step(kind_id: &str, step_config: Config) -> Variant {
    Variant::Object(config(&[
        ("kind_id", text(kind_id)),
        ("config", Variant::Object(step_config)),
    ]))
}

fn chain(steps: Vec<Variant>) -> Variant {
    Variant::Array(steps)
}

fn action_step(kind_id: &str, step_config: Config) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: step_config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn action(queue_id: QueueId, steps: Vec<SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: "Packed".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions: steps,
    }
}

fn internal(namespace: InternalNamespace) -> ReferenceNamespace {
    ReferenceNamespace::Internal(namespace)
}

fn external(options_key: &str) -> ReferenceNamespace {
    ReferenceNamespace::for_options_key(options_key)
}

fn summary(reference: &ConfigReference) -> (ReferenceNamespace, String, String) {
    (
        reference.namespace.clone(),
        reference.value.clone(),
        reference.location.to_string(),
    )
}

fn summaries(listing: &ReferenceListing) -> Vec<(ReferenceNamespace, String, String)> {
    listing.references.iter().map(summary).collect()
}

fn listed(
    namespace: ReferenceNamespace,
    value: &str,
    location: &str,
) -> (ReferenceNamespace, String, String) {
    (namespace, value.to_owned(), location.to_owned())
}

fn overlay_kinds() -> OverlayKindRegistry {
    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).unwrap();
    kinds
}

fn alert_fields() -> Vec<FormField> {
    overlay_kinds()
        .get("overlay.alert")
        .unwrap()
        .config_fields()
        .into_iter()
        .map(|sectioned| sectioned.field)
        .collect()
}

fn nested_steps() -> Config {
    nested_steps_naming("action-deep", "action-other")
}

fn nested_steps_naming(deep: &str, other: &str) -> Config {
    config(&[
        ("condition", text("true")),
        (
            "then_chain",
            chain(vec![step(
                "core.logic.switch_case",
                config(&[
                    ("expression", text("x")),
                    (
                        "cases",
                        Variant::Array(vec![
                            Variant::Object(config(&[("chain", chain(Vec::new()))])),
                            Variant::Object(config(&[(
                                "chain",
                                chain(vec![step(
                                    "core.action.run",
                                    config(&[("action_id", text(deep))]),
                                )]),
                            )])),
                        ]),
                    ),
                    (
                        "default_chain",
                        chain(vec![step(
                            "core.queue.pause",
                            config(&[("queue_id", text("queue-default"))]),
                        )]),
                    ),
                ]),
            )]),
        ),
        (
            "else_chain",
            chain(vec![
                step(
                    "obs.scenes.set_preview",
                    config(&[("scene", text("action-deep"))]),
                ),
                step("core.action.run", config(&[("action_id", text(other))])),
            ]),
        ),
    ])
}

fn collect_select_keys(field: &FormField, owner: &str, out: &mut BTreeSet<(String, String)>) {
    match field {
        FormField::DynamicSelect {
            key, options_key, ..
        } => {
            out.insert((format!("{owner}.{key}"), (*options_key).to_owned()));
        }
        FormField::DependentSelect {
            key,
            options_prefix,
            ..
        } => {
            out.insert((format!("{owner}.{key}"), (*options_prefix).to_owned()));
        }
        FormField::Optional { inner, .. } => collect_select_keys(inner, owner, out),
        _ => {}
    }
}

fn every_select_namespace(
    triggers: &TriggerRegistry,
    sub_actions: &SubActionRegistry,
) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    for runner in sub_actions.all() {
        for field in runner.config_fields() {
            collect_select_keys(&field, runner.id(), &mut found);
        }
        for declared in runner.declared_references() {
            found.insert((
                format!("{}.{}", runner.id(), declared.key),
                declared.options_key.to_owned(),
            ));
        }
    }
    for descriptor in triggers.all() {
        for field in descriptor.config_fields() {
            collect_select_keys(&field, descriptor.id(), &mut found);
        }
        for declared in descriptor.declared_references() {
            found.insert((
                format!("{}.{}", descriptor.id(), declared.key),
                declared.options_key.to_owned(),
            ));
        }
    }
    for kind in overlay_kinds().all() {
        for sectioned in kind.config_fields() {
            collect_select_keys(&sectioned.field, kind.id(), &mut found);
        }
    }
    found
}

fn is_classified(options_key: &str) -> bool {
    InternalNamespace::from_options_key(options_key).is_some()
        || options_key == OVERLAY_SOUND_OPTIONS_KEY
        || options_key == OVERLAY_ICON_OPTIONS_KEY
        || EXTERNAL_OPTIONS_KEYS.contains(&options_key)
}

#[tokio::test]
async fn every_registered_select_field_names_an_internal_or_a_reviewed_external_namespace() {
    let (triggers, sub_actions) = every_registry_entry().await;

    let unclassified: Vec<_> = every_select_namespace(&triggers, &sub_actions)
        .into_iter()
        .filter(|(_, options_key)| !is_classified(options_key))
        .collect();

    assert!(
        unclassified.is_empty(),
        "classify each as an internal namespace (carried and remapped in packs) or add it to \
         EXTERNAL_OPTIONS_KEYS after confirming it names integration-owned data: {unclassified:#?}"
    );
}

#[tokio::test]
async fn a_step_referencing_each_internal_entity_lists_it_in_its_namespace_at_its_key() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);

    for (kind_id, key, namespace) in [
        ("core.action.run", "action_id", InternalNamespace::Action),
        (
            "core.trigger.enable",
            "trigger_instance_id",
            InternalNamespace::TriggerInstance,
        ),
        ("core.queue.pause", "queue_id", InternalNamespace::Queue),
        ("script.run.named", "script_name", InternalNamespace::Script),
        ("core.globals.get", "name", InternalNamespace::Global),
        ("core.globals.set", "name", InternalNamespace::Global),
        (
            "soundboard.sound.play",
            "clip_id",
            InternalNamespace::SoundboardClip,
        ),
        (
            forge_runtime::OVERLAY_SEND_KIND_ID,
            forge_runtime::OVERLAY_TARGET_KEY,
            InternalNamespace::Overlay,
        ),
    ] {
        let listing = catalog.sub_action_references(kind_id, &config(&[(key, text("entity-1"))]));

        assert_eq!(
            summaries(&listing),
            vec![listed(internal(namespace), "entity-1", key)],
            "{kind_id}"
        );
    }
}

#[tokio::test]
async fn an_overlay_names_its_clip_and_managed_image_by_prefix_and_its_local_sound_as_external() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let fields = alert_fields();

    for (key, raw, expected) in [
        (
            "sound",
            "clip:clip-7",
            vec![listed(
                internal(InternalNamespace::SoundboardClip),
                "clip-7",
                "sound",
            )],
        ),
        (
            "sound",
            "/home/streamer/ding.ogg",
            vec![listed(
                external(LOCAL_FILE_NAMESPACE),
                "/home/streamer/ding.ogg",
                "sound",
            )],
        ),
        (
            "icon",
            "image:blob-3",
            vec![listed(
                internal(InternalNamespace::ManagedMedia),
                "blob-3",
                "icon",
            )],
        ),
        ("icon", "star-filled", Vec::new()),
        ("icon", "image:", Vec::new()),
        ("sound", "clip:", Vec::new()),
        ("sound", "", Vec::new()),
    ] {
        let listing = catalog.field_references(&fields, &config(&[(key, text(raw))]));

        assert_eq!(summaries(&listing), expected, "{key} = {raw:?}");
    }
}

#[tokio::test]
async fn integration_owned_names_are_listed_as_external_references() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);

    let scene = catalog.sub_action_references(
        "obs.scenes.set_preview",
        &config(&[("scene", text("Live"))]),
    );
    let reward = catalog.trigger_references(
        "twitch.channel_points.redemption",
        &config(&[("reward_id", text("reward-9"))]),
    );

    for (listing, options_key, value, key) in [
        (&scene, "obs.scene_names", "Live", "scene"),
        (
            &reward,
            "collections.twitch.rewards",
            "reward-9",
            "reward_id",
        ),
    ] {
        assert_eq!(
            summaries(listing),
            vec![listed(external(options_key), value, key)]
        );
        assert_eq!(
            listing.references[0].namespace.scope(),
            ReferenceScope::External
        );
    }
}

#[tokio::test]
async fn references_inside_sub_chains_and_switch_cases_are_listed_at_their_full_path() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);

    let listing = catalog.sub_action_references("core.logic.if_then_else", &nested_steps());

    assert_eq!(
        summaries(&listing),
        vec![
            listed(
                internal(InternalNamespace::Action),
                "action-deep",
                "then_chain[0].config.cases[1].chain[0].config.action_id",
            ),
            listed(
                internal(InternalNamespace::Queue),
                "queue-default",
                "then_chain[0].config.default_chain[0].config.queue_id",
            ),
            listed(
                external("obs.scene_names"),
                "action-deep",
                "else_chain[0].config.scene",
            ),
            listed(
                internal(InternalNamespace::Action),
                "action-other",
                "else_chain[1].config.action_id",
            ),
        ]
    );
}

#[tokio::test]
async fn a_nested_step_of_an_unregistered_kind_is_reported_while_its_siblings_are_still_listed() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let steps = config(&[(
        "then_chain",
        chain(vec![
            step("plugin.removed", config(&[("action_id", text("hidden"))])),
            step("core.action.run", config(&[("action_id", text("visible"))])),
        ]),
    )]);

    let listing = catalog.sub_action_references("core.logic.if_then_else", &steps);

    let unknown: Vec<_> = listing
        .unknown_kinds
        .iter()
        .map(|unknown| (unknown.kind_id.as_str(), unknown.location.to_string()))
        .collect();
    assert_eq!(
        unknown,
        vec![("plugin.removed", "then_chain[0].config".to_owned())]
    );
    assert_eq!(
        summaries(&listing),
        vec![listed(
            internal(InternalNamespace::Action),
            "visible",
            "then_chain[1].config.action_id",
        )]
    );
}

#[tokio::test]
async fn an_unregistered_top_level_step_or_trigger_kind_is_reported_at_the_root() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let any = config(&[("action_id", text("a"))]);

    for listing in [
        catalog.sub_action_references("plugin.removed", &any),
        catalog.trigger_references("plugin.removed", &any),
    ] {
        assert!(listing.references.is_empty());
        assert_eq!(listing.unknown_kinds.len(), 1);
        assert_eq!(listing.unknown_kinds[0].kind_id, "plugin.removed");
        assert!(listing.unknown_kinds[0].location.segments().is_empty());
    }
}

#[tokio::test]
async fn a_value_built_from_variables_is_flagged_templated_and_a_plain_one_is_not() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);

    for (raw, templated) in [
        ("%target_action%", true),
        ("prefix-%suffix%", true),
        ("action-1", false),
    ] {
        let listing =
            catalog.sub_action_references("core.action.run", &config(&[("action_id", text(raw))]));

        assert_eq!(listing.references[0].templated, templated, "{raw:?}");
    }
}

#[tokio::test]
async fn rewrite_replaces_only_the_mapped_internal_values_and_leaves_everything_else_identical() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut remap = ReferenceRemap::default();
    remap.insert(InternalNamespace::Action, "action-deep", "action-new");
    let mut rewritten = nested_steps();

    let count =
        catalog.rewrite_sub_action_config("core.logic.if_then_else", &mut rewritten, &remap);

    assert_eq!(
        (count, rewritten),
        (1, nested_steps_naming("action-new", "action-other"))
    );
}

#[tokio::test]
async fn swapping_two_ids_rewrites_each_reference_once_from_its_original_value() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut remap = ReferenceRemap::default();
    remap.insert(InternalNamespace::Action, "action-deep", "action-other");
    remap.insert(InternalNamespace::Action, "action-other", "action-deep");
    let mut steps = nested_steps();

    catalog.rewrite_sub_action_config("core.logic.if_then_else", &mut steps, &remap);

    assert_eq!(steps, nested_steps_naming("action-other", "action-deep"));
}

#[tokio::test]
async fn rewrite_never_touches_a_templated_value_even_when_the_raw_text_is_mapped() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut remap = ReferenceRemap::default();
    remap.insert(InternalNamespace::Action, "%target%", "action-new");
    let original = config(&[("action_id", text("%target%"))]);
    let mut rewritten = original.clone();

    let count = catalog.rewrite_sub_action_config("core.action.run", &mut rewritten, &remap);

    assert_eq!((count, rewritten), (0, original));
}

#[tokio::test]
async fn rewrite_keeps_the_clip_and_image_prefixes_of_an_overlay_reference() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut remap = ReferenceRemap::default();
    remap.insert(InternalNamespace::SoundboardClip, "clip-old", "clip-new");
    remap.insert(InternalNamespace::ManagedMedia, "blob-old", "blob-new");
    let mut overlay = config(&[
        ("sound", text("clip:clip-old")),
        ("icon", text("image:blob-old")),
    ]);

    let count = catalog.rewrite_field_config(&alert_fields(), &mut overlay, &remap);

    assert_eq!(count, 2);
    assert_eq!(
        overlay,
        config(&[
            ("sound", text("clip:clip-new")),
            ("icon", text("image:blob-new")),
        ])
    );
}

#[tokio::test]
async fn rewrite_leaves_a_local_sound_path_alone_even_when_a_clip_with_that_id_is_remapped() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut remap = ReferenceRemap::default();
    remap.insert(InternalNamespace::SoundboardClip, "ding.ogg", "clip-new");
    let original = config(&[("sound", text("ding.ogg"))]);
    let mut overlay = original.clone();

    catalog.rewrite_field_config(&alert_fields(), &mut overlay, &remap);

    assert_eq!(overlay, original);
}

#[tokio::test]
async fn a_global_named_by_a_set_step_is_renamed_by_rewrite() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut remap = ReferenceRemap::default();
    remap.insert(InternalNamespace::Global, "score", "score (2)");
    let mut set = config(&[("name", text("score")), ("value", text("score"))]);

    catalog.rewrite_sub_action_config("core.globals.set", &mut set, &remap);

    assert_eq!(
        set,
        config(&[("name", text("score (2)")), ("value", text("score"))])
    );
}

#[tokio::test]
async fn an_action_lists_its_queue_and_its_steps_under_their_paths_in_the_action() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let queue = QueueId::new();
    let packed = action(
        queue,
        vec![
            action_step("core.globals.get", config(&[("name", text("score"))])),
            action_step(
                "core.action.run",
                config(&[("action_id", text("action-1"))]),
            ),
        ],
    );

    let listing = catalog.action_references(&packed);

    assert_eq!(
        summaries(&listing),
        vec![
            listed(
                internal(InternalNamespace::Queue),
                &queue.to_string(),
                "queue_id"
            ),
            listed(
                internal(InternalNamespace::Global),
                "score",
                "sub_actions[0].config.name"
            ),
            listed(
                internal(InternalNamespace::Action),
                "action-1",
                "sub_actions[1].config.action_id"
            ),
        ]
    );
}

#[tokio::test]
async fn an_action_listed_after_rewrite_names_exactly_the_remapped_entities() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let (old_queue, new_queue) = (QueueId::new(), QueueId::new());
    let mut packed = action(
        old_queue,
        vec![
            action_step("core.logic.if_then_else", nested_steps()),
            action_step(
                "script.run.named",
                config(&[("script_name", text("intro"))]),
            ),
        ],
    );
    let mut remap = ReferenceRemap::default();
    remap.insert(
        InternalNamespace::Queue,
        old_queue.to_string(),
        new_queue.to_string(),
    );
    remap.insert(InternalNamespace::Action, "action-deep", "action-deep-2");
    remap.insert(InternalNamespace::Queue, "queue-default", "queue-default-2");
    remap.insert(InternalNamespace::Script, "intro", "intro (2)");
    let before = catalog.action_references(&packed);

    let count = catalog.rewrite_action(&mut packed, &remap).unwrap();

    let after = catalog.action_references(&packed);
    let expected: Vec<_> = before
        .references
        .iter()
        .map(|reference| {
            let value = reference
                .namespace
                .internal()
                .and_then(|namespace| remap.get(namespace, &reference.value))
                .unwrap_or(&reference.value);
            listed(
                reference.namespace.clone(),
                value,
                &reference.location.to_string(),
            )
        })
        .collect();
    assert_eq!(count, 4);
    assert_eq!(summaries(&after), expected);
}

#[tokio::test]
async fn a_queue_remapped_to_a_non_id_is_refused_and_the_action_is_left_unchanged() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let queue = QueueId::new();
    let original = action(
        queue,
        vec![action_step(
            "core.action.run",
            config(&[("action_id", text("action-1"))]),
        )],
    );
    let mut packed = original.clone();
    let mut remap = ReferenceRemap::default();
    remap.insert(
        InternalNamespace::Queue,
        queue.to_string(),
        "not-a-queue-id",
    );
    remap.insert(InternalNamespace::Action, "action-1", "action-2");

    let result = catalog.rewrite_action(&mut packed, &remap);

    assert!(matches!(
        result,
        Err(RegistryError::InvalidRemap { ref namespace, ref value })
            if namespace == "queue.ids" && value == "not-a-queue-id"
    ));
    assert_eq!(packed, original);
}

#[tokio::test]
async fn a_trigger_instance_lists_its_override_references_under_overrides() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let catalog = ReferenceCatalog::new(&sub_actions, &triggers);
    let mut trigger = crate::test_support::trigger_of("obs.scenes.current_changed");
    trigger.overrides = config(&[("scene", text("Live"))]);

    let listing = catalog.trigger_instance_references(&trigger);

    assert_eq!(
        summaries(&listing),
        vec![listed(
            external("obs.scene_names"),
            "Live",
            "overrides.scene"
        )]
    );
}

struct OneActionField;

impl FormSchemaSource for OneActionField {
    fn fields_for(&self, _schema_key: &str, _selector_value: &str) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "follow_up",
            label: "Follow-up",
            options_key: "action.ids",
        }]
    }
}

#[tokio::test]
async fn fields_refined_by_a_schema_are_walked_only_when_the_schema_is_supplied() {
    let (triggers, sub_actions) = every_registry_entry().await;
    let send = config(&[
        (forge_runtime::OVERLAY_TARGET_KEY, text("overlay-1")),
        ("follow_up", text("action-1")),
    ]);

    let plain = ReferenceCatalog::new(&sub_actions, &triggers);
    let refined = ReferenceCatalog::new(&sub_actions, &triggers).with_schema(&OneActionField);

    assert_eq!(
        summaries(&plain.sub_action_references(forge_runtime::OVERLAY_SEND_KIND_ID, &send)),
        vec![listed(
            internal(InternalNamespace::Overlay),
            "overlay-1",
            forge_runtime::OVERLAY_TARGET_KEY
        )]
    );
    assert_eq!(
        summaries(&refined.sub_action_references(forge_runtime::OVERLAY_SEND_KIND_ID, &send)),
        vec![
            listed(
                internal(InternalNamespace::Overlay),
                "overlay-1",
                forge_runtime::OVERLAY_TARGET_KEY
            ),
            listed(internal(InternalNamespace::Action), "action-1", "follow_up"),
        ]
    );
}
