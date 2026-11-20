use super::*;
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
