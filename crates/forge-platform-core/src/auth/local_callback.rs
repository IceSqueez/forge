use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::TryRng;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::error::PlatformError;

pub const CALLBACK_PATH: &str = "/oauth/callback";

const VERIFIER_BYTES: usize = 32;
const STATE_BYTES: usize = 32;
const READ_BUFFER: usize = 8192;
const SUCCESS_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>forge</title></head><body style=\"font-family:system-ui;text-align:center;padding:3rem\"><h2>Authentication successful</h2><p>You may close this tab and return to forge.</p></body></html>";

pub struct LocalCallbackDriver {
    listener: TcpListener,
    state: String,
    code_verifier: String,
    code_challenge: String,
    redirect_uri: String,
}

#[derive(Debug, Clone)]
pub struct CallbackCode {
    pub code: String,
}

impl LocalCallbackDriver {
    pub async fn bind(preferred_port: Option<u16>) -> Result<Self, PlatformError> {
        let listener =
            match preferred_port {
                Some(port) => TcpListener::bind(("127.0.0.1", port))
                    .await
                    .map_err(|e| match e.kind() {
                        std::io::ErrorKind::AddrInUse => PlatformError::LoopbackPortInUse { port },
                        _ => PlatformError::Io(e),
                    })?,
                None => TcpListener::bind("127.0.0.1:0").await?,
            };
        let port = listener.local_addr()?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");

        let code_verifier = random_url_safe(VERIFIER_BYTES);
        let code_challenge = challenge_from_verifier(&code_verifier);
        let state = random_url_safe(STATE_BYTES);

        Ok(Self {
            listener,
            state,
            code_verifier,
            code_challenge,
            redirect_uri,
        })
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn code_challenge(&self) -> &str {
        &self.code_challenge
    }

    pub fn code_verifier(&self) -> &str {
        &self.code_verifier
    }

    /// Verifies `state`; fails with `PlatformError::Auth` on mismatch, missing `code`, denial,
    /// or expiry past `dur`.
    pub async fn await_callback(self, dur: Duration) -> Result<CallbackCode, PlatformError> {
        let (mut socket, _peer) =
            timeout(dur, self.listener.accept())
                .await
                .map_err(|_| PlatformError::Auth {
                    reason: "callback timeout".into(),
                })??;

        let request = read_request(&mut socket).await?;
        let result = process_request(&request, &self.state);

        let (status, body) = match &result {
            Ok(_) => ("200 OK", SUCCESS_HTML.to_owned()),
            Err(err) => (
                "400 Bad Request",
                format!(
                    "<!doctype html><html><body style=\"font-family:system-ui;text-align:center;padding:3rem\"><h2>Authentication failed</h2><p>{}</p></body></html>",
                    err
                ),
            ),
        };
        let _ = write_response(&mut socket, status, &body).await;

        result.map(|code| CallbackCode { code })
    }
}

async fn read_request(socket: &mut TcpStream) -> Result<String, PlatformError> {
    let mut buf = [0u8; READ_BUFFER];
    let n = socket.read(&mut buf).await?;
    std::str::from_utf8(&buf[..n])
        .map(str::to_owned)
        .map_err(|_| PlatformError::Auth {
            reason: "callback request was not valid UTF-8".into(),
        })
}

fn process_request(request: &str, expected_state: &str) -> Result<String, PlatformError> {
    let request_line = request.lines().next().ok_or_else(|| PlatformError::Auth {
        reason: "empty callback request".into(),
    })?;
    let path_query = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| PlatformError::Auth {
            reason: "missing path in callback request".into(),
        })?;

    let (path, query) = path_query
        .split_once('?')
        .ok_or_else(|| PlatformError::Auth {
            reason: "callback request missing query string".into(),
        })?;
    if path != CALLBACK_PATH {
        return Err(PlatformError::Auth {
            reason: format!("unexpected callback path: {path}"),
        });
    }

    let mut code = None;
    let mut returned_state = None;
    let mut error = None;
    for pair in query.split('&') {
        let Some((key, raw_value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(raw_value);
        match key {
            "code" => code = Some(value),
            "state" => returned_state = Some(value),
            "error" => error = Some(value),
            _ => {}
        }
    }

    if let Some(err) = error {
        return Err(PlatformError::Auth {
            reason: format!("authorization denied: {err}"),
        });
    }

    let returned_state = returned_state.ok_or_else(|| PlatformError::Auth {
        reason: "state parameter missing from callback".into(),
    })?;
    if returned_state != expected_state {
        return Err(PlatformError::Auth {
            reason: "state mismatch (possible CSRF)".into(),
        });
    }

    code.ok_or_else(|| PlatformError::Auth {
        reason: "code parameter missing from callback".into(),
    })
}

async fn write_response(
    socket: &mut TcpStream,
    status: &str,
    body: &str,
) -> Result<(), PlatformError> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

fn random_url_safe(byte_count: usize) -> String {
    let mut bytes = vec![0u8; byte_count];
    // try_fill_bytes returns Result<(), Infallible>; matched instead of unwrap()-ed.
    match rand::rng().try_fill_bytes(&mut bytes) {
        Ok(()) => {}
        Err(never) => match never {},
    }
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn challenge_from_verifier(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                match (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h << 4) | l);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_rfc7636_test_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(challenge_from_verifier(verifier), expected);
    }

    #[test]
    fn percent_decode_handles_plus_and_hex() {
        assert_eq!(percent_decode("hello+world"), "hello world");
        assert_eq!(percent_decode("a%3Db"), "a=b");
        assert_eq!(percent_decode("%2520"), "%20");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn percent_decode_leaves_invalid_escape_intact() {
        assert_eq!(percent_decode("%ZZ"), "%ZZ");
        assert_eq!(percent_decode("%"), "%");
    }

    #[test]
    fn hex_digit_covers_all_cases() {
        assert_eq!(hex_digit(b'0'), Some(0));
        assert_eq!(hex_digit(b'9'), Some(9));
        assert_eq!(hex_digit(b'a'), Some(10));
        assert_eq!(hex_digit(b'F'), Some(15));
        assert_eq!(hex_digit(b'g'), None);
    }

    #[test]
    fn process_request_extracts_code() {
        let req = "GET /oauth/callback?code=abc123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let code = process_request(req, "xyz").unwrap();
        assert_eq!(code, "abc123");
    }

    #[test]
    fn process_request_rejects_state_mismatch() {
        let req = "GET /oauth/callback?code=abc&state=wrong HTTP/1.1\r\n\r\n";
        let err = process_request(req, "expected").unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("state mismatch"))
        );
    }

    #[test]
    fn process_request_propagates_user_denial() {
        let req = "GET /oauth/callback?error=access_denied&state=xyz HTTP/1.1\r\n\r\n";
        let err = process_request(req, "xyz").unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("access_denied"))
        );
    }

    #[test]
    fn process_request_requires_state() {
        let req = "GET /oauth/callback?code=abc HTTP/1.1\r\n\r\n";
        let err = process_request(req, "xyz").unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("state parameter missing"))
        );
    }

    #[test]
    fn process_request_requires_code() {
        let req = "GET /oauth/callback?state=xyz HTTP/1.1\r\n\r\n";
        let err = process_request(req, "xyz").unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("code parameter missing"))
        );
    }

    #[test]
    fn process_request_rejects_wrong_path() {
        let req = "GET /not/the/callback?code=abc&state=xyz HTTP/1.1\r\n\r\n";
        let err = process_request(req, "xyz").unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("unexpected callback path"))
        );
    }

    #[test]
    fn process_request_decodes_percent_encoded_code() {
        let req = "GET /oauth/callback?code=ab%2Fcd&state=xyz HTTP/1.1\r\n\r\n";
        let code = process_request(req, "xyz").unwrap();
        assert_eq!(code, "ab/cd");
    }

    #[tokio::test]
    async fn bind_generates_distinct_values_each_time() {
        let a = LocalCallbackDriver::bind(None).await.unwrap();
        let b = LocalCallbackDriver::bind(None).await.unwrap();
        assert_ne!(a.state(), b.state());
        assert_ne!(a.code_verifier(), b.code_verifier());
        assert_ne!(a.code_challenge(), b.code_challenge());
        assert_eq!(
            challenge_from_verifier(a.code_verifier()),
            a.code_challenge()
        );
        assert_eq!(
            challenge_from_verifier(b.code_verifier()),
            b.code_challenge()
        );
    }

    #[tokio::test]
    async fn bind_produces_loopback_uri_with_callback_path() {
        let d = LocalCallbackDriver::bind(None).await.unwrap();
        assert!(d.redirect_uri().starts_with("http://127.0.0.1:"));
        assert!(d.redirect_uri().ends_with(CALLBACK_PATH));
    }

    // Why: a platform whose redirect URI is registered upfront (Kick) needs the callback on a
    // fixed port. Never bind that real port here - a running forge instance holds it; borrow an
    // ephemeral one the OS just released instead.
    async fn released_ephemeral_port() -> u16 {
        let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        port
    }

    #[tokio::test]
    async fn bind_with_preferred_port_exposes_that_exact_port_in_the_redirect_uri() {
        let port = released_ephemeral_port().await;
        let d = LocalCallbackDriver::bind(Some(port)).await.unwrap();
        let uri = reqwest::Url::parse(d.redirect_uri()).unwrap();
        assert_eq!(uri.port(), Some(port));
    }

    #[tokio::test]
    async fn bind_on_a_port_held_by_another_listener_names_that_port_in_the_error() {
        let holder = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = holder.local_addr().unwrap().port();

        let err = LocalCallbackDriver::bind(Some(port))
            .await
            .err()
            .expect("binding a port held by a live listener must fail");

        assert!(matches!(err, PlatformError::LoopbackPortInUse { port: p } if p == port));
        assert!(
            err.to_string().contains(&port.to_string()),
            "the user must be told which port is occupied: {err}"
        );
    }

    #[tokio::test]
    async fn bind_state_and_verifier_are_url_safe_lengths() {
        let d = LocalCallbackDriver::bind(None).await.unwrap();
        assert_eq!(d.state().len(), 43);
        assert_eq!(d.code_verifier().len(), 43);
        assert_eq!(d.code_challenge().len(), 43);
    }

    #[tokio::test]
    async fn await_callback_times_out_when_no_request_arrives() {
        let d = LocalCallbackDriver::bind(None).await.unwrap();
        let err = d
            .await_callback(Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Auth { ref reason } if reason.contains("timeout")));
    }
}
