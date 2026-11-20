//! Self-hostable sync relay (decision D9, P0-10).
//!
//! The relay never interprets document content: clients exchange opaque CRDT updates
//! (Loro), which the relay **stores** (one SQLite log per document, via `nb-store`) and
//! **forwards** to the other connected clients. Loro imports are idempotent and
//! order-independent, so duplicates and reordering are harmless.
//!
//! ## Protocol (WebSocket, binary frames)
//!
//! `GET /sync/<doc-id>?after=<seq>` with `Authorization: Bearer <token>` (or `?token=`) when
//! the server has a token.
//!
//! - Server → client: `seq (u64 big-endian) ‖ update`. First every stored update with
//!   `seq > after` (catch-up), then live updates from other clients.
//! - Client → server: `update`. The server stores it and answers the sender with
//!   `seq ‖` (empty payload) so the sender can advance its cursor.
//!
//! A client keeps the highest `seq` it has seen and reconnects with `after=<seq>`.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use nb_store::Store;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct Config {
    /// Directory holding one `<doc-id>.db` per document.
    pub data_dir: PathBuf,
    /// Shared secret clients must present; `None` disables authentication (local use only).
    pub token: Option<String>,
}

struct Room {
    store: Arc<Mutex<Store>>,
    tx: broadcast::Sender<(i64, Arc<Vec<u8>>, u64)>,
}

struct Relay {
    cfg: Config,
    rooms: Mutex<HashMap<String, Arc<Room>>>,
    next_conn: AtomicU64,
}

/// Document ids become file names: keep them boring.
pub fn valid_doc_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Relay {
    fn room(&self, doc: &str) -> Result<Arc<Room>, String> {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(r) = rooms.get(doc) {
            return Ok(r.clone());
        }
        let store = Store::open(self.cfg.data_dir.join(format!("{doc}.db"))).map_err(|e| e.to_string())?;
        let (tx, _) = broadcast::channel(4096);
        let r = Arc::new(Room {
            store: Arc::new(Mutex::new(store)),
            tx,
        });
        rooms.insert(doc.to_string(), r.clone());
        Ok(r)
    }
}

/// The relay's HTTP routes, to serve with `axum::serve`.
pub fn router(cfg: Config) -> Router {
    std::fs::create_dir_all(&cfg.data_dir).expect("create data dir");
    let relay = Arc::new(Relay {
        cfg,
        rooms: Mutex::new(HashMap::new()),
        next_conn: AtomicU64::new(1),
    });
    Router::new()
        .route("/sync/{doc}", get(sync))
        .route("/health", get(|| async { "ok" }))
        .with_state(relay)
}

fn frame(seq: i64, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(8 + payload.len());
    f.extend_from_slice(&(seq as u64).to_be_bytes());
    f.extend_from_slice(payload);
    f
}

async fn sync(
    State(relay): State<Arc<Relay>>,
    Path(doc): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Some(expected) = &relay.cfg.token {
        let bearer = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let given = bearer.or(q.get("token").map(String::as_str));
        if given != Some(expected.as_str()) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    if !valid_doc_id(&doc) {
        return (StatusCode::BAD_REQUEST, "invalid document id").into_response();
    }
    let after: i64 = q.get("after").and_then(|v| v.parse().ok()).unwrap_or(0);
    let room = match relay.room(&doc) {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let conn = relay.next_conn.fetch_add(1, Ordering::Relaxed);
    ws.max_message_size(16 << 20)
        .on_upgrade(move |socket| client(socket, room, after, conn))
}

async fn client(mut socket: WebSocket, room: Arc<Room>, after: i64, conn: u64) {
    // Subscribe before reading the backlog so nothing falls between the two.
    let mut rx = room.tx.subscribe();
    let store = room.store.clone();
    let backlog = tokio::task::spawn_blocking(move || store.lock().unwrap().updates_after(after)).await;
    let mut sent_up_to = after;
    match backlog {
        Ok(Ok(rows)) => {
            for (seq, bytes) in rows {
                if socket.send(Message::Binary(frame(seq, &bytes).into())).await.is_err() {
                    return;
                }
                sent_up_to = seq;
            }
        }
        _ => return,
    }
    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else { return };
                let bytes = match msg {
                    Message::Binary(b) => b.to_vec(),
                    Message::Close(_) => return,
                    _ => continue,
                };
                let store = room.store.clone();
                let payload = Arc::new(bytes);
                let p = payload.clone();
                let seq = match tokio::task::spawn_blocking(move || store.lock().unwrap().append_update(&p)).await {
                    Ok(Ok(seq)) => seq,
                    _ => return,
                };
                let _ = room.tx.send((seq, payload, conn));
                if socket.send(Message::Binary(frame(seq, &[]).into())).await.is_err() {
                    return;
                }
            }
            update = rx.recv() => {
                match update {
                    Ok((seq, bytes, from)) => {
                        if from == conn || seq <= sent_up_to {
                            continue;
                        }
                        if socket.send(Message::Binary(frame(seq, &bytes).into())).await.is_err() {
                            return;
                        }
                    }
                    // Too slow to keep up: drop the connection; the client reconnects with
                    // its cursor and catches up from storage.
                    Err(broadcast::error::RecvError::Lagged(_)) => return,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }
}
