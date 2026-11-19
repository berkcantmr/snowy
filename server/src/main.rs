//! snowy-hub: the Snowy friends server.
//!
//! - `GET /ws`     WebSocket: login with a key, friend codes, presence, relay (see proto.rs)
//! - `POST /ping`  anonymous daily usage ping: `{"ver":"0.1.6"}` (counted, nothing stored per user)
//! - `GET /stats`  daily totals (JSON)
//! - `GET /health` liveness
//!
//! Listens on SNOWY_HUB_ADDR (default 127.0.0.1:8790) behind a Cloudflare tunnel; data in
//! SNOWY_HUB_DB (default ./snowy-hub.sqlite3). No IP addresses are written anywhere.

mod code;
mod db;
mod hub;
mod proto;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use hub::Hub;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Pings counted per address per day (only in memory, to blunt casual inflation).
const PINGS_PER_ADDR: u32 = 3;

struct App {
    hub: Hub,
    /// day -> address -> pings; cleared when the day changes. Never written to disk.
    ping_limit: Mutex<(String, HashMap<String, u32>)>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();
    let addr = std::env::var("SNOWY_HUB_ADDR").unwrap_or_else(|_| "127.0.0.1:8790".into());
    let path = std::env::var("SNOWY_HUB_DB").unwrap_or_else(|_| "snowy-hub.sqlite3".into());
    let app = Arc::new(App { hub: Hub::new(db::Db::open(&path)?), ping_limit: Mutex::new((String::new(), HashMap::new())) });

    // Housekeeping: friend requests expire after 30 days.
    let bg = app.clone();
    tokio::spawn(async move {
        loop {
            let cutoff = chrono::Utc::now().timestamp() - 30 * 86_400;
            if let Ok(n) = bg.hub.db().expire_requests(cutoff) {
                if n > 0 {
                    tracing::info!("expired {n} friend requests");
                }
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    let router = router(app);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("snowy-hub listening on {addr}, data in {path}");
    axum::serve(listener, router).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/ws", get(ws))
        .route("/ping", post(ping))
        .route("/stats", get(stats))
        .route("/health", get(|| async { "ok" }))
        .with_state(app)
}

async fn ws(State(app): State<Arc<App>>, up: WebSocketUpgrade) -> Response {
    up.max_message_size(16 * 1024).max_frame_size(16 * 1024).on_upgrade(move |socket| session(app, socket))
}

async fn session(app: Arc<App>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&m) else { continue };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });
    let mut s = app.hub.open(tx);
    // Clients ping every 30 s; a silent connection is dropped after 90 s.
    loop {
        let next = tokio::time::timeout(Duration::from_secs(90), stream.next()).await;
        let Ok(Some(Ok(msg))) = next else { break };
        match msg {
            Message::Text(t) => match serde_json::from_str::<proto::In>(&t) {
                Ok(m) => app.hub.handle(&mut s, m),
                Err(_) => app.hub.handle_bad(&mut s),
            },
            Message::Close(_) => break,
            _ => {}
        }
        if s.abusive() {
            tracing::info!("dropping a flooding session");
            break;
        }
    }
    app.hub.close(&s);
    drop(s);
    let _ = writer.await;
}

#[derive(serde::Deserialize)]
struct Ping {
    ver: String,
}

async fn ping(State(app): State<Arc<App>>, headers: HeaderMap, Json(p): Json<Ping>) -> StatusCode {
    let ver_ok = !p.ver.is_empty() && p.ver.len() <= 16 && p.ver.chars().all(|c| c.is_ascii_digit() || c == '.');
    if !ver_ok {
        return StatusCode::BAD_REQUEST;
    }
    // Behind the tunnel the client address comes in this header; it is only used here.
    let addr = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
    let day = hub::today();
    {
        let mut lim = app.ping_limit.lock().unwrap_or_else(|e| e.into_inner());
        if lim.0 != day {
            *lim = (day.clone(), HashMap::new());
        }
        let n = lim.1.entry(addr).or_insert(0);
        *n += 1;
        if *n > PINGS_PER_ADDR {
            return StatusCode::NO_CONTENT;
        }
    }
    match app.hub.db().count_ping(&day, &p.ver) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn stats(State(app): State<Arc<App>>) -> Response {
    match app.hub.db().days(90) {
        Ok(days) => {
            let body = serde_json::json!({ "online_now": app.hub.online_count(), "days": days });
            ([("cache-control", "public, max-age=300")], Json(body)).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use tokio_tungstenite::tungstenite::Message as TMsg;

    async fn start() -> String {
        let app = Arc::new(App { hub: Hub::new(db::Db::memory().unwrap()), ping_limit: Mutex::new((String::new(), HashMap::new())) });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(app)).await.unwrap() });
        format!("127.0.0.1:{}", addr.port())
    }

    async fn recv(ws: &mut (impl StreamExt<Item = Result<TMsg, tokio_tungstenite::tungstenite::Error>> + Unpin)) -> serde_json::Value {
        loop {
            let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
            if let TMsg::Text(t) = m {
                return serde_json::from_str(&t).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn two_friends_over_real_sockets() {
        let addr = start().await;
        let mut clients = Vec::new();
        for seed in [1u8, 2] {
            let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await.unwrap();
            let sk = SigningKey::from_bytes(&[seed; 32]);
            let hello = serde_json::json!({"t":"hello","key":B64.encode(sk.verifying_key().to_bytes()),"ver":"0.1.6"});
            ws.send(TMsg::Text(hello.to_string().into())).await.unwrap();
            let ch = recv(&mut ws).await;
            let mut msg = proto::AUTH_PREFIX.to_vec();
            msg.extend_from_slice(&B64.decode(ch["nonce"].as_str().unwrap()).unwrap());
            let auth = serde_json::json!({"t":"auth","sig":B64.encode(sk.sign(&msg).to_bytes())});
            ws.send(TMsg::Text(auth.to_string().into())).await.unwrap();
            let w = recv(&mut ws).await;
            assert_eq!(w["t"], "welcome");
            clients.push((ws, w["code"].as_str().unwrap().to_string()));
        }
        let (mut b, bc) = clients.pop().unwrap();
        let (mut a, ac) = clients.pop().unwrap();
        a.send(TMsg::Text(serde_json::json!({"t":"add","code":bc}).to_string().into())).await.unwrap();
        assert_eq!(recv(&mut a).await["t"], "request_sent");
        assert_eq!(recv(&mut b).await, serde_json::json!({"t":"request","code":ac}));
        b.send(TMsg::Text(serde_json::json!({"t":"accept","code":ac}).to_string().into())).await.unwrap();
        assert_eq!(recv(&mut b).await["friend"]["code"], ac);
        assert_eq!(recv(&mut a).await["friend"]["code"], bc);
        a.send(TMsg::Text(serde_json::json!({"t":"relay","to":bc,"data":"aGk="}).to_string().into())).await.unwrap();
        assert_eq!(recv(&mut b).await, serde_json::json!({"t":"relay","from":ac,"data":"aGk="}));
        // Garbage is answered, not fatal.
        b.send(TMsg::Text("{nope".into())).await.unwrap();
        assert_eq!(recv(&mut b).await, serde_json::json!({"t":"error","code":"bad_message"}));
    }

    #[tokio::test]
    async fn ping_counts_and_stats_show_it() {
        let addr = start().await;
        let body = r#"{"ver":"0.1.6"}"#;
        for _ in 0..5 {
            let st = http(&addr, "POST", "/ping", body).await;
            assert!(st.starts_with("HTTP/1.1 204"), "{st}");
        }
        assert!(http(&addr, "POST", "/ping", r#"{"ver":"x; drop"}"#).await.starts_with("HTTP/1.1 400"));
        let stats = http(&addr, "GET", "/stats", "").await;
        // Same address: only PINGS_PER_ADDR counted.
        assert!(stats.contains(r#""pings":3"#), "{stats}");
    }

    async fn http(addr: &str, method: &str, path: &str, body: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }
}
