use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const STAGED_SUFFIX: &str = ".tmp";

#[cfg(unix)]
const OWNER_ONLY_MODE: u32 = 0o600;

static STAGED_WRITES: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
pub fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY_MODE)
        .open(path)
}

#[cfg(not(unix))]
pub fn create_private(path: &Path) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace_atomically(path, |file| file.write_all(bytes))
}

pub fn copy_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    let mut source = File::open(source)?;
    replace_atomically(destination, |target| {
        io::copy(&mut source, target).map(drop)
    })
}

pub fn replace_atomically(
    path: &Path,
    fill: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let directory = containing_directory(path);
    std::fs::create_dir_all(directory)?;
    let staged = staged_sibling(path)?;
    if let Err(error) = fill_then_rename(&staged, path, fill) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "best-effort cleanup; the write failure is the error returned"
        )]
        let _ = std::fs::remove_file(&staged);
        return Err(error);
    }
    sync_dir(directory)
}

pub fn discard_leftovers_as_sole_writer(path: &Path) -> io::Result<()> {
    let name = file_name(path)?;
    let entries = match std::fs::read_dir(containing_directory(path)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if is_staged_for(&entry.file_name().to_string_lossy(), &name) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
pub fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

pub fn sync_parent(path: &Path) -> io::Result<()> {
    sync_dir(containing_directory(path))
}

fn fill_then_rename(
    staged: &Path,
    path: &Path,
    fill: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let mut file = create_private(staged)?;
    fill(&mut file)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(staged, path)
}

fn containing_directory(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn file_name(path: &Path) -> io::Result<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))
}

fn staged_sibling(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let writer = STAGED_WRITES.fetch_add(1, Ordering::Relaxed);
    let mut staged = OsString::from(".");
    staged.push(name);
    staged.push(format!(".{}.{writer}{STAGED_SUFFIX}", std::process::id()));
    Ok(path.with_file_name(staged))
}

fn is_staged_for(entry: &str, name: &str) -> bool {
    let Some(writer) = entry
        .strip_prefix('.')
        .and_then(|rest| rest.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(STAGED_SUFFIX))
    else {
        return false;
    };
    writer.split_once('.').is_some_and(|(process, sequence)| {
        [process, sequence]
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    })
}
