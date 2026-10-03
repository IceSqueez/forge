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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_types::{DonationOrigin, Donor};

    use super::*;

    const PLACEHOLDER: &str = "Anonymous donor";
    const DONOR_NAME: &str = "SecretDonorName";
    const MESSAGE: &str = "private message body";

    fn donation(donor: Donor, origin: DonationOrigin) -> Donation {
        Donation {
            provider: IntegrationId::from_static("donatello"),
            donation_id: "d-1".to_owned(),
            donor,
            message: Some(MESSAGE.to_owned()),
            amount: MoneyAmount::from_micros(12_500_000, CurrencyCode::parse("UAH").unwrap()),
            occurred_at: OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
            origin,
        }
    }

    #[test]
    fn announce_skips_history_and_flags_only_test_donations() {
        for (origin, announced_test_flag) in [
            (DonationOrigin::Live, Some(false)),
            (DonationOrigin::Test, Some(true)),
            (DonationOrigin::History, None),
        ] {
            let announced = DonationReceived::announce(
                &donation(Donor::Named(DONOR_NAME.to_owned()), origin),
                PLACEHOLDER,
            );
            assert_eq!(
                announced.map(|payload| payload.test),
                announced_test_flag,
                "origin {origin:?}"
            );
        }
    }

    #[test]
    fn announce_shows_the_placeholder_for_anonymous_and_hidden_donors() {
        for (donor, shown, visibility) in [
            (
                Donor::Named(DONOR_NAME.to_owned()),
                DONOR_NAME,
                DonorVisibility::Named,
            ),
            (Donor::Anonymous, PLACEHOLDER, DonorVisibility::Anonymous),
            (Donor::Hidden, PLACEHOLDER, DonorVisibility::Hidden),
        ] {
            let announced =
                DonationReceived::announce(&donation(donor, DonationOrigin::Live), PLACEHOLDER)
                    .unwrap();
            assert_eq!(
                (announced.donor_name.as_str(), announced.donor_visibility),
                (shown, visibility)
            );
        }
    }

    #[test]
    fn announced_payload_carries_the_donation_amount_and_identity() {
        let source = donation(Donor::Anonymous, DonationOrigin::Live);
        let announced = DonationReceived::announce(&source, PLACEHOLDER).unwrap();
        assert_eq!(
            (
                announced.amount(),
                announced.provider.clone(),
                announced.donation_id.clone(),
                announced.message.clone(),
                announced.occurred_at,
            ),
            (
                source.amount.clone(),
                source.provider.clone(),
                source.donation_id.clone(),
                source.message.clone(),
                source.occurred_at,
            )
        );
    }

    #[test]
    fn a_donation_event_round_trips_through_the_bus_payload() {
        let announced =
            DonationReceived::announce(&donation(Donor::Hidden, DonationOrigin::Test), PLACEHOLDER)
                .unwrap();
        let event = announced.clone().into_event().unwrap();

        assert_eq!(
            (
                event.source,
                event.kind.as_str(),
                DonationReceived::from_event(&event)
            ),
            (
                EventSource::Donation,
                DONATION_RECEIVED_KIND,
                Some(announced)
            )
        );
    }

    #[test]
    fn from_event_ignores_other_kinds_and_malformed_payloads() {
        let valid_payload = DonationReceived::announce(
            &donation(Donor::Anonymous, DonationOrigin::Live),
            PLACEHOLDER,
        )
        .unwrap()
        .into_event()
        .unwrap()
        .payload;
        let mut bad_currency = valid_payload.clone();
        bad_currency["currency"] = serde_json::json!("UAH1");
        let mut negative_amount = valid_payload.clone();
        negative_amount["amount_micros"] = serde_json::json!(-1);

        for (case, event) in [
            (
                "other kind",
                Event::new(EventSource::Donation, "donation.refunded", valid_payload),
            ),
            (
                "null payload",
                Event::new(
                    EventSource::Donation,
                    DONATION_RECEIVED_KIND,
                    serde_json::Value::Null,
                ),
            ),
            (
                "invalid currency",
                Event::new(EventSource::Donation, DONATION_RECEIVED_KIND, bad_currency),
            ),
            (
                "negative amount",
                Event::new(
                    EventSource::Donation,
                    DONATION_RECEIVED_KIND,
                    negative_amount,
                ),
            ),
        ] {
            assert_eq!(DonationReceived::from_event(&event), None, "case: {case}");
        }
    }

    #[test]
    fn debug_never_prints_the_donor_name_or_message() {
        let announced = DonationReceived::announce(
            &donation(Donor::Named(DONOR_NAME.to_owned()), DonationOrigin::Live),
            PLACEHOLDER,
        )
        .unwrap();
        let rendered = format!("{announced:?}");
        assert!(
            !rendered.contains(DONOR_NAME) && !rendered.contains(MESSAGE),
            "{rendered}"
        );
    }
}
