use forge_types::{CurrencyCode, Donation, DonationOrigin, Donor, IntegrationId, MoneyAmount};
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use time::{OffsetDateTime, UtcOffset};

use crate::error::DonatelloError;
use crate::wire::DonateWire;

pub const DONATELLO_ASSUMED_TIME_ZONE: &str = "Europe/Kyiv";

const CREATED_AT_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub(crate) fn donation_from_wire(
    provider: &IntegrationId,
    wire: DonateWire,
    origin: DonationOrigin,
) -> Result<Donation, DonatelloError> {
    let invalid = |reason: String| DonatelloError::InvalidDonation {
        donation_id: wire.pub_id.clone(),
        reason,
    };
    let currency =
        CurrencyCode::parse(&wire.currency).map_err(|error| invalid(error.to_string()))?;
    let amount = MoneyAmount::parse_decimal(&wire.amount.as_text(), currency)
        .map_err(|error| invalid(error.to_string()))?;
    let occurred_at = created_at_instant(&wire.created_at).map_err(invalid)?;
    let donor = if wire.is_published == Some(false) {
        Donor::Hidden
    } else {
        Donor::named(wire.client_name.as_deref().unwrap_or_default())
    };
    let message = wire
        .message
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    Ok(Donation {
        provider: provider.clone(),
        donation_id: wire.pub_id,
        donor,
        message,
        amount,
        occurred_at,
        origin,
    })
}

fn created_at_instant(text: &str) -> Result<OffsetDateTime, String> {
    let wall_clock = DateTime::strptime(CREATED_AT_FORMAT, text.trim())
        .map_err(|_| format!("createdAt {text:?} is not YYYY-MM-DD HH:MM:SS"))?;
    let zone = TimeZone::get(DONATELLO_ASSUMED_TIME_ZONE)
        .map_err(|_| format!("time zone {DONATELLO_ASSUMED_TIME_ZONE} is unknown"))?;
    let zoned = wall_clock.to_zoned(zone).map_err(|_| {
        format!("createdAt {text:?} does not exist in {DONATELLO_ASSUMED_TIME_ZONE}")
    })?;
    let offset = UtcOffset::from_whole_seconds(zoned.offset().seconds())
        .map_err(|_| format!("offset of createdAt {text:?} is out of range"))?;
    OffsetDateTime::from_unix_timestamp(zoned.timestamp().as_second())
        .map(|instant| instant.to_offset(offset))
        .map_err(|_| format!("createdAt {text:?} is out of range"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_types::{Donation, DonationOrigin, Donor, IntegrationId};
    use serde_json::{Value, json};
    use time::macros::datetime;

    use super::donation_from_wire;
    use crate::error::DonatelloError;
    use crate::wire::DonateWire;

    const DONOR_NAME: &str = "Secret Donor Name";
    const MESSAGE: &str = "private donor message";

    fn record(overrides: Value) -> Value {
        let mut record = json!({
            "pubId": "D-1",
            "clientName": DONOR_NAME,
            "message": MESSAGE,
            "amount": "100",
            "currency": "UAH",
            "goal": "",
            "isPublished": true,
            "createdAt": "2026-07-01 12:00:00",
        });
        for (key, value) in overrides.as_object().unwrap() {
            record[key] = value.clone();
        }
        record
    }

    fn normalize(overrides: Value) -> Result<Donation, DonatelloError> {
        let wire: DonateWire = serde_json::from_value(record(overrides)).unwrap();
        donation_from_wire(
            &IntegrationId::from_static("donatello"),
            wire,
            DonationOrigin::Live,
        )
    }

    #[test]
    fn amount_accepts_whole_and_decimal_strings_and_json_numbers() {
        for (amount, micros) in [
            (json!("100"), 100_000_000),
            (json!("100.50"), 100_500_000),
            (json!(" 7 "), 7_000_000),
            (json!(100), 100_000_000),
            (json!(100.5), 100_500_000),
        ] {
            let donation = normalize(json!({ "amount": amount.clone() })).unwrap();
            assert_eq!(donation.amount.micros(), micros, "amount {amount}");
        }
    }

    #[test]
    fn currency_is_carried_upper_cased() {
        let donation = normalize(json!({ "currency": "usd" })).unwrap();
        assert_eq!(donation.amount.currency().as_str(), "USD");
    }

    #[test]
    fn created_at_is_read_as_kyiv_wall_clock_across_both_dst_transitions() {
        for (created_at, instant) in [
            ("2026-01-15 12:00:00", datetime!(2026-01-15 10:00:00 UTC)),
            ("2026-07-01 12:00:00", datetime!(2026-07-01 09:00:00 UTC)),
            ("2026-03-29 02:59:59", datetime!(2026-03-29 00:59:59 UTC)),
            ("2026-03-29 04:00:00", datetime!(2026-03-29 01:00:00 UTC)),
            ("2026-10-25 02:59:59", datetime!(2026-10-24 23:59:59 UTC)),
            ("2026-10-25 04:00:00", datetime!(2026-10-25 02:00:00 UTC)),
            (" 2026-07-01 12:00:00 ", datetime!(2026-07-01 09:00:00 UTC)),
        ] {
            let donation = normalize(json!({ "createdAt": created_at })).unwrap();
            assert_eq!(donation.occurred_at, instant, "createdAt {created_at:?}");
        }
    }

    #[test]
    fn created_at_inside_the_dst_gap_or_fold_is_kept_not_rejected() {
        for (created_at, instant) in [
            ("2026-03-29 03:30:00", datetime!(2026-03-29 01:30:00 UTC)),
            ("2026-10-25 03:30:00", datetime!(2026-10-25 00:30:00 UTC)),
        ] {
            let donation = normalize(json!({ "createdAt": created_at })).unwrap();
            assert_eq!(donation.occurred_at, instant, "createdAt {created_at:?}");
        }
    }

    #[test]
    fn unreadable_fields_reject_the_donation_naming_its_id() {
        for overrides in [
            json!({ "currency": "UAHX" }),
            json!({ "currency": "" }),
            json!({ "amount": "abc" }),
            json!({ "amount": "-5" }),
            json!({ "amount": "1,5" }),
            json!({ "createdAt": "2026-07-01T12:00:00" }),
            json!({ "createdAt": "2026-02-30 12:00:00" }),
            json!({ "createdAt": "" }),
        ] {
            let error = normalize(overrides.clone()).unwrap_err();
            assert!(
                matches!(&error, DonatelloError::InvalidDonation { donation_id, .. } if donation_id == "D-1"),
                "overrides {overrides}: {error:?}"
            );
        }
    }

    #[test]
    fn rejection_never_echoes_the_donor_name_or_message() {
        let error = normalize(json!({ "currency": "UAHX" })).unwrap_err();
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(DONOR_NAME), "{rendered}");
            assert!(!rendered.contains(MESSAGE), "{rendered}");
        }
    }

    #[test]
    fn unpublished_donation_hides_the_donor_name() {
        let donation = normalize(json!({ "isPublished": false })).unwrap();
        assert_eq!(donation.donor, Donor::Hidden);
    }

    #[test]
    fn unpublished_donation_carries_the_name_nowhere_in_its_output() {
        let donation = normalize(json!({ "isPublished": false })).unwrap();
        let serialized = serde_json::to_string(&donation).unwrap();
        assert!(!serialized.contains(DONOR_NAME), "{serialized}");
        assert!(!format!("{donation:?}").contains(DONOR_NAME));
    }

    #[test]
    fn donor_is_named_unless_the_name_is_blank() {
        for (overrides, donor) in [
            (json!({}), Donor::Named(DONOR_NAME.to_owned())),
            (
                json!({ "isPublished": null }),
                Donor::Named(DONOR_NAME.to_owned()),
            ),
            (
                json!({ "clientName": "  Олена  " }),
                Donor::Named("Олена".to_owned()),
            ),
            (json!({ "clientName": "" }), Donor::Anonymous),
            (json!({ "clientName": "   " }), Donor::Anonymous),
            (json!({ "clientName": null }), Donor::Anonymous),
        ] {
            assert_eq!(
                normalize(overrides.clone()).unwrap().donor,
                donor,
                "{overrides}"
            );
        }
    }

    #[test]
    fn message_is_trimmed_and_blank_or_absent_becomes_none() {
        for (message, expected) in [
            (json!(" thanks! "), Some("thanks!")),
            (json!(""), None),
            (json!(" \n "), None),
            (json!(null), None),
        ] {
            let donation = normalize(json!({ "message": message.clone() })).unwrap();
            assert_eq!(donation.message.as_deref(), expected, "message {message}");
        }
    }
}
