#![allow(clippy::expect_used, clippy::panic)]
use std::collections::BTreeSet;

use forge_registry::{SubActionRegistry, TriggerRegistry};

const FROZEN_RENAME_MIGRATION: &str =
    include_str!("../../forge-storage-sqlite/migrations/0058_obs_kind_ids.sql");

type Rename = (String, String, String);

fn frozen_rows() -> BTreeSet<Rename> {
    FROZEN_RENAME_MIGRATION
        .lines()
        .filter_map(|line| line.trim().strip_prefix("('"))
        .filter_map(|row| row.split_once("')"))
        .map(|(row, _)| {
            let fields: Vec<&str> = row.split("', '").collect();
            match fields.as_slice() {
                [store, retired, current] => (
                    (*store).to_owned(),
                    (*retired).to_owned(),
                    (*current).to_owned(),
                ),
                _ => panic!("malformed rename row {row:?}"),
            }
        })
        .collect()
}

fn obs_registries() -> (TriggerRegistry, SubActionRegistry) {
    let mut triggers = TriggerRegistry::new();
    forge_obs::register_obs_triggers(&mut triggers).expect("obs triggers");
    let mut sub_actions = SubActionRegistry::new();
    forge_obs::register_obs_sub_actions(&mut sub_actions, forge_obs::SwitchableObsSink::new())
        .expect("obs steps");
    (triggers, sub_actions)
}

fn declared_rows() -> BTreeSet<Rename> {
    let (triggers, sub_actions) = obs_registries();
    let row = |store: &str, (retired, current): (&str, &str)| {
        (store.to_owned(), retired.to_owned(), current.to_owned())
    };
    triggers
        .retired_ids()
        .map(|ids| row("trigger", ids))
        .chain(sub_actions.retired_ids().map(|ids| row("sub_action", ids)))
        .collect()
}

#[test]
fn the_frozen_migration_table_equals_the_obs_registry_retired_id_declaration() {
    assert_eq!(frozen_rows(), declared_rows());
}

#[test]
fn every_retired_obs_id_names_a_registered_successor_and_no_registered_kind() {
    let (triggers, sub_actions) = obs_registries();

    let broken: Vec<String> = triggers
        .retired_ids()
        .filter(|(retired, current)| {
            triggers.get(current).is_none() || triggers.get(retired).is_some()
        })
        .chain(sub_actions.retired_ids().filter(|(retired, current)| {
            sub_actions.get(current).is_none() || sub_actions.get(retired).is_some()
        }))
        .map(|(retired, current)| format!("{retired} -> {current}"))
        .collect();
    assert!(broken.is_empty(), "{broken:#?}");
}
