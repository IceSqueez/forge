//! The content target is the one place chat text may enter a log field. Its value is the audit
//! anchor: a bundle taken without `forge::command=trace` provably carries no command line, and
//! that claim only holds while exactly one call site names the target.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

const COMMAND_TARGET: &str = "forge::command";

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_command_line_target_is_named_exactly_once_across_the_workspace() {
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(&crates).unwrap().flatten() {
        rust_sources(&entry.path().join("src"), &mut sources);
    }

    // A walk that found nothing would make the assertions below vacuous.
    assert!(
        sources.len() > 100,
        "the source walk found only {} files under {}",
        sources.len(),
        crates.display()
    );

    let sites: Vec<(PathBuf, usize)> = sources
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            // Only production call sites are in scope; forge keeps its unit tests in a
            // `#[cfg(test)]` module at the bottom of the file they cover.
            let count = text
                .split("#[cfg(test)]")
                .next()
                .unwrap_or_default()
                .matches(COMMAND_TARGET)
                .count();
            (count > 0).then_some((path, count))
        })
        .collect();

    assert_eq!(
        sites.len(),
        1,
        "{COMMAND_TARGET} must be named in one file only, found: {sites:?}"
    );
    assert_eq!(
        sites[0].1,
        1,
        "{COMMAND_TARGET} must be named once - the const definition - found {} in {}",
        sites[0].1,
        sites[0].0.display()
    );
    assert!(
        sites[0].0.ends_with("trigger_evaluator.rs"),
        "the content target moved out of the evaluator: {}",
        sites[0].0.display()
    );
}
