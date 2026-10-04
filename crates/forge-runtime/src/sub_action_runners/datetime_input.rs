use forge_types::Variant;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(crate) fn resolve_datetime(v: &Variant, stack_interp: &str) -> Result<OffsetDateTime, String> {
    if let Some(dt) = v.as_datetime() {
        return Ok(*dt);
    }
    if let Some(secs) = v.as_int() {
        return OffsetDateTime::from_unix_timestamp(secs)
            .map_err(|e| format!("unix timestamp out of range: {e}"));
    }
    let s = stack_interp.trim();
    OffsetDateTime::parse(s, &Rfc3339).or_else(|_| {
        s.parse::<i64>()
            .ok()
            .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
            .ok_or_else(|| format!("cannot parse '{s}' as a datetime or unix timestamp"))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::UtcOffset;

    fn instant(unix_secs: i64, offset_hours: i8) -> (OffsetDateTime, UtcOffset) {
        let offset = UtcOffset::from_hms(offset_hours, 0, 0).unwrap();
        let at = OffsetDateTime::from_unix_timestamp(unix_secs)
            .unwrap()
            .to_offset(offset);
        (at, offset)
    }

    fn resolved(raw: &Variant, interpolated: &str) -> Result<(OffsetDateTime, UtcOffset), String> {
        resolve_datetime(raw, interpolated).map(|at| (at, at.offset()))
    }

    #[test]
    fn resolve_datetime_reads_each_accepted_input_shape() {
        let typed = instant(1_791_115_200, 0).0;
        let placeholder = Variant::String("%when%".to_owned());
        for (raw, interpolated, expected) in [
            (
                Variant::Datetime(typed),
                "ignored",
                instant(1_791_115_200, 0),
            ),
            (
                Variant::Int(1_791_115_200),
                "ignored",
                instant(1_791_115_200, 0),
            ),
            (Variant::Int(-1), "", instant(-1, 0)),
            (
                placeholder.clone(),
                "2026-10-04T15:00:00+03:00",
                instant(1_791_115_200, 3),
            ),
            (
                placeholder.clone(),
                "  2026-10-04T12:00:00Z\n",
                instant(1_791_115_200, 0),
            ),
            (
                placeholder.clone(),
                " 1791115200 ",
                instant(1_791_115_200, 0),
            ),
            (placeholder, "-1", instant(-1, 0)),
        ] {
            assert_eq!(
                resolved(&raw, interpolated),
                Ok(expected),
                "{raw:?} / {interpolated:?}"
            );
        }
    }

    #[test]
    fn resolve_datetime_rejects_unreadable_or_unrepresentable_input() {
        let placeholder = Variant::String("%when%".to_owned());
        for (raw, interpolated, reason) in [
            (Variant::Int(i64::MAX), "", "out of range"),
            (placeholder.clone(), "", "cannot parse ''"),
            (placeholder.clone(), "tomorrow", "cannot parse 'tomorrow'"),
            (placeholder.clone(), "2026-10-04 12:00:00", "cannot parse"),
            (placeholder.clone(), "2026-10-04T12:00:00", "cannot parse"),
            (placeholder, "99999999999999999", "cannot parse"),
            (Variant::Float(1.5), "1.5", "cannot parse '1.5'"),
        ] {
            let error = resolve_datetime(&raw, interpolated).unwrap_err();
            assert!(
                error.contains(reason),
                "{raw:?} / {interpolated:?}: {error}"
            );
        }
    }
}
