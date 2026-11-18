//! Two people make an unnamed group at once. The engine names both `*A1`, and both peers give
//! the record the identity `*A1`, so after the merge the replica holds two group records with
//! one stored identity. The next edit must keep each record on its own object: the drawing
//! stays exactly what its replica holds, and the peers still converge on two groups.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_engine::Session;
use kition_cad_engine::doc::Drawing;
use serde_json::{Value, json};

fn new_session() -> Session {
    let mut s = Session::new();
    for points in [[[0, 0], [10, 0]], [[0, 5], [8, 9]], [[20, 0], [30, 0]], [[20, 5], [28, 9]]] {
        s.execute("line", &json!({ "points": points })).unwrap();
    }
    s
}

fn peer(a: &mut Session) -> Session {
    let mut b = Session::empty();
    b.open_snapshot(&a.snapshot().unwrap()).unwrap();
    b
}

fn send(from: &mut Session, to: &mut Session) {
    let since = to.version().unwrap();
    let bytes = from.export_changes(&since).unwrap();
    to.import_changes(&bytes).unwrap();
}

fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version().unwrap() == b.version().unwrap() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

fn doc(s: &Session) -> &Drawing {
    s.doc().unwrap()
}

fn canonical(d: &Drawing) -> Value {
    kdm_model::canonical(d)
}

fn handles(s: &Session) -> Vec<String> {
    doc(s).model.iter().map(|e| e.handle.hex()).collect()
}

/// The engine's drawing is exactly what its replica holds.
fn assert_holds(s: &Session, what: &str) {
    let kdm = s.kdm().expect("an edit made the replica");
    let held = kdm_model::read_document(kdm.replica()).unwrap().0;
    assert_eq!(canonical(doc(s))["groups"], canonical(&held)["groups"], "{what}: the drawing's groups are not what its replica holds");
    assert!(canonical(doc(s)) == canonical(&held), "{what}: the drawing is not what its replica holds");
}

fn group_names(s: &Session) -> Vec<String> {
    doc(s).groups.iter().map(|g| g.name.clone()).collect()
}

#[test]
fn an_edit_after_two_concurrent_unnamed_groups_keeps_both_records() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let hs = handles(&a);
    a.execute("group", &json!({ "handles": [hs[0], hs[1]] })).unwrap();
    b.execute("group", &json!({ "handles": [hs[2], hs[3]] })).unwrap();
    sync(&mut a, &mut b);
    assert_eq!(group_names(&a), ["*A1", "*A1"], "both people's groups survive the merge");
    assert_eq!(canonical(doc(&a)), canonical(doc(&b)));
    for (name, s) in [("a", &mut a), ("b", &mut b)] {
        s.execute("move", &json!({ "handles": [hs[0]], "delta": [1, 1] })).unwrap();
        assert_holds(s, name);
        assert_eq!(group_names(s), ["*A1", "*A1"], "{name}: the edit neither adds nor drops a group");
    }
    sync(&mut a, &mut b);
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap(), "the replicas differ");
    assert_eq!(canonical(doc(&a)), canonical(doc(&b)), "the drawings differ");
    assert_holds(&a, "a after the exchange");
    assert_holds(&b, "b after the exchange");
    assert_eq!(group_names(&a), ["*A1", "*A1"]);
    let c = peer(&mut a);
    assert_eq!(canonical(doc(&a)), canonical(doc(&c)), "a fresh peer matches");
}
