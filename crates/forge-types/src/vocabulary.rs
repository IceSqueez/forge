use crate::data_flow::SynthesisHint;
use crate::variant::VariantKind;

const USER_ID: &str = "user_id";
const USER_NAME: &str = "user_name";
const USER_LOGIN: &str = "user_login";
const USER_PLATFORM: &str = "user_platform";
const MODERATOR_ID: &str = "moderator_id";
const MODERATOR_NAME: &str = "moderator_name";
const MODERATOR_LOGIN: &str = "moderator_login";
const MODERATOR_PLATFORM: &str = "moderator_platform";
const GIFTER_ID: &str = "gifter_id";
const GIFTER_NAME: &str = "gifter_name";
const GIFTER_LOGIN: &str = "gifter_login";
const GIFTER_PLATFORM: &str = "gifter_platform";
const RECIPIENT_ID: &str = "recipient_id";
const RECIPIENT_NAME: &str = "recipient_name";
const RECIPIENT_LOGIN: &str = "recipient_login";
const RECIPIENT_PLATFORM: &str = "recipient_platform";
const MESSAGE_TEXT: &str = "message_text";
const VIEWER_COUNT: &str = "viewer_count";
const BITS_AMOUNT: &str = "bits_amount";
const GIFT_COUNT: &str = "gift_count";
const SUB_TIER: &str = "sub_tier";
const SUB_CUMULATIVE_MONTHS: &str = "sub_cumulative_months";
const SUB_STREAK_MONTHS: &str = "sub_streak_months";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActorRole {
    Principal,
    Moderator,
    Gifter,
    Recipient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActorSlot {
    Id,
    Name,
    Login,
    Platform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalCount {
    ViewerCount,
    BitsAmount,
    GiftCount,
    SubCumulativeMonths,
    SubStreakMonths,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalVariable {
    Actor { role: ActorRole, slot: ActorSlot },
    MessageText,
    Count(CanonicalCount),
    SubTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPresence {
    Always,
    WhereThePlatformHasOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableStanding {
    Canonical(CanonicalVariable),
    EventSpecific,
    Legacy(CanonicalVariable),
}

const ACTOR_SLOT_COUNT: u16 = ActorSlot::ALL.len() as u16;
const ACTOR_SPAN: u16 = ActorRole::ALL.len() as u16 * ACTOR_SLOT_COUNT;
const COUNT_SPAN: u16 = CanonicalCount::ALL.len() as u16;

impl ActorRole {
    pub const ALL: [ActorRole; 4] = [
        ActorRole::Principal,
        ActorRole::Moderator,
        ActorRole::Gifter,
        ActorRole::Recipient,
    ];

    const fn label(self) -> &'static str {
        match self {
            ActorRole::Principal => "User",
            ActorRole::Moderator => "Moderator",
            ActorRole::Gifter => "Gifter",
            ActorRole::Recipient => "Recipient",
        }
    }
}

impl ActorSlot {
    pub const ALL: [ActorSlot; 4] = [
        ActorSlot::Id,
        ActorSlot::Name,
        ActorSlot::Login,
        ActorSlot::Platform,
    ];

    pub const fn presence(self) -> SlotPresence {
        match self {
            ActorSlot::Login => SlotPresence::WhereThePlatformHasOne,
            ActorSlot::Id | ActorSlot::Name | ActorSlot::Platform => SlotPresence::Always,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            ActorSlot::Id => "ID",
            ActorSlot::Name => "display name",
            ActorSlot::Login => "login",
            ActorSlot::Platform => "platform",
        }
    }
}

impl CanonicalCount {
    pub const ALL: [CanonicalCount; 5] = [
        CanonicalCount::ViewerCount,
        CanonicalCount::BitsAmount,
        CanonicalCount::GiftCount,
        CanonicalCount::SubCumulativeMonths,
        CanonicalCount::SubStreakMonths,
    ];

    const fn label(self) -> &'static str {
        match self {
            CanonicalCount::ViewerCount => "Viewer count",
            CanonicalCount::BitsAmount => "Bits amount",
            CanonicalCount::GiftCount => "Gift count",
            CanonicalCount::SubCumulativeMonths => "Cumulative subscription months",
            CanonicalCount::SubStreakMonths => "Subscription streak months",
        }
    }

    const fn synthesis(self) -> SynthesisHint {
        match self {
            CanonicalCount::ViewerCount => SynthesisHint::BoundedInt {
                min: 0,
                max: 50_000,
            },
            CanonicalCount::BitsAmount => SynthesisHint::BoundedInt {
                min: 1,
                max: 10_000,
            },
            CanonicalCount::GiftCount => SynthesisHint::BoundedInt { min: 1, max: 100 },
            CanonicalCount::SubCumulativeMonths | CanonicalCount::SubStreakMonths => {
                SynthesisHint::BoundedInt { min: 1, max: 120 }
            }
        }
    }
}

impl CanonicalVariable {
    pub const fn actor(role: ActorRole, slot: ActorSlot) -> Self {
        CanonicalVariable::Actor { role, slot }
    }

    pub fn all() -> Vec<CanonicalVariable> {
        let mut every: Vec<CanonicalVariable> = ActorRole::ALL
            .into_iter()
            .flat_map(|role| {
                ActorSlot::ALL
                    .into_iter()
                    .map(move |slot| CanonicalVariable::actor(role, slot))
            })
            .chain(std::iter::once(CanonicalVariable::MessageText))
            .chain(
                CanonicalCount::ALL
                    .into_iter()
                    .map(CanonicalVariable::Count),
            )
            .chain(std::iter::once(CanonicalVariable::SubTier))
            .collect();
        every.sort_by_key(|canonical| canonical.order());
        every
    }

    pub const fn name(self) -> &'static str {
        match self {
            CanonicalVariable::Actor { role, slot } => match (role, slot) {
                (ActorRole::Principal, ActorSlot::Id) => USER_ID,
                (ActorRole::Principal, ActorSlot::Name) => USER_NAME,
                (ActorRole::Principal, ActorSlot::Login) => USER_LOGIN,
                (ActorRole::Principal, ActorSlot::Platform) => USER_PLATFORM,
                (ActorRole::Moderator, ActorSlot::Id) => MODERATOR_ID,
                (ActorRole::Moderator, ActorSlot::Name) => MODERATOR_NAME,
                (ActorRole::Moderator, ActorSlot::Login) => MODERATOR_LOGIN,
                (ActorRole::Moderator, ActorSlot::Platform) => MODERATOR_PLATFORM,
                (ActorRole::Gifter, ActorSlot::Id) => GIFTER_ID,
                (ActorRole::Gifter, ActorSlot::Name) => GIFTER_NAME,
                (ActorRole::Gifter, ActorSlot::Login) => GIFTER_LOGIN,
                (ActorRole::Gifter, ActorSlot::Platform) => GIFTER_PLATFORM,
                (ActorRole::Recipient, ActorSlot::Id) => RECIPIENT_ID,
                (ActorRole::Recipient, ActorSlot::Name) => RECIPIENT_NAME,
                (ActorRole::Recipient, ActorSlot::Login) => RECIPIENT_LOGIN,
                (ActorRole::Recipient, ActorSlot::Platform) => RECIPIENT_PLATFORM,
            },
            CanonicalVariable::MessageText => MESSAGE_TEXT,
            CanonicalVariable::Count(count) => match count {
                CanonicalCount::ViewerCount => VIEWER_COUNT,
                CanonicalCount::BitsAmount => BITS_AMOUNT,
                CanonicalCount::GiftCount => GIFT_COUNT,
                CanonicalCount::SubCumulativeMonths => SUB_CUMULATIVE_MONTHS,
                CanonicalCount::SubStreakMonths => SUB_STREAK_MONTHS,
            },
            CanonicalVariable::SubTier => SUB_TIER,
        }
    }

    pub const fn kind(self) -> VariantKind {
        match self {
            CanonicalVariable::Actor { .. }
            | CanonicalVariable::MessageText
            | CanonicalVariable::SubTier => VariantKind::String,
            CanonicalVariable::Count(_) => VariantKind::Int,
        }
    }

    pub const fn synthesis(self) -> Option<SynthesisHint> {
        match self {
            CanonicalVariable::Actor { slot, .. } => match slot {
                ActorSlot::Name => Some(SynthesisHint::DisplayName),
                ActorSlot::Login => Some(SynthesisHint::Username),
                ActorSlot::Id | ActorSlot::Platform => None,
            },
            CanonicalVariable::MessageText => Some(SynthesisHint::Message),
            CanonicalVariable::Count(count) => Some(count.synthesis()),
            CanonicalVariable::SubTier => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            CanonicalVariable::Actor { role, slot } => {
                format!("{} {}", role.label(), slot.label())
            }
            CanonicalVariable::MessageText => "Message text".to_owned(),
            CanonicalVariable::Count(count) => count.label().to_owned(),
            CanonicalVariable::SubTier => "Subscription tier".to_owned(),
        }
    }

    pub const fn order(self) -> u16 {
        match self {
            CanonicalVariable::Actor { role, slot } => role as u16 * ACTOR_SLOT_COUNT + slot as u16,
            CanonicalVariable::MessageText => ACTOR_SPAN,
            CanonicalVariable::Count(count) => ACTOR_SPAN + 1 + count as u16,
            CanonicalVariable::SubTier => ACTOR_SPAN + 1 + COUNT_SPAN,
        }
    }
}

impl VariableStanding {
    pub const fn superseded_by(self) -> Option<CanonicalVariable> {
        match self {
            VariableStanding::Legacy(canonical) => Some(canonical),
            VariableStanding::Canonical(_) | VariableStanding::EventSpecific => None,
        }
    }
}
