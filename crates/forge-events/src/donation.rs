use std::fmt;

use forge_types::{
    CurrencyCode, Donation, DonorVisibility, IntegrationId, MoneyAmount, RedactedText,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{Event, EventSource};

pub const DONATION_RECEIVED_KIND: &str = "donation.received";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DonationReceived {
    pub provider: IntegrationId,
    pub donation_id: String,
    pub donor_name: String,
    pub donor_visibility: DonorVisibility,
    pub message: Option<String>,
    pub amount_micros: u64,
    pub currency: CurrencyCode,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    pub test: bool,
}

impl DonationReceived {
    pub fn announce(donation: &Donation, anonymous_name: &str) -> Option<Self> {
        if !donation.origin.announces() {
            return None;
        }
        Some(Self {
            provider: donation.provider.clone(),
            donation_id: donation.donation_id.clone(),
            donor_name: donation
                .donor
                .shown_name()
                .unwrap_or(anonymous_name)
                .to_owned(),
            donor_visibility: donation.donor.visibility(),
            message: donation.message.clone(),
            amount_micros: donation.amount.micros(),
            currency: donation.amount.currency().clone(),
            occurred_at: donation.occurred_at,
            test: donation.origin.is_test(),
        })
    }

    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != DONATION_RECEIVED_KIND {
            return None;
        }
        serde_json::from_value(event.payload.clone()).ok()
    }

    pub fn into_event(self) -> Result<Event, serde_json::Error> {
        Ok(Event::new(
            EventSource::Donation,
            DONATION_RECEIVED_KIND,
            serde_json::to_value(self)?,
        ))
    }

    pub fn amount(&self) -> MoneyAmount {
        MoneyAmount::from_micros(self.amount_micros, self.currency.clone())
    }
}

impl fmt::Debug for DonationReceived {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DonationReceived")
            .field("provider", &self.provider)
            .field("donation_id", &self.donation_id)
            .field("donor_name", &RedactedText::new(&self.donor_name))
            .field("donor_visibility", &self.donor_visibility)
            .field("message", &self.message.as_deref().map(RedactedText::new))
            .field("amount_micros", &self.amount_micros)
            .field("currency", &self.currency)
            .field("occurred_at", &self.occurred_at)
            .field("test", &self.test)
            .finish()
    }
}
