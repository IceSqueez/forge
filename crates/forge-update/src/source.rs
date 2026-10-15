pub const MANIFEST_ASSET: &str = "forge-update.json";
pub const SIGNATURE_ASSET: &str = "forge-update.json.minisig";

const GITHUB_WEB_PREFIX: &str = "https://github.com/";
const GIT_SUFFIX: &str = ".git";
const LATEST_DOWNLOAD_PATH: &str = "/releases/latest/download/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSource {
    manifest_url: String,
    signature_url: String,
}

impl ManifestSource {
    pub fn new(manifest_url: impl Into<String>, signature_url: impl Into<String>) -> Self {
        Self {
            manifest_url: manifest_url.into(),
            signature_url: signature_url.into(),
        }
    }

    pub fn github_latest(repo_url: &str) -> Option<Self> {
        let slug = repo_url
            .trim()
            .strip_prefix(GITHUB_WEB_PREFIX)?
            .trim_end_matches('/');
        let slug = slug.strip_suffix(GIT_SUFFIX).unwrap_or(slug);
        if slug.is_empty() {
            return None;
        }
        let base = format!("{GITHUB_WEB_PREFIX}{slug}{LATEST_DOWNLOAD_PATH}");
        Some(Self::new(
            format!("{base}{MANIFEST_ASSET}"),
            format!("{base}{SIGNATURE_ASSET}"),
        ))
    }

    pub fn manifest_url(&self) -> &str {
        &self.manifest_url
    }

    pub fn signature_url(&self) -> &str {
        &self.signature_url
    }
}
