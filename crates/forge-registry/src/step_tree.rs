use forge_types::{SubActionConfig, SubActionStep, Variant};

use crate::form::FormField;
use crate::registry::SubActionRegistry;

pub const CASE_CHAIN_KEY: &str = "chain";
pub const CASE_MATCH_KEY: &str = "match";
pub const STEP_KIND_KEY: &str = "kind_id";
pub const STEP_CONFIG_KEY: &str = "config";
pub const STEP_ENABLED_KEY: &str = "enabled";
pub const STEP_CONTINUE_ON_ERROR_KEY: &str = "continue_on_error";
pub const STEP_CONDITION_KEY: &str = "condition";
pub const STEP_LABEL_KEY: &str = "label";

const DEFAULT_ENABLED: bool = true;
const DEFAULT_CONTINUE_ON_ERROR: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndecodableReason {
    NotAnObject,
    MissingKind,
    MalformedField(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UndecodableStep {
    pub position: usize,
    pub reason: UndecodableReason,
    pub raw: Variant,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChainEntry {
    Step(SubActionStep),
    Undecodable(UndecodableStep),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecodedChain {
    entries: Vec<ChainEntry>,
}

impl DecodedChain {
    pub fn entries(&self) -> &[ChainEntry] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<ChainEntry> {
        self.entries
    }

    pub fn steps(&self) -> impl Iterator<Item = &SubActionStep> {
        self.entries.iter().filter_map(|entry| match entry {
            ChainEntry::Step(step) => Some(step),
            ChainEntry::Undecodable(_) => None,
        })
    }

    pub fn undecodable(&self) -> impl Iterator<Item = &UndecodableStep> {
        self.entries.iter().filter_map(|entry| match entry {
            ChainEntry::Step(_) => None,
            ChainEntry::Undecodable(undecodable) => Some(undecodable),
        })
    }

    pub fn into_steps(self) -> Vec<SubActionStep> {
        self.entries
            .into_iter()
            .filter_map(|entry| match entry {
                ChainEntry::Step(step) => Some(step),
                ChainEntry::Undecodable(_) => None,
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepReach {
    Enabled,
    All,
}

#[derive(Clone, Copy, Debug)]
pub enum StepNode<'a> {
    Step(&'a SubActionStep),
    Undecodable(&'a UndecodableStep),
}

pub fn decode_step(raw: &Variant) -> Result<SubActionStep, UndecodableReason> {
    let entry = raw.as_object().ok_or(UndecodableReason::NotAnObject)?;
    let kind_id = match entry.get(STEP_KIND_KEY) {
        None => return Err(UndecodableReason::MissingKind),
        Some(Variant::String(kind_id)) => kind_id.clone(),
        Some(_) => return Err(UndecodableReason::MalformedField(STEP_KIND_KEY)),
    };
    let config = match entry.get(STEP_CONFIG_KEY) {
        None => SubActionConfig::new(),
        Some(Variant::Object(config)) => config.clone(),
        Some(_) => return Err(UndecodableReason::MalformedField(STEP_CONFIG_KEY)),
    };
    Ok(SubActionStep {
        kind_id,
        config,
        enabled: flag(entry, STEP_ENABLED_KEY, DEFAULT_ENABLED)?,
        continue_on_error: flag(entry, STEP_CONTINUE_ON_ERROR_KEY, DEFAULT_CONTINUE_ON_ERROR)?,
        condition: text(entry, STEP_CONDITION_KEY)?,
        label: text(entry, STEP_LABEL_KEY)?,
    })
}

fn flag(
    entry: &SubActionConfig,
    key: &'static str,
    default: bool,
) -> Result<bool, UndecodableReason> {
    match entry.get(key) {
        None => Ok(default),
        Some(Variant::Bool(value)) => Ok(*value),
        Some(_) => Err(UndecodableReason::MalformedField(key)),
    }
}

fn text(entry: &SubActionConfig, key: &'static str) -> Result<Option<String>, UndecodableReason> {
    match entry.get(key) {
        None => Ok(None),
        Some(Variant::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(UndecodableReason::MalformedField(key)),
    }
}

pub fn decode_chain(value: Option<&Variant>) -> DecodedChain {
    let Some(items) = value.and_then(Variant::as_array) else {
        return DecodedChain::default();
    };
    let entries = items
        .iter()
        .enumerate()
        .map(|(position, raw)| match decode_step(raw) {
            Ok(step) => ChainEntry::Step(step),
            Err(reason) => ChainEntry::Undecodable(UndecodableStep {
                position,
                reason,
                raw: raw.clone(),
            }),
        })
        .collect();
    DecodedChain { entries }
}

pub fn decode_steps(value: Option<&Variant>) -> Vec<SubActionStep> {
    decode_chain(value).into_steps()
}

pub fn encode_step(step: &SubActionStep) -> Variant {
    let mut entry = SubActionConfig::new();
    entry.insert(
        STEP_KIND_KEY.to_owned(),
        Variant::String(step.kind_id.clone()),
    );
    entry.insert(
        STEP_CONFIG_KEY.to_owned(),
        Variant::Object(step.config.clone()),
    );
    entry.insert(STEP_ENABLED_KEY.to_owned(), Variant::Bool(step.enabled));
    entry.insert(
        STEP_CONTINUE_ON_ERROR_KEY.to_owned(),
        Variant::Bool(step.continue_on_error),
    );
    if let Some(condition) = &step.condition {
        entry.insert(
            STEP_CONDITION_KEY.to_owned(),
            Variant::String(condition.clone()),
        );
    }
    if let Some(label) = &step.label {
        entry.insert(STEP_LABEL_KEY.to_owned(), Variant::String(label.clone()));
    }
    Variant::Object(entry)
}

pub fn encode_step_onto(step: &SubActionStep, stored: &Variant) -> Variant {
    if decode_step(stored).as_ref() == Ok(step) {
        return stored.clone();
    }
    let (Variant::Object(mut entry), Variant::Object(fresh)) = (stored.clone(), encode_step(step))
    else {
        return encode_step(step);
    };
    entry.remove(STEP_CONDITION_KEY);
    entry.remove(STEP_LABEL_KEY);
    entry.extend(fresh);
    Variant::Object(entry)
}

pub fn encode_chain(entries: &[ChainEntry]) -> Variant {
    Variant::Array(
        entries
            .iter()
            .map(|entry| match entry {
                ChainEntry::Step(step) => encode_step(step),
                ChainEntry::Undecodable(undecodable) => undecodable.raw.clone(),
            })
            .collect(),
    )
}

#[derive(Clone, Copy)]
enum ChainSlot {
    Whole(&'static str),
    Cases(&'static str),
}

fn push_slots(field: &FormField, slots: &mut Vec<ChainSlot>) {
    match field {
        FormField::SubChain { key, .. } => slots.push(ChainSlot::Whole(key)),
        FormField::CaseList { key, .. } => slots.push(ChainSlot::Cases(key)),
        FormField::Optional { inner, .. } => push_slots(inner, slots),
        _ => {}
    }
}

fn chain_slots(kind_id: &str, registry: &SubActionRegistry) -> Vec<ChainSlot> {
    let mut slots = Vec::new();
    if let Some(runner) = registry.get(kind_id) {
        for field in runner.config_fields() {
            push_slots(&field, &mut slots);
        }
    }
    slots
}

fn chain_values<'c>(config: &'c SubActionConfig, slots: &[ChainSlot]) -> Vec<Option<&'c Variant>> {
    let mut values = Vec::new();
    for slot in slots {
        match *slot {
            ChainSlot::Whole(key) => values.push(config.get(key)),
            ChainSlot::Cases(key) => {
                let cases = config.get(key).and_then(Variant::as_array);
                for case in cases.into_iter().flatten() {
                    values.push(case.as_object().and_then(|case| case.get(CASE_CHAIN_KEY)));
                }
            }
        }
    }
    values
}

pub fn nested_chains(step: &SubActionStep, registry: &SubActionRegistry) -> Vec<DecodedChain> {
    let slots = chain_slots(&step.kind_id, registry);
    chain_values(&step.config, &slots)
        .into_iter()
        .map(decode_chain)
        .collect()
}

pub fn walk_steps(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    reach: StepReach,
    visit: &mut dyn FnMut(StepNode<'_>),
) {
    for step in steps {
        walk_step(step, registry, reach, visit);
    }
}

fn walk_step(
    step: &SubActionStep,
    registry: &SubActionRegistry,
    reach: StepReach,
    visit: &mut dyn FnMut(StepNode<'_>),
) {
    if reach == StepReach::Enabled && !step.enabled {
        return;
    }
    visit(StepNode::Step(step));
    for chain in nested_chains(step, registry) {
        for entry in chain.entries() {
            match entry {
                ChainEntry::Step(nested) => walk_step(nested, registry, reach, visit),
                ChainEntry::Undecodable(undecodable) => visit(StepNode::Undecodable(undecodable)),
            }
        }
    }
}

pub fn rewrite_steps(
    steps: &mut [SubActionStep],
    registry: &SubActionRegistry,
    edit: &mut dyn FnMut(&mut String, &mut SubActionConfig) -> bool,
) -> bool {
    let mut changed = false;
    for step in steps {
        changed |= rewrite_step(&mut step.kind_id, &mut step.config, registry, edit);
    }
    changed
}

fn rewrite_step(
    kind_id: &mut String,
    config: &mut SubActionConfig,
    registry: &SubActionRegistry,
    edit: &mut dyn FnMut(&mut String, &mut SubActionConfig) -> bool,
) -> bool {
    let mut changed = edit(kind_id, config);
    for slot in chain_slots(kind_id, registry) {
        match slot {
            ChainSlot::Whole(key) => {
                if let Some(chain) = config.get_mut(key) {
                    changed |= rewrite_chain(chain, registry, edit);
                }
            }
            ChainSlot::Cases(key) => {
                if let Some(Variant::Array(cases)) = config.get_mut(key) {
                    for case in cases {
                        if let Variant::Object(case) = case
                            && let Some(chain) = case.get_mut(CASE_CHAIN_KEY)
                        {
                            changed |= rewrite_chain(chain, registry, edit);
                        }
                    }
                }
            }
        }
    }
    changed
}

fn rewrite_chain(
    chain: &mut Variant,
    registry: &SubActionRegistry,
    edit: &mut dyn FnMut(&mut String, &mut SubActionConfig) -> bool,
) -> bool {
    let Variant::Array(entries) = chain else {
        return false;
    };
    let mut changed = false;
    for entry in entries {
        if decode_step(entry).is_err() {
            continue;
        }
        let Variant::Object(fields) = entry else {
            continue;
        };
        changed |= rewrite_entry(fields, registry, edit);
    }
    changed
}

fn rewrite_entry(
    fields: &mut SubActionConfig,
    registry: &SubActionRegistry,
    edit: &mut dyn FnMut(&mut String, &mut SubActionConfig) -> bool,
) -> bool {
    let stored_config = fields.contains_key(STEP_CONFIG_KEY);
    if !stored_config {
        fields.insert(
            STEP_CONFIG_KEY.to_owned(),
            Variant::Object(SubActionConfig::new()),
        );
    }
    let mut kind_id = None;
    let mut config = None;
    for (key, value) in fields.iter_mut() {
        match (key.as_str(), value) {
            (STEP_KIND_KEY, Variant::String(kind)) => kind_id = Some(kind),
            (STEP_CONFIG_KEY, Variant::Object(stored)) => config = Some(stored),
            _ => {}
        }
    }
    let changed = match (kind_id, config) {
        (Some(kind_id), Some(config)) => rewrite_step(kind_id, config, registry, edit),
        _ => false,
    };
    if !stored_config
        && matches!(fields.get(STEP_CONFIG_KEY), Some(Variant::Object(config)) if config.is_empty())
    {
        fields.remove(STEP_CONFIG_KEY);
    }
    changed
}
