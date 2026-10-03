use async_trait::async_trait;
use forge_storage::{DonationRepo, StorageError, StoredDonation};
use forge_types::{
    CurrencyCode, Donation, DonationOrigin, Donor, DonorVisibility, IntegrationId, MoneyAmount,
};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

const NANOS_PER_MILLI: i128 = 1_000_000;

const ORIGIN_LIVE: &str = "live";
const ORIGIN_HISTORY: &str = "history";

const SELECT_COLUMNS: &str = "SELECT provider, donation_id, amount_micros, currency, donor_kind,
        donor_name, message, occurred_at, received_at, origin, announced_at
     FROM donations";

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / NANOS_PER_MILLI) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch {ms}: {e}")))
}

fn encode_origin(origin: DonationOrigin) -> Result<&'static str, StorageError> {
    match origin {
        DonationOrigin::Live => Ok(ORIGIN_LIVE),
        DonationOrigin::History => Ok(ORIGIN_HISTORY),
        DonationOrigin::Test => Err(StorageError::ValidationFailed {
            field: "origin".to_owned(),
            reason: "test donations are never recorded".to_owned(),
        }),
    }
}

fn decode_origin(raw: &str) -> Result<DonationOrigin, SqliteStorageError> {
    match raw {
        ORIGIN_LIVE => Ok(DonationOrigin::Live),
        ORIGIN_HISTORY => Ok(DonationOrigin::History),
        other => Err(SqliteStorageError::Decode(format!(
            "invalid donation origin '{other}'"
        ))),
    }
}

fn decode_donor(kind: &str, name: Option<String>) -> Result<Donor, SqliteStorageError> {
    match (kind, name) {
        (kind, Some(name)) if kind == DonorVisibility::Named.as_str() => Ok(Donor::Named(name)),
        (kind, None) if kind == DonorVisibility::Anonymous.as_str() => Ok(Donor::Anonymous),
        (kind, None) if kind == DonorVisibility::Hidden.as_str() => Ok(Donor::Hidden),
        (kind, _) => Err(SqliteStorageError::Decode(format!(
            "invalid donor kind '{kind}' for the stored name presence"
        ))),
    }
}

fn limit_param(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

#[derive(sqlx::FromRow)]
struct DonationRow {
    provider: String,
    donation_id: String,
    amount_micros: i64,
    currency: String,
    donor_kind: String,
    donor_name: Option<String>,
    message: Option<String>,
    occurred_at: i64,
    received_at: i64,
    origin: String,
    announced_at: Option<i64>,
}

fn decode_row(row: DonationRow) -> Result<StoredDonation, SqliteStorageError> {
    let micros = u64::try_from(row.amount_micros).map_err(|_| {
        SqliteStorageError::Decode(format!("negative donation amount {}", row.amount_micros))
    })?;
    let currency = CurrencyCode::parse(&row.currency)
        .map_err(|e| SqliteStorageError::Decode(e.to_string()))?;
    Ok(StoredDonation {
        donation: Donation {
            provider: IntegrationId::new(row.provider),
            donation_id: row.donation_id,
            donor: decode_donor(&row.donor_kind, row.donor_name)?,
            message: row.message,
            amount: MoneyAmount::from_micros(micros, currency),
            occurred_at: from_epoch_ms(row.occurred_at)?,
            origin: decode_origin(&row.origin)?,
        },
        received_at: from_epoch_ms(row.received_at)?,
        announced_at: row.announced_at.map(from_epoch_ms).transpose()?,
    })
}

fn decode_rows(rows: Vec<DonationRow>) -> Result<Vec<StoredDonation>, StorageError> {
    Ok(rows
        .into_iter()
        .map(decode_row)
        .collect::<Result<Vec<_>, _>>()?)
}

pub struct SqliteDonationRepo {
    db: SqlitePools,
}

impl SqliteDonationRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl DonationRepo for SqliteDonationRepo {
    async fn insert_if_new(
        &self,
        donation: &Donation,
        received_at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let origin = encode_origin(donation.origin)?;
        let amount_micros = i64::try_from(donation.amount.micros()).map_err(|_| {
            StorageError::ValidationFailed {
                field: "amount".to_owned(),
                reason: "amount does not fit the ledger range".to_owned(),
            }
        })?;
        let received_at_ms = to_epoch_ms(received_at);
        let announced_at = (!donation.origin.announces()).then_some(received_at_ms);

        let result = sqlx::query(
            "INSERT INTO donations
                (provider, donation_id, amount_micros, currency, donor_kind, donor_name,
                 message, occurred_at, received_at, origin, announced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(provider, donation_id) DO NOTHING",
        )
        .bind(donation.provider.as_str())
        .bind(&donation.donation_id)
        .bind(amount_micros)
        .bind(donation.amount.currency().as_str())
        .bind(donation.donor.visibility().as_str())
        .bind(donation.donor.shown_name())
        .bind(donation.message.as_deref())
        .bind(to_epoch_ms(donation.occurred_at))
        .bind(received_at_ms)
        .bind(origin)
        .bind(announced_at)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn mark_announced(
        &self,
        provider: &IntegrationId,
        donation_id: &str,
        announced_at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE donations SET announced_at = ?
             WHERE provider = ? AND donation_id = ? AND announced_at IS NULL",
        )
        .bind(to_epoch_ms(announced_at))
        .bind(provider.as_str())
        .bind(donation_id)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<StoredDonation>, StorageError> {
        let rows: Vec<DonationRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLUMNS} ORDER BY occurred_at DESC, seq DESC LIMIT ?"
        )))
        .bind(limit_param(limit))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn has_donations_from(&self, provider: &IntegrationId) -> Result<bool, StorageError> {
        let found: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM donations WHERE provider = ?)")
                .bind(provider.as_str())
                .fetch_one(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        Ok(found)
    }

    async fn list_unannounced_occurred_since(
        &self,
        not_before: OffsetDateTime,
    ) -> Result<Vec<StoredDonation>, StorageError> {
        let rows: Vec<DonationRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLUMNS} WHERE announced_at IS NULL AND occurred_at >= ?
             ORDER BY occurred_at ASC, seq ASC"
        )))
        .bind(to_epoch_ms(not_before))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn mark_announced_all_occurred_before(
        &self,
        not_before: OffsetDateTime,
        announced_at: OffsetDateTime,
    ) -> Result<u64, StorageError> {
        let result = sqlx::query(
            "UPDATE donations SET announced_at = ?
             WHERE announced_at IS NULL AND occurred_at < ?",
        )
        .bind(to_epoch_ms(announced_at))
        .bind(to_epoch_ms(not_before))
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }
}
