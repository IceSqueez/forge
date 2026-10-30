#![allow(clippy::expect_used)]
use std::fs;
use std::path::Path;

use forge_registry::SubActionRegistry;

fn migration_sources() -> Vec<String> {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../forge-storage-sqlite/migrations");
    fs::read_dir(directory)
        .expect("the migrations directory")
        .map(|entry| fs::read_to_string(entry.expect("a migration entry").path()))
        .collect::<Result<_, _>>()
        .expect("readable migrations")
}

#[test]
fn every_retired_vtube_step_id_is_renamed_in_stored_actions_by_a_migration() {
    let mut sub_actions = SubActionRegistry::new();
    forge_vtube::register_vtube_sub_actions(
        &mut sub_actions,
        forge_vtube::SwitchableVTubeSink::new(),
    )
    .expect("vtube steps");
    let migrations = migration_sources();

    let unmigrated: Vec<&str> = sub_actions
        .retired_ids()
        .filter(|(retired, current)| {
            !migrations
                .iter()
                .any(|sql| sql.contains(retired) && sql.contains(current))
        })
        .map(|(retired, _)| retired)
        .collect();

    assert_eq!(unmigrated, Vec::<&str>::new());
}
