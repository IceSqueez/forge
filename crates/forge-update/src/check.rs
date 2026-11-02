use crate::UpdateError;
use crate::http::UpdateClient;
use crate::manifest::{UpdateOffer, verify_manifest};
use crate::source::ManifestSource;
use crate::trust::TrustedKeys;
use crate::version::Version;

const MANIFEST_SIZE_LIMIT: u64 = 256 * 1024;
const SIGNATURE_SIZE_LIMIT: u64 = 4 * 1024;

pub async fn fetch_release_offer(
    client: &UpdateClient,
    source: &ManifestSource,
    keys: &TrustedKeys,
    release: &Version,
) -> Result<UpdateOffer, UpdateError> {
    let body = client
        .fetch_release_file(source.manifest_url(), MANIFEST_SIZE_LIMIT)
        .await?;
    let signature = client
        .fetch_release_file(source.signature_url(), SIGNATURE_SIZE_LIMIT)
        .await?;
    let signature = String::from_utf8(signature).map_err(|_| UpdateError::MalformedSignature)?;
    let offer = verify_manifest(&body, &signature, keys)?;
    if offer.version != *release {
        return Err(UpdateError::ReleaseVersionMismatch {
            release: release.to_string(),
            manifest: offer.version.to_string(),
        });
    }
    Ok(offer)
}
