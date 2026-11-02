use std::collections::{BTreeSet, HashMap};

use forge_events::Event;
use forge_types::{ArgStack, IntegrationId, PlatformId, TriggerConfig};

use crate::descriptor::TriggerKindDescriptor;
use crate::error::RegistryError;
use crate::runner::SubActionRunner;
use crate::variables::{DescriptorDeclaration, TriggerVariable};

struct RegisteredTrigger {
    descriptor: Box<dyn TriggerKindDescriptor>,
    declaration: DescriptorDeclaration,
}

#[derive(Default)]
pub struct TriggerRegistry {
    descriptors: HashMap<String, RegisteredTrigger>,
    owners: HashMap<String, IntegrationId>,
}

impl TriggerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        descriptor: Box<dyn TriggerKindDescriptor>,
    ) -> Result<(), RegistryError> {
        let id = descriptor.id().to_owned();
        if self.descriptors.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        let declaration = DescriptorDeclaration::of(descriptor.as_ref());
        self.descriptors.insert(
            id,
            RegisteredTrigger {
                descriptor,
                declaration,
            },
        );
        Ok(())
    }

    pub fn owned_by(&mut self, owner: IntegrationId) -> OwnedTriggerRegistration<'_> {
        OwnedTriggerRegistration {
            registry: self,
            owner,
        }
    }

    pub fn get(&self, kind_id: &str) -> Option<&dyn TriggerKindDescriptor> {
        self.descriptors
            .get(kind_id)
            .map(|registered| registered.descriptor.as_ref())
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn TriggerKindDescriptor> {
        self.descriptors
            .values()
            .map(|registered| registered.descriptor.as_ref())
    }

    pub fn declared_variables(&self, kind_id: &str) -> Option<&[TriggerVariable]> {
        self.descriptors.get(kind_id)?.declaration.listed()
    }

    pub fn arg_stack(&self, kind_id: &str, event: &Event, matched: &TriggerConfig) -> ArgStack {
        let Some(registered) = self.descriptors.get(kind_id) else {
            return ArgStack::new();
        };
        match registered.declaration.variables() {
            Some(variables) => variables.arg_stack_of_match(event, matched),
            None => registered.descriptor.build_arg_stack(event),
        }
    }

    pub fn owning_integration(&self, kind_id: &str) -> Option<&IntegrationId> {
        self.owners.get(kind_id)
    }

    pub fn with_chat_event(&self, kind_id: &str, args: ArgStack) -> ArgStack {
        match self
            .get(kind_id)
            .and_then(|descriptor| descriptor.chat_event())
        {
            Some(event) => event.marked(args),
            None => args,
        }
    }

    pub fn platforms(&self) -> BTreeSet<PlatformId> {
        self.all()
            .filter_map(|descriptor| descriptor.platform_contract().platform())
            .collect()
    }
}

pub struct OwnedTriggerRegistration<'a> {
    registry: &'a mut TriggerRegistry,
    owner: IntegrationId,
}

impl OwnedTriggerRegistration<'_> {
    pub fn register(
        &mut self,
        descriptor: Box<dyn TriggerKindDescriptor>,
    ) -> Result<(), RegistryError> {
        let id = descriptor.id().to_owned();
        self.registry.register(descriptor)?;
        self.registry.owners.insert(id, self.owner.clone());
        Ok(())
    }
}

#[derive(Default)]
pub struct SubActionRegistry {
    runners: HashMap<String, Box<dyn SubActionRunner>>,
    owners: HashMap<String, IntegrationId>,
}

impl SubActionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, runner: Box<dyn SubActionRunner>) -> Result<(), RegistryError> {
        let id = runner.id().to_owned();
        if self.runners.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        self.runners.insert(id, runner);
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
}

pub struct OwnedSubActionRegistration<'a> {
    registry: &'a mut SubActionRegistry,
    owner: IntegrationId,
}

impl OwnedSubActionRegistration<'_> {
    pub fn register(&mut self, runner: Box<dyn SubActionRunner>) -> Result<(), RegistryError> {
        let id = runner.id().to_owned();
        self.registry.register(runner)?;
        self.registry.owners.insert(id, self.owner.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::{SubActionCategory, TriggerCategory};
    use crate::chat_event::ChatEventKind;
    use crate::descriptor::TriggerCatalogEntry;
    use crate::event_filter::EventFilter;
    use crate::form::FormField;
    use crate::kind_platform_contract::KindPlatformContract;
    use crate::run_context::RunContext;
    use crate::runner::StepCatalogEntry;
    use crate::runner::SubActionConfig;
    use async_trait::async_trait;
    use forge_events::Event;
    use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
    use time::OffsetDateTime;

    struct StubDescriptor {
        kind_id: &'static str,
    }

    impl TriggerCatalogEntry for StubDescriptor {
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
    }

    impl TriggerKindDescriptor for StubDescriptor {
        fn id(&self) -> &str {
            self.kind_id
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

    struct ContractStub {
        kind_id: &'static str,
        contract: KindPlatformContract,
        chat_event: Option<ChatEventKind>,
    }

    impl TriggerCatalogEntry for ContractStub {
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
    }

    impl TriggerKindDescriptor for ContractStub {
        fn id(&self) -> &str {
            self.kind_id
        }
        fn platform_contract(&self) -> KindPlatformContract {
            self.contract
        }
        fn chat_event(&self) -> Option<ChatEventKind> {
            self.chat_event
        }
        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            vec![]
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

    impl StepCatalogEntry for StubRunner {
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
    }

    #[async_trait]
    impl SubActionRunner for StubRunner {
        fn id(&self) -> &str {
            self.kind_id
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
                    position: forge_types::StepPosition::TopLevel(0),
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

    fn registry_of(contracts: &[(&'static str, KindPlatformContract)]) -> TriggerRegistry {
        let mut reg = TriggerRegistry::new();
        for (kind_id, contract) in contracts {
            reg.register(Box::new(ContractStub {
                kind_id,
                contract: *contract,
                chat_event: None,
            }))
            .unwrap();
        }
        reg
    }

    fn chat_event_registry() -> TriggerRegistry {
        let mut reg = TriggerRegistry::new();
        for (kind_id, chat_event) in [
            ("twitch.raid", Some(ChatEventKind::Raid)),
            ("twitch.chat", None),
        ] {
            reg.register(Box::new(ContractStub {
                kind_id,
                contract: KindPlatformContract::Universal,
                chat_event,
            }))
            .unwrap();
        }
        reg
    }

    fn user_args() -> ArgStack {
        ArgStack::new().set(
            "user".to_owned(),
            forge_types::Variant::String("ann".to_owned()),
        )
    }

    #[test]
    fn with_chat_event_marks_a_run_of_an_event_trigger_with_its_kind() {
        let args = chat_event_registry().with_chat_event("twitch.raid", user_args());

        assert_eq!(ChatEventKind::of_run(&args), Some(ChatEventKind::Raid));
    }

    #[test]
    fn with_chat_event_keeps_the_trigger_arguments_it_marks() {
        let args = chat_event_registry().with_chat_event("twitch.raid", user_args());

        assert_eq!(
            args.get("user"),
            Some(&forge_types::Variant::String("ann".to_owned()))
        );
    }

    #[test]
    fn with_chat_event_leaves_runs_of_plain_and_unknown_triggers_untouched() {
        let reg = chat_event_registry();
        for kind_id in ["twitch.chat", "twitch.unregistered"] {
            let args = reg.with_chat_event(kind_id, user_args());
            assert_eq!(args.snapshot(), user_args().snapshot(), "{kind_id}");
        }
    }

    #[test]
    fn platforms_collects_platform_specific_contracts_deduplicated_and_ordered() {
        use PlatformId::{Kick, Twitch, YouTube};
        let reg = registry_of(&[
            ("kick.chat", KindPlatformContract::PlatformSpecific(Kick)),
            (
                "twitch.chat",
                KindPlatformContract::PlatformSpecific(Twitch),
            ),
            (
                "twitch.raid",
                KindPlatformContract::PlatformSpecific(Twitch),
            ),
            (
                "youtube.chat",
                KindPlatformContract::PlatformSpecific(YouTube),
            ),
        ]);

        let platforms: Vec<PlatformId> = reg.platforms().into_iter().collect();

        assert_eq!(platforms, vec![Twitch, YouTube, Kick]);
    }

    #[test]
    fn platforms_ignores_universal_contracts() {
        let reg = registry_of(&[
            ("core.a", KindPlatformContract::Universal),
            (
                "twitch.chat",
                KindPlatformContract::PlatformSpecific(PlatformId::Twitch),
            ),
        ]);

        assert_eq!(
            reg.platforms().into_iter().collect::<Vec<_>>(),
            vec![PlatformId::Twitch]
        );
        assert!(
            registry_of(&[("core.a", KindPlatformContract::Universal)])
                .platforms()
                .is_empty()
        );
        assert!(TriggerRegistry::new().platforms().is_empty());
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
