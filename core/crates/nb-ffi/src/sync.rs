//! Relay sync client (decision D9; server in `nb-relay`, protocol in its crate docs).
//!
//! A [`SyncSession`] runs on its own thread: it connects to the relay, uploads the changes
//! the relay lacks (`CrdtPage::sync_outgoing`), imports the ones it sends
//! (`CrdtPage::sync_incoming`), saves both to the page's database and tells the UI to redraw.
//! It reconnects with exponential backoff. The relay cursor (version vector and highest
//! sequence number received) is stored in the page database, so it survives restarts and
//! offline edits are uploaded on the next connection.
//!
//! ## End-to-end encryption
//!
//! With an end-to-end code (`e…`/`v…` keys, `nb_store::access`), every update is sealed
//! before upload as `KHE1 ‖ nonce (12) ‖ ChaCha20-Poly1305(update) ‖ tag (16)`, with the
//! relay document id as associated data (an update cannot be replayed into another page).
//! The relay only sees credentials derived from the keys. Incoming payloads that are not
//! sealed, or fail authentication, are ignored in that mode.

use crate::notebook::NotebookDoc;
use crate::PageDoc;
use loro::VersionVector;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const META_VV: &str = "relay_vv";
const NONCE_LEN: usize = 12;
/// End-to-end mode: an editor uploads an encrypted snapshot once the relay holds this many
/// updates since the last one (the relay cannot compact encrypted logs itself).
const COMPACT_EVERY: i64 = if cfg!(test) { 8 } else { 1000 };

/// Content key and associated data of an end-to-end encrypted document.
struct Sealer {
    key: ring::aead::LessSafeKey,
    aad: Vec<u8>,
}

impl Sealer {
    fn new(content_key: &[u8; 32], doc_id: &str) -> Self {
        let unbound = ring::aead::UnboundKey::new(&ring::aead::CHACHA20_POLY1305, content_key).expect("32-byte key");
        Self {
            key: ring::aead::LessSafeKey::new(unbound),
            aad: doc_id.as_bytes().to_vec(),
        }
    }

    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        use ring::rand::SecureRandom;
        let mut nonce = [0u8; NONCE_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| "no random source")?;
        let mut out = Vec::with_capacity(4 + NONCE_LEN + plain.len() + 16);
        out.extend_from_slice(nb_store::access::SEALED_MAGIC);
        out.extend_from_slice(&nonce);
        let mut body = plain.to_vec();
        self.key
            .seal_in_place_append_tag(
                ring::aead::Nonce::assume_unique_for_key(nonce),
                ring::aead::Aad::from(&self.aad),
                &mut body,
            )
            .map_err(|_| "encryption failed")?;
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// The plaintext, or `None` if `sealed` is not a valid update for this document.
    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        let rest = sealed.strip_prefix(nb_store::access::SEALED_MAGIC.as_slice())?;
        if rest.len() < NONCE_LEN + 16 {
            return None;
        }
        let (nonce, body) = rest.split_at(NONCE_LEN);
        let nonce = ring::aead::Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut body = body.to_vec();
        let plain = self
            .key
            .open_in_place(nonce, ring::aead::Aad::from(&self.aad), &mut body)
            .ok()?;
        Some(plain.to_vec())
    }
}
const META_SEQ: &str = "relay_seq";

#[derive(uniffi::Enum, Clone, Debug, PartialEq, Eq)]
pub enum SyncStatus {
    Connecting,
    Online,
    Offline { detail: String },
}

/// Implemented by the app; called from the sync thread (post to the UI thread).
#[uniffi::export(callback_interface)]
pub trait SyncListener: Send + Sync {
    /// Remote changes were merged into the page: redraw it.
    fn on_remote_change(&self);
    fn on_status(&self, status: SyncStatus);
}

struct Silent;
impl SyncListener for Silent {
    fn on_remote_change(&self) {}
    fn on_status(&self, _: SyncStatus) {}
}

/// A notebook share code, parsed (see `nb_store::access`).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ShareInfo {
    /// Relay document group: the outline is `<group>`, pages are `<group>-<page-id>`.
    pub group: String,
    /// Key to pass to the sync functions (empty for codes from before keys).
    pub key: String,
    pub read_only: bool,
    /// Code to give viewers who may only read (`None` unless this is an editor code).
    pub read_only_code: Option<String>,
    /// Updates are end-to-end encrypted: the relay cannot read the notebook.
    pub encrypted: bool,
}

/// Parse a share code typed or stored by the app; `None` if it is malformed.
#[uniffi::export]
pub fn parse_share_code(code: String) -> Option<ShareInfo> {
    use nb_store::access::{Role, ShareCode};
    let c = ShareCode::parse(&code)?;
    Some(ShareInfo {
        read_only: c.role() == Role::Read,
        read_only_code: c.read_only().map(|r| r.to_code()),
        encrypted: c.encrypted(),
        group: c.group,
        key: c.key,
    })
}

/// A new editor share code: random group and editor key (the first device to sync with it
/// claims the notebook on the relay). `encrypted`: end-to-end encryption (key `e…`), so the
/// relay cannot read the notebook; otherwise a plain write key (`w…`).
#[uniffi::export]
pub fn new_share_code(encrypted: bool) -> String {
    let mut b = [0u8; 24];
    let _ = rustls::crypto::ring::default_provider().secure_random.fill(&mut b);
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let role = if encrypted { 'e' } else { 'w' };
    format!("n{}.{role}{}", &hex[..15], &hex[16..48])
}

/// One-shot sync of a page that is not open: upload what the relay lacks, import what it
/// has, and return once both sides are quiet for `idle_ms`. Returns the number of updates
/// imported. Blocking: call it from a background thread.
#[uniffi::export]
pub fn sync_page_once(
    page: Arc<PageDoc>,
    url: String,
    token: String,
    doc_id: String,
    key: String,
    idle_ms: u32,
) -> Result<u32, crate::CoreError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let stop = AtomicBool::new(false);
    run_connection(
        page.as_ref(),
        &Endpoint {
            url: &url,
            token: &token,
            doc_id: &doc_id,
            key: &key,
        },
        &Silent,
        &stop,
        Some(Duration::from_millis(idle_ms.into())),
    )
    .map_err(|detail| crate::CoreError::Io { detail })
}

#[derive(uniffi::Object)]
pub struct SyncSession {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

struct Cursor {
    vv: VersionVector,
    seq: i64,
}

/// Something that syncs through the relay: a page or a notebook outline.
pub(crate) trait SyncTarget: Send + Sync {
    fn outgoing(&self, relay: &mut VersionVector) -> Option<Vec<u8>>;
    /// The whole document as one Loro snapshot (end-to-end compaction).
    fn snapshot(&self) -> Vec<u8>;
    /// Import, save to the local database.
    fn incoming(&self, bytes: &[u8], relay: &mut VersionVector) -> Result<(), String>;
    fn meta(&self, key: &str) -> Option<Vec<u8>>;
    fn set_meta(&self, key: &str, value: &[u8]);
}

impl SyncTarget for PageDoc {
    fn outgoing(&self, relay: &mut VersionVector) -> Option<Vec<u8>> {
        self.state.lock().unwrap().doc.sync_outgoing(relay)
    }
    fn snapshot(&self) -> Vec<u8> {
        self.state.lock().unwrap().doc.snapshot()
    }
    fn incoming(&self, bytes: &[u8], relay: &mut VersionVector) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        st.doc.sync_incoming(bytes, relay).map_err(|e| e.to_string())?;
        st.persist();
        Ok(())
    }
    fn meta(&self, key: &str) -> Option<Vec<u8>> {
        self.state.lock().unwrap().store.as_ref()?.meta(key).ok().flatten()
    }
    fn set_meta(&self, key: &str, value: &[u8]) {
        if let Some(s) = self.state.lock().unwrap().store.as_ref() {
            let _ = s.set_meta(key, value);
        }
    }
}

impl SyncTarget for NotebookDoc {
    fn outgoing(&self, relay: &mut VersionVector) -> Option<Vec<u8>> {
        self.state.lock().unwrap().nb.sync_outgoing(relay)
    }
    fn snapshot(&self) -> Vec<u8> {
        self.state.lock().unwrap().nb.snapshot()
    }
    fn incoming(&self, bytes: &[u8], relay: &mut VersionVector) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        st.nb.sync_incoming(bytes, relay).map_err(|e| e.to_string())?;
        st.persist();
        Ok(())
    }
    fn meta(&self, key: &str) -> Option<Vec<u8>> {
        self.state.lock().unwrap().store.meta(key).ok().flatten()
    }
    fn set_meta(&self, key: &str, value: &[u8]) {
        let _ = self.state.lock().unwrap().store.set_meta(key, value);
    }
}

fn load_cursor(t: &dyn SyncTarget) -> Cursor {
    let vv = t
        .meta(META_VV)
        .and_then(|b| VersionVector::decode(&b).ok())
        .unwrap_or_default();
    let seq = t
        .meta(META_SEQ)
        .and_then(|b| std::str::from_utf8(&b).ok()?.parse().ok())
        .unwrap_or(0);
    Cursor { vv, seq }
}

fn save_cursor(t: &dyn SyncTarget, c: &Cursor) {
    t.set_meta(META_VV, &c.vv.encode());
    t.set_meta(META_SEQ, c.seq.to_string().as_bytes());
}

fn set_read_timeout(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>, d: Duration) {
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(Some(d));
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.get_mut().set_read_timeout(Some(d));
        }
        _ => {}
    }
}

/// One connection, until it fails (Err) or the session is stopped (Ok).
/// Where to sync: relay URL, server token, document id and notebook key.
struct Endpoint<'a> {
    url: &'a str,
    token: &'a str,
    doc_id: &'a str,
    key: &'a str,
}

fn run_connection(
    page: &dyn SyncTarget,
    ep: &Endpoint,
    listener: &dyn SyncListener,
    stop: &AtomicBool,
    until_idle: Option<Duration>,
) -> Result<u32, String> {
    let mut cursor = load_cursor(page);
    let Endpoint {
        url,
        token,
        doc_id,
        key,
    } = *ep;
    let code = nb_store::access::ShareCode {
        group: nb_store::access::doc_group(doc_id).to_string(),
        key: key.to_string(),
    };
    let sealer = code.content_key().map(|k| Sealer::new(&k, doc_id));
    let mut req = format!("{}/sync/{doc_id}?after={}", url.trim_end_matches('/'), cursor.seq)
        .into_client_request()
        .map_err(|e| e.to_string())?;
    if !token.is_empty() {
        req.headers_mut().insert(
            "authorization",
            format!("Bearer {token}").parse().map_err(|_| "bad token")?,
        );
    }
    if !key.is_empty() {
        // End-to-end codes present a derived credential; the key itself never leaves.
        req.headers_mut()
            .insert("x-kahie-key", code.relay_key().parse().map_err(|_| "bad key")?);
        if let Some(h) = code.relay_read_hash() {
            req.headers_mut()
                .insert("x-kahie-read-hash", h.parse().map_err(|_| "bad key")?);
        }
    }
    let (mut ws, resp) = tungstenite::connect(req).map_err(|e| e.to_string())?;
    // Read-only access (a viewer's code): never upload; local edits stay local.
    let can_write = resp.headers().get("x-kahie-role").is_none_or(|v| v != "read");
    // Updates on the relay since its last snapshot (for client-side compaction).
    let mut pending: i64 = resp
        .headers()
        .get("x-kahie-pending")
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .unwrap_or(0);
    // Highest seq such that every update ≤ it is in our document: received updates come in
    // order; our own count once the ones before them are in (acks can overtake broadcasts).
    let mut contig = cursor.seq;
    let snapshot_seq: Option<i64> = resp
        .headers()
        .get("x-kahie-snapshot")
        .and_then(|v| v.to_str().ok()?.parse().ok());
    let mut own_acked: std::collections::BTreeSet<i64> = Default::default();
    let absorb = |contig: &mut i64, own: &mut std::collections::BTreeSet<i64>| {
        while own.remove(&(*contig + 1)) {
            *contig += 1;
        }
    };
    set_read_timeout(&mut ws, Duration::from_millis(200));
    listener.on_status(SyncStatus::Online);
    // At most one upload in flight; the cursor advances only when the relay acknowledges it.
    let mut in_flight: Option<VersionVector> = None;
    let mut imported = 0u32;
    let mut last_activity = std::time::Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = ws.close(None);
            return Ok(imported);
        }
        if let Some(idle) = until_idle {
            if in_flight.is_none() && last_activity.elapsed() >= idle {
                // One-shot sync: nothing left to send and the relay has gone quiet.
                let mut probe = cursor.vv.clone();
                if !can_write || page.outgoing(&mut probe).is_none() {
                    let _ = ws.close(None);
                    return Ok(imported);
                }
            }
        }
        let compact_now = can_write && in_flight.is_none() && pending >= COMPACT_EVERY && contig > 0;
        if let (Some(sealer), true) = (&sealer, compact_now) {
            let mut probe = cursor.vv.clone();
            if page.outgoing(&mut probe).is_none() {
                let sealed = sealer.seal(&page.snapshot())?;
                let mut frame = nb_store::access::COMPACT_MAGIC.to_vec();
                frame.extend_from_slice(&(contig as u64).to_be_bytes());
                frame.extend_from_slice(&sealed);
                ws.send(Message::Binary(frame.into())).map_err(|e| e.to_string())?;
                pending = 0;
            }
        }
        if can_write && in_flight.is_none() {
            let mut candidate = cursor.vv.clone();
            let update = page.outgoing(&mut candidate);
            if let Some(mut bytes) = update {
                if let Some(s) = &sealer {
                    bytes = s.seal(&bytes)?;
                }
                ws.send(Message::Binary(bytes.into())).map_err(|e| e.to_string())?;
                in_flight = Some(candidate);
                last_activity = std::time::Instant::now();
            }
        }
        match ws.read() {
            Ok(Message::Binary(b)) if b.len() >= 8 => {
                last_activity = std::time::Instant::now();
                let seq = u64::from_be_bytes(b[..8].try_into().expect("8 bytes")) as i64;
                let payload = &b[8..];
                if payload.is_empty() {
                    // Acknowledgement of our upload. The seq is not used as a cursor: remote
                    // updates with a lower seq may still be on their way.
                    if let Some(v) = in_flight.take() {
                        cursor.vv = v;
                        save_cursor(page, &cursor);
                        own_acked.insert(seq);
                        absorb(&mut contig, &mut own_acked);
                        pending += 1;
                    }
                    continue;
                }
                let opened = match &sealer {
                    None => Some(payload.to_vec()),
                    // Not sealed with this notebook's key (or tampered with): ignore it.
                    Some(s) => s.open(payload),
                };
                cursor.seq = cursor.seq.max(seq);
                // Consecutive, or the relay's snapshot (it covers every update ≤ its seq).
                if seq == contig + 1 || (seq > contig && Some(seq) == snapshot_seq) {
                    contig = seq;
                    absorb(&mut contig, &mut own_acked);
                }
                pending += 1;
                let Some(plain) = opened else {
                    save_cursor(page, &cursor);
                    continue;
                };
                page.incoming(&plain, &mut cursor.vv)?;
                imported += 1;
                save_cursor(page, &cursor);
                listener.on_remote_change();
            }
            Ok(Message::Close(_)) => return Err("closed by the relay".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[uniffi::export]
impl SyncSession {
    /// Start syncing `page` as document `doc_id` on the relay at `url` (`ws://` or `wss://`).
    /// `key`: the notebook key from `parse_share_code` (empty for none).
    #[uniffi::constructor]
    pub fn start(
        page: Arc<PageDoc>,
        url: String,
        token: String,
        doc_id: String,
        key: String,
        listener: Box<dyn SyncListener>,
    ) -> Arc<Self> {
        Self::spawn(page, url, token, doc_id, key, listener)
    }

    /// Sync a notebook's outline (sections, pages, titles) as document `doc_id`.
    #[uniffi::constructor]
    pub fn start_notebook(
        notebook: Arc<NotebookDoc>,
        url: String,
        token: String,
        doc_id: String,
        key: String,
        listener: Box<dyn SyncListener>,
    ) -> Arc<Self> {
        Self::spawn(notebook, url, token, doc_id, key, listener)
    }

    /// Stop syncing and wait for the thread to finish (≤ ~0.3 s).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

impl SyncSession {
    fn spawn(
        page: Arc<dyn SyncTarget>,
        url: String,
        token: String,
        doc_id: String,
        key: String,
        listener: Box<dyn SyncListener>,
    ) -> Arc<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let thread = std::thread::Builder::new()
            .name("kahie-sync".into())
            .spawn(move || {
                let mut backoff = Duration::from_secs(1);
                let ep = Endpoint {
                    url: &url,
                    token: &token,
                    doc_id: &doc_id,
                    key: &key,
                };
                while !s.load(Ordering::Relaxed) {
                    listener.on_status(SyncStatus::Connecting);
                    match run_connection(page.as_ref(), &ep, listener.as_ref(), &s, None) {
                        Ok(_) => break,
                        Err(detail) => listener.on_status(SyncStatus::Offline { detail }),
                    }
                    // Interruptible wait before reconnecting.
                    let mut waited = Duration::ZERO;
                    while waited < backoff && !s.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                        waited += Duration::from_millis(100);
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                }
            })
            .expect("spawn sync thread");
        Arc::new(Self {
            stop,
            thread: Mutex::new(Some(thread)),
        })
    }
}

impl Drop for SyncSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InkTool;
    use std::sync::atomic::AtomicUsize;

    struct Counter(Arc<AtomicUsize>, Arc<Mutex<Vec<SyncStatus>>>);
    impl SyncListener for Counter {
        fn on_remote_change(&self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn on_status(&self, status: SyncStatus) {
            self.1.lock().unwrap().push(status);
        }
    }

    fn draw(page: &PageDoc, x: f64) {
        page.begin_stroke(InkTool::Pen, 0, 3.0, x, 10.0, 0.5);
        page.add_points((1..30).flat_map(|k| [x + k as f64, 10.0 + k as f64, 0.5]).collect());
        page.end_stroke(1.0, false);
    }

    fn wait_for(cond: impl Fn() -> bool) -> bool {
        for _ in 0..100 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn two_devices_sync_through_the_relay_and_resume_after_restart() {
        let base = std::env::temp_dir().join(format!("kahie-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let cfg = nb_relay::Config {
            data_dir: base.join("relay"),
            token: Some("t".into()),
        };
        rt.spawn(async move { axum::serve(listener, nb_relay::router(cfg)).await.unwrap() });

        let pa = PageDoc::open(base.join("a.kpage").to_string_lossy().into()).unwrap();
        let pb = PageDoc::open(base.join("b.kpage").to_string_lossy().into()).unwrap();
        let (ca, cb) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let statuses = Arc::new(Mutex::new(vec![]));
        let sa = SyncSession::start(
            pa.clone(),
            url.clone(),
            "t".into(),
            "page1".into(),
            String::new(),
            Box::new(Counter(ca.clone(), statuses.clone())),
        );
        let sb = SyncSession::start(
            pb.clone(),
            url.clone(),
            "t".into(),
            "page1".into(),
            String::new(),
            Box::new(Counter(cb.clone(), Arc::default())),
        );

        draw(&pa, 0.0);
        assert!(wait_for(|| pb.item_count() == 1), "A's stroke reaches B");
        draw(&pb, 100.0);
        assert!(wait_for(|| pa.item_count() == 2), "B's stroke reaches A");
        assert_eq!(pa.layout(), pb.layout());
        assert!(cb.load(Ordering::Relaxed) >= 1);
        assert!(statuses.lock().unwrap().contains(&SyncStatus::Online));

        // B goes offline, both draw, B restarts (new PageDoc from disk) and catches up.
        sb.stop();
        drop(pb);
        draw(&pa, 200.0);
        let pb = PageDoc::open(base.join("b.kpage").to_string_lossy().into()).unwrap();
        draw(&pb, 300.0);
        let sb = SyncSession::start(
            pb.clone(),
            url.clone(),
            "t".into(),
            "page1".into(),
            String::new(),
            Box::new(Counter(cb, Arc::default())),
        );
        assert!(
            wait_for(|| pa.item_count() == 4 && pb.item_count() == 4),
            "offline edits merge both ways"
        );
        assert_eq!(pa.layout(), pb.layout());

        // A wrong token reports Offline and keeps retrying without crashing.
        let bad = Arc::new(Mutex::new(vec![]));
        let url_nb = url.clone();
        let sc = SyncSession::start(
            pa.clone(),
            url,
            "wrong".into(),
            "page1".into(),
            String::new(),
            Box::new(Counter(Arc::default(), bad.clone())),
        );
        assert!(wait_for(|| bad
            .lock()
            .unwrap()
            .iter()
            .any(|s| matches!(s, SyncStatus::Offline { .. }))));
        sc.stop();
        sa.stop();
        sb.stop();

        // A page that is not open syncs with one call.
        let pc = PageDoc::open(base.join("c.kpage").to_string_lossy().into()).unwrap();
        let n = sync_page_once(
            pc.clone(),
            url_nb.clone(),
            "t".into(),
            "page1".into(),
            String::new(),
            300,
        )
        .unwrap();
        assert!(
            n >= 1 && pc.layout() == pa.layout(),
            "one-shot sync caught up ({n} updates)"
        );
        draw(&pc, 400.0);
        sync_page_once(
            pc.clone(),
            url_nb.clone(),
            "t".into(),
            "page1".into(),
            String::new(),
            300,
        )
        .unwrap();
        sync_page_once(
            pa.clone(),
            url_nb.clone(),
            "t".into(),
            "page1".into(),
            String::new(),
            300,
        )
        .unwrap();
        assert_eq!(pa.item_count(), 5, "one-shot sync uploads local edits");

        // Notebook outlines: device B joins A's notebook and sees its pages, without an extra page.
        let na = NotebookDoc::open(base.join("nb-a").to_string_lossy().into(), "Physique".into()).unwrap();
        na.add_page(None);
        let nb_b = NotebookDoc::join(base.join("nb-b").to_string_lossy().into()).unwrap();
        assert!(nb_b.page_ids().is_empty(), "a joining notebook starts empty");
        let s1 = SyncSession::start_notebook(
            na.clone(),
            url_nb.clone(),
            "t".into(),
            "nb1".into(),
            String::new(),
            Box::new(Counter(Arc::default(), Arc::default())),
        );
        let s2 = SyncSession::start_notebook(
            nb_b.clone(),
            url_nb,
            "t".into(),
            "nb1".into(),
            String::new(),
            Box::new(Counter(Arc::default(), Arc::default())),
        );
        assert!(wait_for(|| nb_b.page_ids() == na.page_ids()), "outline synced");
        assert_eq!(nb_b.title(), "Physique");
        s1.stop();
        s2.stop();
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn share_codes_and_read_only_viewers() {
        let code = new_share_code(false);
        let info = parse_share_code(code.clone()).unwrap();
        assert!(!info.read_only && info.key.starts_with('w') && code.starts_with(&info.group));
        let ro = parse_share_code(info.read_only_code.clone().unwrap()).unwrap();
        assert!(ro.read_only && ro.read_only_code.is_none() && ro.group == info.group);
        assert_ne!(new_share_code(false), code, "random");
        assert!(!info.encrypted);
        assert!(parse_share_code("bad code".into()).is_none());
        assert_eq!(parse_share_code("nlegacy1".into()).unwrap().key, "");

        let base = std::env::temp_dir().join(format!("kahie-sync-ro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let cfg = nb_relay::Config {
            data_dir: base.join("relay"),
            token: None,
        };
        rt.spawn(async move { axum::serve(listener, nb_relay::router(cfg)).await.unwrap() });
        let doc = format!("{}-p1", info.group);
        let teacher = PageDoc::open(base.join("t.kpage").to_string_lossy().into()).unwrap();
        let student = PageDoc::open(base.join("s.kpage").to_string_lossy().into()).unwrap();
        draw(&teacher, 0.0);
        sync_page_once(
            teacher.clone(),
            url.clone(),
            String::new(),
            doc.clone(),
            info.key.clone(),
            300,
        )
        .unwrap();
        // The student (read-only code) receives the page; their own ink stays local.
        draw(&student, 100.0);
        let n = sync_page_once(
            student.clone(),
            url.clone(),
            String::new(),
            doc.clone(),
            ro.key.clone(),
            300,
        )
        .unwrap();
        assert_eq!((n, student.item_count()), (1, 2));
        sync_page_once(
            teacher.clone(),
            url.clone(),
            String::new(),
            doc.clone(),
            info.key.clone(),
            300,
        )
        .unwrap();
        assert_eq!(teacher.item_count(), 1, "nothing uploaded by a viewer");
        // Without a key, the claimed notebook is refused.
        assert!(sync_page_once(student.clone(), url, String::new(), doc, String::new(), 300).is_err());
        std::fs::remove_dir_all(base).ok();
    }

    /// Every relay file of notebook `group` (databases, journals, `.acl`), concatenated.
    fn all_bytes(dir: &std::path::Path, group: &str) -> Vec<u8> {
        let mut out = vec![];
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.file_name().to_string_lossy().starts_with(group) {
                out.extend(std::fs::read(e.path()).unwrap_or_default());
            }
        }
        out
    }

    fn contains(hay: &[u8], needle: &str) -> bool {
        hay.windows(needle.len()).any(|w| w == needle.as_bytes())
    }

    #[test]
    fn sealer_rejects_tampering_and_other_documents() {
        let k = [7u8; 32];
        let a = Sealer::new(&k, "n1-p1");
        let sealed = a.seal(b"loro update").unwrap();
        assert!(nb_store::access::is_sealed(&sealed));
        assert_eq!(a.open(&sealed).as_deref(), Some(b"loro update".as_slice()));
        assert_ne!(a.seal(b"loro update").unwrap(), sealed, "fresh nonce each time");
        assert!(
            Sealer::new(&k, "n1-p2").open(&sealed).is_none(),
            "bound to its document"
        );
        assert!(Sealer::new(&[8u8; 32], "n1-p1").open(&sealed).is_none(), "other key");
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(a.open(&bad).is_none() && a.open(b"KHE1").is_none() && a.open(b"loro").is_none());
    }

    #[test]
    fn end_to_end_encryption_editor_and_viewer() {
        let base = std::env::temp_dir().join(format!("kahie-sync-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let relay_dir = base.join("relay");
        let cfg = nb_relay::Config {
            data_dir: relay_dir.clone(),
            token: None,
        };
        rt.spawn(async move { axum::serve(listener, nb_relay::router(cfg)).await.unwrap() });

        const SECRET: &str = "Interrogation surprise mardi 8h";
        const TITLE: &str = "Physique-chimie 2nde B";
        let once = |p: &Arc<PageDoc>, doc: &str, key: &str| {
            sync_page_once(p.clone(), url.clone(), String::new(), doc.into(), key.into(), 300).unwrap()
        };

        // Control: a plain code leaves the text readable on the relay.
        let plain = parse_share_code(new_share_code(false)).unwrap();
        let p0 = PageDoc::open(base.join("plain.kpage").to_string_lossy().into()).unwrap();
        p0.add_text(10.0, 10.0, 200.0, 40.0, SECRET.into(), 16.0, 0);
        once(&p0, &format!("{}-p1", plain.group), &plain.key);
        assert!(
            contains(&all_bytes(&relay_dir, &plain.group), SECRET),
            "the check can see plaintext"
        );

        let editor = parse_share_code(new_share_code(true)).unwrap();
        assert!(editor.encrypted && !editor.read_only && editor.key.starts_with('e'));
        let viewer = parse_share_code(editor.read_only_code.clone().unwrap()).unwrap();
        assert!(viewer.encrypted && viewer.read_only && viewer.key.starts_with('v'));
        let doc = format!("{}-p1", editor.group);

        // Teacher writes a page and the outline; a second editor device and a student sync.
        let teacher = PageDoc::open(base.join("t.kpage").to_string_lossy().into()).unwrap();
        teacher.add_text(10.0, 10.0, 200.0, 40.0, SECRET.into(), 16.0, 0);
        draw(&teacher, 0.0);
        once(&teacher, &doc, &editor.key);
        let colleague = PageDoc::open(base.join("c.kpage").to_string_lossy().into()).unwrap();
        assert_eq!(once(&colleague, &doc, &editor.key), 1);
        draw(&colleague, 100.0);
        once(&colleague, &doc, &editor.key);
        let student = PageDoc::open(base.join("s.kpage").to_string_lossy().into()).unwrap();
        once(&student, &doc, &viewer.key);
        once(&teacher, &doc, &editor.key);
        assert_eq!(teacher.item_count(), 3);
        assert_eq!(student.layout(), teacher.layout(), "the viewer decrypts everything");
        assert_eq!(student.text_blocks()[0].text, SECRET);

        let na = NotebookDoc::open(base.join("nb-a").to_string_lossy().into(), TITLE.into()).unwrap();
        na.add_page(None);
        let nb_s = NotebookDoc::join(base.join("nb-s").to_string_lossy().into()).unwrap();
        let s1 = SyncSession::start_notebook(
            na.clone(),
            url.clone(),
            String::new(),
            editor.group.clone(),
            editor.key.clone(),
            Box::new(Silent),
        );
        let s2 = SyncSession::start_notebook(
            nb_s.clone(),
            url.clone(),
            String::new(),
            viewer.group.clone(),
            viewer.key.clone(),
            Box::new(Silent),
        );
        assert!(wait_for(|| nb_s.title() == TITLE), "the outline reaches the viewer");
        s1.stop();
        s2.stop();

        // The relay holds no plaintext and no secret key, and its log is only sealed updates.
        let stored = all_bytes(&relay_dir, &editor.group);
        assert!(!stored.is_empty());
        for leak in [SECRET, TITLE, &editor.key[1..], &viewer.key[1..]] {
            assert!(!contains(&stored, leak), "relay data contains {leak:?}");
        }
        let store = nb_store::Store::open(relay_dir.join(format!("{doc}.db"))).unwrap();
        let log = store.load().unwrap();
        assert!(
            log.snapshot.as_ref().is_none_or(|s| nb_store::access::is_sealed(s)),
            "only editors' sealed snapshots"
        );
        assert!(log.updates.iter().all(|u| nb_store::access::is_sealed(u)));
        // The relay's own compaction leaves encrypted logs alone.
        nb_relay::compact_document(&relay_dir, &doc).unwrap();
        assert_eq!(store.load().unwrap(), log);

        // The plain key of another notebook, or no key, is refused; a plaintext update
        // injected by the relay is ignored by end-to-end clients.
        let fresh = PageDoc::open(base.join("f.kpage").to_string_lossy().into()).unwrap();
        assert!(sync_page_once(
            fresh.clone(),
            url.clone(),
            String::new(),
            doc.clone(),
            String::new(),
            300
        )
        .is_err());
        store.append_update(&p0_update(&p0)).unwrap();
        drop(store);
        // Every stored frame (an editor's snapshot, if any, then the updates) is imported.
        let frames = log.updates.len() + usize::from(log.snapshot.is_some());
        assert_eq!(once(&fresh, &doc, &viewer.key), frames as u32);
        assert_eq!(fresh.layout(), teacher.layout(), "injected plaintext ignored");
        std::fs::remove_dir_all(base).ok();
    }

    /// The whole content of a page as one Loro update (a forged, unsealed relay payload).
    fn p0_update(p: &PageDoc) -> Vec<u8> {
        let mut vv = VersionVector::default();
        p.outgoing(&mut vv).unwrap()
    }

    #[test]
    fn editors_compact_encrypted_logs_without_losing_updates() {
        let base = std::env::temp_dir().join(format!("kahie-sync-e2e-compact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let relay_dir = base.join("relay");
        let cfg = nb_relay::Config {
            data_dir: relay_dir.clone(),
            token: None,
        };
        rt.spawn(async move { axum::serve(listener, nb_relay::router(cfg)).await.unwrap() });
        let editor = parse_share_code(new_share_code(true)).unwrap();
        let viewer = parse_share_code(editor.read_only_code.clone().unwrap()).unwrap();
        let doc = format!("{}-p1", editor.group);
        let once = |p: &Arc<PageDoc>, key: &str| {
            sync_page_once(p.clone(), url.clone(), String::new(), doc.clone(), key.into(), 300).unwrap()
        };
        let a = PageDoc::open(base.join("a.kpage").to_string_lossy().into()).unwrap();
        let b = PageDoc::open(base.join("b.kpage").to_string_lossy().into()).unwrap();
        a.add_text(10.0, 10.0, 200.0, 40.0, "Secret de famille".into(), 16.0, 0);
        // Two editors take turns, so the log mixes both authors' updates.
        for i in 0..6 {
            draw(&a, i as f64 * 10.0);
            once(&a, &editor.key);
            draw(&b, 300.0 + i as f64 * 10.0);
            once(&b, &editor.key);
        }
        once(&a, &editor.key);
        once(&b, &editor.key);
        let store = nb_store::Store::open(relay_dir.join(format!("{doc}.db"))).unwrap();
        let (snap_seq, snap) = store.latest_snapshot().unwrap().expect("an editor compacted the log");
        assert!(nb_store::access::is_sealed(&snap), "the snapshot is encrypted");
        assert!(store.updates_after(0).unwrap().iter().all(|(s, _)| *s > snap_seq));
        assert!(store.pending_updates().unwrap() < 13, "log shortened");
        // A new viewer catches up from the snapshot and sees everything.
        let c = PageDoc::open(base.join("c.kpage").to_string_lossy().into()).unwrap();
        once(&c, &viewer.key);
        once(&a, &editor.key);
        assert_eq!(a.item_count(), 13);
        assert_eq!(c.layout(), a.layout());
        assert_eq!(b.layout(), a.layout());
        let mut raw = vec![];
        for e in std::fs::read_dir(&relay_dir).unwrap().flatten() {
            raw.extend(std::fs::read(e.path()).unwrap_or_default());
        }
        assert!(!contains(&raw, "Secret de famille"));
        std::fs::remove_dir_all(base).ok();
    }
}
