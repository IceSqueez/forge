use std::collections::{BTreeSet, HashMap};

use forge_types::{IntegrationId, PlatformId};

use crate::descriptor::TriggerKindDescriptor;
use crate::error::RegistryError;
use crate::retired_ids::{IdSuccession, RetiredIds};
use crate::runner::SubActionRunner;

#[derive(Default)]
pub struct TriggerRegistry {
    descriptors: HashMap<String, Box<dyn TriggerKindDescriptor>>,
    owners: HashMap<String, IntegrationId>,
    retired: RetiredIds,
}

impl TriggerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, d: Box<dyn TriggerKindDescriptor>) -> Result<(), RegistryError> {
        let id = d.id().to_owned();
        if self.descriptors.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        self.descriptors.insert(id, d);
        Ok(())
    }

    pub fn owned_by(&mut self, owner: IntegrationId) -> OwnedTriggerRegistration<'_> {
        OwnedTriggerRegistration {
            registry: self,
            owner,
        }
    }

    pub fn get(&self, kind_id: &str) -> Option<&dyn TriggerKindDescriptor> {
        self.descriptors.get(kind_id).map(|b| b.as_ref())
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn TriggerKindDescriptor> {
        self.descriptors.values().map(|b| b.as_ref())
    }

    pub fn owning_integration(&self, kind_id: &str) -> Option<&IntegrationId> {
        self.owners.get(kind_id)
    }

    pub fn platforms(&self) -> BTreeSet<PlatformId> {
        self.all()
            .filter_map(|descriptor| descriptor.platform_contract().platform())
            .collect()
    }

    pub fn declare_retired(&mut self, succession: &IdSuccession) -> Result<(), RegistryError> {
        self.retired.declare(succession)
    }

    pub fn current_id(&self, retired_id: &str) -> Option<&str> {
        self.retired.current_id(retired_id)
    }

    pub fn retired_ids(&self) -> impl Iterator<Item = (&str, &str)> {
        self.retired.successions()
    }
}

pub struct OwnedTriggerRegistration<'a> {
    registry: &'a mut TriggerRegistry,
    owner: IntegrationId,
}

impl OwnedTriggerRegistration<'_> {
    pub fn register(&mut self, d: Box<dyn TriggerKindDescriptor>) -> Result<(), RegistryError> {
        let id = d.id().to_owned();
        self.registry.register(d)?;
        self.registry.owners.insert(id, self.owner.clone());
        Ok(())
    }

    pub fn declare_retired(&mut self, succession: &IdSuccession) -> Result<(), RegistryError> {
        self.registry.declare_retired(succession)
    }
}

#[derive(Default)]
pub struct SubActionRegistry {
    runners: HashMap<String, Box<dyn SubActionRunner>>,
    owners: HashMap<String, IntegrationId>,
    retired: RetiredIds,
}

impl SubActionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, r: Box<dyn SubActionRunner>) -> Result<(), RegistryError> {
        let id = r.id().to_owned();
        if self.runners.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        self.runners.insert(id, r);
        Ok(())
    }

    pub fn owned_by(&mut self, owner: IntegrationId) -> OwnedSubActionRegistration<'_> {
        OwnedSubActionRegistration {
            registry: self,
            owner,
        }
    }

    pub fn get(&self, kind_id: &str) -> Option<&dyn SubActionRunner> {
        self.runners.get(kind_id).map(|b| b.as_ref())
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn SubActionRunner> {
        self.runners.values().map(|b| b.as_ref())
    }

    pub fn owning_integration(&self, kind_id: &str) -> Option<&IntegrationId> {
        self.owners.get(kind_id)
    }

    pub fn declare_retired(&mut self, succession: &IdSuccession) -> Result<(), RegistryError> {
        self.retired.declare(succession)
    }

    pub fn current_id(&self, retired_id: &str) -> Option<&str> {
        self.retired.current_id(retired_id)
    }

    pub fn retired_ids(&self) -> impl Iterator<Item = (&str, &str)> {
        self.retired.successions()
    }
}

pub struct OwnedSubActionRegistration<'a> {
    registry: &'a mut SubActionRegistry,
    owner: IntegrationId,
}

impl OwnedSubActionRegistration<'_> {
    pub fn register(&mut self, r: Box<dyn SubActionRunner>) -> Result<(), RegistryError> {
        let id = r.id().to_owned();
        self.registry.register(r)?;
        self.registry.owners.insert(id, self.owner.clone());
        Ok(())
    }

    pub fn declare_retired(&mut self, succession: &IdSuccession) -> Result<(), RegistryError> {
        self.registry.declare_retired(succession)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::{SubActionCategory, TriggerCategory};
    use crate::evaluator::EventFilter;
    use crate::form::FormField;
    use crate::kind_platform_contract::KindPlatformContract;
    use crate::run_context::RunContext;
    use crate::runner::SubActionConfig;
    use async_trait::async_trait;
    use forge_events::Event;
    use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, TriggerConfig};
    use time::OffsetDateTime;

    struct StubDescriptor {
        kind_id: &'static str,
    }

    impl TriggerKindDescriptor for StubDescriptor {
        fn id(&self) -> &str {
            self.kind_id
        }
        fn category(&self) -> TriggerCategory {
            TriggerCategory::Core
        }
        fn label(&self) -> &str {
            "Stub"
        }
        fn summary(&self) -> &str {
            "stub"
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            "bolt"
        }
        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::Universal
        }
        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            vec![]
        }
        fn condition_display(&self, _config: &TriggerConfig) -> String {
            String::new()
        }
        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: None,
                kind_prefix: None,
            }
        }
        fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
            false
        }
        fn build_arg_stack(&self, _event: &Event) -> ArgStack {
            ArgStack::new()
        }
    }

    struct StubRunner {
        kind_id: &'static str,
    }

    #[async_trait]
    impl SubActionRunner for StubRunner {
        fn id(&self) -> &str {
            self.kind_id
        }
        fn category(&self) -> SubActionCategory {
            SubActionCategory::Util
        }
        fn label(&self) -> &str {
            "Stub"
        }
        fn summary(&self) -> &str {
            "stub"
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            "bolt"
        }
        fn default_config(&self) -> SubActionConfig {
            SubActionConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            vec![]
        }
        fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            _config: &SubActionConfig,
            _ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    index: 0,
                    kind: self.kind_id.to_owned(),
                    started_at: OffsetDateTime::now_utc(),
                    duration_ms: 0,
                    outcome: SubActionOutcome::Success,
                },
                None,
            )
        }
    }

    #[test]
    fn trigger_registry_register_and_get() {
        let mut reg = TriggerRegistry::new();
        reg.register(Box::new(StubDescriptor {
            kind_id: "core.test",
        }))
        .unwrap();
        assert!(reg.get("core.test").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[test]
    fn trigger_registry_duplicate_id_returns_error() {
        let mut reg = TriggerRegistry::new();
        reg.register(Box::new(StubDescriptor {
            kind_id: "core.test",
        }))
        .unwrap();
        let result = reg.register(Box::new(StubDescriptor {
            kind_id: "core.test",
        }));
        assert!(matches!(result, Err(RegistryError::DuplicateId(id)) if id == "core.test"));
    }

    #[test]
    fn trigger_registry_all_returns_registered_descriptors() {
        let mut reg = TriggerRegistry::new();
        reg.register(Box::new(StubDescriptor { kind_id: "a" }))
            .unwrap();
        reg.register(Box::new(StubDescriptor { kind_id: "b" }))
            .unwrap();
        assert_eq!(reg.all().count(), 2);
    }

    #[test]
    fn sub_action_registry_register_and_get() {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(StubRunner {
            kind_id: "core.log.write",
        }))
        .unwrap();
        assert!(reg.get("core.log.write").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[test]
    fn sub_action_registry_duplicate_id_returns_error() {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(StubRunner {
            kind_id: "core.delay",
        }))
        .unwrap();
        let result = reg.register(Box::new(StubRunner {
            kind_id: "core.delay",
        }));
        assert!(matches!(result, Err(RegistryError::DuplicateId(id)) if id == "core.delay"));
    }

    #[test]
    fn sub_action_registry_all_returns_registered_runners() {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(StubRunner { kind_id: "a" })).unwrap();
        reg.register(Box::new(StubRunner { kind_id: "b" })).unwrap();
        reg.register(Box::new(StubRunner { kind_id: "c" })).unwrap();
        assert_eq!(reg.all().count(), 3);
    }

    const SUCCESSOR: IdSuccession = IdSuccession {
        current: "obs.recording.start",
        retired: &["obs.record.start", "obs.rec.start"],
    };

    #[test]
    fn every_retired_id_of_a_declared_succession_resolves_to_its_current_id() {
        let mut triggers = TriggerRegistry::new();
        let mut sub_actions = SubActionRegistry::new();
        triggers.declare_retired(&SUCCESSOR).unwrap();
        sub_actions.declare_retired(&SUCCESSOR).unwrap();

        let ids = ["obs.record.start", "obs.rec.start", "obs.recording.start"];
        let current = Some("obs.recording.start");

        assert_eq!(
            (
                ids.map(|id| triggers.current_id(id)),
                ids.map(|id| sub_actions.current_id(id)),
            ),
            ([current, current, None], [current, current, None])
        );
    }

    #[test]
    fn a_succession_reusing_a_retired_id_is_rejected_whole() {
        let mut reg = SubActionRegistry::new();
        reg.declare_retired(&SUCCESSOR).unwrap();

        let result = reg.declare_retired(&IdSuccession {
            current: "obs.other",
            retired: &["obs.fresh", "obs.record.start"],
        });

        assert!(matches!(result, Err(RegistryError::DuplicateId(id)) if id == "obs.record.start"));
        assert_eq!(
            reg.retired_ids().collect::<Vec<_>>(),
            [
                ("obs.rec.start", "obs.recording.start"),
                ("obs.record.start", "obs.recording.start")
            ]
        );
    }

    #[test]
    fn retired_ids_declared_through_an_owned_registration_reach_the_registry() {
        let obs = forge_types::IntegrationId::from_static("obs");
        let mut triggers = TriggerRegistry::new();

        triggers.owned_by(obs).declare_retired(&SUCCESSOR).unwrap();

        assert_eq!(
            triggers.current_id("obs.record.start"),
            Some("obs.recording.start")
        );
    }

    #[test]
    fn a_rejected_duplicate_under_an_owner_leaves_the_original_ownership_untouched() {
        let twitch = forge_types::IntegrationId::from_static("twitch");
        let mut triggers = TriggerRegistry::new();
        triggers
            .register(Box::new(StubDescriptor { kind_id: "shared" }))
            .unwrap();
        let mut sub_actions = SubActionRegistry::new();
        sub_actions
            .register(Box::new(StubRunner { kind_id: "shared" }))
            .unwrap();

        let trigger_dup = triggers
            .owned_by(twitch.clone())
            .register(Box::new(StubDescriptor { kind_id: "shared" }));
        let runner_dup = sub_actions
            .owned_by(twitch)
            .register(Box::new(StubRunner { kind_id: "shared" }));

        assert!(trigger_dup.is_err());
        assert!(runner_dup.is_err());
        assert_eq!(triggers.owning_integration("shared"), None);
        assert_eq!(sub_actions.owning_integration("shared"), None);
    }
}
