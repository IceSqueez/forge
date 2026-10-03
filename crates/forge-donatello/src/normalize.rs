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
