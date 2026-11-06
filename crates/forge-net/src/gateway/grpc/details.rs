use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD_PAD_INDIFFERENT;

const MAX_ENCODED_DETAILS_BYTES: usize = 16 * 1024;
const MAX_REASON_CHARS: usize = 128;
const MAX_VARINT_BYTES: usize = 10;
const VARINT_PAYLOAD_MASK: u8 = 0x7f;
const VARINT_CONTINUES: u8 = 0x80;
const VARINT_GROUP_BITS: usize = 7;
const FIELD_NUMBER_SHIFT: u32 = 3;
const WIRE_TYPE_MASK: u64 = 0b111;
const WIRE_VARINT: u64 = 0;
const WIRE_FIXED64: u64 = 1;
const WIRE_LENGTH_DELIMITED: u64 = 2;
const WIRE_FIXED32: u64 = 5;
const FIXED64_BYTES: usize = 8;
const FIXED32_BYTES: usize = 4;
const TYPE_URL_SEPARATOR: char = '/';

const STATUS_MESSAGE_FIELD: u64 = 2;
const STATUS_DETAILS_FIELD: u64 = 3;
const ANY_TYPE_URL_FIELD: u64 = 1;
const ANY_VALUE_FIELD: u64 = 2;
const ERROR_INFO_REASON_FIELD: u64 = 1;
const RETRY_INFO_DELAY_FIELD: u64 = 1;
const DURATION_SECONDS_FIELD: u64 = 1;
const DURATION_NANOS_FIELD: u64 = 2;
const ERROR_INFO_TYPE: &str = "google.rpc.ErrorInfo";
const RETRY_INFO_TYPE: &str = "google.rpc.RetryInfo";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct StatusDetails {
    pub(super) message: Option<String>,
    pub(super) reason: Option<String>,
    pub(super) retry_delay: Option<Duration>,
}

pub(super) fn decode_status_details(encoded: &[u8]) -> StatusDetails {
    if encoded.len() > MAX_ENCODED_DETAILS_BYTES {
        return StatusDetails::default();
    }
    let Ok(status) = STANDARD_PAD_INDIFFERENT.decode(encoded) else {
        return StatusDetails::default();
    };
    read_status(&status).unwrap_or_default()
}

struct Malformed;

enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Skipped,
}

struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Malformed> {
        let taken = self.rest.get(..count).ok_or(Malformed)?;
        self.rest = self.rest.get(count..).ok_or(Malformed)?;
        Ok(taken)
    }

    fn varint(&mut self) -> Result<u64, Malformed> {
        let mut value = 0_u64;
        for group in 0..MAX_VARINT_BYTES {
            let [byte] = *self.take(1)? else {
                return Err(Malformed);
            };
            let payload = u64::from(byte & VARINT_PAYLOAD_MASK);
            value |= payload << (group * VARINT_GROUP_BITS);
            if byte & VARINT_CONTINUES == 0 {
                return Ok(value);
            }
        }
        Err(Malformed)
    }

    fn field(&mut self) -> Result<Option<(u64, Value<'a>)>, Malformed> {
        if self.rest.is_empty() {
            return Ok(None);
        }
        let key = self.varint()?;
        let number = key >> FIELD_NUMBER_SHIFT;
        let value = match key & WIRE_TYPE_MASK {
            WIRE_VARINT => Value::Varint(self.varint()?),
            WIRE_LENGTH_DELIMITED => {
                let length = usize::try_from(self.varint()?).map_err(|_| Malformed)?;
                Value::Bytes(self.take(length)?)
            }
            WIRE_FIXED64 => {
                self.take(FIXED64_BYTES)?;
                Value::Skipped
            }
            WIRE_FIXED32 => {
                self.take(FIXED32_BYTES)?;
                Value::Skipped
            }
            _ => return Err(Malformed),
        };
        Ok(Some((number, value)))
    }
}

fn read_status(bytes: &[u8]) -> Result<StatusDetails, Malformed> {
    let mut details = StatusDetails::default();
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match (number, value) {
            (STATUS_MESSAGE_FIELD, Value::Bytes(text)) => {
                details.message = std::str::from_utf8(text).ok().map(str::to_owned);
            }
            (STATUS_DETAILS_FIELD, Value::Bytes(any)) => read_any(any, &mut details)?,
            _ => {}
        }
    }
    Ok(details)
}

fn read_any(bytes: &[u8], details: &mut StatusDetails) -> Result<(), Malformed> {
    let mut type_name = None;
    let mut payload: &[u8] = &[];
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match (number, value) {
            (ANY_TYPE_URL_FIELD, Value::Bytes(url)) => {
                type_name = std::str::from_utf8(url)
                    .ok()
                    .and_then(|url| url.rsplit(TYPE_URL_SEPARATOR).next());
            }
            (ANY_VALUE_FIELD, Value::Bytes(value)) => payload = value,
            _ => {}
        }
    }
    match type_name {
        Some(ERROR_INFO_TYPE) if details.reason.is_none() => {
            details.reason = read_error_reason(payload)?;
        }
        Some(RETRY_INFO_TYPE) if details.retry_delay.is_none() => {
            details.retry_delay = read_retry_delay(payload)?;
        }
        _ => {}
    }
    Ok(())
}

fn read_error_reason(bytes: &[u8]) -> Result<Option<String>, Malformed> {
    let mut reason = None;
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        if let (ERROR_INFO_REASON_FIELD, Value::Bytes(text)) = (number, value) {
            reason = std::str::from_utf8(text)
                .ok()
                .filter(|text| is_reason_identifier(text))
                .map(str::to_owned);
        }
    }
    Ok(reason)
}

fn is_reason_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_REASON_CHARS
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn read_retry_delay(bytes: &[u8]) -> Result<Option<Duration>, Malformed> {
    let mut delay = None;
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        if let (RETRY_INFO_DELAY_FIELD, Value::Bytes(duration)) = (number, value) {
            delay = read_duration(duration)?;
        }
    }
    Ok(delay)
}

fn read_duration(bytes: &[u8]) -> Result<Option<Duration>, Malformed> {
    let mut seconds = 0_u64;
    let mut nanos = 0_u64;
    let mut reader = Reader::new(bytes);
    while let Some((number, value)) = reader.field()? {
        match (number, value) {
            (DURATION_SECONDS_FIELD, Value::Varint(raw)) => seconds = raw,
            (DURATION_NANOS_FIELD, Value::Varint(raw)) => nanos = raw,
            _ => {}
        }
    }
    if i64::try_from(seconds).is_err() {
        return Ok(None);
    }
    let Ok(nanos) = u32::try_from(nanos) else {
        return Ok(None);
    };
    Ok(Duration::from_secs(seconds).checked_add(Duration::from_nanos(u64::from(nanos))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let low = u8::try_from(value & u64::from(VARINT_PAYLOAD_MASK)).unwrap();
            value >>= VARINT_GROUP_BITS;
            if value == 0 {
                out.push(low);
                return out;
            }
            out.push(low | VARINT_CONTINUES);
        }
    }

    fn bytes_field(number: u64, bytes: &[u8]) -> Vec<u8> {
        let mut out = varint((number << FIELD_NUMBER_SHIFT) | WIRE_LENGTH_DELIMITED);
        out.extend(varint(u64::try_from(bytes.len()).unwrap()));
        out.extend_from_slice(bytes);
        out
    }

    fn varint_field(number: u64, value: u64) -> Vec<u8> {
        let mut out = varint((number << FIELD_NUMBER_SHIFT) | WIRE_VARINT);
        out.extend(varint(value));
        out
    }

    fn any(type_name: &str, value: &[u8]) -> Vec<u8> {
        let url = format!("type.googleapis.com/{type_name}");
        let mut out = bytes_field(ANY_TYPE_URL_FIELD, url.as_bytes());
        out.extend(bytes_field(ANY_VALUE_FIELD, value));
        out
    }

    fn error_info(reason: &str) -> Vec<u8> {
        any(
            ERROR_INFO_TYPE,
            &bytes_field(ERROR_INFO_REASON_FIELD, reason.as_bytes()),
        )
    }

    fn retry_info(seconds: u64, nanos: u64) -> Vec<u8> {
        let mut duration = varint_field(DURATION_SECONDS_FIELD, seconds);
        duration.extend(varint_field(DURATION_NANOS_FIELD, nanos));
        any(
            RETRY_INFO_TYPE,
            &bytes_field(RETRY_INFO_DELAY_FIELD, &duration),
        )
    }

    fn status(message: &str, details: &[Vec<u8>]) -> Vec<u8> {
        let mut out = varint_field(1, 8);
        out.extend(bytes_field(STATUS_MESSAGE_FIELD, message.as_bytes()));
        for detail in details {
            out.extend(bytes_field(STATUS_DETAILS_FIELD, detail));
        }
        out
    }

    fn decoded(status: &[u8]) -> StatusDetails {
        decode_status_details(STANDARD.encode(status).as_bytes())
    }

    #[test]
    fn message_reason_and_retry_delay_are_read_from_a_full_status() {
        let details = decoded(&status(
            "slow down",
            &[error_info("rateLimitExceeded"), retry_info(3, 250_000_000)],
        ));

        assert_eq!(
            details,
            StatusDetails {
                message: Some("slow down".to_owned()),
                reason: Some("rateLimitExceeded".to_owned()),
                retry_delay: Some(Duration::from_millis(3_250)),
            }
        );
    }

    #[test]
    fn unknown_detail_types_and_fixed_width_fields_are_skipped() {
        let mut bytes = status("busy", &[any("google.rpc.Help", b"\x0a\x01x")]);
        bytes.extend(varint((9 << FIELD_NUMBER_SHIFT) | WIRE_FIXED64));
        bytes.extend([0_u8; FIXED64_BYTES]);
        bytes.extend(varint((10 << FIELD_NUMBER_SHIFT) | WIRE_FIXED32));
        bytes.extend([0_u8; FIXED32_BYTES]);
        bytes.extend(bytes_field(
            STATUS_DETAILS_FIELD,
            &error_info("quotaExceeded"),
        ));

        assert_eq!(decoded(&bytes).reason.as_deref(), Some("quotaExceeded"));
    }

    fn status_of_raw_len(raw_len: usize) -> Vec<u8> {
        let details = [error_info("quotaExceeded")];
        let mut filler = raw_len;
        loop {
            let candidate = status(&"m".repeat(filler), &details);
            if candidate.len() <= raw_len {
                assert_eq!(candidate.len(), raw_len);
                return candidate;
            }
            filler -= 1;
        }
    }

    #[test]
    fn encoded_details_exactly_at_the_size_cap_are_read() {
        let encoded = STANDARD.encode(status_of_raw_len(MAX_ENCODED_DETAILS_BYTES / 4 * 3));
        assert_eq!(encoded.len(), MAX_ENCODED_DETAILS_BYTES);

        let read = decode_status_details(encoded.as_bytes());

        assert_eq!(read.reason.as_deref(), Some("quotaExceeded"));
    }

    #[test]
    fn encoded_details_one_base64_group_over_the_size_cap_are_refused() {
        let encoded = STANDARD.encode(status_of_raw_len(MAX_ENCODED_DETAILS_BYTES / 4 * 3 + 3));
        assert_eq!(encoded.len(), MAX_ENCODED_DETAILS_BYTES + 4);

        assert_eq!(
            decode_status_details(encoded.as_bytes()),
            StatusDetails::default()
        );
    }

    #[test]
    fn every_truncation_of_a_status_yields_no_reason() {
        let whole = status("cut", &[error_info("quotaExceeded")]);
        assert_eq!(decoded(&whole).reason.as_deref(), Some("quotaExceeded"));

        for cut in 0..whole.len() {
            assert_eq!(decoded(&whole[..cut]).reason, None, "cut at {cut}");
        }
    }

    #[test]
    fn malformed_wire_input_decodes_to_empty_details() {
        let overlong_varint = [VARINT_CONTINUES; MAX_VARINT_BYTES + 1];
        let mut length_past_end =
            varint((STATUS_MESSAGE_FIELD << FIELD_NUMBER_SHIFT) | WIRE_LENGTH_DELIMITED);
        length_past_end.extend(varint(u64::MAX));
        let mut group_wire_type = status("kept", &[]);
        group_wire_type.push(0x0b);
        for (label, encoded) in [
            ("bad base64", b"%%not-base64%%".to_vec()),
            (
                "overlong varint",
                STANDARD.encode(overlong_varint).into_bytes(),
            ),
            (
                "length past end",
                STANDARD.encode(&length_past_end).into_bytes(),
            ),
            (
                "group wire type",
                STANDARD.encode(&group_wire_type).into_bytes(),
            ),
        ] {
            assert_eq!(
                decode_status_details(&encoded),
                StatusDetails::default(),
                "{label}"
            );
        }
    }

    #[test]
    fn reason_must_be_a_bounded_identifier() {
        let longest = "a".repeat(MAX_REASON_CHARS);
        let too_long = "a".repeat(MAX_REASON_CHARS + 1);
        for (reason, kept) in [
            ("quota_exceeded.v2-x", true),
            (longest.as_str(), true),
            (too_long.as_str(), false),
            ("", false),
            ("quota exceeded", false),
            ("quota\nExceeded", false),
            ("quotaExceeded\u{00e9}", false),
        ] {
            let read = decoded(&status("", &[error_info(reason)])).reason;

            assert_eq!(read.is_some(), kept, "{reason:?}");
        }
    }

    #[test]
    fn retry_delay_out_of_range_is_dropped() {
        for (seconds, nanos) in [(u64::MAX, 0), (0, u64::from(u32::MAX) + 1)] {
            let read = decoded(&status("", &[retry_info(seconds, nanos)])).retry_delay;

            assert_eq!(read, None, "{seconds}s {nanos}ns");
        }
    }

    #[test]
    fn first_error_info_reason_wins_over_later_ones() {
        let read = decoded(&status(
            "",
            &[error_info("quotaExceeded"), error_info("rateLimitExceeded")],
        ));

        assert_eq!(read.reason.as_deref(), Some("quotaExceeded"));
    }
}
