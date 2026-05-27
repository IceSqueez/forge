use std::collections::BTreeMap;

use forge_types::{SubActionStep, Variant};

use crate::error::SqliteStorageError;

pub async fn migrate_registry_format(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    migrate_triggers(pool).await?;
    migrate_actions(pool).await?;
    Ok(())
}

async fn migrate_triggers(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    type Row = (String, String, String);

    let rows: Vec<Row> =
        sqlx::query_as("SELECT id, kind, config FROM triggers WHERE format_version = 0")
            .fetch_all(pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

    if rows.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await.map_err(SqliteStorageError::Sqlx)?;

    for (id, kind, config) in rows {
        let (new_kind, new_config) = convert_trigger_kind(&kind, &config)?;
        sqlx::query("UPDATE triggers SET kind = ?, config = ?, format_version = 1 WHERE id = ?")
            .bind(&new_kind)
            .bind(&new_config)
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
    }

    tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

async fn migrate_actions(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    type Row = (String, String);

    let rows: Vec<Row> =
        sqlx::query_as("SELECT id, sub_actions FROM actions WHERE format_version = 0")
            .fetch_all(pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

    if rows.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await.map_err(SqliteStorageError::Sqlx)?;

    for (id, sub_actions_json) in rows {
        let new_sub_actions_json = convert_sub_actions(&sub_actions_json)?;
        sqlx::query("UPDATE actions SET sub_actions = ?, format_version = 1 WHERE id = ?")
            .bind(&new_sub_actions_json)
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
    }

    tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

fn convert_trigger_kind(
    kind: &str,
    existing_config: &str,
) -> Result<(String, String), SqliteStorageError> {
    if !kind.starts_with('{') && !kind.starts_with('"') {
        return Ok((kind.to_owned(), existing_config.to_owned()));
    }

    let v: serde_json::Value = serde_json::from_str(kind)
        .map_err(|e| SqliteStorageError::Decode(format!("trigger kind parse: {e}")))?;

    let (new_kind_id, new_config) = map_trigger_value(&v);

    let config_json = serde_json::to_string(&new_config)
        .map_err(|e| SqliteStorageError::Decode(format!("trigger config serialize: {e}")))?;

    Ok((new_kind_id, config_json))
}

fn map_trigger_value(v: &serde_json::Value) -> (String, BTreeMap<String, Variant>) {
    match v {
        serde_json::Value::String(name) => map_trigger_unit_variant(name),
        serde_json::Value::Object(map) => {
            if let Some((name, fields)) = map.iter().next() {
                map_trigger_struct_variant(name, fields)
            } else {
                unknown_trigger_kind("empty_object")
            }
        }
        _ => unknown_trigger_kind("unexpected_json_shape"),
    }
}

fn map_trigger_unit_variant(name: &str) -> (String, BTreeMap<String, Variant>) {
    let kind_id = match name {
        "TwitchChatAnyMessage" => "twitch.chat.message",
        "TwitchSubscribe" => "twitch.support.subscriber",
        "TwitchResubscribe" => "twitch.support.resubscriber",
        "TwitchGiftSub" => "twitch.support.gift_sub",
        "TwitchCheer" => "twitch.support.cheer",
        "TwitchRaid" => "twitch.channel.raid_received",
        other => {
            tracing::warn!(
                variant = other,
                "unknown trigger kind variant during registry migration"
            );
            return (format!("unknown.{other}"), BTreeMap::new());
        }
    };
    (kind_id.to_owned(), BTreeMap::new())
}

fn map_trigger_struct_variant(
    name: &str,
    fields: &serde_json::Value,
) -> (String, BTreeMap<String, Variant>) {
    match name {
        "TwitchChatCommand" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "phrase");
            insert_bool(&mut config, fields, "case_sensitive");
            ("twitch.chat.command".to_owned(), config)
        }
        "TwitchChatAnyMessage" => ("twitch.chat.message".to_owned(), BTreeMap::new()),
        "TwitchSubscribe" => ("twitch.support.subscriber".to_owned(), BTreeMap::new()),
        "TwitchResubscribe" => ("twitch.support.resubscriber".to_owned(), BTreeMap::new()),
        "TwitchGiftSub" => ("twitch.support.gift_sub".to_owned(), BTreeMap::new()),
        "TwitchCheer" => ("twitch.support.cheer".to_owned(), BTreeMap::new()),
        "TwitchRaid" => ("twitch.channel.raid_received".to_owned(), BTreeMap::new()),
        "ObsSceneChanged" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "scene");
            ("obs.scenes.current_changed".to_owned(), config)
        }
        "CodeEvent" => {
            let mut config = BTreeMap::new();
            if let Some(event_name) = fields.get("name").and_then(|v| v.as_str()) {
                config.insert(
                    "event_name".to_owned(),
                    Variant::String(event_name.to_owned()),
                );
            }
            ("script.event.custom".to_owned(), config)
        }
        other => {
            tracing::warn!(
                variant = other,
                "unknown trigger kind variant during registry migration"
            );
            unknown_trigger_kind(other)
        }
    }
}

fn unknown_trigger_kind(tag: &str) -> (String, BTreeMap<String, Variant>) {
    (format!("unknown.{tag}"), BTreeMap::new())
}

fn convert_sub_actions(sub_actions_json: &str) -> Result<String, SqliteStorageError> {
    if serde_json::from_str::<Vec<SubActionStep>>(sub_actions_json).is_ok() {
        return Ok(sub_actions_json.to_owned());
    }

    let entries: Vec<serde_json::Value> = serde_json::from_str(sub_actions_json)
        .map_err(|e| SqliteStorageError::Decode(format!("sub_actions parse: {e}")))?;

    let steps: Vec<SubActionStep> = entries.iter().map(convert_sub_action_entry).collect();

    serde_json::to_string(&steps)
        .map_err(|e| SqliteStorageError::Decode(format!("sub_actions serialize: {e}")))
}

fn convert_sub_action_entry(entry: &serde_json::Value) -> SubActionStep {
    let null = serde_json::Value::Null;
    let (variant_name, fields): (&str, &serde_json::Value) = match entry {
        serde_json::Value::String(s) => (s.as_str(), &null),
        serde_json::Value::Object(map) => {
            if let Some((k, v)) = map.iter().next() {
                (k.as_str(), v)
            } else {
                return unknown_sub_action("empty_object");
            }
        }
        _ => return unknown_sub_action("unexpected_shape"),
    };

    let (kind_id, config) = map_sub_action_variant(variant_name, fields);
    SubActionStep {
        kind_id,
        config,
        enabled: true,
        label: None,
    }
}

fn map_sub_action_variant(
    name: &str,
    fields: &serde_json::Value,
) -> (String, BTreeMap<String, Variant>) {
    match name {
        "SendChat" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "message");
            insert_str(&mut config, fields, "target");
            ("twitch.chat.send_message".to_owned(), config)
        }
        "SetGlobal" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "name");
            insert_bool(&mut config, fields, "persisted");
            if let Some(raw) = fields.get("value")
                && let Ok(v) = serde_json::from_value::<Variant>(raw.clone())
            {
                config.insert("value".to_owned(), v);
            }
            ("core.globals.set".to_owned(), config)
        }
        "GetGlobal" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "name");
            insert_str(&mut config, fields, "arg_name");
            ("core.globals.get".to_owned(), config)
        }
        "IncrementGlobal" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "name");
            insert_int(&mut config, fields, "by");
            ("core.globals.increment".to_owned(), config)
        }
        "DeleteGlobal" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "name");
            ("core.globals.delete".to_owned(), config)
        }
        "Delay" => {
            let mut config = BTreeMap::new();
            insert_int(&mut config, fields, "ms");
            ("core.logic.wait".to_owned(), config)
        }
        "Log" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "level");
            insert_str(&mut config, fields, "message");
            ("core.log.write".to_owned(), config)
        }
        "RunScript" => {
            let mut config = BTreeMap::new();
            if fields.get("script_name").is_some() {
                insert_str(&mut config, fields, "script_name");
                ("script.run.named".to_owned(), config)
            } else {
                if let Some(map) = fields.as_object() {
                    for (k, v) in map {
                        if let Some(variant) = json_value_to_variant(v) {
                            config.insert(k.clone(), variant);
                        }
                    }
                }
                ("script.run.inline".to_owned(), config)
            }
        }
        "ObsSetScene" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "scene");
            ("obs.scenes.switch_current".to_owned(), config)
        }
        "ObsSetSourceVisible" | "ObsSetSource" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "scene");
            insert_str(&mut config, fields, "source");
            insert_bool(&mut config, fields, "visible");
            ("obs.sources.set_visible".to_owned(), config)
        }
        "ObsSetInputMute" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "source");
            insert_bool(&mut config, fields, "muted");
            ("obs.audio.set_mute".to_owned(), config)
        }
        "ObsStartRecord" => ("obs.record.start".to_owned(), BTreeMap::new()),
        "ObsStopRecord" => ("obs.record.stop".to_owned(), BTreeMap::new()),
        "ObsStartStream" => ("obs.stream.start".to_owned(), BTreeMap::new()),
        "ObsStopStream" => ("obs.stream.stop".to_owned(), BTreeMap::new()),
        "ObsRaw" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "request_type");
            if let Some(data) = fields.get("request_data")
                && !data.is_null()
            {
                let s = serde_json::to_string(data).unwrap_or_else(|_| String::new());
                config.insert("request_data".to_owned(), Variant::String(s));
            }
            ("obs.misc.raw_request".to_owned(), config)
        }
        "PlaySound" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "clip_id");
            ("soundboard.sound.play".to_owned(), config)
        }
        "Speak" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "text");
            if let Some(alias) = fields.get("voice_alias").and_then(|v| v.as_str()) {
                config.insert("voice_alias".to_owned(), Variant::String(alias.to_owned()));
            }
            ("tts.speak.text".to_owned(), config)
        }
        "ReadFile" => {
            let mut config = BTreeMap::new();
            insert_str(&mut config, fields, "path");
            insert_str(&mut config, fields, "arg_name");
            ("core.file.read".to_owned(), config)
        }
        "RandomInt" => {
            let mut config = BTreeMap::new();
            insert_int(&mut config, fields, "min");
            insert_int(&mut config, fields, "max");
            insert_str(&mut config, fields, "arg_name");
            ("core.random.int".to_owned(), config)
        }
        other => {
            tracing::warn!(
                variant = other,
                "unknown sub-action variant during registry migration"
            );
            (format!("unknown.{other}"), BTreeMap::new())
        }
    }
}

fn unknown_sub_action(tag: &str) -> SubActionStep {
    SubActionStep {
        kind_id: format!("unknown.{tag}"),
        config: BTreeMap::new(),
        enabled: true,
        label: None,
    }
}

fn insert_str(config: &mut BTreeMap<String, Variant>, fields: &serde_json::Value, key: &str) {
    if let Some(s) = fields.get(key).and_then(|v| v.as_str()) {
        config.insert(key.to_owned(), Variant::String(s.to_owned()));
    }
}

fn insert_bool(config: &mut BTreeMap<String, Variant>, fields: &serde_json::Value, key: &str) {
    if let Some(b) = fields.get(key).and_then(|v| v.as_bool()) {
        config.insert(key.to_owned(), Variant::Bool(b));
    }
}

fn insert_int(config: &mut BTreeMap<String, Variant>, fields: &serde_json::Value, key: &str) {
    if let Some(n) = fields.get(key).and_then(|v| v.as_i64()) {
        config.insert(key.to_owned(), Variant::Int(n));
    }
}

fn json_value_to_variant(v: &serde_json::Value) -> Option<Variant> {
    match v {
        serde_json::Value::Bool(b) => Some(Variant::Bool(*b)),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Variant::Int)
            .or_else(|| n.as_f64().map(Variant::Float)),
        serde_json::Value::String(s) => Some(Variant::String(s.clone())),
        serde_json::Value::Object(_) => serde_json::from_value(v.clone()).ok(),
        _ => None,
    }
}
