use std::collections::HashMap;

use crate::descriptor::TriggerKindDescriptor;
use crate::error::RegistryError;
use crate::runner::SubActionRunner;

#[derive(Default)]
pub struct TriggerRegistry {
    descriptors: HashMap<String, Box<dyn TriggerKindDescriptor>>,
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

    pub fn get(&self, kind_id: &str) -> Option<&dyn TriggerKindDescriptor> {
        self.descriptors.get(kind_id).map(|b| b.as_ref())
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn TriggerKindDescriptor> {
        self.descriptors.values().map(|b| b.as_ref())
    }
}

#[derive(Default)]
pub struct SubActionRegistry {
    runners: HashMap<String, Box<dyn SubActionRunner>>,
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

    pub fn get(&self, kind_id: &str) -> Option<&dyn SubActionRunner> {
        self.runners.get(kind_id).map(|b| b.as_ref())
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn SubActionRunner> {
        self.runners.values().map(|b| b.as_ref())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    fn trigger_registry_get_returns_correct_id() {
        let mut reg = TriggerRegistry::new();
        reg.register(Box::new(StubDescriptor {
            kind_id: "twitch.chat.command",
        }))
        .unwrap();
        let d = reg.get("twitch.chat.command").unwrap();
        assert_eq!(d.id(), "twitch.chat.command");
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
    fn sub_action_registry_get_returns_correct_id() {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(StubRunner {
            kind_id: "core.globals.set",
        }))
        .unwrap();
        let r = reg.get("core.globals.set").unwrap();
        assert_eq!(r.id(), "core.globals.set");
    }
}
