use crate::UpdateError;
use crate::http::UpdateClient;
use crate::manifest::{UpdateOffer, verify_manifest};
use crate::source::ManifestSource;
use crate::trust::TrustedKeys;
use crate::version::{Version, is_newer};

const MANIFEST_SIZE_LIMIT: u64 = 256 * 1024;
const SIGNATURE_SIZE_LIMIT: u64 = 4 * 1024;

pub async fn fetch_offer(
    client: &UpdateClient,
    source: &ManifestSource,
    keys: &TrustedKeys,
) -> Result<UpdateOffer, UpdateError> {
    let body = client
        .fetch_small(source.manifest_url(), MANIFEST_SIZE_LIMIT)
        .await?;
    let signature = client
        .fetch_small(source.signature_url(), SIGNATURE_SIZE_LIMIT)
        .await?;
    let signature = String::from_utf8(signature).map_err(|_| UpdateError::MalformedSignature)?;
    verify_manifest(&body, &signature, keys)
}

pub async fn check_for_update(
    client: &UpdateClient,
    source: &ManifestSource,
    keys: &TrustedKeys,
    running: &Version,
) -> Result<Option<UpdateOffer>, UpdateError> {
    let offer = fetch_offer(client, source, keys).await?;
    Ok(is_newer(running, &offer.version).then_some(offer))
}
