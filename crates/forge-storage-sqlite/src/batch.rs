use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::error::SqliteStorageError;

/// SQLite's compile-time ceiling on `?` parameters in one statement (3.32 and later).
const MAX_BOUND_PARAMETERS: usize = 32_766;
const MAX_ROWS_PER_STATEMENT: usize = 256;

pub(crate) fn rows_per_statement(columns: usize) -> usize {
    (MAX_BOUND_PARAMETERS / columns.max(1)).clamp(1, MAX_ROWS_PER_STATEMENT)
}

/// Runs one multi-row `INSERT` per chunk on `conn`; the caller owns the transaction.
pub(crate) async fn insert_rows<T>(
    conn: &mut SqliteConnection,
    head: &str,
    columns: usize,
    tail: &str,
    rows: &[T],
    mut bind_row: impl FnMut(&mut sqlx::query_builder::Separated<'_, Sqlite, &'static str>, &T),
) -> Result<(), SqliteStorageError> {
    for chunk in rows.chunks(rows_per_statement(columns)) {
        let mut builder = QueryBuilder::<Sqlite>::new(head);
        builder.push_values(chunk, |mut separated, row| bind_row(&mut separated, row));
        builder.push(tail);
        builder
            .build()
            .execute(&mut *conn)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
    }
    Ok(())
}

