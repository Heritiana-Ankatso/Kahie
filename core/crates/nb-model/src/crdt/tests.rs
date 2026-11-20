use super::*;
use loro::VersionVector;
use nb_recog::Shape;

fn stroke(id: &str, pts: &[(f64, f64)]) -> Item {
    Item {
        id: id.into(),
        content: Content::Stroke(Stroke {
            tool: Tool::Pen,
            color: 0x1d1d1f,
            width: 3.0,
            points: pts.iter().map(|&(x, y)| Point::new(x, y, 0.5)).collect(),
            interp: None,
        }),
    }
}

fn text(id: &str, s: &str) -> Item {
    Item {
        id: id.into(),
        content: Content::Text {
            rect: crate::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 20.0,
            },
            text: s.into(),
            size: 16.0,
            color: 0,
        },
    }
}

fn ids(p: &CrdtPage) -> Vec<&str> {
    p.page().items.iter().map(|i| i.id.as_str()).collect()
}

fn raw_ink(p: &CrdtPage, id: &str) -> Vec<u8> {
    let m = child_map(&p.doc.get_map(OBJECTS), id).unwrap();
    match m.get("ink") {
        Some(ValueOrContainer::Value(LoroValue::Binary(b))) => b.to_vec(),
        _ => panic!("no ink"),
    }
}

fn sync(a: &mut CrdtPage, b: &mut CrdtPage) {
    let ab = a.updates_since(&b.version());
    let ba = b.updates_since(&a.version());
    b.import(&ab).unwrap();
    a.import(&ba).unwrap();
}

#[test]
fn add_remove_update_undo_redo() {
    let mut p = CrdtPage::new("p", 1);
    let op = Op::add(p.page(), vec![stroke("a", &[(0.0, 0.0), (10.0, 0.0)]), text("b", "x")]);
    assert!(p.apply(&op));
    assert_eq!(ids(&p), ["a", "b"]);
    let op = Op::remove(p.page(), &["a"]);
    p.apply(&op);
    assert_eq!(ids(&p), ["b"]);
    assert_eq!(p.trash().len(), 1);
    assert!(p.undo());
    assert_eq!(ids(&p), ["a", "b"], "restored at its index");
    let moved = p.page().items[0].translated(5.0, 5.0);
    let op = Op::update(p.page(), vec![moved]);
    p.apply(&op);
    assert_eq!(p.page().items[0].bounds().x, 5.0 - 2.5);
    p.undo();
    assert_eq!(p.page().items[0].bounds().x, -2.5);
    assert!(p.redo());
    assert_eq!(p.page().items[0].bounds().x, 2.5);
    while p.undo() {}
    assert!(p.page().items.is_empty());
    assert!(!p.apply(&Op::remove(p.page(), &["missing"])), "empty op ignored");
}

#[test]
fn snapshot_round_trip_and_migration() {
    let mut old = Page::new("legacy");
    old.template = "grid".into();
    old.items.push(stroke("a", &[(1.0, 2.0), (3.0, 4.0)]));
    old.items.push(text("b", "bonjour"));
    let p = CrdtPage::from_page(&old, 1);
    assert!(!p.can_undo(), "migration is not an undo step");
    assert_eq!(p.template(), "grid");
    assert_eq!(p.page().items, old.items, "points on the 1/16 grid survive exactly");
    let q = CrdtPage::from_snapshot(&p.snapshot(), 2).unwrap();
    assert_eq!(q.page(), p.page());
    assert!(CrdtPage::from_snapshot(b"not a page", 3).is_err());
}

#[test]
fn undo_is_per_user() {
    let mut a = CrdtPage::new("p", 1);
    let mut b = CrdtPage::from_snapshot(&a.snapshot(), 2).unwrap();
    a.apply(&Op::add(a.page(), vec![stroke("mine", &[(0.0, 0.0)])]));
    b.apply(&Op::add(b.page(), vec![stroke("theirs", &[(5.0, 5.0)])]));
    sync(&mut a, &mut b);
    assert_eq!(a.page().items.len(), 2);
    assert!(a.undo());
    assert_eq!(ids(&a), ["theirs"], "only the local change is undone");
    assert!(!a.undo(), "remote changes are not on the local undo stack");
}

#[test]
fn strokes_are_never_rewritten() {
    // ADR-0003: interpretations and moves leave the stored ink untouched.
    let mut p = CrdtPage::new("p", 1);
    p.apply(&Op::add(
        p.page(),
        vec![stroke("s", &[(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)])],
    ));
    let before = raw_ink(&p, "s");
    let mut it = p.page().items[0].clone();
    if let Content::Stroke(st) = &mut it.content {
        st.interp = Some(Interp {
            shape: Shape::Line {
                a: Point::xy(0.0, 0.0),
                b: Point::xy(100.0, 0.0),
            },
            show: true,
        });
    }
    p.apply(&Op::update(p.page(), vec![it]));
    let moved = p.page().items[0].translated(10.0, 20.0);
    p.apply(&Op::update(p.page(), vec![moved]));
    assert_eq!(raw_ink(&p, "s"), before);
    let Content::Stroke(st) = &p.page().items[0].content else {
        unreachable!()
    };
    assert_eq!((st.points[2].x, st.points[2].y), (110.0, 20.0));
    assert_eq!(st.interp.as_ref().unwrap().shape.points()[1], Point::xy(110.0, 20.0));
    while p.undo() {}
    assert!(p.trash().is_empty() && p.page().items.is_empty());
}

#[test]
fn trash_and_restore() {
    let mut p = CrdtPage::new("p", 1);
    p.apply(&Op::add(p.page(), vec![text("a", "1"), text("b", "2")]));
    p.apply(&Op::remove(p.page(), &["a"]));
    assert_eq!(p.trash()[0].id, "a");
    assert!(p.restore(&["a"]));
    assert_eq!(ids(&p), ["b", "a"], "restored on top");
    assert!(!p.restore(&["a"]), "already restored");
}

/// Deterministic PRNG so failures are reproducible.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn random_command(p: &mut CrdtPage, r: &mut Rng, tag: &str, i: usize) {
    let n = p.page().items.len();
    match r.below(10) {
        0..=4 => {
            let x = r.below(500) as f64;
            let it = stroke(&format!("{tag}{i}"), &[(x, 1.0), (x + 4.0, 9.0)]);
            let op = Op::Add(vec![(r.below(n + 1), it)]);
            p.apply(&op);
        }
        5 | 6 if n > 0 => {
            let id = p.page().items[r.below(n)].id.clone();
            p.apply(&Op::remove(p.page(), &[&id]));
        }
        7 if n > 0 => {
            let it = p.page().items[r.below(n)].translated(1.0, -2.0);
            p.apply(&Op::update(p.page(), vec![it]));
        }
        8 => {
            p.undo();
        }
        _ => {
            p.set_template(["lined", "grid", "dots"][r.below(3)]);
        }
    }
}

#[test]
fn three_replicas_converge_under_random_concurrent_commands() {
    for seed in 1..=20u64 {
        let mut r = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let base = CrdtPage::new("p", 100);
        let mut reps: Vec<CrdtPage> = (0..3)
            .map(|k| CrdtPage::from_snapshot(&base.snapshot(), k + 1).unwrap())
            .collect();
        for round in 0..15 {
            for (k, rep) in reps.iter_mut().enumerate() {
                for i in 0..r.below(6) {
                    random_command(rep, &mut r, &format!("r{k}-{round}-"), i);
                }
            }
            // Partial, random gossip between two replicas.
            let (a, b) = (r.below(3), r.below(3));
            if a != b {
                let (lo, hi) = (a.min(b), a.max(b));
                let (left, right) = reps.split_at_mut(hi);
                sync(&mut left[lo], &mut right[0]);
            }
        }
        // Full sync: everyone ends with everything.
        for _ in 0..2 {
            for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                let (left, right) = reps.split_at_mut(b);
                sync(&mut left[a], &mut right[0]);
            }
        }
        assert_eq!(reps[0].page(), reps[1].page(), "seed {seed}");
        assert_eq!(reps[1].page(), reps[2].page(), "seed {seed}");
        let unique: std::collections::HashSet<_> = ids(&reps[0]).into_iter().collect();
        assert_eq!(
            unique.len(),
            reps[0].page().items.len(),
            "no duplicate objects, seed {seed}"
        );
    }
}

#[test]
fn unsaved_changes_are_handed_out_once_and_reload_exactly() {
    let mut p = CrdtPage::new("p", 1);
    let mut log: Vec<Vec<u8>> = vec![];
    log.push(p.take_unsaved().expect("initial meta"));
    assert!(p.take_unsaved().is_none());
    p.apply(&Op::add(p.page(), vec![stroke("a", &[(0.0, 0.0), (4.0, 4.0)])]));
    log.push(p.take_unsaved().unwrap());
    p.undo();
    p.redo();
    p.set_template("dots");
    log.push(p.take_unsaved().unwrap());
    let chunks: Vec<&[u8]> = log.iter().map(Vec::as_slice).collect();
    let mut q = CrdtPage::load(&chunks, 2).unwrap();
    assert_eq!(q.page(), p.page());
    assert!(q.take_unsaved().is_none(), "loaded state counts as saved");
}

#[test]
#[ignore]
fn print_update_sizes() {
    let mut p = CrdtPage::new("p", 1);
    p.take_unsaved();
    for i in 0..3 {
        let pts: Vec<(f64, f64)> = (0..150)
            .map(|k| (k as f64 * 0.7 + i as f64, (k as f64 * 0.1).sin() * 20.0))
            .collect();
        p.apply(&Op::add(p.page(), vec![stroke(&format!("s{i}"), &pts)]));
        let u = p.take_unsaved().unwrap();
        let ink = inkcodec::encode(&match &p.page().items[i].content {
            Content::Stroke(s) => s.points.clone(),
            _ => unreachable!(),
        })
        .len();
        eprintln!("stroke {i}: update {} bytes, ink blob {ink} bytes", u.len());
    }
}

#[test]
fn sync_cursor_sends_local_changes_once_and_never_echoes_remote_ones() {
    let mut a = CrdtPage::new("p", 1);
    let mut relay_a = VersionVector::default();
    let init = a.sync_outgoing(&mut relay_a).unwrap();
    assert!(a.sync_outgoing(&mut relay_a).is_none());
    let mut b = CrdtPage::load(&[&init], 2).unwrap();
    let mut relay_b = b.version();
    b.apply(&Op::add(b.page(), vec![stroke("b", &[(0.0, 0.0), (5.0, 5.0)])]));
    let from_b = b.sync_outgoing(&mut relay_b).unwrap();
    a.sync_incoming(&from_b, &mut relay_a).unwrap();
    assert!(a.sync_outgoing(&mut relay_a).is_none(), "remote change not echoed back");
    a.apply(&Op::add(a.page(), vec![stroke("a", &[(9.0, 9.0), (1.0, 1.0)])]));
    let from_a = a.sync_outgoing(&mut relay_a).unwrap();
    assert!(from_a.len() < from_b.len() + 400, "only the new stroke is sent");
    b.sync_incoming(&from_a, &mut relay_b).unwrap();
    assert_eq!(a.page(), b.page());
    // Offline edits from an earlier session (another peer id) are still uploaded.
    let mut a2 = CrdtPage::load(&[&a.snapshot()], 3).unwrap();
    let mut stale = relay_a.clone();
    a.apply(&Op::add(a.page(), vec![stroke("offline", &[(2.0, 2.0), (3.0, 3.0)])]));
    a2.import(&a.snapshot()).unwrap();
    assert!(a2.sync_outgoing(&mut stale).is_some());
}

#[test]
#[ignore]
fn profile_remote_import() {
    let a0 = CrdtPage::new("p", 1);
    let mut b = CrdtPage::load(&[&a0.snapshot()], 2).unwrap();
    let mut a = CrdtPage::load(&[&a0.snapshot()], 3).unwrap();
    let pts: Vec<(f64, f64)> = (0..150).map(|k| (k as f64, (k as f64 * 0.1).sin() * 9.0)).collect();
    for i in 0..3000 {
        b.apply(&Op::Add(vec![(usize::MAX, stroke(&format!("b{i}"), &pts))]));
    }
    a.import(&b.snapshot()).unwrap();
    let mut t_doc = 0.0;
    let mut t_all = 0.0;
    for i in 0..200 {
        b.apply(&Op::Add(vec![(usize::MAX, stroke(&format!("n{i}"), &pts))]));
        let u = b.updates_since(&a.version());
        let t = std::time::Instant::now();
        a.doc.import(&u).unwrap();
        t_doc += t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        a.rebuild();
        t_all += t.elapsed().as_secs_f64();
    }
    eprintln!(
        "per import: loro import {:.3} ms, rebuild {:.3} ms",
        t_doc * 5.0,
        t_all * 5.0
    );
}

#[test]
fn assets_are_stored_once_and_sync() {
    let mut a = CrdtPage::new("p", 1);
    a.put_asset("abc", b"png-bytes");
    a.put_asset("abc", b"ignored");
    a.set_template("grid"); // the next change commits the asset with it
    assert_eq!(a.asset("abc").as_deref(), Some(&b"png-bytes"[..]));
    assert_eq!(a.asset_hashes(), vec!["abc".to_string()]);
    let b = CrdtPage::load(&[&a.snapshot()], 2).unwrap();
    assert_eq!(b.asset("abc").as_deref(), Some(&b"png-bytes"[..]));
    assert!(b.asset("missing").is_none());
}

#[test]
fn background_is_set_cleared_and_synced() {
    let mut a = CrdtPage::new("p", 1);
    assert!(a.background().is_none());
    a.set_background(Some(("h1", b"jpeg")));
    assert_eq!(a.background().as_deref(), Some("h1"));
    assert_eq!(a.asset("h1").as_deref(), Some(&b"jpeg"[..]));
    let b = CrdtPage::load(&[&a.snapshot()], 2).unwrap();
    assert_eq!(b.background().as_deref(), Some("h1"));
    assert!(a.undo());
    assert!(a.background().is_none(), "setting a background is one undo step");
    a.set_background(None);
    assert!(a.background().is_none());
}

#[test]
fn reorder_brings_to_front_and_sends_to_back_in_one_undo_step() {
    let mut p = CrdtPage::new("p", 1);
    let items: Vec<Item> = ["a", "b", "c", "d"]
        .iter()
        .map(|id| stroke(id, &[(0.0, 0.0), (5.0, 5.0)]))
        .collect();
    p.apply(&Op::add(p.page(), items));
    assert!(p.reorder(&["a", "c"], true));
    assert_eq!(ids(&p), ["b", "d", "a", "c"]);
    assert!(!p.reorder(&["a", "c"], true), "already on top");
    assert!(p.reorder(&["c"], false));
    assert_eq!(ids(&p), ["c", "b", "d", "a"]);
    assert!(!p.reorder(&["zz"], false));
    assert!(p.undo());
    assert_eq!(ids(&p), ["b", "d", "a", "c"]);
    // Another replica converges.
    let mut q = CrdtPage::load(&[&p.snapshot()], 2).unwrap();
    assert_eq!(ids(&q), ids(&p));
    q.reorder(&["b"], true);
    p.import(&q.updates_since(&p.version())).unwrap();
    assert_eq!(ids(&p), ["d", "a", "c", "b"]);
}

#[test]
fn revert_to_an_earlier_version_is_one_new_undoable_change() {
    let mut p = CrdtPage::new("p", 1);
    p.apply(&Op::add(
        p.page(),
        vec![stroke("a", &[(0.0, 0.0), (5.0, 5.0)]), text("t", "hello")],
    ));
    let then = p.frontiers();
    let before = p.page().items.clone();
    // Later: move the stroke, edit the text, delete... then add something.
    let moved = p.page().items[0].translated(10.0, 0.0);
    p.apply(&Op::update(p.page(), vec![moved]));
    p.apply(&Op::remove(p.page(), &["t"]));
    p.apply(&Op::add(p.page(), vec![stroke("b", &[(1.0, 1.0), (2.0, 2.0)])]));
    assert_eq!(p.page_at(&then).unwrap().items, before, "preview of the old version");
    let after = p.page().items.clone();

    assert!(p.revert_to(&then).unwrap());
    assert_eq!(p.page().items, before);
    assert!(!p.revert_to(&then).unwrap(), "already like that");
    assert!(p.undo(), "one undo step");
    assert_eq!(p.page().items, after);
    assert!(p.redo());
    assert_eq!(p.page().items, before);
    // It is an ordinary change: another replica converges to it.
    let q = CrdtPage::load(&[&p.snapshot()], 2).unwrap();
    assert_eq!(q.page().items, before);
}
