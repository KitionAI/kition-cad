//! The command bridge to each drawing's KDM replica (Kition phase 3g): every mutating command
//! (or interactive command) replicates as one transaction that reproduces the drawing in a peer,
//! the post-command passes (constraint re-solve, associative dimensions) are part of it, undo
//! after a remote edit keeps the remote edit, an interactive command is one undo step,
//! selection-only changes write nothing, a remote deletion of a selected entity fixes the
//! selection, two people drawing at once never mint the same handle, and two sessions running
//! random commands converge with the engine's drawing equal to its own replica after every step.

use kition_cad_engine::doc::{Drawing, EntityKind, Handle};
use kition_cad_engine::{Origin, Session, Version};
use serde_json::{Value, json};

/// A session with a drawing: two lines, a circle, a layer `Walls`.
fn new_session() -> Session {
    let mut s = Session::new();
    s.execute("layer.new", &json!({"name": "Walls"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 5], [8, 9]]})).unwrap();
    s.execute("circle", &json!({"center": [20, 10], "radius": 3})).unwrap();
    s
}

/// A second session holding a replica of `a`'s active drawing.
fn peer(a: &mut Session) -> Session {
    let mut b = Session::empty();
    b.open_snapshot(&a.snapshot().unwrap()).unwrap();
    b
}

/// Sends `from`'s changes that `to` lacks.
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

/// `a` and `b` hold the same replicated state and the same drawing (ids aside).
fn assert_same(a: &mut Session, b: &mut Session, what: &str) {
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap(), "{what}: replicas differ");
    let (x, y) = (canonical(doc(a)), canonical(doc(b)));
    assert!(x == y, "{what}: drawings differ:\n{:#?}", differences(&x, &y));
}

/// The drawing as read back fresh from the session's replica, without writing anything.
fn replica_doc(s: &Session) -> Drawing {
    let kdm = s.kdm().expect("an edit made the replica");
    kdm_model::read_document(kdm.replica()).unwrap().0
}

/// The engine's drawing is exactly what its replica holds.
fn assert_holds(s: &Session, what: &str) {
    let (mine, held) = (canonical(doc(s)), canonical(&replica_doc(s)));
    assert!(mine == held, "{what}: the drawing is not what its replica holds:\n{:#?}", differences(&mine, &held));
}

/// The replica's own undo steps (one per undoable transaction).
fn replica_steps(s: &Session) -> usize {
    s.kdm().map_or(0, |k| k.steps().0)
}

fn last_steps(s: &Session) -> u32 {
    s.state().unwrap().undo.last().unwrap().steps
}

fn handles(r: &Value) -> Vec<String> {
    r["handles"].as_array().map(|a| a.iter().filter_map(|h| h.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn handle(r: &Value) -> String {
    r["handle"].as_str().map(str::to_owned).unwrap_or_else(|| handles(r).into_iter().next().unwrap_or_default())
}

/// The model-space handles in draw order.
fn model(s: &Session) -> Vec<String> {
    doc(s).model.iter().map(|e| e.handle.hex()).collect()
}

fn first_line(s: &Session) -> String {
    doc(s).model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle.hex()).unwrap()
}

#[test]
fn one_command_one_transaction_reproduces_in_a_peer() {
    let cases: Vec<(&str, fn(&mut Session))> = vec![
        ("draw a line", |s| {
            s.execute("line", &json!({"points": [[1, 1], [4, 6]]})).unwrap();
        }),
        ("move an entity", |s| {
            let h = first_line(s);
            s.execute("move", &json!({"handles": [h], "delta": [3, 2]})).unwrap();
        }),
        ("an interactive LINE", |s| {
            for t in ["line", "30,30", "40,30", "40,40", ""] {
                s.cmdline(t).unwrap();
            }
        }),
        ("erase", |s| {
            let h = first_line(s);
            s.execute("erase", &json!({"handles": [h]})).unwrap();
        }),
        ("rename a layer in use", |s| {
            s.execute("layer.current", &json!({"name": "Walls"})).unwrap();
            s.execute("line", &json!({"points": [[0, 20], [5, 20]]})).unwrap();
            s.execute("rename", &json!({"table": "layer", "from": "Walls", "to": "Partitions"})).unwrap();
        }),
        ("bring to front", |s| {
            let h = model(s)[0].clone();
            s.execute("draworder.front", &json!({"handles": [h]})).unwrap();
        }),
        ("a constraint, solved in the same transaction", |s| {
            let h = model(s)[1].clone();
            s.execute("geomconstraint", &json!({"type": "horizontal", "h1": h})).unwrap();
        }),
        ("an associative dimension follows its line", |s| {
            let h = first_line(s);
            s.execute("dimlinear", &json!({"object": h, "at": [5, -4]})).unwrap();
            s.execute("move", &json!({"handles": [h], "delta": [0, 2]})).unwrap();
        }),
        ("a block from model space objects", |s| {
            let hs = model(s);
            s.execute("block", &json!({"name": "Pair", "base": [0, 0], "handles": [hs[0], hs[1]]})).unwrap();
        }),
        ("a group", |s| {
            let hs = model(s);
            s.execute("group", &json!({"name": "G", "handles": [hs[0], hs[2]]})).unwrap();
        }),
        ("copy", |s| {
            let h = first_line(s);
            s.execute("copy", &json!({"handles": [h], "delta": [0, 30]})).unwrap();
        }),
    ];
    for (name, run) in cases {
        let mut a = new_session();
        let mut b = peer(&mut a);
        let steps = replica_steps(&a);
        let entries = a.state().unwrap().undo.len();
        run(&mut a);
        let commands = a.state().unwrap().undo.len() - entries;
        assert!(commands >= 1, "{name}: an undo step");
        assert_eq!(last_steps(&a), 1, "{name}: exactly one transaction for the last step");
        assert_eq!(replica_steps(&a) - steps, commands, "{name}: one transaction per step, nothing else");
        assert_holds(&a, name);
        send(&mut a, &mut b);
        assert_same(&mut a, &mut b, name);
    }
}

#[test]
fn an_interactive_command_is_one_undo_step() {
    let mut s = new_session();
    let before = canonical(doc(&s));
    let entries = s.state().unwrap().undo.len();
    for t in ["line", "30,30", "40,30", "40,40", "30,40", ""] {
        s.cmdline(t).unwrap();
    }
    assert_eq!(doc(&s).model.len(), 6, "three segments");
    assert_eq!(s.state().unwrap().undo.len(), entries + 1, "one history step");
    assert_eq!(last_steps(&s), 1, "one replica step");
    s.execute("u", &json!({})).unwrap();
    assert!(canonical(doc(&s)) == before, "one undo takes the whole command back");
    assert_holds(&s, "after the undo");
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(doc(&s).model.len(), 6);
}

#[test]
fn a_remote_edit_in_the_middle_of_a_command_keeps_both() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    for t in ["line", "30,30", "40,30"] {
        a.cmdline(t).unwrap();
    }
    let c = doc(&b).model.iter().find(|e| matches!(e.kind, EntityKind::Circle(_))).map(|e| e.handle.hex()).unwrap();
    b.execute("move", &json!({"handles": [c], "delta": [5, 0]})).unwrap();
    send(&mut b, &mut a);
    assert!(a.running.is_some(), "the command goes on");
    for t in ["40,40", ""] {
        a.cmdline(t).unwrap();
    }
    assert_holds(&a, "after the command");
    let circle_x = |s: &Session| doc(s).model.iter().find_map(|e| if let EntityKind::Circle(c) = &e.kind { Some(c.center.x) } else { None });
    assert_eq!(circle_x(&a), Some(25.0), "the remote move is in");
    a.execute("u", &json!({})).unwrap();
    assert_eq!(doc(&a).model.len(), 3, "the whole LINE undoes");
    assert_eq!(circle_x(&a), Some(25.0), "the remote edit stays");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
}

#[test]
fn undo_back_to_the_saved_drawing_is_clean() {
    let mut s = new_session();
    let st = s.state_mut().unwrap();
    st.saved = st.doc.clone();
    let h = first_line(&s);
    s.execute("move", &json!({"handles": [h], "delta": [1, 1]})).unwrap();
    assert!(s.state().unwrap().is_dirty());
    s.execute("u", &json!({})).unwrap();
    assert!(!s.state().unwrap().is_dirty(), "the replica's undo result is the saved drawing");
}

#[test]
fn undo_after_a_remote_edit_keeps_it() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let h = first_line(&a);
    a.execute("move", &json!({"handles": [h], "delta": [20, 0]})).unwrap();
    b.execute("line", &json!({"points": [[50, 50], [60, 60]]})).unwrap();
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
    assert_eq!(doc(&a).model.len(), 4);
    a.execute("u", &json!({})).unwrap();
    let x = doc(&a).entity(Handle::parse_hex(&h).unwrap()).map(|e| match &e.kind {
        EntityKind::Line(l) => l.a.x,
        _ => f64::NAN,
    });
    assert_eq!(x, Some(0.0), "the local move is undone");
    assert_eq!(doc(&a).model.len(), 4, "the remote line survives the local undo");
    assert_holds(&a, "after the undo");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after the undo");
}

#[test]
fn selection_only_changes_write_nothing() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let version = a.version().unwrap();
    let hs = model(&a);
    a.execute("select", &json!({"handles": hs})).unwrap();
    a.execute("selectall", &json!({})).unwrap();
    a.execute("zoom", &json!({"mode": "extents"})).unwrap();
    a.idle_click(kition_cad_engine::geom::Vec2::new(500.0, 500.0), false).unwrap();
    assert_eq!(a.version().unwrap(), version, "no transaction");
    assert!(b.selection().is_empty(), "the peer's own selection is untouched");
    send(&mut a, &mut b);
    assert_same(&mut a, &mut b, "nothing to send");
}

#[test]
fn a_remote_deletion_of_a_selected_entity_fixes_the_selection() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let hs = model(&a);
    a.execute("select", &json!({"handles": [hs[0], hs[2]]})).unwrap();
    b.execute("erase", &json!({"handles": [hs[0]]})).unwrap();
    send(&mut b, &mut a);
    let selected: Vec<String> = a.selection().iter().map(|h| h.hex()).collect();
    assert_eq!(selected, vec![hs[2].clone()], "the deleted entity is no longer selected");
    a.execute("line", &json!({"points": [[1, 2], [3, 4]]})).unwrap();
    assert_holds(&a, "an edit after the import");
}

#[test]
fn two_people_drawing_at_once_never_mint_the_same_handle() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    let x = handle(&a.execute("line", &json!({"points": [[0, 40], [5, 40]]})).unwrap());
    let y = handle(&b.execute("line", &json!({"points": [[0, 50], [5, 50]]})).unwrap());
    assert_ne!(x, y);
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "both lines");
    assert_eq!(doc(&a).model.len(), 5);
    let (start, end) = a.kdm().unwrap().handle_range();
    assert!((start..end).contains(&doc(&a).handseed), "the next handle is in a's own range");
}

#[test]
fn a_layer_deleted_while_someone_draws_on_it_rehomes_to_layer_0() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    a.execute("layer.delete", &json!({"name": "Walls"})).unwrap();
    b.execute("layer.current", &json!({"name": "Walls"})).unwrap();
    let h = handle(&b.execute("line", &json!({"points": [[0, 60], [5, 60]]})).unwrap());
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "merged");
    let e = doc(&a).entity(Handle::parse_hex(&h).unwrap()).cloned().unwrap();
    assert_eq!(e.common.layer, "0");
    assert_eq!(doc(&b).header.str("CLAYER", "?"), "0", "the current layer that is gone falls back to 0");
}

#[test]
fn agent_edits_record_their_origin_and_undo_on_this_replica() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("line", &json!({"points": [[0, 70], [5, 70]]}), &agent).unwrap();
    assert_eq!(s.origin(), kition_cad_engine::kdm_bridge::local_person(), "the origin is restored after the call");
    assert_eq!(doc(&s).model.len(), 4);
    s.execute("u", &json!({})).unwrap();
    assert_eq!(doc(&s).model.len(), 3);
}

#[test]
fn a_file_newer_than_its_stored_replica_wins_on_open() {
    let mut a = new_session();
    let snapshot = a.snapshot().unwrap();
    let doc_id = a.doc_id().unwrap();
    a.execute("line", &json!({"points": [[0, 80], [5, 80]]})).unwrap();
    let file = kition_cad_io::write(doc(&a), "Plan.dxf").unwrap();

    let mut b = Session::empty();
    b.open_with_replica(&file, &snapshot).unwrap();
    assert_eq!(doc(&b).model.len(), 4, "the file's content");
    assert_holds(&b, "recorded onto the replica");
    assert_eq!(b.doc_id().unwrap(), doc_id, "the replica's own identity");
    assert!(!b.state().unwrap().is_dirty(), "opening is clean");
    assert!(b.state().unwrap().undo.is_empty(), "opening is not an undo step");

    let mut c = Session::empty();
    c.open_with_replica(&[], &snapshot).unwrap();
    send(&mut b, &mut c);
    assert_same(&mut b, &mut c, "a peer of the older replica gets the file's content");
}

#[test]
fn a_file_saved_with_its_replica_reopens_as_that_replica() {
    let mut a = new_session();
    s_dim(&mut a);
    let file = kition_cad_io::write(doc(&a), "Plan.dxf").unwrap();
    let snapshot = a.snapshot().unwrap();
    let version = a.version().unwrap();
    let mut b = Session::empty();
    b.open_with_replica(&file, &snapshot).unwrap();
    assert_eq!(b.version().unwrap(), version, "nothing recorded: the replica holds what the file holds");
    assert!(!b.state().unwrap().is_dirty());
    let (start, end) = b.kdm().unwrap().handle_range();
    assert!((start..end).contains(&doc(&b).handseed), "b mints from its own range");
}

/// A dimension made into a block keeps its attachment to a model-space line; erasing that line
/// drops the attachment in the drawing as the replica (which deletes the line's object) does.
#[test]
fn erasing_the_object_of_a_dimension_inside_a_block_drops_its_attachment() {
    let mut a = new_session();
    let line = first_line(&a);
    let dim = handle(&a.execute("dimlinear", &json!({"object": line, "at": [5, -4]})).unwrap());
    a.execute("block", &json!({"name": "D", "base": [0, 0], "handles": [dim]})).unwrap();
    let in_block = |a: &Session| -> Vec<usize> {
        doc(a).blocks["D"].entities.iter().filter_map(|e| if let EntityKind::Dimension(d) = &e.kind { Some(d.assoc.len()) } else { None }).collect()
    };
    assert_eq!(in_block(&a), vec![2], "the block's dimension is still attached to the line");
    assert_holds(&a, "block made");
    a.execute("erase", &json!({"handles": [line]})).unwrap();
    assert_eq!(in_block(&a), vec![0], "the attachment to the erased line is dropped");
    assert_holds(&a, "line erased");
    let mut b = peer(&mut a);
    assert_same(&mut a, &mut b, "a peer");
}

/// Adds an associative dimension and a constraint, so a file holds more than plain geometry.
fn s_dim(s: &mut Session) {
    let h = first_line(s);
    s.execute("dimlinear", &json!({"object": h, "at": [5, -4]})).unwrap();
    s.execute("geomconstraint", &json!({"type": "horizontal", "h1": h})).unwrap();
}

#[test]
fn passthroughs_answer_before_any_edit() {
    let mut s = Session::new();
    assert!(s.doc_id().is_none(), "no replica before anything asks for one");
    let version = s.version().unwrap();
    assert!(s.doc_id().is_some());
    assert_eq!(s.kdm_resets(), 0);
    assert!(s.export_changes(&Version::empty()).unwrap().len() > 1);
    assert_eq!(s.version().unwrap(), version, "asking writes nothing");
    assert!(!s.state().unwrap().is_dirty(), "making the replica leaves a new drawing clean");
}

/// A tiny deterministic generator (no extra dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn coord(&mut self) -> i64 {
        self.below(100) as i64
    }
}

/// One random command: lines (programmatic and interactive), circles, moves, copies, erases,
/// draw order, layers, a rename, constraints, associative dimensions, groups, blocks, undo and redo.
fn random_command(s: &mut Session, rng: &mut Rng) -> String {
    let hs = model(s);
    let pick = |rng: &mut Rng| hs.get(rng.below(hs.len() as u64) as usize).cloned();
    let lines: Vec<String> = doc(s).model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle.hex()).collect();
    let (id, params): (&str, Value) = match rng.below(18) {
        0 | 1 => ("line", json!({"points": [[rng.coord(), rng.coord()], [rng.coord() + 101, rng.coord()]]})),
        2 => {
            let pts = [rng.coord(), rng.coord(), rng.coord() + 101, rng.coord()];
            for t in ["line".to_owned(), format!("{},{}", pts[0], pts[1]), format!("{},{}", pts[2], pts[3]), String::new()] {
                let _ = s.cmdline(&t);
            }
            return "interactive line".into();
        }
        3 => ("circle", json!({"center": [rng.coord(), rng.coord()], "radius": 1 + rng.below(9)})),
        4 | 5 => match pick(rng) {
            Some(h) => ("move", json!({"handles": [h], "delta": [rng.coord() - 50, rng.coord() - 50]})),
            None => ("line", json!({"points": [[0, 0], [1, 1]]})),
        },
        6 => match pick(rng) {
            Some(h) => ("copy", json!({"handles": [h], "delta": [rng.coord(), 0]})),
            None => ("circle", json!({"center": [0, 0], "radius": 1})),
        },
        7 => match pick(rng) {
            Some(h) => ("erase", json!({"handles": [h]})),
            None => ("circle", json!({"center": [3, 3], "radius": 1})),
        },
        8 => match pick(rng) {
            Some(h) => (if rng.below(2) == 0 { "draworder.front" } else { "draworder.back" }, json!({"handles": [h]})),
            None => ("line", json!({"points": [[2, 0], [3, 1]]})),
        },
        9 => ("layer.new", json!({"name": format!("L{}", rng.below(5)), "current": rng.below(2) == 0})),
        10 => ("layer.current", json!({"name": "0"})),
        11 => ("rename", json!({"table": "layer", "from": format!("L{}", rng.below(5)), "to": format!("R{}", rng.below(50))})),
        12 => match lines.get(rng.below(lines.len() as u64) as usize) {
            Some(h) => ("geomconstraint", json!({"type": if rng.below(2) == 0 { "horizontal" } else { "fix" }, "h1": h})),
            None => ("line", json!({"points": [[4, 0], [9, 1]]})),
        },
        13 => match lines.get(rng.below(lines.len() as u64) as usize) {
            Some(h) => ("dimlinear", json!({"object": h, "at": [rng.coord(), rng.coord() - 60]})),
            None => ("line", json!({"points": [[5, 0], [7, 1]]})),
        },
        14 => match (pick(rng), pick(rng)) {
            (Some(x), Some(y)) if x != y => ("group", json!({"handles": [x, y]})),
            _ => ("line", json!({"points": [[6, 0], [8, 2]]})),
        },
        15 => match pick(rng) {
            Some(h) => ("block", json!({"name": format!("B{}", rng.below(1000)), "base": [0, 0], "handles": [h]})),
            None => ("line", json!({"points": [[7, 0], [9, 3]]})),
        },
        16 => ("redo", json!({})),
        _ => ("u", json!({})),
    };
    let r = s.execute(id, &params);
    format!("{id} {params} -> {}", if r.is_ok() { "ok" } else { "refused" })
}

/// Two sessions run ~250 random commands between them, exchanging changes every few commands.
/// After every command the drawing is exactly what its own replica holds; after every exchange
/// both hold the same replicated state and the same drawing.
#[test]
fn random_commands_keep_drawing_and_replica_equal_and_converge() {
    for seed in [3u64, 11, 2026] {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut a = new_session();
        let mut b = peer(&mut a);
        let mut log: Vec<String> = Vec::new();
        for step in 0..63 {
            for (name, s) in [("a", &mut a), ("b", &mut b)] {
                let what = random_command(s, &mut rng);
                log.push(format!("{name}: {what}"));
                let context = || format!("seed {seed}, step {step}, {name}; last commands:\n{}", log[log.len().saturating_sub(6)..].join("\n"));
                assert_eq!(s.kdm_resets(), 0, "{}", context());
                let (mine, held) = (canonical(doc(s)), canonical(&replica_doc(s)));
                assert!(mine == held, "the drawing is not what its replica holds: {}\n{:#?}", context(), differences(&mine, &held));
            }
            if step % 4 == 3 {
                sync(&mut a, &mut b);
                assert_same(&mut a, &mut b, &format!("seed {seed}, step {step}"));
            }
        }
        sync(&mut a, &mut b);
        assert_same(&mut a, &mut b, &format!("seed {seed}, final"));
        let mut c = peer(&mut a);
        assert_same(&mut a, &mut c, &format!("seed {seed}: a fresh peer matches"));
    }
}

/// The first few paths at which two JSON values differ (for a readable failure).
fn differences(a: &Value, b: &Value) -> Vec<String> {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        if out.len() >= 5 || a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    walk(x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null), format!("{path}.{key}"), out);
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    walk(p, q, format!("{path}[{i}]"), out);
                }
            }
            _ => {
                let short = |v: &Value| v.to_string().chars().take(300).collect::<String>();
                out.push(format!("{path}: {} vs {}", short(a), short(b)));
            }
        }
    }
    let mut out = Vec::new();
    walk(a, b, String::new(), &mut out);
    out
}

#[test]
fn the_persons_undo_keeps_the_agents_step_and_the_agents_undo_takes_it_back() {
    // KitModel 1.1 M12a and engine rule E14a: the person's and the agents' undo histories are
    // separate; UNDO and REDO work in their `scope`, else in that of who runs them.
    let mut s = new_session();
    let agent = Origin::agent("test");
    let before = model(&s);
    s.execute("line", &json!({"points": [[50, 50], [60, 60]]})).unwrap();
    s.execute_as("circle", &json!({"center": [40, 40], "radius": 2}), &agent).unwrap();
    assert_eq!(model(&s).len(), before.len() + 2);
    s.execute("undo", &json!({})).unwrap();
    assert!(!has_line(&s), "the person's undo takes back the person's line");
    assert!(has_circle(&s), "and keeps the agent's later circle");
    s.execute("redo", &json!({})).unwrap();
    assert!(has_line(&s));
    s.execute_as("undo", &json!({}), &agent).unwrap();
    assert!(!has_circle(&s), "the agent's undo takes back the agent's circle");
    assert!(has_line(&s), "and keeps the person's line");
    s.execute("redo", &json!({"scope": "agent"})).unwrap();
    assert!(has_circle(&s), "the person can redo the agent's step by naming its scope");
    assert!(s.execute("undo", &json!({"scope": "everyone"})).is_err(), "an unknown scope is refused");
    assert_eq!(model(&s).len(), before.len() + 2);
}

#[test]
fn an_agent_undo_under_the_persons_later_step_keeps_the_persons_edit() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("circle", &json!({"center": [40, 40], "radius": 2}), &agent).unwrap();
    s.execute("line", &json!({"points": [[50, 50], [60, 60]]})).unwrap();
    let with_both = model(&s);
    s.execute_as("undo", &json!({}), &agent).unwrap();
    assert_eq!(model(&s).len(), with_both.len() - 1, "only the agent's circle goes");
    assert!(!has_circle(&s));
    assert!(has_line(&s), "the person's later line stays");
    s.execute("undo", &json!({})).unwrap();
    assert!(!has_line(&s), "then the person's undo takes back the line");
    s.execute_as("redo", &json!({}), &agent).unwrap();
    assert!(has_circle(&s), "and the agent's redo brings the circle back");
    assert!(!has_line(&s));
}

fn has_circle(s: &Session) -> bool {
    doc(s).model.iter().any(|e| matches!(&e.kind, EntityKind::Circle(c) if c.radius == 2.0))
}

fn has_line(s: &Session) -> bool {
    doc(s).model.iter().any(|e| matches!(&e.kind, EntityKind::Line(l) if l.b.x == 60.0))
}
