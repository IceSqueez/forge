use serde::Deserialize;

use crate::UpdateError;
use crate::http::UpdateClient;
use crate::source::{MANIFEST_ASSET, ManifestSource, SIGNATURE_ASSET};

const GITHUB_WEB_PREFIX: &str = "https://github.com/";
const GITHUB_API_REPOS: &str = "https://api.github.com/repos/";
const GIT_SUFFIX: &str = ".git";
const LATEST_DOWNLOAD_PATH: &str = "/releases/latest/download/";
const LATEST_RELEASE_PATH: &str = "/releases/latest";
const RELEASE_TAG_PATH: &str = "/releases/tag/v";
const TAG_PREFIX: char = 'v';

const RELEASE_METADATA_SIZE_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRepo {
    slug: String,
}

impl GithubRepo {
    pub fn parse(repo_url: &str) -> Option<Self> {
        let slug = repo_url
            .trim()
            .strip_prefix(GITHUB_WEB_PREFIX)?
            .trim_end_matches('/');
        let slug = slug.strip_suffix(GIT_SUFFIX).unwrap_or(slug);
        (!slug.is_empty()).then(|| Self {
            slug: slug.to_owned(),
        })
    }

    pub fn latest_manifest_source(&self) -> ManifestSource {
        let base = format!("{GITHUB_WEB_PREFIX}{}{LATEST_DOWNLOAD_PATH}", self.slug);
        ManifestSource::new(
            format!("{base}{MANIFEST_ASSET}"),
            format!("{base}{SIGNATURE_ASSET}"),
        )
    }

    pub fn latest_release_api_url(&self) -> String {
        format!("{GITHUB_API_REPOS}{}{LATEST_RELEASE_PATH}", self.slug)
    }

    pub fn release_page_url(&self, version: &str) -> Option<String> {
        let version = version.trim();
        let version = version.strip_prefix(TAG_PREFIX).unwrap_or(version);
        (!version.is_empty()).then(|| {
            format!(
                "{GITHUB_WEB_PREFIX}{}{RELEASE_TAG_PATH}{version}",
                self.slug
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedRelease {
    pub tag: String,
    pub page_url: String,
}

#[derive(Deserialize)]
struct ReleaseDocument {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
}

pub fn parse_latest_release(body: &[u8]) -> Result<Option<PublishedRelease>, UpdateError> {
    let document: ReleaseDocument = serde_json::from_slice(body)
        .map_err(|error| UpdateError::MalformedRelease(error.to_string()))?;
    if document.draft {
        return Ok(None);
    }
    let tag = document.tag_name.trim();
    let page_url = document.html_url.trim();
    if tag.is_empty() || page_url.is_empty() {
        return Err(UpdateError::MalformedRelease(
            "the release has an empty tag or page".to_owned(),
        ));
    }
    Ok(Some(PublishedRelease {
        tag: tag.to_owned(),
        page_url: page_url.to_owned(),
    }))
}

pub async fn fetch_latest_release(
    client: &UpdateClient,
    repo: &GithubRepo,
) -> Result<Option<PublishedRelease>, UpdateError> {
    let body = client
        .fetch_small(&repo.latest_release_api_url(), RELEASE_METADATA_SIZE_LIMIT)
        .await?;
    parse_latest_release(&body)
}
