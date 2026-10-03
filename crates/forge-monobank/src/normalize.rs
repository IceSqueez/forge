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
