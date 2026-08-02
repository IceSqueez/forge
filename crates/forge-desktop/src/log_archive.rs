use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use time::OffsetDateTime;

const LOG_PREFIX: &str = "forge.log";

/// Oldest first - the rolling appender's date suffix makes name order chronological.
pub fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_log_file(path))
        .collect();
    paths.sort();
    Ok(paths)
}

pub fn bundle(dir: &Path) -> Result<String, String> {
    let mut out = format!(
        "forge {}\nos: {}\narch: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );

    for path in files(dir)? {
        out.push_str(&format!("\n---- {} ----\n", path.display()));
        match fs::read_to_string(&path) {
            Ok(text) => out.push_str(&text),
            Err(e) => out.push_str(&format!("<unreadable: {e}>\n")),
        }
    }
    Ok(out)
}

pub fn clear(dir: &Path) -> Result<(), String> {
    let active = active_file_name();
    for path in files(dir)? {
        if path.file_name().and_then(|name| name.to_str()) == Some(active.as_str()) {
            // Windows refuses to unlink the file the log appender still holds open.
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn is_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(LOG_PREFIX))
}

fn active_file_name() -> String {
    let today = OffsetDateTime::now_utc().date();
    format!(
        "{LOG_PREFIX}.{:04}-{:02}-{:02}",
        today.year(),
        u8::from(today.month()),
        today.day(),
    )
}
