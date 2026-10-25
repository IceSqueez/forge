use async_trait::async_trait;
use forge_events::Event;
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, RegistryError, RunContext, SubActionCategory,
    SubActionIo, SubActionRunner, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    Action, ActionId, ArgStack, ExecutionMode, PermissionRung, PlatformId, PlatformScope, QueueId,
    SubActionConfig, SubActionStep, SubActionTelemetry, TriggerConfig, TriggerInstance,
    TriggerInstanceId, Variant,
};

pub struct FakeRunner {
    pub id: &'static str,
    pub fields: Vec<FormField>,
    pub io: SubActionIo,
    pub config_error: Option<forge_registry::ConfigProblem>,
    pub checks_while_editing: bool,
}

pub fn runner(id: &'static str) -> FakeRunner {
    FakeRunner {
        id,
        fields: Vec::new(),
        io: SubActionIo::default(),
        config_error: None,
        checks_while_editing: false,
    }
}

#[async_trait]
impl SubActionRunner for FakeRunner {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        "bolt"
    }

    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        self.fields.clone()
    }

    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        match &self.config_error {
            Some(problem) => Err(RegistryError::InvalidConfig(problem.clone())),
            None => Ok(()),
        }
    }

    fn checks_config_while_editing(&self) -> bool {
        self.checks_while_editing
    }

    fn scope_io(&self) -> SubActionIo {
        self.io.clone()
    }

    async fn execute(
        &self,
        _: &SubActionConfig,
        _: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        unreachable!("fake steps are inspected, never run")
    }
}

pub struct FakeTrigger {
    pub id: &'static str,
    pub variables: Option<fn() -> TriggerVariables>,
}

impl TriggerKindDescriptor for FakeTrigger {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn condition_display(&self, _: &TriggerConfig) -> String {
        String::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        self.variables.map(|build| build())
    }
}

pub fn step(kind_id: &str, config: SubActionConfig, enabled: bool) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config,
        enabled,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub fn encode_chain(steps: &[SubActionStep]) -> Variant {
    Variant::Array(
        steps
            .iter()
            .map(|nested| {
                Variant::Object(SubActionConfig::from([
                    (
                        "kind_id".to_owned(),
                        Variant::String(nested.kind_id.clone()),
                    ),
                    ("config".to_owned(), Variant::Object(nested.config.clone())),
                    ("enabled".to_owned(), Variant::Bool(nested.enabled)),
                ]))
            })
            .collect(),
    )
}

pub fn action(name: &str, steps: Vec<SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    }
}

pub fn trigger_of(kind_id: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: kind_id.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: false,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::default(),
    }
}
