use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::integration::IntegrationId;
use crate::money::MoneyAmount;
use crate::redaction::RedactedText;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "visibility", content = "name", rename_all = "snake_case")]
pub enum Donor {
    Named(String),
    Anonymous,
    Hidden,
}

impl Donor {
    pub fn named(name: &str) -> Self {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            Self::Anonymous
        } else {
            Self::Named(trimmed.to_owned())
        }
    }

    pub fn shown_name(&self) -> Option<&str> {
        match self {
            Self::Named(name) => Some(name),
            Self::Anonymous | Self::Hidden => None,
        }
    }

    pub const fn visibility(&self) -> DonorVisibility {
        match self {
            Self::Named(_) => DonorVisibility::Named,
            Self::Anonymous => DonorVisibility::Anonymous,
            Self::Hidden => DonorVisibility::Hidden,
        }
    }
}

impl fmt::Debug for Donor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => f
                .debug_tuple("Named")
                .field(&RedactedText::new(name))
                .finish(),
            Self::Anonymous => f.write_str("Anonymous"),
            Self::Hidden => f.write_str("Hidden"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DonorVisibility {
    Named,
    Anonymous,
    Hidden,
}

impl DonorVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Named => "named",
            Self::Anonymous => "anonymous",
            Self::Hidden => "hidden",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DonationOrigin {
    Live,
    History,
    Test,
}

impl DonationOrigin {
    pub const fn announces(self) -> bool {
        matches!(self, Self::Live | Self::Test)
    }

    pub const fn is_test(self) -> bool {
        matches!(self, Self::Test)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Donation {
    pub provider: IntegrationId,
    pub donation_id: String,
    pub donor: Donor,
    pub message: Option<String>,
    pub amount: MoneyAmount,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    pub origin: DonationOrigin,
}

impl fmt::Debug for Donation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Donation")
            .field("provider", &self.provider)
            .field("donation_id", &self.donation_id)
            .field("donor", &self.donor)
            .field("message", &self.message.as_deref().map(RedactedText::new))
            .field("amount", &self.amount)
            .field("occurred_at", &self.occurred_at)
            .field("origin", &self.origin)
            .finish()
    }
}
