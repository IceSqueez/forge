#![allow(clippy::unwrap_used)]
use forge_update::{
    ArtifactKind, Platform, TrustedKeys, UpdateError, parse_version, verify_manifest,
};

type ErrorCheck = fn(&UpdateError) -> bool;

const TEST_KEY: &str = "RWS0+7RuIDjM/BDQuhL8FdT+3ic+WhaoX5wl1/vemvOb9gMZb5FU6dmr";
const OTHER_KEY: &str = "RWSOtSJsOfPO8WA/kgdNuiJaYi4P66tOr5o57CGD8vRc+dKQUtd77O+2";

const MANIFEST: &[u8] = include_bytes!("fixtures/manifest.json");
const SIGNATURE: &str = include_str!("fixtures/manifest.json.minisig");

fn test_keys() -> TrustedKeys {
    TrustedKeys::from_base64(&[TEST_KEY]).unwrap()
}

#[test]
fn a_manifest_signed_by_a_trusted_key_yields_its_version_and_artifacts() {
    let offer = verify_manifest(MANIFEST, SIGNATURE, &test_keys()).unwrap();

    assert_eq!(offer.version, parse_version("0.6.2").unwrap());
    let artifact = offer
        .artifact_for(Platform::LinuxX64, ArtifactKind::AppImage)
        .expect("the linux appimage entry");
    assert_eq!(artifact.name, "forge-0.6.2-linux-x64.AppImage");
    assert_eq!(artifact.size, 1024);
    assert_eq!(artifact.sha256.to_string(), "a".repeat(64));
}

#[test]
fn a_manifest_verifies_against_a_rotation_set_holding_its_key_second() {
    let keys = TrustedKeys::from_base64(&[OTHER_KEY, TEST_KEY]).unwrap();

    assert!(verify_manifest(MANIFEST, SIGNATURE, &keys).is_ok());
}

#[test]
fn a_body_changed_by_one_byte_fails_verification() {
    let size_digit = MANIFEST
        .windows(4)
        .position(|window| window == b"1024")
        .unwrap();
    let mut tampered = Vec::new();
    for (offset, replacement) in [
        (0, b'X'),
        (size_digit + 3, b'5'),
        (MANIFEST.len() - 1, b' '),
    ] {
        let mut body = MANIFEST.to_vec();
        body[offset] = replacement;
        tampered.push(body);
    }
    let mut appended = MANIFEST.to_vec();
    appended.push(b'\n');
    tampered.push(appended);
    tampered.push(MANIFEST[..MANIFEST.len() - 1].to_vec());

    for body in tampered {
        let result = verify_manifest(&body, SIGNATURE, &test_keys());

        assert!(
            matches!(result, Err(UpdateError::UntrustedSignature)),
            "{} -> {result:?}",
            String::from_utf8_lossy(&body),
        );
    }
}

#[test]
fn a_signature_from_an_untrusted_key_fails_verification() {
    let signature = include_str!("fixtures/manifest.other-key.minisig");

    let result = verify_manifest(MANIFEST, signature, &test_keys());

    assert!(matches!(result, Err(UpdateError::UntrustedSignature)));
}

#[test]
fn the_production_key_set_rejects_a_manifest_signed_by_the_test_key() {
    let production = TrustedKeys::production().unwrap();

    let result = verify_manifest(MANIFEST, SIGNATURE, &production);

    assert!(matches!(result, Err(UpdateError::UntrustedSignature)));
}

#[test]
fn a_legacy_non_prehashed_signature_is_rejected_even_from_a_trusted_key() {
    let signature = include_str!("fixtures/manifest.legacy.minisig");

    let result = verify_manifest(MANIFEST, signature, &test_keys());

    assert!(matches!(result, Err(UpdateError::UntrustedSignature)));
}

#[test]
fn a_malformed_signature_is_reported_as_malformed() {
    let lines: Vec<&str> = SIGNATURE.lines().collect();
    let truncated = lines[..2].join("\n");
    let bad_base64 = [lines[0], "!!!not base64!!!", lines[2], lines[3]].join("\n");
    let unknown_algorithm = SIGNATURE.replacen("RUS", "RXS", 1);
    let untagged_comment = SIGNATURE.replacen("\ntrusted comment: ", "\ncomment: ", 1);
    for signature in [
        "",
        "garbage",
        truncated.as_str(),
        bad_base64.as_str(),
        unknown_algorithm.as_str(),
        untagged_comment.as_str(),
    ] {
        let result = verify_manifest(MANIFEST, signature, &test_keys());

        assert!(
            matches!(result, Err(UpdateError::MalformedSignature)),
            "{signature:?} -> {result:?}",
        );
    }
}

#[test]
fn a_signed_version_that_differs_from_the_body_version_is_rejected() {
    let signature = include_str!("fixtures/manifest.version-0.6.3.minisig");

    let result = verify_manifest(MANIFEST, signature, &test_keys());

    match result {
        Err(UpdateError::VersionMismatch { signed, manifest }) => {
            assert_eq!((signed.as_str(), manifest.as_str()), ("0.6.3", "0.6.2"));
        }
        other => panic!("expected a version mismatch, got {other:?}"),
    }
}

#[test]
fn a_trusted_comment_without_a_version_token_is_rejected() {
    let signature = include_str!("fixtures/manifest.no-version.minisig");

    let result = verify_manifest(MANIFEST, signature, &test_keys());

    assert!(matches!(result, Err(UpdateError::MissingSignedVersion)));
}

#[test]
fn a_correctly_signed_manifest_with_invalid_content_is_still_refused() {
    let cases: [(&[u8], &str, ErrorCheck); 5] = [
        (
            include_bytes!("fixtures/manifest-format-2.json"),
            include_str!("fixtures/manifest-format-2.json.minisig"),
            |error| matches!(error, UpdateError::UnsupportedManifestFormat(2)),
        ),
        (
            include_bytes!("fixtures/manifest-traversal-name.json"),
            include_str!("fixtures/manifest-traversal-name.json.minisig"),
            |error| matches!(error, UpdateError::InvalidArtifactName),
        ),
        (
            include_bytes!("fixtures/manifest-bad-sha256.json"),
            include_str!("fixtures/manifest-bad-sha256.json.minisig"),
            |error| matches!(error, UpdateError::MalformedManifest(_)),
        ),
        (
            include_bytes!("fixtures/manifest-not-json.json"),
            include_str!("fixtures/manifest-not-json.json.minisig"),
            |error| matches!(error, UpdateError::MalformedManifest(_)),
        ),
        (
            include_bytes!("fixtures/manifest-bad-version.json"),
            include_str!("fixtures/manifest-bad-version.json.minisig"),
            |error| matches!(error, UpdateError::InvalidVersion(raw) if raw == "next"),
        ),
    ];
    for (index, (body, signature, expected)) in cases.into_iter().enumerate() {
        let error = verify_manifest(body, signature, &test_keys()).unwrap_err();

        assert!(expected(&error), "case {index} -> {error:?}");
    }
}

#[test]
fn a_trusted_key_set_refuses_an_empty_list_or_any_undecodable_key() {
    let empty: [&str; 0] = [];
    for keys in [
        &empty[..],
        &[""][..],
        &["not a minisign key"][..],
        &[TEST_KEY, "RWQ="][..],
    ] {
        assert!(
            matches!(
                TrustedKeys::from_base64(keys),
                Err(UpdateError::InvalidTrustedKey)
            ),
            "accepted {keys:?}",
        );
    }
}
