//! Relay sync client (decision D9; server in `nb-relay`, protocol in its crate docs).
//!
//! A [`SyncSession`] runs on its own thread: it connects to the relay, uploads the changes
//! the relay lacks (`CrdtPage::sync_outgoing`), imports the ones it sends
//! (`CrdtPage::sync_incoming`), saves both to the page's database and tells the UI to redraw.
//! It reconnects with exponential backoff. The relay cursor (version vector and highest
//! sequence number received) is stored in the page database, so it survives restarts and
//! offline edits are uploaded on the next connection.

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
}

/// Parse a share code typed or stored by the app; `None` if it is malformed.
#[uniffi::export]
pub fn parse_share_code(code: String) -> Option<ShareInfo> {
    use nb_store::access::{Role, ShareCode};
    let c = ShareCode::parse(&code)?;
    Some(ShareInfo {
        read_only: c.role() == Role::Read,
        read_only_code: c.read_only().map(|r| r.to_code()),
        group: c.group,
        key: c.key,
    })
}

/// A new editor share code: random group and write key (the first device to sync with it
/// claims the notebook on the relay).
#[uniffi::export]
pub fn new_share_code() -> String {
    let mut b = [0u8; 24];
    let _ = rustls::crypto::ring::default_provider().secure_random.fill(&mut b);
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("n{}.w{}", &hex[..15], &hex[16..48])
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
    /// Import, save to the local database.
    fn incoming(&self, bytes: &[u8], relay: &mut VersionVector) -> Result<(), String>;
    fn meta(&self, key: &str) -> Option<Vec<u8>>;
    fn set_meta(&self, key: &str, value: &[u8]);
}

impl SyncTarget for PageDoc {
    fn outgoing(&self, relay: &mut VersionVector) -> Option<Vec<u8>> {
        self.state.lock().unwrap().doc.sync_outgoing(relay)
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
        req.headers_mut()
            .insert("x-kahie-key", key.parse().map_err(|_| "bad key")?);
    }
    let (mut ws, resp) = tungstenite::connect(req).map_err(|e| e.to_string())?;
    // Read-only access (a viewer's code): never upload; local edits stay local.
    let can_write = resp.headers().get("x-kahie-role").is_none_or(|v| v != "read");
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
        if can_write && in_flight.is_none() {
            let mut candidate = cursor.vv.clone();
            let update = page.outgoing(&mut candidate);
            if let Some(bytes) = update {
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
                    }
                    continue;
                }
                page.incoming(payload, &mut cursor.vv)?;
                imported += 1;
                cursor.seq = cursor.seq.max(seq);
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
        let code = new_share_code();
        let info = parse_share_code(code.clone()).unwrap();
        assert!(!info.read_only && info.key.starts_with('w') && code.starts_with(&info.group));
        let ro = parse_share_code(info.read_only_code.clone().unwrap()).unwrap();
        assert!(ro.read_only && ro.read_only_code.is_none() && ro.group == info.group);
        assert_ne!(new_share_code(), code, "random");
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
}
