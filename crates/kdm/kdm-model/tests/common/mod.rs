//! Shared test helpers: a small drawing to edit, and a multi-replica harness over
//! `KitSession<CadModel>`. Each `tests/*.rs` binary uses only some.
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::Arc;

use kdm_model::{CadModel, Ids, write_changes, write_document};
use kition_cad_doc::geom::{Vec2, Vec3};
use kition_cad_doc::{
    AssocSnap, Block, Circle, Common, Constraint, ConstraintKind, DimAssoc, DimKind, Dimension, Drawing, Entity, EntityKind, GeomRef, Group, HVal,
    Handle, Insert, Layer, Line, MText, Space,
};
use kition_kit::kdm::{ActorId, DocId, ObjectId, Origin, Replica};
use kition_kit::session::{KitError, KitModel, KitSession, MemoryBlobs};
use serde_json::Value;

pub type Session = KitSession<CadModel>;

pub fn actor(value: u64) -> ActorId {
    ActorId::from_u64(value).unwrap_or_else(|_| panic!("actor {value}"))
}

pub fn person(name: &str) -> Origin {
    Origin::person(format!("user:{name}"))
}

pub fn line(a: (f64, f64), b: (f64, f64)) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })
}

pub fn circle(c: (f64, f64), r: f64) -> EntityKind {
    EntityKind::Circle(Circle { center: Vec3::new(c.0, c.1, 0.0), radius: r })
}

pub fn on(layer: &str) -> Common {
    Common { layer: layer.into(), ..Common::default() }
}

/// A metric drawing exercising every replicated part: layers (one empty, `Walls`), lines and a
/// circle in model space, a horizontal constraint on line A, an aligned dimension associated with
/// line B's ends, a group of A and B, a block `Door` with an insert of it, text, a circle in a
/// layout, and header variables of every type.
pub fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer::new("Walls"));
    d.layers.push(Layer::new("Doors"));
    let a = d.add(&Space::Model, on("0"), line((0.0, 0.0), (100.0, 0.0))).unwrap_or_else(|e| panic!("{e}"));
    let b = d.add(&Space::Model, on("Doors"), line((0.0, 50.0), (80.0, 90.0))).unwrap_or_else(|e| panic!("{e}"));
    d.add(&Space::Model, on("0"), circle((200.0, 100.0), 25.0)).unwrap_or_else(|e| panic!("{e}"));
    let dim = Dimension {
        kind: DimKind::Aligned,
        defpt: Vec3::new(80.0, 100.0, 0.0),
        text_mid: Vec3::new(40.0, 80.0, 0.0),
        p13: Vec3::new(0.0, 50.0, 0.0),
        p14: Vec3::new(80.0, 90.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "ISO-25".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: serde_json::Map::from_iter([("arrowSize".to_owned(), serde_json::json!(1.5))]),
        assoc: vec![
            DimAssoc { point: "p13".into(), handle: b, snap: AssocSnap::Start },
            DimAssoc { point: "p14".into(), handle: b, snap: AssocSnap::Intersection { other: a } },
        ],
    };
    d.add(&Space::Model, on("0"), EntityKind::Dimension(dim)).unwrap_or_else(|e| panic!("{e}"));
    let mut door = Block::new("Door");
    door.base = Vec3::new(0.0, 0.0, 0.0);
    let h1 = Handle(d.handseed);
    d.handseed += 1;
    door.entities.push(Entity::new(h1, line((0.0, 0.0), (0.0, 9.0))));
    let h2 = Handle(d.handseed);
    d.handseed += 1;
    door.entities.push(Entity::new(h2, EntityKind::Arc(kition_cad_doc::Arc { center: Vec3::ZERO, radius: 9.0, start: 0.0, end: 1.5 })));
    d.blocks.insert("Door".into(), Arc::new(door));
    let insert = Insert {
        block: "Door".into(),
        insert: Vec3::new(150.0, 20.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    d.add(&Space::Model, on("Doors"), EntityKind::Insert(insert)).unwrap_or_else(|e| panic!("{e}"));
    let text = MText {
        insert: Vec3::new(10.0, 200.0, 0.0),
        height: 2.5,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Plan".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
    };
    d.add(&Space::Model, on("0"), EntityKind::MText(text)).unwrap_or_else(|e| panic!("{e}"));
    d.add(&Space::Paper("Layout1".into()), on("0"), circle((50.0, 50.0), 10.0)).unwrap_or_else(|e| panic!("{e}"));
    d.constraints.push(Constraint {
        id: 1,
        kind: ConstraintKind::Horizontal,
        refs: vec![GeomRef::whole(a)],
        name: String::new(),
        expr: String::new(),
    });
    d.groups.push(Group { name: "Pair".into(), description: "two lines".into(), selectable: true, members: vec![a, b] });
    d.header.set_f64("LTSCALE", 2.0);
    d.header.set_i64("ORTHOMODE", 1);
    d.header.set_i64("PDMODE", 3);
    d.header.set("INSBASE", HVal::Point(Vec3::new(1.0, 2.0, 0.0)));
    d.header.set_str("CLAYER", "0");
    d
}

/// The handle of the first model-space entity matching `f`.
pub fn find(d: &Drawing, f: impl Fn(&Entity) -> bool) -> Handle {
    d.model.iter().find(|e| f(e)).map(|e| e.handle).unwrap_or_else(|| panic!("no such entity"))
}

/// Line A (horizontal, at y = 0).
pub fn line_a(d: &Drawing) -> Handle {
    find(d, |e| matches!(&e.kind, EntityKind::Line(l) if l.a.y == 0.0 && l.b.y == 0.0))
}

/// Line B (on layer Doors).
pub fn line_b(d: &Drawing) -> Handle {
    find(d, |e| matches!(e.kind, EntityKind::Line(_)) && e.common.layer == "Doors")
}

pub fn the_circle(d: &Drawing) -> Handle {
    find(d, |e| matches!(e.kind, EntityKind::Circle(_)))
}

/// Moves the entity `h` (in model space) by `(dx, dy)`.
pub fn move_entity(d: &mut Drawing, h: Handle, dx: f64, dy: f64) {
    let m = kition_cad_doc::geom::Mat3::translate(Vec2::new(dx, dy));
    assert!(d.model.modify(h, |e| e.kind.transform(&m)), "no entity {}", h.hex());
}

/// A fresh replica of `d` (its creating transaction, not an undo step) and the ids it minted.
pub fn new_replica(d: &Drawing) -> (Replica, Ids) {
    let mut ids = Ids::default();
    let (replica, ()) =
        Replica::create_with(CadModel::schema(), DocId::new(), actor(1), &Origin::system("create"), |tx| write_document(tx, d, &mut ids).map(|_| ()))
            .unwrap_or_else(|error| panic!("create: {error}"));
    (replica, ids)
}

pub fn first(d: &Drawing) -> Session {
    let (replica, _) = new_replica(d);
    KitSession::from_replica(replica, Box::new(MemoryBlobs::default())).unwrap_or_else(|error| panic!("attach: {error}"))
}

pub fn peer(of: &Session, value: u64) -> Session {
    let snapshot = of.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    KitSession::open_snapshot(&snapshot, Box::new(MemoryBlobs::default()), actor(value)).unwrap_or_else(|error| panic!("open_snapshot: {error}"))
}

/// Edits `session`'s view with `f` and records what changed, the way the engine bridge does.
pub fn edit(session: &mut Session, f: impl FnOnce(&mut Drawing)) {
    let before = session.model().drawing.clone();
    let mut after = before.clone();
    f(&mut after);
    let mut ids = session.model().ids().clone();
    session
        .record(&person("test"), |tx| write_changes(tx, &before, &after, &mut ids).map(|_| ()).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    session.model_mut().adopt(after, ids);
}

/// Sends `from`'s changes that `to` lacks.
pub fn send(from: &Session, to: &mut Session) {
    let bytes = from.export_changes(&to.version()).unwrap_or_else(|error| panic!("export: {error}"));
    to.import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
}

/// Exchanges until every session holds the same version (repairs are changes too).
pub fn sync_all(sessions: &mut [Session]) {
    for _ in 0..8 {
        for i in 0..sessions.len() {
            for j in 0..sessions.len() {
                if i != j {
                    let bytes = sessions[i].export_changes(&sessions[j].version()).unwrap_or_else(|error| panic!("export: {error}"));
                    sessions[j].import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
                }
            }
        }
        if sessions.windows(2).all(|w| w[0].version() == w[1].version()) {
            return;
        }
    }
    panic!("the replicas did not converge");
}

pub fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version() == b.version() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

/// The view as its id-blind JSON, for comparing views of different replicas.
pub fn canonical(d: &Drawing) -> Value {
    kdm_model::canonical(d)
}

/// The first few paths at which two JSON values differ (for a readable failure).
pub fn differences(a: &Value, b: &Value) -> Vec<String> {
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

pub fn assert_same(what: &str, a: &Drawing, b: &Drawing) {
    let (x, y) = (canonical(a), canonical(b));
    assert!(x == y, "{what}: the drawings differ:\n{:#?}", differences(&x, &y));
}

/// The replica itself holds nothing a merge left dangling: no list or map entry names a deleted
/// object, every entity is listed, no two listed entities share a handle, no constraint names a
/// deleted entity.
pub fn assert_replica_clean(replica: &Replica) {
    let root = ObjectId::from_string("drawing");
    let mut listed = HashSet::new();
    let mut handles = HashSet::new();
    let mut owners = vec![(root.clone(), "model")];
    for field in ["blocks", "layouts"] {
        for (key, value) in replica.map_entries(&root, field).unwrap_or_default() {
            let object = ObjectId::from_string(value.as_str().unwrap_or_default());
            assert!(replica.exists(&object), "{field}[{key}] names a deleted object");
            owners.push((object, "entities"));
        }
    }
    for table in kdm_model::Table::ALL {
        for (key, value) in replica.map_entries(&root, table.field()).unwrap_or_default() {
            assert!(replica.exists(&ObjectId::from_string(value.as_str().unwrap_or_default())), "{}[{key}] names a deleted object", table.field());
        }
    }
    for (owner, field) in owners {
        for item in replica.list(&owner, field).unwrap_or_default() {
            assert!(replica.exists(&item), "{owner}.{field} lists deleted {item}");
            if listed.insert(item.clone()) {
                let h = replica.get(&item, "handle").unwrap_or_default();
                assert!(handles.insert(h.as_str().unwrap_or_default().to_owned()), "handle {h} is held twice");
            }
        }
    }
    for entity in replica.objects_of_type("Entity") {
        assert!(listed.contains(&entity), "entity {entity} is listed nowhere");
    }
    for c in replica.list(&root, "constraints").unwrap_or_default() {
        assert!(replica.exists(&c), "the constraints list a deleted constraint");
        for r in replica.get(&c, "refs").unwrap_or_default().as_array().into_iter().flatten() {
            let h = r.get("handle").and_then(Value::as_str).unwrap_or_default();
            assert!(h.starts_with('#') || replica.exists(&ObjectId::from_string(h)), "constraint {c} names a deleted entity");
        }
    }
}

/// Every reference in `d` resolves: entity layers, block references, constraints, group members,
/// dimension associations.
pub fn assert_no_dangling(d: &Drawing) {
    let all: HashSet<Handle> = std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|s| s.iter().map(|e| e.handle))
        .collect();
    for store in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities)) {
        for e in store.iter() {
            assert!(d.layer(&e.common.layer).is_some(), "entity {} is on layer {}, which does not exist", e.handle.hex(), e.common.layer);
            match &e.kind {
                EntityKind::Insert(i) => assert!(d.block(&i.block).is_some(), "insert {} of missing block {}", e.handle.hex(), i.block),
                EntityKind::Dimension(dim) => {
                    for a in &dim.assoc {
                        assert!(all.contains(&a.handle), "dimension {} is associated with {}, which is gone", e.handle.hex(), a.handle.hex());
                    }
                }
                _ => {}
            }
        }
    }
    for c in &d.constraints {
        for r in &c.refs {
            assert!(all.contains(&r.handle), "constraint {} names {}, which is gone", c.id, r.handle.hex());
        }
    }
    for g in &d.groups {
        for h in &g.members {
            assert!(all.contains(h), "group {} lists {}, which is gone", g.name, h.hex());
        }
    }
}
