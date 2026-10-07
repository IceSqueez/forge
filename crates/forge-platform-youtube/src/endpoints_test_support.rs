#![allow(clippy::unwrap_used)]

use forge_platform_core::{EndpointSurface, PlatformEndpoints};

pub(crate) fn unreachable_endpoints() -> PlatformEndpoints {
    let closed_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
        PlatformEndpoints::resolve(|variable| {
            EndpointSurface::ALL
                .into_iter()
                .any(|surface| surface.env_var() == variable)
                .then(|| {
                    let scheme = if variable.contains("_WS_") {
                        "ws"
                    } else {
                        "http"
                    };
                    std::ffi::OsString::from(format!("{scheme}://127.0.0.1:{closed_port}"))
                })
        })
        .unwrap()
    })
}
