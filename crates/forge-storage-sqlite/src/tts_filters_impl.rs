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

#[allow(clippy::type_complexity)]
type SettingsRow = (
    String,
    Option<i64>,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
);
type SettingsExtRow = (Option<String>, i64, i64, String, i64);

fn decode_settings_row(
    row: SettingsRow,
    ext: SettingsExtRow,
) -> Result<TtsPipelineSettings, StorageError> {
    let (
        url_mode_str,
        max_length,
        blocklist_mode_str,
        strip_twitch,
        strip_reward,
        skip_contains_url,
        skip_starts_with_bang,
        skip_from_bot_accounts,
        bot_accounts_json,
        skip_longer_than,
        longer_than_max_chars,
        skip_repeat_of_recent,
        repeat_of_recent_window,
        output_read_display_name_first,
        output_emote_to_word,
    ) = row;
    let (
        skip_prefix,
        skip_emote_only,
        skip_mostly_non_latin,
        skip_custom_regexes_json,
        output_sanitize_punctuation,
    ) = ext;

    let url_mode: UrlMode = serde_json::from_str(&format!("\"{url_mode_str}\""))
        .map_err(|_| StorageError::Parse(format!("unknown url_mode: {url_mode_str}")))?;

    let blocklist_mode: BlocklistMode = serde_json::from_str(&format!("\"{blocklist_mode_str}\""))
        .map_err(|_| {
            StorageError::Parse(format!("unknown blocklist_mode: {blocklist_mode_str}"))
        })?;

    let bot_accounts: Vec<String> = serde_json::from_str(&bot_accounts_json).map_err(|_| {
        StorageError::Parse(format!("invalid bot_accounts json: {bot_accounts_json}"))
    })?;

    let skip_custom_regexes: Vec<String> = serde_json::from_str(&skip_custom_regexes_json)
        .map_err(|_| {
            StorageError::Parse(format!(
                "invalid skip_custom_regexes json: {skip_custom_regexes_json}"
            ))
        })?;

    Ok(TtsPipelineSettings {
        url_mode,
        max_length: max_length.map(|v| v as u32),
        blocklist_mode,
        strip_twitch_emotes: strip_twitch != 0,
        strip_reward_emotes: strip_reward != 0,
        skip_contains_url: skip_contains_url != 0,
        skip_starts_with_bang: skip_starts_with_bang != 0,
        skip_prefix,
        skip_from_bot_accounts: skip_from_bot_accounts != 0,
        bot_accounts,
        skip_longer_than: skip_longer_than != 0,
        longer_than_max_chars: longer_than_max_chars as u32,
        skip_repeat_of_recent: skip_repeat_of_recent != 0,
        repeat_of_recent_window: repeat_of_recent_window as u32,
        output_read_display_name_first: output_read_display_name_first != 0,
        output_emote_to_word: output_emote_to_word != 0,
        skip_emote_only: skip_emote_only != 0,
        skip_mostly_non_latin: skip_mostly_non_latin != 0,
        skip_custom_regexes,
        output_sanitize_punctuation: output_sanitize_punctuation != 0,
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
                    strip_twitch_emotes, strip_reward_emotes,
                    skip_contains_url, skip_starts_with_bang, skip_from_bot_accounts,
                    bot_accounts, skip_longer_than, longer_than_max_chars,
                    skip_repeat_of_recent, repeat_of_recent_window,
                    output_read_display_name_first, output_emote_to_word
             FROM tts_pipeline_settings WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let Some(row) = row else {
            return Ok(TtsPipelineSettings::default());
        };

        let ext: SettingsExtRow = sqlx::query_as(
            "SELECT skip_prefix, skip_emote_only, skip_mostly_non_latin,
                    skip_custom_regexes, output_sanitize_punctuation
             FROM tts_pipeline_settings WHERE id = 1",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_settings_row(row, ext)
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
        let bot_accounts_json =
            serde_json::to_string(&settings.bot_accounts).map_err(StorageError::Serialization)?;
        let skip_custom_regexes_json = serde_json::to_string(&settings.skip_custom_regexes)
            .map_err(StorageError::Serialization)?;

        sqlx::query(
            "INSERT INTO tts_pipeline_settings
                (id, url_mode, max_length, blocklist_mode, strip_twitch_emotes, strip_reward_emotes,
                 skip_contains_url, skip_starts_with_bang, skip_from_bot_accounts, bot_accounts,
                 skip_longer_than, longer_than_max_chars, skip_repeat_of_recent,
                 repeat_of_recent_window, output_read_display_name_first, output_emote_to_word,
                 skip_prefix, skip_emote_only, skip_mostly_non_latin, skip_custom_regexes,
                 output_sanitize_punctuation)
             VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                url_mode                       = excluded.url_mode,
                max_length                     = excluded.max_length,
                blocklist_mode                 = excluded.blocklist_mode,
                strip_twitch_emotes            = excluded.strip_twitch_emotes,
                strip_reward_emotes            = excluded.strip_reward_emotes,
                skip_contains_url              = excluded.skip_contains_url,
                skip_starts_with_bang          = excluded.skip_starts_with_bang,
                skip_from_bot_accounts         = excluded.skip_from_bot_accounts,
                bot_accounts                   = excluded.bot_accounts,
                skip_longer_than               = excluded.skip_longer_than,
                longer_than_max_chars          = excluded.longer_than_max_chars,
                skip_repeat_of_recent          = excluded.skip_repeat_of_recent,
                repeat_of_recent_window        = excluded.repeat_of_recent_window,
                output_read_display_name_first = excluded.output_read_display_name_first,
                output_emote_to_word           = excluded.output_emote_to_word,
                skip_prefix                    = excluded.skip_prefix,
                skip_emote_only                = excluded.skip_emote_only,
                skip_mostly_non_latin          = excluded.skip_mostly_non_latin,
                skip_custom_regexes            = excluded.skip_custom_regexes,
                output_sanitize_punctuation    = excluded.output_sanitize_punctuation",
        )
        .bind(&url_mode_str)
        .bind(settings.max_length.map(|v| v as i64))
        .bind(&blocklist_mode_str)
        .bind(settings.strip_twitch_emotes as i64)
        .bind(settings.strip_reward_emotes as i64)
        .bind(settings.skip_contains_url as i64)
        .bind(settings.skip_starts_with_bang as i64)
        .bind(settings.skip_from_bot_accounts as i64)
        .bind(&bot_accounts_json)
        .bind(settings.skip_longer_than as i64)
        .bind(settings.longer_than_max_chars as i64)
        .bind(settings.skip_repeat_of_recent as i64)
        .bind(settings.repeat_of_recent_window as i64)
        .bind(settings.output_read_display_name_first as i64)
        .bind(settings.output_emote_to_word as i64)
        .bind(&settings.skip_prefix)
        .bind(settings.skip_emote_only as i64)
        .bind(settings.skip_mostly_non_latin as i64)
        .bind(&skip_custom_regexes_json)
        .bind(settings.output_sanitize_punctuation as i64)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }
}
