#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod gateway_support;

use forge_net::{GatewayFailure, GatewayRequest};
use gateway_support::{Harness, platform};
use rustls::server::Acceptor;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

const HTTP1_ALPN: &[u8] = b"http/1.1";

async fn client_alpn_offer(listener: TcpListener) -> Vec<Vec<u8>> {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut acceptor = Acceptor::default();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = stream.read(&mut buffer).await.unwrap();
        assert_ne!(read, 0, "client closed before finishing its ClientHello");
        acceptor.read_tls(&mut &buffer[..read]).unwrap();
        if let Some(accepted) = acceptor.accept().map_err(|(error, _)| error).unwrap() {
            return accepted
                .client_hello()
                .alpn()
                .map(|offer| offer.map(<[u8]>::to_vec).collect())
                .unwrap_or_default();
        }
    }
}

#[tokio::test]
async fn tls_handshake_offers_only_http1_so_an_h2_capable_server_stays_on_http1() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("https://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(client_alpn_offer(listener));
    let harness = Harness::new(&base, platform());

    let failure = harness.fail(GatewayRequest::get("/")).await;
    let offer = server.await.unwrap();

    assert!(matches!(failure, GatewayFailure::Transport(_)));
    assert_eq!(offer, vec![HTTP1_ALPN.to_vec()]);
}
