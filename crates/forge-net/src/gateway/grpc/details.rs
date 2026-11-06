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
