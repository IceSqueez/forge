#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_net::TokenBucketRateLimiter;
use forge_update::UpdateClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const SECRET: &str = "s3cret-query-token";

#[derive(Clone)]
pub enum Reply {
    Full(Vec<u8>),
    CloseDelimited(Vec<u8>),
    Declared {
        content_length: u64,
        body: Vec<u8>,
    },
    Stall {
        content_length: u64,
        prefix: Vec<u8>,
    },
    Status(u16),
    Redirect(String),
    Silent,
}

pub struct TestServer {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    heads: Arc<Mutex<Vec<String>>>,
}

impl TestServer {
    pub async fn start(routes: Vec<(&str, Reply)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let routes: Arc<HashMap<String, Reply>> = Arc::new(
            routes
                .into_iter()
                .map(|(path, reply)| (path.to_owned(), reply))
                .collect(),
        );
        let heads = Arc::new(Mutex::new(Vec::new()));
        let counter = Arc::clone(&hits);
        let recorded = Arc::clone(&heads);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(serve(stream, Arc::clone(&routes), Arc::clone(&recorded)));
            }
        });
        Self { addr, hits, heads }
    }

    pub fn plain_url(&self, host: &str, path: &str) -> String {
        format!("http://{host}:{}{path}", self.addr.port())
    }

    pub fn heads(&self) -> Vec<String> {
        self.heads.lock().unwrap().clone()
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}?token={SECRET}", self.addr)
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

async fn serve(
    mut stream: TcpStream,
    routes: Arc<HashMap<String, Reply>>,
    heads: Arc<Mutex<Vec<String>>>,
) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte).await {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    heads.lock().unwrap().push(head.clone());
    let target = head.split_whitespace().nth(1).unwrap_or_default();
    let path = target.split('?').next().unwrap_or_default();
    let reply = routes.get(path).cloned().unwrap_or(Reply::Status(404));
    match reply {
        Reply::Full(body) => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            write_all(&mut stream, header.as_bytes(), &body).await;
        }
        Reply::CloseDelimited(body) => {
            write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n",
                &body,
            )
            .await;
        }
        Reply::Declared {
            content_length,
            body,
        } => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
            );
            write_all(&mut stream, header.as_bytes(), &body).await;
        }
        Reply::Stall {
            content_length,
            prefix,
        } => {
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\n\r\n");
            if stream.write_all(header.as_bytes()).await.is_err()
                || stream.write_all(&prefix).await.is_err()
                || stream.flush().await.is_err()
            {
                return;
            }
            std::future::pending::<()>().await;
        }
        Reply::Redirect(location) => {
            let header = format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            write_all(&mut stream, header.as_bytes(), &[]).await;
        }
        Reply::Silent => std::future::pending::<()>().await,
        Reply::Status(code) => {
            let header =
                format!("HTTP/1.1 {code} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            write_all(&mut stream, header.as_bytes(), &[]).await;
        }
    }
}

async fn write_all(stream: &mut TcpStream, header: &[u8], body: &[u8]) {
    if stream.write_all(header).await.is_ok() && stream.write_all(body).await.is_ok() {
        let _ = stream.shutdown().await;
    }
}

pub fn client() -> UpdateClient {
    UpdateClient::new(Arc::new(TokenBucketRateLimiter::new(
        1_000,
        Duration::from_secs(1),
    )))
    .unwrap()
}

pub async fn refused_url() -> String {
    format!("http://127.0.0.1:0/forge.AppImage?token={SECRET}")
}
