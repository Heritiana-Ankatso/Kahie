use futures_util::{SinkExt, StreamExt};
use nb_ink::Point;
use nb_model::crdt::CrdtPage;
use nb_model::{Content, Item, Op, Stroke, Tool};
use nb_relay::{router, valid_doc_id, Config};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn start(token: Option<&str>) -> (String, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("nb-relay-{}-{}", std::process::id(), rand_suffix()));
    let cfg = Config {
        data_dir: dir.clone(),
        token: token.map(String::from),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(cfg)).await.unwrap() });
    (format!("ws://{addr}"), dir)
}

fn rand_suffix() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u8(0);
    h.finish()
}

async fn connect(base: &str, doc: &str, after: i64, token: Option<&str>) -> Result<Ws, String> {
    let mut req = format!("{base}/sync/{doc}?after={after}")
        .into_client_request()
        .unwrap();
    if let Some(t) = token {
        req.headers_mut()
            .insert("authorization", format!("Bearer {t}").parse().unwrap());
    }
    tokio_tungstenite::connect_async(req)
        .await
        .map(|(ws, _)| ws)
        .map_err(|e| e.to_string())
}

async fn next_frame(ws: &mut Ws) -> (i64, Vec<u8>) {
    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timeout")
            .unwrap()
            .unwrap()
        {
            Message::Binary(b) => {
                let seq = u64::from_be_bytes(b[..8].try_into().unwrap()) as i64;
                return (seq, b[8..].to_vec());
            }
            _ => continue,
        }
    }
}

fn stroke(id: &str, x: f64) -> Item {
    Item {
        id: id.into(),
        content: Content::Stroke(Stroke {
            tool: Tool::Pen,
            color: 0,
            width: 3.0,
            points: (0..50).map(|k| Point::new(x + k as f64, 10.0, 0.5)).collect(),
            interp: None,
        }),
    }
}

#[tokio::test]
async fn auth_catch_up_live_forwarding_and_convergence() {
    let (base, dir) = start(Some("s3cret")).await;
    assert!(connect(&base, "doc1", 0, None).await.is_err(), "no token → refused");
    assert!(connect(&base, "doc1", 0, Some("wrong")).await.is_err());
    assert!(connect(&base, "../etc", 0, Some("s3cret")).await.is_err());
    assert!(!valid_doc_id("a/b") && valid_doc_id("class-2B_2026"));

    // Two replicas of the same page sync through the relay.
    // The creator uploads the page's initial state first (protocol rule: every update's
    // dependencies must reach the relay before it).
    let mut a = CrdtPage::new("p", 1);
    let mut wa = connect(&base, "doc1", 0, Some("s3cret")).await.unwrap();
    wa.send(Message::Binary(a.take_unsaved().unwrap().into()))
        .await
        .unwrap();
    assert_eq!(next_frame(&mut wa).await.0, 1);
    // A joining replica is built from what the relay sends, never from a fresh page.
    let mut wb = connect(&base, "doc1", 0, Some("s3cret")).await.unwrap();
    let (_, init) = next_frame(&mut wb).await;
    let mut b = CrdtPage::load(&[&init], 2).unwrap();

    a.apply(&Op::add(a.page(), vec![stroke("a1", 0.0)]));
    wa.send(Message::Binary(a.take_unsaved().unwrap().into()))
        .await
        .unwrap();
    let (ack, empty) = next_frame(&mut wa).await;
    assert_eq!((ack, empty.len()), (2, 0), "sender gets an ack with the seq");
    let (seq, upd) = next_frame(&mut wb).await;
    assert_eq!(seq, 2);
    b.import(&upd).unwrap();

    b.apply(&Op::add(b.page(), vec![stroke("b1", 100.0)]));
    wb.send(Message::Binary(b.take_unsaved().unwrap().into()))
        .await
        .unwrap();
    let (_, upd) = next_frame(&mut wa).await;
    a.import(&upd).unwrap();
    assert_eq!(a.page(), b.page());
    assert_eq!(a.page().items.len(), 2);

    // A late client catches up from storage, and a reconnect with a cursor gets only newer updates.
    let mut late = connect(&base, "doc1", 0, Some("s3cret")).await.unwrap();
    let mut chunks = vec![];
    for _ in 0..3 {
        chunks.push(next_frame(&mut late).await.1);
    }
    let refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
    let c = CrdtPage::load(&refs, 3).unwrap();
    assert_eq!(c.page().items, a.page().items);
    drop(late);
    let mut again = connect(&base, "doc1", 2, Some("s3cret")).await.unwrap();
    assert_eq!(next_frame(&mut again).await.0, 3);
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn compaction_keeps_old_and_new_clients_correct() {
    let (base, dir) = start(None).await;
    let mut a = CrdtPage::new("p", 1);
    let mut wa = connect(&base, "doc2", 0, None).await.unwrap();
    wa.send(Message::Binary(a.take_unsaved().unwrap().into()))
        .await
        .unwrap();
    next_frame(&mut wa).await;
    for i in 0..5 {
        a.apply(&Op::add(a.page(), vec![stroke(&format!("s{i}"), i as f64 * 10.0)]));
        wa.send(Message::Binary(a.take_unsaved().unwrap().into()))
            .await
            .unwrap();
        next_frame(&mut wa).await; // ack
    }
    drop(wa);
    // Compact while nobody is connected, then add one more update.
    nb_relay::compact_document(&dir, "doc2").unwrap();
    let mut wa = connect(&base, "doc2", 6, None).await.unwrap();
    a.apply(&Op::add(a.page(), vec![stroke("late", 99.0)]));
    wa.send(Message::Binary(a.take_unsaved().unwrap().into()))
        .await
        .unwrap();
    assert_eq!(next_frame(&mut wa).await.0, 7);

    // A brand-new client (cursor 0) gets the snapshot (seq 6) then update 7.
    let mut fresh = connect(&base, "doc2", 0, None).await.unwrap();
    let (s1, snap) = next_frame(&mut fresh).await;
    let (s2, upd) = next_frame(&mut fresh).await;
    assert_eq!((s1, s2), (6, 7));
    let c = CrdtPage::load(&[&snap, &upd], 9).unwrap();
    assert_eq!(c.page().items, a.page().items);
    // A client already at seq 6 gets only update 7, not the snapshot.
    let mut current = connect(&base, "doc2", 6, None).await.unwrap();
    assert_eq!(next_frame(&mut current).await.0, 7);
    std::fs::remove_dir_all(dir).ok();
}

/// Connect with a notebook key; returns the socket and the role the relay granted.
async fn connect_key(base: &str, doc: &str, key: Option<&str>) -> Result<(Ws, String), String> {
    let mut req = format!("{base}/sync/{doc}?after=0").into_client_request().unwrap();
    if let Some(k) = key {
        req.headers_mut().insert(nb_relay::KEY_HEADER, k.parse().unwrap());
    }
    let (ws, resp) = tokio_tungstenite::connect_async(req).await.map_err(|e| e.to_string())?;
    let role = resp
        .headers()
        .get(nb_relay::ROLE_HEADER)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    Ok((ws, role))
}

#[tokio::test]
async fn notebook_keys_claim_read_only_and_refusal() {
    use nb_store::access::read_key;
    let (base, dir) = start(None).await;
    let w = "w00112233445566778899aabbccddeeff";
    let r = read_key(w).unwrap();

    // Before any key: open as before (write), and a read key reads without claiming.
    assert_eq!(connect_key(&base, "nold", None).await.unwrap().1, "write");
    assert_eq!(connect_key(&base, "nbook", Some(&r)).await.unwrap().1, "read");
    assert!(!dir.join("nbook.acl").exists());

    // The write key claims the group (outline and pages alike).
    let (mut writer, role) = connect_key(&base, "nbook-p1", Some(w)).await.unwrap();
    assert_eq!(role, "write");
    assert!(dir.join("nbook.acl").exists());
    assert!(connect_key(&base, "nbook", None).await.is_err(), "no key → refused");
    assert!(
        connect_key(&base, "nbook-p2", Some("w99999999999999999999999999999999"))
            .await
            .is_err()
    );
    assert!(connect_key(&base, "nbook", Some("not-a-key")).await.is_err());
    assert_eq!(connect_key(&base, "nbook", Some(w)).await.unwrap().1, "write");

    // A reader receives the writer's updates but may not send any.
    let (mut reader, role) = connect_key(&base, "nbook-p1", Some(&r)).await.unwrap();
    assert_eq!(role, "read");
    let mut a = CrdtPage::new("p1", 1);
    a.apply(&Op::add(a.page(), vec![stroke("s", 0.0)]));
    writer
        .send(Message::Binary(a.take_unsaved().unwrap().into()))
        .await
        .unwrap();
    assert_eq!(next_frame(&mut writer).await, (1, vec![]));
    let (seq, upd) = next_frame(&mut reader).await;
    assert_eq!(seq, 1);
    assert_eq!(CrdtPage::load(&[&upd], 2).unwrap().page().items, a.page().items);
    reader.send(Message::Binary(vec![1, 2, 3].into())).await.unwrap();
    let closed = loop {
        match tokio::time::timeout(std::time::Duration::from_secs(5), reader.next())
            .await
            .expect("timeout")
        {
            Some(Ok(Message::Close(Some(f)))) => break u16::from(f.code),
            Some(Ok(_)) => continue,
            _ => break 0,
        }
    };
    assert_eq!(closed, 1008);
    // Nothing was stored from the reader: a new client sees only update 1.
    let (mut fresh, _) = connect_key(&base, "nbook-p1", Some(w)).await.unwrap();
    assert_eq!(next_frame(&mut fresh).await.0, 1);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), fresh.next())
            .await
            .is_err()
    );
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn end_to_end_claim_and_sealed_updates_are_never_compacted() {
    use nb_store::access::ShareCode;
    let (base, dir) = start(None).await;
    let editor = ShareCode::parse("ne2e.e00112233445566778899aabbccddeeff").unwrap();
    let viewer = editor.read_only().unwrap();
    let (wa, ra) = (editor.relay_key(), viewer.relay_key());

    // Claim with the derived credential and the viewers' credential hash.
    let mut req = format!("{base}/sync/ne2e-p1?after=0").into_client_request().unwrap();
    req.headers_mut().insert(nb_relay::KEY_HEADER, wa.parse().unwrap());
    req.headers_mut().insert(
        nb_relay::READ_HASH_HEADER,
        editor.relay_read_hash().unwrap().parse().unwrap(),
    );
    let (mut writer, resp) = tokio_tungstenite::connect_async(req).await.unwrap();
    assert_eq!(resp.headers().get(nb_relay::ROLE_HEADER).unwrap(), "write");
    let acl = std::fs::read_to_string(dir.join("ne2e.acl")).unwrap();
    assert!(!acl.contains(&editor.key[1..]) && !acl.contains(&viewer.key[1..]));
    assert_eq!(connect_key(&base, "ne2e", Some(&ra)).await.unwrap().1, "read");
    assert!(
        connect_key(&base, "ne2e", Some(&viewer.key)).await.is_err(),
        "secret keys are not credentials"
    );
    let bad = format!("{base}/sync/nzz?after=0").into_client_request().map(|mut r| {
        r.headers_mut()
            .insert(nb_relay::READ_HASH_HEADER, "xyz".parse().unwrap());
        r
    });
    assert!(tokio_tungstenite::connect_async(bad.unwrap()).await.is_err());

    // Sealed updates are stored and forwarded byte for byte, and never compacted.
    let sealed: Vec<Vec<u8>> = (0..3u8).map(|i| [b"KHE1".as_slice(), &[i; 40]].concat()).collect();
    for s in &sealed {
        writer.send(Message::Binary(s.clone().into())).await.unwrap();
        next_frame(&mut writer).await;
    }
    nb_relay::compact_document(&dir, "ne2e-p1").unwrap();
    let (mut reader, _) = connect_key(&base, "ne2e-p1", Some(&ra)).await.unwrap();
    for (i, s) in sealed.iter().enumerate() {
        assert_eq!(
            next_frame(&mut reader).await,
            (i as i64 + 1, s.clone()),
            "not compacted"
        );
    }
    std::fs::remove_dir_all(dir).ok();
}
