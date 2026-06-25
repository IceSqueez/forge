use async_trait::async_trait;
use forge_storage::{
    BlocklistMode, FilterRule, FilterRuleKind, StorageError, TtsFiltersRepo, TtsPipelineSettings,
    UrlMode,
};

use crate::error::SqliteStorageError;

pub struct SqliteTtsFiltersRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteTtsFiltersRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

type RuleRow = (String, String, i64, i64, String, String);

fn decode_rule_row(row: RuleRow) -> Result<FilterRule, StorageError> {
    let (id, name, enabled, position, kind_str, params_json) = row;
    let kind = decode_rule_kind(&kind_str, &params_json)?;
    Ok(FilterRule {
        id,
        name,
        enabled: enabled != 0,
        position: position as u32,
        kind,
    })
}

fn decode_rule_kind(kind: &str, params_json: &str) -> Result<FilterRuleKind, StorageError> {
    #[derive(serde::Deserialize)]
    struct LiteralParams {
        pattern: String,
        replacement: String,
    }
    #[derive(serde::Deserialize)]
    struct RegexParams {
        pattern: String,
        replacement: String,
    }
    #[derive(serde::Deserialize)]
    struct BlocklistParams {
        words: Vec<String>,
        mode: BlocklistMode,
    }

    match kind {
        "literal" => {
            let p: LiteralParams =
                serde_json::from_str(params_json).map_err(StorageError::Serialization)?;
            Ok(FilterRuleKind::Literal {
                pattern: p.pattern,
                replacement: p.replacement,
            })
        }
        "regex" => {
            let p: RegexParams =
                serde_json::from_str(params_json).map_err(StorageError::Serialization)?;
            Ok(FilterRuleKind::Regex {
                pattern: p.pattern,
                replacement: p.replacement,
            })
        }
        "blocklist" => {
            let p: BlocklistParams =
                serde_json::from_str(params_json).map_err(StorageError::Serialization)?;
            Ok(FilterRuleKind::Blocklist {
                words: p.words,
                mode: p.mode,
            })
        }
        other => Err(StorageError::Parse(format!(
            "unknown filter rule kind: {other}"
        ))),
    }
}

fn encode_rule_kind(kind: &FilterRuleKind) -> (&'static str, String) {
    #[derive(serde::Serialize)]
    struct LiteralParams<'a> {
        pattern: &'a str,
        replacement: &'a str,
    }
    #[derive(serde::Serialize)]
    struct RegexParams<'a> {
        pattern: &'a str,
        replacement: &'a str,
    }
    #[derive(serde::Serialize)]
    struct BlocklistParams<'a> {
        words: &'a [String],
        mode: BlocklistMode,
    }

    match kind {
        FilterRuleKind::Literal {
            pattern,
            replacement,
        } => {
            let json = serde_json::to_string(&LiteralParams {
                pattern,
                replacement,
            })
            .unwrap_or_default();
            ("literal", json)
        }
        FilterRuleKind::Regex {
            pattern,
            replacement,
        } => {
            let json = serde_json::to_string(&RegexParams {
                pattern,
                replacement,
            })
            .unwrap_or_default();
            ("regex", json)
        }
        FilterRuleKind::Blocklist { words, mode } => {
            let json =
                serde_json::to_string(&BlocklistParams { words, mode: *mode }).unwrap_or_default();
            ("blocklist", json)
        }
    }
}

type SettingsRow = (String, Option<i64>, String, i64, i64);

fn decode_settings_row(row: SettingsRow) -> Result<TtsPipelineSettings, StorageError> {
    let (url_mode_str, max_length, blocklist_mode_str, strip_twitch, strip_reward) = row;

    let url_mode: UrlMode = serde_json::from_str(&format!("\"{url_mode_str}\""))
        .map_err(|_| StorageError::Parse(format!("unknown url_mode: {url_mode_str}")))?;

    let blocklist_mode: BlocklistMode = serde_json::from_str(&format!("\"{blocklist_mode_str}\""))
        .map_err(|_| {
            StorageError::Parse(format!("unknown blocklist_mode: {blocklist_mode_str}"))
        })?;

    Ok(TtsPipelineSettings {
        url_mode,
        max_length: max_length.map(|v| v as u32),
        blocklist_mode,
        strip_twitch_emotes: strip_twitch != 0,
        strip_reward_emotes: strip_reward != 0,
    })
}

#[async_trait]
impl TtsFiltersRepo for SqliteTtsFiltersRepo {
    async fn list_rules(&self) -> Result<Vec<FilterRule>, StorageError> {
        let rows: Vec<RuleRow> = sqlx::query_as(
            "SELECT id, name, enabled, position, kind, params
             FROM tts_filter_rules ORDER BY position ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter().map(decode_rule_row).collect()
    }

    async fn replace_rules(&self, rules: &[FilterRule]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await.map_err(SqliteStorageError::Sqlx)?;

        sqlx::query("DELETE FROM tts_filter_rules")
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        for rule in rules {
            let (kind_str, params_json) = encode_rule_kind(&rule.kind);
            sqlx::query(
                "INSERT INTO tts_filter_rules (id, name, enabled, position, kind, params)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&rule.id)
            .bind(&rule.name)
            .bind(rule.enabled as i64)
            .bind(rule.position as i64)
            .bind(kind_str)
            .bind(&params_json)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        }

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn get_pipeline_settings(&self) -> Result<TtsPipelineSettings, StorageError> {
        let row: Option<SettingsRow> = sqlx::query_as(
            "SELECT url_mode, max_length, blocklist_mode,
                    strip_twitch_emotes, strip_reward_emotes
             FROM tts_pipeline_settings WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(TtsPipelineSettings::default()),
            Some(r) => decode_settings_row(r),
        }
    }

    async fn set_pipeline_settings(
        &self,
        settings: &TtsPipelineSettings,
    ) -> Result<(), StorageError> {
        let url_mode_str = serde_json::to_string(&settings.url_mode)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_owned();
        let blocklist_mode_str = serde_json::to_string(&settings.blocklist_mode)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_owned();

        sqlx::query(
            "INSERT INTO tts_pipeline_settings
                (id, url_mode, max_length, blocklist_mode, strip_twitch_emotes, strip_reward_emotes)
             VALUES (1, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                url_mode            = excluded.url_mode,
                max_length          = excluded.max_length,
                blocklist_mode      = excluded.blocklist_mode,
                strip_twitch_emotes = excluded.strip_twitch_emotes,
                strip_reward_emotes = excluded.strip_reward_emotes",
        )
        .bind(&url_mode_str)
        .bind(settings.max_length.map(|v| v as i64))
        .bind(&blocklist_mode_str)
        .bind(settings.strip_twitch_emotes as i64)
        .bind(settings.strip_reward_emotes as i64)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }
}
