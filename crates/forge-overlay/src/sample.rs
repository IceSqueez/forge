use std::collections::BTreeMap;

use forge_registry::{KindPlatformContract, SynthesisSample, TriggerVariable, synthesize_args};
use forge_types::{ArgStack, CanonicalVariable, Variant};

use crate::config::SPEECH;
use crate::content::delivered_content;
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};

pub struct SampleTrigger {
    pub kind_id: String,
    pub contract: KindPlatformContract,
    pub variables: Vec<TriggerVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleContext(BTreeMap<String, Variant>);

impl SampleContext {
    pub fn neutral() -> Self {
        let vocabulary: Vec<TriggerVariable> = CanonicalVariable::all()
            .into_iter()
            .map(TriggerVariable::canonical)
            .collect();
        SampleContext(
            synthesize_args(
                &vocabulary,
                &SynthesisSample::stable(KindPlatformContract::Universal),
            )
            .snapshot(),
        )
    }

    pub fn args(&self) -> ArgStack {
        self.0.iter().fold(ArgStack::new(), |stack, (name, value)| {
            stack.set(name.clone(), value.clone())
        })
    }
}

pub fn sample_context(feeding: &[SampleTrigger]) -> SampleContext {
    match distinct_kind(feeding) {
        Some(trigger) => SampleContext(
            synthesize_args(
                &trigger.variables,
                &SynthesisSample::stable(trigger.contract),
            )
            .snapshot(),
        ),
        None => SampleContext::neutral(),
    }
}

/// Never carries the speech text: a page plays speech, it never draws it.
pub fn sample_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    context: &SampleContext,
) -> OverlayConfig {
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
