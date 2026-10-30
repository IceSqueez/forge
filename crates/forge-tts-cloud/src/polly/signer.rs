use std::fmt::Write as _;

use forge_net::{GatewayRequest, Method, RequestBody};
use forge_types::Secret;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::credentials::PollyCredentials;
use crate::network::ProviderLink;
use crate::polly::error::PollyError;

type HmacSha256 = Hmac<Sha256>;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const SERVICE: &str = "polly";
const SCOPE_TERMINATOR: &str = "aws4_request";
const SECRET_KEY_PREFIX: &str = "AWS4";
const AMZ_DATE_HEADER: &str = "x-amz-date";
const CONTENT_TYPE_HEADER: &str = "content-type";
const HOST_HEADER: &str = "host";
const JSON_CONTENT_TYPE: &str = "application/json";
const URI_UNRESERVED_PUNCTUATION: &[u8] = b"-_.~";
const PATH_SEPARATOR: u8 = b'/';

#[derive(Clone)]
pub(super) struct PollySigner {
    credentials: PollyCredentials,
}

pub(super) struct SignedCall<'a> {
    pub(super) method: Method,
    pub(super) path: &'a str,
    pub(super) query: &'a [(&'a str, &'a str)],
    pub(super) json_body: Option<Vec<u8>>,
}

impl PollySigner {
    pub(super) fn new(credentials: PollyCredentials) -> Self {
        Self { credentials }
    }

    pub(super) fn request(
        &self,
        link: &ProviderLink,
        call: SignedCall<'_>,
    ) -> Result<GatewayRequest, PollyError> {
        self.request_at(link, call, OffsetDateTime::now_utc())
    }

    fn request_at(
        &self,
        link: &ProviderLink,
        call: SignedCall<'_>,
        now: OffsetDateTime,
    ) -> Result<GatewayRequest, PollyError> {
        let SignedCall {
            method,
            path,
            query,
            json_body,
        } = call;
        let target = link
            .resolve_path(path)
            .map_err(|failure| PollyError::SignatureError(failure.to_string()))?;
        let canonical_uri = uri_encode(target.path(), true);
        let canonical_query = canonical_query(query);
        let date_time = format_datetime(&now);
        let date = format_date(&now);
        let payload_hash = hex_sha256(json_body.as_deref().unwrap_or_default());

        let mut canonical_headers = String::new();
        let mut signed_headers = Vec::new();
        if json_body.is_some() {
            push_header(
                &mut canonical_headers,
                &mut signed_headers,
                CONTENT_TYPE_HEADER,
                JSON_CONTENT_TYPE,
            );
        }
        push_header(
            &mut canonical_headers,
            &mut signed_headers,
            HOST_HEADER,
            target.host_header(),
        );
        push_header(
            &mut canonical_headers,
            &mut signed_headers,
            AMZ_DATE_HEADER,
            &date_time,
        );
        let signed_headers = signed_headers.join(";");

        let canonical_request = format!(
            "{}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
            method_name(method),
        );
        let region = &self.credentials.region;
        let scope = format!("{date}/{region}/{SERVICE}/{SCOPE_TERMINATOR}");
        let string_to_sign = format!(
            "{ALGORITHM}\n{date_time}\n{scope}\n{}",
            hex_sha256(canonical_request.as_bytes())
        );
        let signing_key = derive_signing_key(
            self.credentials.secret_access_key.expose().as_bytes(),
            &date,
            region,
        )?;
        let signature = hex_encode(&hmac_sha256(&signing_key, string_to_sign.as_bytes())?);
        let credential = Secret::new(format!(
            "Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.credentials.access_key_id.expose()
        ));

        let request = query.iter().fold(
            GatewayRequest::new(method, path)
                .header(AMZ_DATE_HEADER, date_time)
                .authorization(ALGORITHM, credential),
            |request, (name, value)| request.query(*name, *value),
        );
        Ok(match json_body {
            Some(bytes) => request.body(RequestBody::raw(JSON_CONTENT_TYPE, bytes)),
            None => request,
        })
    }
}

fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(name, value)| (uri_encode(name, false), uri_encode(value, false)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn push_header(
    canonical: &mut String,
    signed: &mut Vec<&'static str>,
    name: &'static str,
    value: &str,
) {
    canonical.push_str(name);
    canonical.push(':');
    canonical.push_str(value.trim());
    canonical.push('\n');
    signed.push(name);
}

fn method_name(method: Method) -> &'static str {
    match method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Patch => "PATCH",
        Method::Delete => "DELETE",
    }
}

fn uri_encode(text: &str, keep_separator: bool) -> String {
    text.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric()
            || URI_UNRESERVED_PUNCTUATION.contains(&byte)
            || (keep_separator && byte == PATH_SEPARATOR)
        {
            encoded.push(char::from(byte));
        } else {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "writing to a String cannot fail"
            )]
            let _ = write!(encoded, "%{byte:02X}");
        }
        encoded
    })
}

fn hex_sha256(data: &[u8]) -> String {
    hex_encode(&Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, PollyError> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|e| PollyError::SignatureError(e.to_string()))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn derive_signing_key(secret: &[u8], date: &str, region: &str) -> Result<Vec<u8>, PollyError> {
    let k_date_key: Vec<u8> = [SECRET_KEY_PREFIX.as_bytes(), secret].concat();
    let k_date = hmac_sha256(&k_date_key, date.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, SERVICE.as_bytes())?;
    hmac_sha256(&k_service, SCOPE_TERMINATOR.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "writing to a String cannot fail"
        )]
        let _ = write!(s, "{b:02x}");
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
pub(crate) mod test_support {
    use forge_types::Secret;
    use std::fmt::Write as _;

    use hmac::{Hmac, KeyInit, Mac};
    use reqwest::Url;
    use sha2::{Digest, Sha256};
    use time::OffsetDateTime;

    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

    use crate::credentials::PollyCredentials;
    use crate::polly::error::PollyError;

    pub(crate) const ACCESS_KEY_ID: &str = "AKIAIOSFODNN7EXAMPLE";
    pub(crate) const SECRET_ACCESS_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    pub(crate) const REGION: &str = "us-east-1";
    pub(crate) const POLLY_SERVICE: &str = "polly";
    pub(crate) const FIXED_AMZ_DATE: &str = "20240601T120000Z";
    const FIXED_UNIX_SECS: i64 = 1_717_243_200;
    const SCHEME_PREFIX: &str = "AWS4-HMAC-SHA256 ";

    pub(crate) fn credentials(base_url: Option<&str>) -> PollyCredentials {
        PollyCredentials {
            access_key_id: Secret::new(ACCESS_KEY_ID),
            secret_access_key: Secret::new(SECRET_ACCESS_KEY),
            region: REGION.into(),
            base_url: base_url.map(str::to_owned),
        }
    }

    pub(crate) fn fixed_time() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(FIXED_UNIX_SECS).unwrap()
    }

    pub(crate) struct WireRequest {
        pub(crate) method: String,
        pub(crate) path: String,
        pub(crate) query: Option<String>,
        pub(crate) headers: Vec<(String, String)>,
        pub(crate) body: Vec<u8>,
    }

    impl From<&wiremock::Request> for WireRequest {
        fn from(request: &wiremock::Request) -> Self {
            Self {
                method: request.method.as_str().to_owned(),
                path: request.url.path().to_owned(),
                query: request.url.query().map(str::to_owned),
                headers: request
                    .headers
                    .iter()
                    .map(|(name, value)| {
                        (name.as_str().to_owned(), value.to_str().unwrap().to_owned())
                    })
                    .collect(),
                body: request.body.clone(),
            }
        }
    }

    impl WireRequest {
        pub(crate) fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        }

        pub(crate) fn authorization(&self) -> &str {
            self.header("authorization").unwrap()
        }

        pub(crate) fn credential_token(&self) -> &str {
            self.authorization().strip_prefix(SCHEME_PREFIX).unwrap()
        }

        pub(crate) fn auth_field(&self, field: &str) -> &str {
            self.credential_token()
                .split(", ")
                .find_map(|part| part.strip_prefix(field)?.strip_prefix('='))
                .unwrap()
        }

        pub(crate) fn sent_signature(&self) -> &str {
            self.auth_field("Signature")
        }

        pub(crate) fn verified_signature(
            &self,
            secret: &str,
            region: &str,
            service: &str,
        ) -> String {
            reference_signature(
                self,
                self.auth_field("SignedHeaders"),
                secret,
                region,
                service,
            )
        }
    }

    pub(crate) fn reference_signature(
        wire: &WireRequest,
        signed_headers: &str,
        secret: &str,
        region: &str,
        service: &str,
    ) -> String {
        let amz_date = wire.header("x-amz-date").unwrap();
        let date = &amz_date[..8];
        let canonical_headers: String = signed_headers
            .split(';')
            .map(|name| format!("{name}:{}\n", wire.header(name).unwrap().trim()))
            .collect();
        let canonical_request = [
            wire.method.clone(),
            spec_encode(&wire.path, true),
            canonical_query(wire.query.as_deref()),
            canonical_headers,
            signed_headers.to_owned(),
            hex(&Sha256::digest(&wire.body)),
        ]
        .join("\n");
        let scope = format!("{date}/{region}/{service}/aws4_request");
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            hex(&Sha256::digest(canonical_request.as_bytes()))
        );
        let key = [date, region, service, "aws4_request"]
            .iter()
            .fold(format!("AWS4{secret}").into_bytes(), |key, part| {
                mac(&key, part.as_bytes())
            });
        hex(&mac(&key, string_to_sign.as_bytes()))
    }

    pub(crate) fn spec_encode(text: &str, keep_slash: bool) -> String {
        text.bytes().fold(String::new(), |mut out, byte| {
            let unreserved = byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte);
            if unreserved || (keep_slash && byte == b'/') {
                out.push(char::from(byte));
            } else {
                write!(out, "%{byte:02X}").unwrap();
            }
            out
        })
    }

    fn canonical_query(raw: Option<&str>) -> String {
        let Some(raw) = raw.filter(|raw| !raw.is_empty()) else {
            return String::new();
        };
        let url = Url::parse(&format!("http://wire.invalid/?{raw}")).unwrap();
        let mut pairs: Vec<String> = url
            .query_pairs()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    spec_encode(&name, false),
                    spec_encode(&value, false)
                )
            })
            .collect();
        pairs.sort();
        pairs.join("&")
    }

    pub(crate) struct EchoAuthorization {
        status: u16,
        text: String,
        retry_after_secs: Option<u64>,
    }

    pub(crate) fn echoing(
        status: u16,
        text: &str,
        retry_after_secs: Option<u64>,
    ) -> EchoAuthorization {
        EchoAuthorization {
            status,
            text: text.to_owned(),
            retry_after_secs,
        }
    }

    impl Respond for EchoAuthorization {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let authorization = request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            let template = ResponseTemplate::new(self.status)
                .set_body_string(format!("{} [{authorization}]", self.text));
            match self.retry_after_secs {
                Some(secs) => template.insert_header("Retry-After", secs.to_string()),
                None => template,
            }
        }
    }

    pub(crate) async fn echo_server(echo: EchoAuthorization) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(any()).respond_with(echo).mount(&server).await;
        server
    }

    pub(crate) async fn assert_reveals_no_credential(text: &str, server: &MockServer) {
        let requests = server.received_requests().await.unwrap();
        let wire = WireRequest::from(&requests[0]);
        assert!(
            !text.contains(wire.credential_token()),
            "token leaked: {text}"
        );
        assert!(
            !text.contains(wire.sent_signature()),
            "signature leaked: {text}"
        );
        assert!(!text.contains(SECRET_ACCESS_KEY), "secret leaked: {text}");
    }

    pub(crate) fn outcome(error: &PollyError) -> String {
        match error {
            PollyError::Http(_) => "Http".into(),
            PollyError::Unauthorized(_) => "Unauthorized".into(),
            PollyError::QuotaExceeded(_) => "QuotaExceeded".into(),
            PollyError::RateLimited { retry_after_secs } => {
                format!("RateLimited({retry_after_secs})")
            }
            PollyError::SignatureError(_) => "SignatureError".into(),
            PollyError::VoiceNotFound(_) => "VoiceNotFound".into(),
            PollyError::Rejected(_) => "Rejected".into(),
            PollyError::Decode(_) => "Decode".into(),
            PollyError::Io(_) => "Io".into(),
        }
    }

    fn mac(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut out, byte| {
            write!(out, "{byte:02x}").unwrap();
            out
        })
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{
        ACCESS_KEY_ID, FIXED_AMZ_DATE, POLLY_SERVICE, REGION, SECRET_ACCESS_KEY, WireRequest,
        credentials, fixed_time, reference_signature,
    };
    use super::*;
    use crate::network::CloudGateways;
    use crate::network::test_support::{link_to, serving};
    use forge_net::{GatewayFailure, InvalidRequest};
    use wiremock::{MockServer, ResponseTemplate};

    const SPEECH_BODY: &[u8] = br#"{"Text":"hello"}"#;
    const PADDED_TOKEN: &str = "a+b/c=d==";
    const PADDED_TOKEN_ENCODED: &str = "NextToken=a%2Bb%2Fc%3Dd%3D%3D";

    fn signer() -> PollySigner {
        PollySigner::new(credentials(None))
    }

    fn link_at(base: &str) -> ProviderLink {
        CloudGateways::new().provider("polly").link(base).unwrap()
    }

    async fn wire_of(server: &MockServer) -> Vec<WireRequest> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(WireRequest::from)
            .collect()
    }

    fn call<'a>(
        method: Method,
        path: &'a str,
        query: &'a [(&'a str, &'a str)],
        body: Option<&[u8]>,
    ) -> SignedCall<'a> {
        SignedCall {
            method,
            path,
            query,
            json_body: body.map(<[u8]>::to_vec),
        }
    }

    fn wire_fixture(
        method: &str,
        path: &str,
        query: Option<&str>,
        body: &[u8],
        amz_date: &str,
    ) -> WireRequest {
        WireRequest {
            method: method.into(),
            path: path.into(),
            query: query.map(str::to_owned),
            headers: vec![
                ("host".into(), "example.amazonaws.com".into()),
                ("x-amz-date".into(), amz_date.into()),
            ],
            body: body.to_vec(),
        }
    }

    #[test]
    fn reference_signer_reproduces_the_published_aws_test_vectors() {
        let secret = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
        for (wire, expected) in [
            (
                wire_fixture("GET", "/", None, b"", "20150830T123600Z"),
                "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
            ),
            (
                wire_fixture("POST", "/", None, b"", "20150830T123600Z"),
                "5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b",
            ),
            (
                wire_fixture(
                    "GET",
                    "/",
                    Some("Param2=value2&Param1=value1"),
                    b"",
                    "20150830T123600Z",
                ),
                "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500",
            ),
        ] {
            assert_eq!(
                reference_signature(&wire, "host;x-amz-date", secret, REGION, "service"),
                expected,
                "{} {:?}",
                wire.method,
                wire.query
            );
        }
    }

    #[tokio::test]
    async fn authorization_equals_the_hand_computed_known_answer_for_each_target() {
        let speech: &[(&str, &str)] = &[];
        let token: &[(&str, &str)] = &[("NextToken", PADDED_TOKEN)];
        let cases = [
            (
                "https://polly.us-east-1.amazonaws.com",
                Method::Get,
                "/v1/voices",
                speech,
                None,
                "host;x-amz-date",
                "4978ce6d14fcc497b05e5fdc15cbbf0b426e42dc71ac42d189379a0149d5d493",
            ),
            (
                "https://polly.us-east-1.amazonaws.com",
                Method::Get,
                "/v1/voices",
                token,
                None,
                "host;x-amz-date",
                "7cc0cd0eb21d546cd549edb54f6e289198519a22d59f276137a579b4933ceac4",
            ),
            (
                "https://polly.us-east-1.amazonaws.com",
                Method::Post,
                "/v1/speech",
                speech,
                Some(SPEECH_BODY),
                "content-type;host;x-amz-date",
                "36fa6c5732bd7fed2481c4c99c0191e1c9072ef2e92accb6943563df5d7d83ab",
            ),
            (
                "https://polly.example.com:8443/prefix",
                Method::Get,
                "/v1/voices",
                speech,
                None,
                "host;x-amz-date",
                "95faaa5ad47d94874aac18576b36cf80734b23a122ccef5f564934faa863d873",
            ),
            (
                "https://polly.example.com:443",
                Method::Get,
                "/v1/voices",
                speech,
                None,
                "host;x-amz-date",
                "8f2772fdd9aef1f2a88f571fa519361b4de3e300eb4234c5735eac3cbc8c8a5d",
            ),
            (
                "https://polly.example.com/pro xy",
                Method::Get,
                "/v1/voices",
                speech,
                None,
                "host;x-amz-date",
                "5b93c70c203fe47e608b5747ac3fc09019c5282c4b1ccec116c670f6d6ecc1c7",
            ),
        ];
        for (base, method, path, query, body, signed_headers, signature) in cases {
            let server = serving(ResponseTemplate::new(200)).await;
            let request = signer()
                .request_at(
                    &link_at(base),
                    call(method, path, query, body),
                    fixed_time(),
                )
                .unwrap();
            link_to(&server).send(request).await.unwrap();

            let wire = wire_of(&server).await;
            assert_eq!(
                (wire[0].authorization(), wire[0].header("x-amz-date")),
                (
                    format!(
                        "AWS4-HMAC-SHA256 Credential={ACCESS_KEY_ID}/20240601/{REGION}/{POLLY_SERVICE}/aws4_request, SignedHeaders={signed_headers}, Signature={signature}"
                    )
                    .as_str(),
                    Some(FIXED_AMZ_DATE)
                ),
                "{base} {path} {query:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_signature_verifies_against_the_host_path_query_and_body_on_the_wire() {
        let no_query: &[(&str, &str)] = &[];
        let token: &[(&str, &str)] = &[("NextToken", PADDED_TOKEN)];
        for (prefix, method, query, body) in [
            ("", Method::Get, no_query, None),
            ("/prefix", Method::Get, token, None),
            ("/pro xy/", Method::Get, token, None),
            ("", Method::Post, no_query, Some(SPEECH_BODY)),
            ("/prefix", Method::Post, no_query, Some(SPEECH_BODY)),
        ] {
            let server = serving(ResponseTemplate::new(200)).await;
            let link = link_at(&format!("{}{prefix}", server.uri()));
            let request = signer()
                .request_at(&link, call(method, "/v1/voices", query, body), fixed_time())
                .unwrap();
            link.send(request).await.unwrap();

            let wire = wire_of(&server).await;
            let wire = &wire[0];
            assert_eq!(
                wire.header("host"),
                Some(server.address().to_string().as_str())
            );
            assert!(wire.header("user-agent").is_some());
            assert_eq!(
                wire.sent_signature(),
                wire.verified_signature(SECRET_ACCESS_KEY, REGION, POLLY_SERVICE),
                "{prefix:?} {:?}",
                wire.path
            );
        }
    }

    #[tokio::test]
    async fn next_token_travels_in_its_canonical_encoding() {
        let server = serving(ResponseTemplate::new(200)).await;
        let link = link_to(&server);
        let request = signer()
            .request_at(
                &link,
                call(
                    Method::Get,
                    "/v1/voices",
                    &[("NextToken", PADDED_TOKEN)],
                    None,
                ),
                fixed_time(),
            )
            .unwrap();
        link.send(request).await.unwrap();

        let wire = wire_of(&server).await;
        assert_eq!(wire[0].query.as_deref(), Some(PADDED_TOKEN_ENCODED));
    }

    #[tokio::test]
    async fn the_json_body_is_sent_as_the_exact_bytes_that_were_hashed() {
        let server = serving(ResponseTemplate::new(200)).await;
        let link = link_to(&server);
        let request = signer()
            .request_at(
                &link,
                call(Method::Post, "/v1/speech", &[], Some(SPEECH_BODY)),
                fixed_time(),
            )
            .unwrap();
        link.send(request).await.unwrap();

        let wire = wire_of(&server).await;
        assert_eq!(
            (wire[0].body.as_slice(), wire[0].header("content-type")),
            (SPEECH_BODY, Some("application/json"))
        );
    }

    #[tokio::test]
    async fn the_secret_key_never_travels_on_the_wire() {
        let server = serving(ResponseTemplate::new(200)).await;
        let link = link_to(&server);
        let request = signer()
            .request_at(
                &link,
                call(Method::Post, "/v1/speech", &[], Some(SPEECH_BODY)),
                fixed_time(),
            )
            .unwrap();
        link.send(request).await.unwrap();

        let wire = wire_of(&server).await;
        let wire = &wire[0];
        assert!(
            wire.headers
                .iter()
                .all(|(_, value)| !value.contains(SECRET_ACCESS_KEY))
                && !String::from_utf8_lossy(&wire.body).contains(SECRET_ACCESS_KEY)
                && !wire.path.contains(SECRET_ACCESS_KEY)
        );
    }

    #[test]
    fn a_path_the_gateway_cannot_resolve_is_a_signature_error() {
        let link = link_at("https://polly.us-east-1.amazonaws.com");
        for path in ["/v1/voices?NextToken=x", "/v1/voices#frag"] {
            let result = signer().request(&link, call(Method::Get, path, &[], None));
            assert!(
                matches!(&result, Err(PollyError::SignatureError(detail))
                    if *detail == GatewayFailure::InvalidRequest(InvalidRequest::QueryInPath).to_string()),
                "{path}"
            );
        }
    }
}
