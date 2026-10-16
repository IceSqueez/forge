use std::path::Path;
use std::process::Command;

const BUILD_COMMIT_ENV: &str = "FORGE_BUILD_COMMIT";
const UNKNOWN_COMMIT: &str = "unknown";

fn main() {
    let commit =
        git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| UNKNOWN_COMMIT.to_owned());
    println!("cargo:rustc-env={BUILD_COMMIT_ENV}={commit}");

    let mut watched = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Some(branch_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(branch_ref);
    }
    for name in watched {
        if let Some(path) = git(&["rev-parse", "--git-path", &name])
            && Path::new(&path).exists()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}
