use crate::github::GithubRepo;

pub(crate) const MANIFEST_ASSET: &str = "forge-update.json";
pub(crate) const SIGNATURE_ASSET: &str = "forge-update.json.minisig";

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
        GithubRepo::parse(repo_url).map(|repo| repo.latest_manifest_source())
    }

    pub fn manifest_url(&self) -> &str {
        &self.manifest_url
    }

    pub fn signature_url(&self) -> &str {
        &self.signature_url
    }
}
