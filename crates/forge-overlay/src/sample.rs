use std::collections::BTreeMap;

use forge_registry::{
    CanonicalVariable, KindPlatformContract, SynthesisSample, TriggerVariable, synthesize_args,
};
use forge_types::{ArgStack, NamesLanguage, Variant};

use crate::config::SPEECH;
use crate::content::delivered_content;
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::latest::{latest_binding, latest_content, sample_latest_value};

#[derive(Debug)]
pub struct SampleTrigger {
    pub kind_id: String,
    pub contract: KindPlatformContract,
    pub variables: Vec<TriggerVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleContext {
    values: BTreeMap<String, Variant>,
    names_language: NamesLanguage,
}

impl SampleContext {
    pub fn neutral() -> Self {
        let vocabulary: Vec<TriggerVariable> = CanonicalVariable::all()
            .into_iter()
            .map(TriggerVariable::canonical)
            .collect();
        Self::from_args(&synthesize_args(
            &vocabulary,
            &SynthesisSample::stable(KindPlatformContract::Universal),
        ))
    }

    fn from_args(args: &ArgStack) -> Self {
        Self {
            values: args.snapshot(),
            names_language: NamesLanguage::default(),
        }
    }

    pub fn with_names_language(mut self, names_language: NamesLanguage) -> Self {
        self.names_language = names_language;
        self
    }

    pub fn args(&self) -> ArgStack {
        self.values.iter().fold(
            ArgStack::new().with_names_language(self.names_language),
            |stack, (name, value)| stack.set(name.clone(), value.clone()),
        )
    }
}

pub fn sample_context(feeding: &[SampleTrigger]) -> SampleContext {
    match distinct_kind(feeding) {
        Some(trigger) => SampleContext::from_args(&synthesize_args(
            &trigger.variables,
            &SynthesisSample::stable(trigger.contract),
        )),
        None => SampleContext::neutral(),
    }
}

pub fn sample_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    context: &SampleContext,
) -> OverlayConfig {
    if let Some(binding) = latest_binding(descriptor, stored) {
        let sample = sample_latest_value(&binding.slot);
        return latest_content(descriptor, stored, Some(&sample), context.names_language);
    }
    let mut content = delivered_content(descriptor, stored, &OverlayConfig::new(), &context.args());
    content.remove(SPEECH);
    content
}

fn distinct_kind(feeding: &[SampleTrigger]) -> Option<&SampleTrigger> {
    let first = feeding.first()?;
    feeding
        .iter()
        .all(|trigger| trigger.kind_id == first.kind_id)
        .then_some(first)
}
