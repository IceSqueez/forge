use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::KindPlatformContract;
use forge_runtime::{EventBus, QueueSchedulerHandle, SchedulerRequest};
use forge_types::{
    ActionId, ActorRole, ActorSlot, ArgStack, CanonicalVariable, DeclaredVariable, PlatformId,
    QueueId, SynthesisHint, VariableStanding, Variant, VariantKind,
};
use rand::RngExt;
use serde_json::json;
use time::OffsetDateTime;

use super::trigger_variables::ListedVariable;

const DISPLAY_NAME_POOL: &[&str] = &[
    "TestUser",
    "StreamFan42",
    "NightOwl",
    "PixelPal",
    "CoolViewer",
];
const ACTOR_ID_POOL: &[&str] = &["37402112", "51938264", "60114873", "72840591", "84025617"];
const MESSAGE_POOL: &[&str] = &[
    "hey, great stream!",
    "this is a test message",
    "love the content",
    "what game is this?",
    "first time here, hi!",
];
const SUB_TIER_POOL: &[&str] = &["1000", "2000", "3000"];
const NO_PLATFORM: &str = "";
const FALLBACK_TOKEN: &str = "sample";
const GENERIC_INT_MIN: i64 = 1;
const GENERIC_INT_MAX: i64 = 100;
const GENERIC_FLOAT_MIN: f64 = 0.01;
const GENERIC_FLOAT_MAX: f64 = 99.99;
const RATIO_RESOLUTION: i64 = 1_000;

pub(super) struct SynthesisSample {
    platform: Option<PlatformId>,
    name_index: usize,
    message_index: usize,
    tier_index: usize,
    ratio_step: i64,
    flag: bool,
    now: OffsetDateTime,
}

impl SynthesisSample {
    pub(super) fn random(contract: KindPlatformContract) -> Self {
        let mut rng = rand::rng();
        SynthesisSample {
            platform: match contract {
                KindPlatformContract::PlatformSpecific(platform) => Some(platform),
                KindPlatformContract::Universal => None,
            },
            name_index: rng.random_range(0..DISPLAY_NAME_POOL.len()),
            message_index: rng.random_range(0..MESSAGE_POOL.len()),
            tier_index: rng.random_range(0..SUB_TIER_POOL.len()),
            ratio_step: rng.random_range(0..=RATIO_RESOLUTION),
            flag: rng.random_range(0..=1) == 1,
            now: OffsetDateTime::now_utc(),
        }
    }

    fn display_name(&self, role: ActorRole) -> &'static str {
        rotated(DISPLAY_NAME_POOL, self.name_index, role as usize)
    }

    fn actor_id(&self, role: ActorRole) -> &'static str {
        rotated(ACTOR_ID_POOL, self.name_index, role as usize)
    }

    fn message(&self) -> &'static str {
        rotated(MESSAGE_POOL, self.message_index, 0)
    }

    fn sub_tier(&self) -> &'static str {
        rotated(SUB_TIER_POOL, self.tier_index, 0)
    }

    fn platform_name(&self) -> &'static str {
        self.platform.map_or(NO_PLATFORM, PlatformId::as_str)
    }

    fn ratio(&self) -> f64 {
        self.ratio_step as f64 / RATIO_RESOLUTION as f64
    }

    fn bounded_int(&self, min: i64, max: i64) -> i64 {
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        lo + ((hi - lo) as f64 * self.ratio()).round() as i64
    }

    fn bounded_float(&self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.ratio()
    }
}

pub(super) fn synthesize_args(variables: &[ListedVariable], sample: &SynthesisSample) -> ArgStack {
    let mut minted: BTreeMap<CanonicalVariable, Variant> = BTreeMap::new();
    let mut stack = ArgStack::new();
    for variable in variables {
        let value = match canonical_slot(variable.standing) {
            Some(canonical) => minted
                .entry(canonical)
                .or_insert_with(|| canonical_value(canonical, sample))
                .clone(),
            None => event_specific_value(&variable.declared, sample),
        };
        stack = stack.set(variable.declared.name.clone(), value);
    }
    stack
}

fn canonical_slot(standing: VariableStanding) -> Option<CanonicalVariable> {
    match standing {
        VariableStanding::Canonical(canonical) | VariableStanding::Legacy(canonical) => {
            Some(canonical)
        }
        VariableStanding::EventSpecific => None,
    }
}

fn canonical_value(canonical: CanonicalVariable, sample: &SynthesisSample) -> Variant {
    match canonical {
        CanonicalVariable::Actor { role, slot } => Variant::String(actor_value(role, slot, sample)),
        CanonicalVariable::SubTier => Variant::String(sample.sub_tier().to_owned()),
        CanonicalVariable::MessageText | CanonicalVariable::Count(_) => hinted_value(
            canonical.synthesis().as_ref(),
            canonical.kind(),
            canonical.name(),
            sample,
        ),
    }
}

fn actor_value(role: ActorRole, slot: ActorSlot, sample: &SynthesisSample) -> String {
    match slot {
        ActorSlot::Id => sample.actor_id(role).to_owned(),
        ActorSlot::Name => sample.display_name(role).to_owned(),
        ActorSlot::Login => login_of(sample.display_name(role)),
        ActorSlot::Platform => sample.platform_name().to_owned(),
    }
}

fn login_of(display_name: &str) -> String {
    display_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn event_specific_value(declared: &DeclaredVariable, sample: &SynthesisSample) -> Variant {
    let token_base = if declared.label.is_empty() {
        &declared.name
    } else {
        &declared.label
    };
    hinted_value(
        declared.synthesis.as_ref(),
        declared.kind,
        token_base,
        sample,
    )
}

fn hinted_value(
    hint: Option<&SynthesisHint>,
    kind: VariantKind,
    token_base: &str,
    sample: &SynthesisSample,
) -> Variant {
    match hint {
        Some(SynthesisHint::Username) => {
            Variant::String(login_of(sample.display_name(ActorRole::Principal)))
        }
        Some(SynthesisHint::DisplayName) => {
            Variant::String(sample.display_name(ActorRole::Principal).to_owned())
        }
        Some(SynthesisHint::Message) => Variant::String(sample.message().to_owned()),
        Some(SynthesisHint::BoundedInt { min, max }) => {
            Variant::Int(sample.bounded_int(*min, *max))
        }
        None => untyped_value(kind, token_base, sample),
    }
}

fn untyped_value(kind: VariantKind, token_base: &str, sample: &SynthesisSample) -> Variant {
    match kind {
        VariantKind::String => Variant::String(sample_token(token_base)),
        VariantKind::Int => Variant::Int(sample.bounded_int(GENERIC_INT_MIN, GENERIC_INT_MAX)),
        VariantKind::Float => {
            Variant::Float(sample.bounded_float(GENERIC_FLOAT_MIN, GENERIC_FLOAT_MAX))
        }
        VariantKind::Bool => Variant::Bool(sample.flag),
        VariantKind::Datetime => Variant::Datetime(sample.now),
        VariantKind::Array => Variant::Array(vec![
            Variant::String(sample_token(token_base)),
            Variant::String(login_of(sample.display_name(ActorRole::Principal))),
        ]),
        VariantKind::Object => Variant::Object(BTreeMap::new()),
    }
}

fn rotated(pool: &[&'static str], index: usize, offset: usize) -> &'static str {
    pool[(index + offset) % pool.len()]
}

fn sample_token(base: &str) -> String {
    let slug: String = base
        .split_whitespace()
        .next()
        .unwrap_or(FALLBACK_TOKEN)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if slug.is_empty() {
        FALLBACK_TOKEN.to_owned()
    } else {
        format!("{slug}_sample")
    }
}

pub(super) async fn dispatch_test_run(
    scheduler: &QueueSchedulerHandle,
    bus: &EventBus,
    action_id: ActionId,
    queue_id: QueueId,
    bypass_pause: bool,
    trigger_kind: Option<String>,
    initial_args: ArgStack,
) -> Result<(), String> {
    let root = Event::new(
        EventSource::Core,
        "test.run",
        json!({ "action_id": action_id.to_string() }),
    );
    let trigger_event_id = root.id;
    bus.record(root);
    scheduler
        .dispatch(SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id,
            trigger_kind,
            initial_args,
            bypass_pause,
        })
        .await
        .map_err(|e| e.to_string())
}
