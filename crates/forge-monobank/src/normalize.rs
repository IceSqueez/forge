use forge_types::{Donation, DonationOrigin, Donor, IntegrationId};
use time::OffsetDateTime;

use crate::currency;
use crate::error::MonobankError;
use crate::wire::StatementItemWire;

const SENDER_PREFIX: &str = "Від:";

pub(crate) fn donation_from_item(
    provider: &IntegrationId,
    item: StatementItemWire,
    origin: DonationOrigin,
) -> Result<Option<Donation>, MonobankError> {
    let Ok(minor_units) = u64::try_from(item.amount) else {
        return Ok(None);
    };
    if minor_units == 0 {
        return Ok(None);
    }
    let invalid = |reason: String| MonobankError::InvalidDonation {
        donation_id: item.id.clone(),
        reason,
    };
    let currency = currency::from_iso_numeric(item.currency_code).ok_or_else(|| {
        invalid(format!(
            "currency code {} is not supported",
            item.currency_code
        ))
    })?;
    let amount = currency
        .amount(minor_units)
        .map_err(|error| invalid(error.to_string()))?;
    let occurred_at = OffsetDateTime::from_unix_timestamp(item.time)
        .map_err(|_| invalid(format!("time {} is out of range", item.time)))?;
    let donor = Donor::named(&donor_name(
        item.counter_name.as_deref(),
        item.description.as_deref(),
    ));
    let message = item
        .comment
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    Ok(Some(Donation {
        provider: provider.clone(),
        donation_id: item.id,
        donor,
        message,
        amount,
        occurred_at,
        origin,
    }))
}

fn donor_name(counter_name: Option<&str>, description: Option<&str>) -> String {
    counter_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            description.map(|text| {
                let text = text.trim();
                text.strip_prefix(SENDER_PREFIX).unwrap_or(text).trim()
            })
        })
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_types::{Donation, DonationOrigin, Donor, IntegrationId};
    use serde_json::{Value, json};

    use super::donation_from_item;
    use crate::error::MonobankError;
    use crate::wire::StatementItemWire;

    const DONOR_NAME: &str = "Secret Donor Name";
    const COMMENT: &str = "private donor comment";
    const OCCURRED_UNIX: i64 = 1_791_000_000;

    fn item(overrides: Value) -> Value {
        let mut item = json!({
            "id": "TX-1",
            "time": OCCURRED_UNIX,
            "description": format!("Від: {DONOR_NAME}"),
            "amount": 10_000,
            "operationAmount": 10_000,
            "currencyCode": 980,
            "comment": COMMENT,
            "counterName": DONOR_NAME,
            "hold": false,
        });
        for (key, value) in overrides.as_object().unwrap() {
            item[key] = value.clone();
        }
        item
    }

    fn normalize(overrides: Value) -> Result<Option<Donation>, MonobankError> {
        let wire: StatementItemWire = serde_json::from_value(item(overrides)).unwrap();
        donation_from_item(
            &IntegrationId::from_static("monobank"),
            wire,
            DonationOrigin::Live,
        )
    }

    fn donation(overrides: Value) -> Donation {
        normalize(overrides).unwrap().unwrap()
    }

    #[test]
    fn minor_units_become_micros_in_the_iso_numeric_currency() {
        for (amount, currency, micros, code) in [
            (1, 980, 10_000, "UAH"),
            (10_000, 980, 100_000_000, "UAH"),
            (12_345, 840, 123_450_000, "USD"),
            (99, 978, 990_000, "EUR"),
        ] {
            let donation = donation(json!({ "amount": amount, "currencyCode": currency }));
            assert_eq!(
                (
                    donation.amount.micros(),
                    donation.amount.currency().as_str()
                ),
                (micros, code),
                "amount {amount} currency {currency}"
            );
        }
    }

    #[test]
    fn zero_and_outgoing_amounts_are_skipped_not_rejected() {
        for amount in [0, -1, -10_000, i64::MIN] {
            assert!(
                matches!(normalize(json!({ "amount": amount })), Ok(None)),
                "amount {amount}"
            );
        }
    }

    #[test]
    fn occurred_at_is_the_transaction_unix_time() {
        assert_eq!(
            donation(json!({})).occurred_at.unix_timestamp(),
            OCCURRED_UNIX
        );
    }

    #[test]
    fn unreadable_items_are_rejected_naming_the_transaction() {
        for overrides in [
            json!({ "currencyCode": 643 }),
            json!({ "currencyCode": 0 }),
            json!({ "time": i64::MAX }),
            json!({ "amount": i64::MAX }),
        ] {
            let error = normalize(overrides.clone()).unwrap_err();
            assert!(
                matches!(&error, MonobankError::InvalidDonation { donation_id, .. } if donation_id == "TX-1"),
                "overrides {overrides}: {error:?}"
            );
        }
    }

    #[test]
    fn rejection_never_echoes_the_donor_name_or_comment() {
        let error = normalize(json!({ "currencyCode": 643 })).unwrap_err();
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(DONOR_NAME), "{rendered}");
            assert!(!rendered.contains(COMMENT), "{rendered}");
        }
    }

    #[test]
    fn donor_prefers_the_counter_name_then_the_description_without_the_sender_prefix() {
        for (counter_name, description, donor) in [
            (
                json!("Олена"),
                json!("Від: Петро"),
                Donor::Named("Олена".to_owned()),
            ),
            (
                json!("  Олена  "),
                json!(null),
                Donor::Named("Олена".to_owned()),
            ),
            (
                json!("   "),
                json!("Від: Петро"),
                Donor::Named("Петро".to_owned()),
            ),
            (
                json!(null),
                json!("Від: Петро"),
                Donor::Named("Петро".to_owned()),
            ),
            (
                json!(null),
                json!("  Від:Петро "),
                Donor::Named("Петро".to_owned()),
            ),
            (
                json!(null),
                json!("Петро"),
                Donor::Named("Петро".to_owned()),
            ),
            (json!(null), json!("Від: "), Donor::Anonymous),
            (json!(""), json!(""), Donor::Anonymous),
            (json!(null), json!(null), Donor::Anonymous),
        ] {
            let donation = donation(json!({
                "counterName": counter_name.clone(),
                "description": description.clone(),
            }));
            assert_eq!(
                donation.donor, donor,
                "counterName {counter_name} description {description}"
            );
        }
    }

    #[test]
    fn comment_becomes_the_trimmed_message_and_blank_or_absent_becomes_none() {
        for (comment, message) in [
            (json!(" дякую! "), Some("дякую!")),
            (json!(""), None),
            (json!(" \n "), None),
            (json!(null), None),
        ] {
            let donation = donation(json!({ "comment": comment.clone() }));
            assert_eq!(donation.message.as_deref(), message, "comment {comment}");
        }
    }
}
