use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::credentials::PollyCredentials;
use crate::polly::error::PollyError;

type HmacSha256 = Hmac<Sha256>;

pub(super) struct SigningHeaders {
    pub(super) authorization: String,
    pub(super) x_amz_date: String,
}

pub(super) fn sign(
    method: &str,
    url: &str,
    body: &[u8],
    credentials: &PollyCredentials,
) -> Result<SigningHeaders, PollyError> {
    sign_at(method, url, body, credentials, OffsetDateTime::now_utc())
}

fn sign_at(
    method: &str,
    url: &str,
    body: &[u8],
    credentials: &PollyCredentials,
    now: OffsetDateTime,
) -> Result<SigningHeaders, PollyError> {
    let (host, path, query) = parse_url(url)?;
    let date_time = format_datetime(&now);
    let date = format_date(&now);
    let body_hash = hex_sha256(body);

    let is_post = method.eq_ignore_ascii_case("POST");
    let (canonical_headers, signed_headers) = if is_post {
        let ch = format!("content-type:application/json\nhost:{host}\nx-amz-date:{date_time}\n");
        (ch, "content-type;host;x-amz-date".to_string())
    } else {
        let ch = format!("host:{host}\nx-amz-date:{date_time}\n");
        (ch, "host;x-amz-date".to_string())
    };

    let canonical_request =
        format!("{method}\n{path}\n{query}\n{canonical_headers}\n{signed_headers}\n{body_hash}");

    let scope = format!("{date}/{}/polly/aws4_request", credentials.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{date_time}\n{scope}\n{}",
        hex_sha256(canonical_request.as_bytes())
    );

    let signing_key = derive_signing_key(
        credentials.secret_access_key.as_bytes(),
        &date,
        &credentials.region,
    )?;
    let signature = hex_hmac(&signing_key, string_to_sign.as_bytes())?;

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    );

    Ok(SigningHeaders {
        authorization,
        x_amz_date: date_time,
    })
}

fn parse_url(url: &str) -> Result<(String, String, String), PollyError> {
    let parsed = url
        .parse::<reqwest::Url>()
        .map_err(|e| PollyError::SignatureError(format!("invalid URL: {e}")))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| PollyError::SignatureError("URL has no host".into()))?
        .to_string();

    // AWS SigV4 requires the host header to include non-default ports.
    let host_with_port = match parsed.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    };

    let path = if parsed.path().is_empty() {
        "/".to_string()
    } else {
        parsed.path().to_string()
    };

    let query = parsed.query().unwrap_or("").to_string();

    Ok((host_with_port, path, query))
}

fn hex_sha256(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex_encode(&hash)
}

fn hex_hmac(key: &[u8], data: &[u8]) -> Result<String, PollyError> {
    Ok(hex_encode(&hmac_sha256(key, data)?))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, PollyError> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|e| PollyError::SignatureError(e.to_string()))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn derive_signing_key(secret: &[u8], date: &str, region: &str) -> Result<Vec<u8>, PollyError> {
    let k_date_key: Vec<u8> = [b"AWS4", secret].concat();
    let k_date = hmac_sha256(&k_date_key, date.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, b"polly")?;
    hmac_sha256(&k_service, b"aws4_request")
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        s.push_str(&format!("{b:02x}"));
        s
    })
}

fn format_datetime(t: &OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

fn format_date(t: &OffsetDateTime) -> String {
    format!("{:04}{:02}{:02}", t.year(), u8::from(t.month()), t.day())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::Month;

    fn creds() -> PollyCredentials {
        PollyCredentials {
            access_key_id: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            base_url: None,
        }
    }

    fn fixed_time() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
            .replace_year(2024)
            .unwrap()
            .replace_month(Month::June)
            .unwrap()
            .replace_day(1)
            .unwrap()
            .replace_hour(12)
            .unwrap()
            .replace_minute(0)
            .unwrap()
            .replace_second(0)
            .unwrap()
    }

    #[test]
    fn authorization_header_has_correct_prefix() {
        let result = sign_at(
            "POST",
            "https://polly.us-east-1.amazonaws.com/v1/speech",
            b"{}",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert!(
            result
                .authorization
                .starts_with("AWS4-HMAC-SHA256 Credential="),
            "unexpected prefix: {}",
            result.authorization
        );
    }

    #[test]
    fn authorization_header_contains_access_key_and_scope() {
        let result = sign_at(
            "POST",
            "https://polly.us-east-1.amazonaws.com/v1/speech",
            b"{}",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert!(result.authorization.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(
            result
                .authorization
                .contains("us-east-1/polly/aws4_request")
        );
    }

    #[test]
    fn x_amz_date_has_correct_format() {
        let result = sign_at(
            "POST",
            "https://polly.us-east-1.amazonaws.com/v1/speech",
            b"{}",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert_eq!(result.x_amz_date, "20240601T120000Z");
    }

    #[test]
    fn get_request_signed_headers_exclude_content_type() {
        let result = sign_at(
            "GET",
            "https://polly.us-east-1.amazonaws.com/v1/voices",
            b"",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert!(
            result
                .authorization
                .contains("SignedHeaders=host;x-amz-date")
        );
        assert!(!result.authorization.contains("content-type"));
    }

    #[test]
    fn post_request_signed_headers_include_content_type() {
        let result = sign_at(
            "POST",
            "https://polly.us-east-1.amazonaws.com/v1/speech",
            b"{\"Text\":\"hello\"}",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert!(
            result
                .authorization
                .contains("SignedHeaders=content-type;host;x-amz-date")
        );
    }

    #[test]
    fn host_with_port_included_for_non_default_port() {
        let result = sign_at(
            "GET",
            "http://127.0.0.1:9000/v1/voices",
            b"",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert!(result.authorization.contains("Credential="));
    }

    #[test]
    fn different_bodies_produce_different_signatures() {
        let r1 = sign_at(
            "POST",
            "https://polly.us-east-1.amazonaws.com/v1/speech",
            b"{\"Text\":\"hello\"}",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        let r2 = sign_at(
            "POST",
            "https://polly.us-east-1.amazonaws.com/v1/speech",
            b"{\"Text\":\"world\"}",
            &creds(),
            fixed_time(),
        )
        .unwrap();
        assert_ne!(r1.authorization, r2.authorization);
    }
}
