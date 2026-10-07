#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tokio_stream::StreamExt;
use zbus::Connection;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};

pub(crate) const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub(crate) const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";
const REQUEST_PATH_PREFIX: &str = "/org/freedesktop/portal/desktop/request";
const RESPONSE_SIGNAL: &str = "Response";
const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;

static TOKEN_SERIAL: AtomicU64 = AtomicU64::new(0);

pub(crate) enum PortalOutcome {
    Success(HashMap<String, OwnedValue>),
    Cancelled,
    Ended(u32),
}

impl PortalOutcome {
    pub(crate) fn into_results(self, method: &str) -> Result<HashMap<String, OwnedValue>, String> {
        match self {
            Self::Success(results) => Ok(results),
            Self::Cancelled => Err(format!("{method} was cancelled on the desktop")),
            Self::Ended(code) => Err(format!(
                "{method} was ended by the desktop (response {code})"
            )),
        }
    }
}

pub(crate) fn make_token(app_name: &str, purpose: &str) -> String {
    let app: String = app_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let serial = TOKEN_SERIAL.fetch_add(1, Ordering::Relaxed);
    format!("{app}_{purpose}_{serial}")
}

pub(crate) async fn call_with_response<B>(
    conn: &Connection,
    proxy: &zbus::Proxy<'_>,
    method: &str,
    body: &B,
    handle_token: &str,
    timeout: Duration,
) -> zbus::Result<PortalOutcome>
where
    B: Serialize + DynamicType,
{
    let request_path = predicted_request_path(conn, handle_token);
    let request_proxy = zbus::Proxy::new(
        conn,
        PORTAL_DESTINATION,
        request_path.as_str(),
        REQUEST_INTERFACE,
    )
    .await?;
    let responses = request_proxy.receive_signal(RESPONSE_SIGNAL).await?;
    let mut responses = std::pin::pin!(responses);

    let _request: OwnedObjectPath = proxy.call(method, body).await?;

    let msg = tokio::time::timeout(timeout, responses.next())
        .await
        .map_err(|_| zbus::Error::Failure(format!("{method} response timed out")))?
        .ok_or_else(|| zbus::Error::Failure(format!("{method} response stream closed")))?;

    let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize()?;
    Ok(match code {
        RESPONSE_SUCCESS => PortalOutcome::Success(results),
        RESPONSE_CANCELLED => PortalOutcome::Cancelled,
        other => PortalOutcome::Ended(other),
    })
}

pub(crate) fn object_path_result(
    results: &HashMap<String, OwnedValue>,
    key: &str,
) -> Option<OwnedObjectPath> {
    match &**results.get(key)? {
        Value::ObjectPath(p) => OwnedObjectPath::try_from(p.as_str()).ok(),
        Value::Str(s) => OwnedObjectPath::try_from(s.as_str()).ok(),
        _ => None,
    }
}

fn predicted_request_path(conn: &Connection, handle_token: &str) -> String {
    let unique = conn
        .unique_name()
        .map(|n| n.as_str().to_owned())
        .unwrap_or_default();
    let sender = unique.trim_start_matches(':').replace('.', "_");
    format!("{REQUEST_PATH_PREFIX}/{sender}/{handle_token}")
}
