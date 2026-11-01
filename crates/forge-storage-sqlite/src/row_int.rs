use crate::error::SqliteStorageError;

pub(crate) fn decode_u32(column: &str, stored: i64) -> Result<u32, SqliteStorageError> {
    u32::try_from(stored)
        .map_err(|_| SqliteStorageError::Decode(format!("{column} is out of range: {stored}")))
}
