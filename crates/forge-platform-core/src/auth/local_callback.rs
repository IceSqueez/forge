use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use forge_net::canonical_status_reason;
use forge_net::status::{BAD_REQUEST, NOT_FOUND, OK};
use forge_types::Secret;
use rand::TryRng;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, timeout_at};

use crate::error::PlatformError;

pub const CALLBACK_PATH: &str = "/oauth/callback";

const VERIFIER_BYTES: usize = 32;
const STATE_BYTES: usize = 32;
const READ_BUFFER: usize = 8192;
const CONNECTION_READ_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_LANGUAGE_HEADER: &str = "accept-language";
const UKRAINIAN_LANGUAGE: &str = "uk";

struct CallbackPageText {
    success_heading: &'static str,
    success_detail: &'static str,
    failure_heading: &'static str,
    failure_detail: &'static str,
}

const ENGLISH_PAGE: CallbackPageText = CallbackPageText {
    success_heading: "Authentication successful",
    success_detail: "You may close this tab and return to forge.",
    failure_heading: "Authentication failed",
    failure_detail: "Return to forge and try again.",
};

const UKRAINIAN_PAGE: CallbackPageText = CallbackPageText {
    success_heading: "Вхід виконано",
    success_detail: "Цю вкладку можна закрити й повернутися до forge.",
    failure_heading: "Не вдалося увійти",
    failure_detail: "Поверніться до forge і спробуйте ще раз.",
};

pub(crate) struct LocalCallbackDriver {
    listener: TcpListener,
    state: String,
    code_verifier: Secret,
    code_challenge: String,
    redirect_uri: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CallbackCode {
    pub code: Secret,
}

impl LocalCallbackDriver {
    pub(crate) async fn bind(preferred_port: Option<u16>) -> Result<Self, PlatformError> {
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
            code_verifier: Secret::new(code_verifier),
            code_challenge,
            redirect_uri,
        })
    }

    pub(crate) fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub(crate) fn state(&self) -> &str {
        &self.state
    }

    pub(crate) fn code_challenge(&self) -> &str {
        &self.code_challenge
    }

    pub(crate) fn code_verifier(&self) -> &Secret {
        &self.code_verifier
    }

    pub(crate) async fn await_callback(self, dur: Duration) -> Result<CallbackCode, PlatformError> {
        let deadline = tokio::time::Instant::now() + dur;
        timeout_at(deadline, self.serve_until_callback())
            .await
            .map_err(|_| PlatformError::Auth {
                reason: "callback timeout".into(),
            })?
    }

    async fn serve_until_callback(&self) -> Result<CallbackCode, PlatformError> {
        loop {
            let (mut socket, _peer) = self.listener.accept().await?;
            let request = match timeout(CONNECTION_READ_TIMEOUT, read_request(&mut socket)).await {
                Ok(Ok(request)) => request,
                Ok(Err(error)) => {
                    tracing::debug!(%error, "callback connection ignored");
                    continue;
                }
                Err(_) => {
                    tracing::debug!(
                        "callback connection ignored: no request before the read deadline"
                    );
                    continue;
                }
            };
            if !targets_callback_path(&request) {
                if let Err(error) = write_response(&mut socket, NOT_FOUND, "").await {
                    tracing::debug!(%error, "not-found response could not be written");
                }
                continue;
            }
            let result = process_request(&request, &self.state);
            let text = page_text(&request);
            let (status, body) = match &result {
                Ok(_) => (OK, callback_page(text.success_heading, text.success_detail)),
                Err(_) => (
                    BAD_REQUEST,
                    callback_page(text.failure_heading, text.failure_detail),
                ),
            };
            if let Err(error) = write_response(&mut socket, status, &body).await {
                tracing::debug!(%error, "callback response could not be written to the browser");
            }
            return result.map(|code| CallbackCode {
                code: Secret::new(code),
            });
        }
    }
}

fn targets_callback_path(request: &str) -> bool {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .is_some_and(|target| target.split('?').next() == Some(CALLBACK_PATH))
}

fn page_text(request: &str) -> &'static CallbackPageText {
    let preferred_language = request
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case(ACCEPT_LANGUAGE_HEADER))
        .and_then(|(_, value)| value.split([',', ';', '-']).next())
        .map(str::trim);
    if preferred_language.is_some_and(|tag| tag.eq_ignore_ascii_case(UKRAINIAN_LANGUAGE)) {
        &UKRAINIAN_PAGE
    } else {
        &ENGLISH_PAGE
    }
}

fn callback_page(heading: &str, detail: &str) -> String {
    let heading = escape_html(heading);
    let detail = escape_html(detail);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>forge</title></head><body style=\"font-family:system-ui;text-align:center;padding:3rem\"><h2>{heading}</h2><p>{detail}</p></body></html>"
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    status: u16,
    body: &str,
) -> Result<(), PlatformError> {
    let reason = canonical_status_reason(status).unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

fn random_url_safe(byte_count: usize) -> String {
    let mut bytes = vec![0u8; byte_count];
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
    fn process_request_extracts_and_decodes_the_code() {
        for (label, req, expected) in [
            (
                "process_request_extracts_code",
                "GET /oauth/callback?code=abc123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
                "abc123",
            ),
            (
                "process_request_decodes_percent_encoded_code",
                "GET /oauth/callback?code=ab%2Fcd&state=xyz HTTP/1.1\r\n\r\n",
                "ab/cd",
            ),
        ] {
            let code = process_request(req, "xyz").unwrap();
            assert_eq!(code, expected, "{label}");
        }
    }

    #[test]
    fn process_request_refuses_each_invalid_callback() {
        for (label, req, expected_state, reason_text) in [
            (
                "process_request_rejects_state_mismatch",
                "GET /oauth/callback?code=abc&state=wrong HTTP/1.1\r\n\r\n",
                "expected",
                "state mismatch",
            ),
            (
                "process_request_propagates_user_denial",
                "GET /oauth/callback?error=access_denied&state=xyz HTTP/1.1\r\n\r\n",
                "xyz",
                "access_denied",
            ),
            (
                "process_request_requires_state",
                "GET /oauth/callback?code=abc HTTP/1.1\r\n\r\n",
                "xyz",
                "state parameter missing",
            ),
            (
                "process_request_requires_code",
                "GET /oauth/callback?state=xyz HTTP/1.1\r\n\r\n",
                "xyz",
                "code parameter missing",
            ),
            (
                "process_request_rejects_wrong_path",
                "GET /not/the/callback?code=abc&state=xyz HTTP/1.1\r\n\r\n",
                "xyz",
                "unexpected callback path",
            ),
        ] {
            let err = process_request(req, expected_state).unwrap_err();
            assert!(
                matches!(err, PlatformError::Auth { ref reason } if reason.contains(reason_text)),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn bind_generates_distinct_values_each_time() {
        let a = LocalCallbackDriver::bind(None).await.unwrap();
        let b = LocalCallbackDriver::bind(None).await.unwrap();
        assert_ne!(a.state(), b.state());
        assert_ne!(a.code_verifier(), b.code_verifier());
        assert_ne!(a.code_challenge(), b.code_challenge());
        assert_eq!(
            challenge_from_verifier(a.code_verifier().expose()),
            a.code_challenge()
        );
        assert_eq!(
            challenge_from_verifier(b.code_verifier().expose()),
            b.code_challenge()
        );
    }

    #[tokio::test]
    async fn bind_produces_loopback_uri_with_callback_path() {
        let d = LocalCallbackDriver::bind(None).await.unwrap();
        assert!(d.redirect_uri().starts_with("http://127.0.0.1:"));
        assert!(d.redirect_uri().ends_with(CALLBACK_PATH));
    }

    const FRESH_PORT_ATTEMPTS: usize = 16;

    async fn released_ephemeral_port() -> u16 {
        let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        port
    }

    #[tokio::test]
    async fn bind_with_preferred_port_exposes_that_exact_port_in_the_redirect_uri() {
        let mut bound = None;
        for _ in 0..FRESH_PORT_ATTEMPTS {
            let port = released_ephemeral_port().await;
            match LocalCallbackDriver::bind(Some(port)).await {
                Err(PlatformError::LoopbackPortInUse { .. }) => {}
                outcome => {
                    bound = Some((port, outcome.unwrap()));
                    break;
                }
            }
        }
        assert!(
            bound.is_some(),
            "no free port obtained after {FRESH_PORT_ATTEMPTS} attempts"
        );
        let (port, d) = bound.unwrap();
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
        assert_eq!(d.code_verifier().expose().len(), 43);
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
    async fn browser_visit(query_without_state: &str) -> (String, String) {
        let driver = LocalCallbackDriver::bind(None).await.unwrap();
        let state = driver.state().to_owned();
        let port: u16 = driver
            .redirect_uri()
            .trim_start_matches("http://127.0.0.1:")
            .trim_end_matches(CALLBACK_PATH)
            .parse()
            .unwrap();
        let callback = tokio::spawn(driver.await_callback(Duration::from_secs(5)));

        let mut socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!(
            "GET {CALLBACK_PATH}?{query_without_state}&state={state} HTTP/1.1\r\nConnection: close\r\n\r\n"
        );
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).await.unwrap();
        callback.await.unwrap().ok();

        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        (head.to_owned(), body.to_owned())
    }

    #[test]
    fn escape_html_neutralizes_every_markup_significant_character() {
        assert_eq!(
            escape_html("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }

    #[test]
    fn callback_page_escapes_both_heading_and_detail() {
        let page = callback_page("<b>h</b>", "<i>d</i>");

        assert!(page.contains("<h2>&lt;b&gt;h&lt;/b&gt;</h2>"), "{page}");
        assert!(page.contains("<p>&lt;i&gt;d&lt;/i&gt;</p>"), "{page}");
    }

    #[tokio::test]
    async fn a_denied_callback_page_never_reflects_the_error_parameter_as_markup() {
        let (head, body) = browser_visit("error=%3Cscript%3Ealert(1)%3C%2Fscript%3E").await;

        assert!(head.starts_with("HTTP/1.1 400 Bad Request"), "{head}");
        assert!(!body.contains("script"), "{body}");
        assert!(body.contains("<h2>Authentication failed</h2>"), "{body}");
    }

    #[test]
    fn the_callback_page_follows_the_first_browser_language() {
        for (request, heading) in [
            (
                "GET / HTTP/1.1\r\nAccept-Language: uk-UA,uk;q=0.9,en;q=0.8\r\n\r\n",
                UKRAINIAN_PAGE.success_heading,
            ),
            (
                "GET / HTTP/1.1\r\naccept-language: uk\r\n\r\n",
                UKRAINIAN_PAGE.success_heading,
            ),
            (
                "GET / HTTP/1.1\r\nAccept-Language: en-US,uk;q=0.5\r\n\r\n",
                ENGLISH_PAGE.success_heading,
            ),
            ("GET / HTTP/1.1\r\n\r\n", ENGLISH_PAGE.success_heading),
        ] {
            assert_eq!(page_text(request).success_heading, heading, "{request}");
        }
    }

    #[tokio::test]
    async fn a_successful_callback_serves_the_success_page_with_status_200() {
        let (head, body) = browser_visit("code=abc").await;

        assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");
        assert!(
            body.contains("<h2>Authentication successful</h2>"),
            "{body}"
        );
    }

    const HANG_GUARD: Duration = Duration::from_secs(30);

    fn port_of(driver: &LocalCallbackDriver) -> u16 {
        driver.listener.local_addr().unwrap().port()
    }

    async fn send(port: u16, raw: &[u8]) -> String {
        let mut socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        socket.write_all(raw).await.unwrap();
        let mut response = Vec::new();
        socket.read_to_end(&mut response).await.unwrap_or_default();
        String::from_utf8_lossy(&response).into_owned()
    }

    #[tokio::test]
    async fn a_connection_that_sends_garbage_is_ignored_and_the_real_callback_still_completes() {
        let driver = LocalCallbackDriver::bind(None).await.unwrap();
        let port = port_of(&driver);
        let state = driver.state().to_owned();
        let waiting = tokio::spawn(driver.await_callback(HANG_GUARD));

        let mut stray = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stray.write_all(&[0xff, 0xfe, 0xfd]).await.unwrap();
        drop(stray);
        let response = send(
            port,
            format!("GET /oauth/callback?code=real&state={state} HTTP/1.1\r\n\r\n").as_bytes(),
        )
        .await;

        let code = waiting.await.unwrap().unwrap();
        assert_eq!(code.code.expose(), "real");
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    }

    #[tokio::test]
    async fn a_request_for_another_path_gets_404_and_does_not_consume_the_callback() {
        let driver = LocalCallbackDriver::bind(None).await.unwrap();
        let port = port_of(&driver);
        let state = driver.state().to_owned();
        let waiting = tokio::spawn(driver.await_callback(HANG_GUARD));

        let probe = send(
            port,
            format!("GET /favicon.ico?code=x&state={state} HTTP/1.1\r\n\r\n").as_bytes(),
        )
        .await;
        let real = send(
            port,
            format!("GET /oauth/callback?code=real&state={state} HTTP/1.1\r\n\r\n").as_bytes(),
        )
        .await;

        let code = waiting.await.unwrap().unwrap();
        assert!(probe.starts_with("HTTP/1.1 404 Not Found"), "{probe}");
        assert!(real.starts_with("HTTP/1.1 200 OK"), "{real}");
        assert_eq!(code.code.expose(), "real");
    }

    #[tokio::test]
    async fn a_callback_with_the_wrong_state_ends_the_wait_with_an_auth_error() {
        let driver = LocalCallbackDriver::bind(None).await.unwrap();
        let port = port_of(&driver);
        let waiting = tokio::spawn(driver.await_callback(HANG_GUARD));

        let response = send(
            port,
            b"GET /oauth/callback?code=real&state=forged HTTP/1.1\r\n\r\n",
        )
        .await;

        let result = waiting.await.unwrap();
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "{response}"
        );
        assert!(
            matches!(result, Err(PlatformError::Auth { ref reason }) if reason.contains("state mismatch")),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn waiting_with_no_connection_times_out_with_an_auth_error() {
        let driver = LocalCallbackDriver::bind(None).await.unwrap();

        let result = driver.await_callback(Duration::from_millis(20)).await;

        assert!(
            matches!(result, Err(PlatformError::Auth { ref reason }) if reason == "callback timeout"),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_connection_that_never_sends_a_request_cannot_outlast_the_overall_timeout() {
        let driver = LocalCallbackDriver::bind(None).await.unwrap();
        let _silent = TcpStream::connect(("127.0.0.1", port_of(&driver)))
            .await
            .unwrap();

        let result = driver.await_callback(Duration::from_millis(50)).await;

        assert!(
            matches!(result, Err(PlatformError::Auth { ref reason }) if reason == "callback timeout"),
            "{result:?}"
        );
    }
}
