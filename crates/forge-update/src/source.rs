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

    pub fn manifest_url(&self) -> &str {
        &self.manifest_url
    }

    pub fn signature_url(&self) -> &str {
        &self.signature_url
    }
}
