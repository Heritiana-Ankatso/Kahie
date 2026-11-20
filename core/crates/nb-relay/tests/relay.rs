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
