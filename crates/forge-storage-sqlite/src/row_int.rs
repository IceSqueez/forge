use crate::error::SqliteStorageError;

pub(crate) fn decode_u32(column: &str, stored: i64) -> Result<u32, SqliteStorageError> {
    u32::try_from(stored)
        .map_err(|_| SqliteStorageError::Decode(format!("{column} is out of range: {stored}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_u32_keeps_both_ends_of_the_range() {
        for stored in [0, i64::from(u32::MAX)] {
            assert_eq!(
                decode_u32("count", stored).map(i64::from).ok(),
                Some(stored)
            );
        }
    }

    #[test]
    fn decode_u32_refuses_a_value_outside_the_range_naming_column_and_value() {
        for stored in [-1, i64::from(u32::MAX) + 1] {
            let error = decode_u32("step_count", stored).unwrap_err();
            let SqliteStorageError::Decode(detail) = error else {
                panic!("{stored} did not fail as a decode error: {error}");
            };
            assert!(
                detail.contains("step_count") && detail.contains(&stored.to_string()),
                "{detail}"
            );
        }
    }
}
