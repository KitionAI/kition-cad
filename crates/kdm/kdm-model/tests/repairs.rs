//! What the repair after a merge, an undo or a redo (`KitModel::validate`, `repair.rs`) fixes, each
//! state built through an entry that still produces it. Most arise from one person deleting
//! something another object refers to:
//! - a draft merge (`merge_changes`): a draft made at a version while the drawing was edited
//!   meanwhile (data model 6.6, D1–D9). An entity the draft draws on a layer deleted meanwhile, a
//!   group or a dimension the draft makes of an entity erased meanwhile, and two entities drawn by
//!   two actors of one handle slot all merge, and the repair fixes what they leave;
//! - an undo of the person under an agent's later step: the agent's step refers to what the
//!   person's step made (a line it constrains, a layout it draws in, a table it adds a row to),
//!   and the person's Undo deletes it. A draft cannot do these: it would write the deleted object
//!   (a constrained entity's `constraintState`, a layout's entity list, a table's grid), which
//!   refuses the merge (D8).

mod common;

use common::{Session, actor, assert_no_dangling, assert_replica_clean, circle, drawing, edit, first, line, line_a, line_b, on, the_circle};
use kition_cad_doc::{
    AssocSnap, Constraint, ConstraintKind, DimAssoc, Dimension, Drawing, EntityKind, GeomRef, Group, Handle, Layout, Space, Table, TableCell,
};
use kition_kit::kdm::{ObjectId, Origin, UndoScope, Version};
use kition_kit::session::{KitError, MemoryBlobs};
use serde_json::{Value, json};

fn agent() -> Origin {
    Origin::agent("agent:test")
}

/// Edits `session`'s view with `f` and records what changed as one transaction of `origin`.
fn edit_as(session: &mut Session, origin: &Origin, f: impl FnOnce(&mut Drawing)) {
    let before = session.model().drawing.clone();
    let mut after = before.clone();
    f(&mut after);
    let mut ids = session.model().ids().clone();
    session
        .record(origin, |tx| kdm_model::write_changes(tx, &before, &after, &mut ids).map(|_| ()).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    session.model_mut().adopt(after, ids);
}

/// A draft of `of`: a replica forked from it at its version, of the actor `value`.
fn draft(of: &Session, value: u64) -> Session {
    of.fork(actor(value), Box::new(MemoryBlobs::default())).unwrap_or_else(|error| panic!("fork: {error}"))
}

/// Merges `draft`'s changes since `base` into `into`, as an agent's draft the person accepts.
fn merge(into: &mut Session, draft: &Session, base: &Version) {
    let changes = draft.export_changes(base).unwrap_or_else(|error| panic!("export: {error}"));
    into.merge_changes(&changes, &agent()).unwrap_or_else(|error| panic!("merge: {error}"));
}

/// Erases `h` from model space as the engine's ERASE does: the groups, constraints and dimension
/// associations that name it go with it.
fn erase(d: &mut Drawing, h: Handle) {
    let _ = d.model.remove(h);
    for g in &mut d.groups {
        g.members.retain(|m| *m != h);
    }
    d.groups.retain(|g| !g.members.is_empty());
    d.constraints.retain(|c| c.refs.iter().all(|r| r.handle != h));
    let dims: Vec<Handle> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Dimension(_))).map(|e| e.handle).collect();
    for dim in dims {
        let _ = d.model.modify(dim, |e| {
            if let EntityKind::Dimension(x) = &mut e.kind {
                x.assoc.retain(|a| a.handle != h && !matches!(a.snap, AssocSnap::Intersection { other } if other == h));
            }
        });
    }
}

/// The dimension of [`drawing`], associated with `h`'s start alone.
fn dimension_on(d: &Drawing, h: Handle) -> EntityKind {
    let mut dim: Dimension = d
        .model
        .iter()
        .find_map(|e| if let EntityKind::Dimension(x) = &e.kind { Some(x.clone()) } else { None })
        .unwrap_or_else(|| panic!("a dimension"));
    dim.assoc = vec![DimAssoc { point: "p13".into(), handle: h, snap: AssocSnap::Start }];
    EntityKind::Dimension(dim)
}

/// The dimension `h` has no association in the view, nor in its replica object.
fn assert_unassociated(s: &Session, h: Handle) {
    let d = &s.model().drawing;
    assert!(matches!(d.entity(h).map(|e| &e.kind), Some(EntityKind::Dimension(x)) if x.assoc.is_empty()), "the dimension stays, unassociated");
    let object = s.model().ids().entity(h).cloned().unwrap_or_else(|| panic!("the dimension's object"));
    let kind = s.replica().get(&object, "kind").unwrap_or_default();
    assert!(kind.get("assoc").and_then(Value::as_array).is_none_or(Vec::is_empty), "the replica holds no association: {kind}");
}

fn assert_sound(s: &Session) {
    assert_replica_clean(s.replica());
    assert_no_dangling(&s.model().drawing);
}

#[test]
fn an_entity_a_draft_draws_on_a_layer_deleted_meanwhile_moves_to_layer_0() {
    let mut a = first(&drawing());
    let base = a.version();
    let mut b = draft(&a, 2);
    let mut made = None;
    edit(&mut b, |d| made = Some(d.add(&Space::Model, on("Walls"), line((0.0, 10.0), (50.0, 10.0))).unwrap_or_else(|e| panic!("{e}"))));
    edit(&mut a, |d| d.layers.retain(|l| l.name != "Walls"));
    merge(&mut a, &b, &base);
    let d = &a.model().drawing;
    assert!(d.layer("Walls").is_none(), "the layer stays deleted");
    let made = made.unwrap_or_else(|| panic!("line"));
    assert_eq!(d.entity(made).map(|e| e.common.layer.as_str()), Some("0"), "the draft's line is re-homed to layer 0");
    assert_sound(&a);
}

#[test]
fn a_draft_group_of_an_entity_erased_meanwhile_keeps_the_rest() {
    let mut a = first(&drawing());
    let base = a.version();
    let mut b = draft(&a, 2);
    let (la, lb, c) = (line_a(&a.model().drawing), line_b(&a.model().drawing), the_circle(&a.model().drawing));
    edit(&mut b, |d| {
        d.groups[0].description = "edited".into();
        d.groups.push(Group { name: "Round".into(), description: String::new(), selectable: true, members: vec![la, c] });
    });
    edit(&mut a, |d| erase(d, la));
    merge(&mut a, &b, &base);
    let d = &a.model().drawing;
    let group = |name: &str| d.groups.iter().find(|g| g.name == name).cloned().unwrap_or_else(|| panic!("group {name}"));
    assert_eq!(group("Pair").description, "edited", "the draft's edit of the group is kept");
    assert_eq!(group("Pair").members, vec![lb], "the erased line left the group here");
    assert_eq!(group("Round").members, vec![c], "the erased line leaves the draft's group, the circle stays");
    let replica = a.replica();
    let round = replica
        .objects_of_type("Group")
        .into_iter()
        .find(|o| replica.get(o, "name").ok().as_ref().and_then(Value::as_str) == Some("Round"))
        .unwrap_or_else(|| panic!("the group's object"));
    let members = replica.map_entries(&round, "data").unwrap_or_default().into_iter().find(|(key, _)| key == "members").map(|(_, v)| v);
    let circle = a.model().ids().entity(c).cloned().unwrap_or_else(|| panic!("the circle's object"));
    assert_eq!(members, Some(json!([circle.as_str()])), "the replica's group names the circle alone");
    assert_sound(&a);
}

#[test]
fn a_draft_dimension_on_an_entity_erased_meanwhile_loses_that_association() {
    let mut a = first(&drawing());
    let base = a.version();
    let mut b = draft(&a, 2);
    let lb = line_b(&a.model().drawing);
    let mut made = None;
    edit(&mut b, |d| {
        let kind = dimension_on(d, lb);
        made = Some(d.add(&Space::Model, on("0"), kind).unwrap_or_else(|e| panic!("{e}")));
    });
    edit(&mut a, |d| erase(d, lb));
    merge(&mut a, &b, &base);
    assert!(a.model().drawing.entity(lb).is_none(), "the line stays erased");
    assert_unassociated(&a, made.unwrap_or_else(|| panic!("dimension")));
    assert_sound(&a);
}

/// Two actors whose handle slots are one (`actor % 2^20`) mint the same handle: merged, the
/// entity drawn here keeps it, the draft's gets the next one above every handle in use.
#[test]
fn entities_a_draft_of_the_same_handle_slot_draws_get_their_own_handles() {
    let mut a = first(&drawing());
    let base = a.version();
    let mut b = draft(&a, 1 + (1 << 20));
    let (mut mine, mut theirs) = (None, None);
    edit(&mut a, |d| mine = Some(d.add(&Space::Model, on("0"), line((1.0, 1.0), (2.0, 2.0))).unwrap_or_else(|e| panic!("{e}"))));
    edit(&mut b, |d| theirs = Some(d.add(&Space::Model, on("0"), line((3.0, 3.0), (4.0, 4.0))).unwrap_or_else(|e| panic!("{e}"))));
    let (mine, theirs) = (mine.unwrap_or_else(|| panic!("line")), theirs.unwrap_or_else(|| panic!("line")));
    assert_eq!(mine, theirs, "two actors of one handle slot mint the same handle");
    merge(&mut a, &b, &base);
    let d = &a.model().drawing;
    let at = |x: f64| d.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == x)).map(|e| e.handle);
    assert_eq!(at(1.0), Some(mine), "the line drawn here keeps its handle");
    assert_eq!(at(3.0), Some(Handle(mine.0 + 1)), "the draft's line gets the next handle above every one in use");
    assert_sound(&a);
}

/// A draft cannot draw in a layout deleted meanwhile: it writes the layout's entity list, so the
/// merge is refused (D8) and changes nothing. An undo can (the next test).
#[test]
fn a_draft_drawing_in_a_layout_deleted_meanwhile_is_refused() {
    let mut a = first(&drawing());
    let base = a.version();
    let mut b = draft(&a, 2);
    edit(&mut b, |d| {
        d.add(&Space::Paper("Layout1".into()), on("0"), circle((5.0, 5.0), 1.0)).unwrap_or_else(|e| panic!("{e}"));
    });
    edit(&mut a, |d| d.layouts.retain(|l| l.name != "Layout1"));
    let changes = b.export_changes(&base).unwrap_or_else(|error| panic!("export: {error}"));
    let at = a.version();
    assert!(a.merge_changes(&changes, &agent()).is_err(), "the merge is refused");
    assert_eq!(a.version(), at, "and changes nothing");
}

#[test]
fn an_entity_an_agent_drew_in_a_layout_whose_making_is_undone_goes_with_it() {
    let mut a = first(&drawing());
    edit(&mut a, |d| d.layouts.push(Layout::new("Sheet2", 2)));
    let mut made = None;
    edit_as(&mut a, &agent(), |d| {
        made = Some(d.add(&Space::Paper("Sheet2".into()), on("0"), circle((5.0, 5.0), 1.0)).unwrap_or_else(|e| panic!("{e}")));
    });
    let made = made.unwrap_or_else(|| panic!("circle"));
    let (undone, _) = a.undo_in(UndoScope::Person).unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    let d = &a.model().drawing;
    assert!(d.layout("Sheet2").is_none(), "the person's undo takes the layout back");
    assert!(d.entity(made).is_none(), "the agent's circle in it goes with it");
    assert!(d.layout("Layout1").is_some(), "the other layout stays");
    assert_sound(&a);
}

#[test]
fn a_constraint_and_a_dimension_an_agent_put_on_a_line_whose_drawing_is_undone_go_with_it() {
    let mut a = first(&drawing());
    let la = line_a(&a.model().drawing);
    let mut l = None;
    edit(&mut a, |d| l = Some(d.add(&Space::Model, on("0"), line((0.0, 300.0), (40.0, 300.0))).unwrap_or_else(|e| panic!("{e}"))));
    let l = l.unwrap_or_else(|| panic!("line"));
    let mut made = None;
    edit_as(&mut a, &agent(), |d| {
        let next = d.constraints.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        d.constraints.push(Constraint {
            id: next,
            kind: ConstraintKind::Fix,
            refs: vec![GeomRef::whole(l)],
            name: String::new(),
            expr: String::new(),
        });
        let kind = dimension_on(d, l);
        made = Some(d.add(&Space::Model, on("0"), kind).unwrap_or_else(|e| panic!("{e}")));
    });
    assert_eq!(a.model().drawing.constraints.len(), 2);
    let (undone, _) = a.undo_in(UndoScope::Person).unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    let d = &a.model().drawing;
    assert!(d.entity(l).is_none(), "the person's undo takes the line back");
    assert!(d.constraints.iter().all(|c| c.refs.iter().all(|r| r.handle != l)), "the constraint on the line is gone");
    assert_eq!(d.constraints.len(), 1, "the other constraint is kept");
    assert!(matches!(d.constraints[0].kind, ConstraintKind::Horizontal) && d.constraints[0].refs[0].handle == la);
    assert_eq!(a.replica().objects_of_type("Constraint").len(), 1, "the replica holds the other constraint alone");
    assert_unassociated(&a, made.unwrap_or_else(|| panic!("dimension")));
    assert_sound(&a);
}

/// A two-by-two table (texts `a`, `b`, `c`, `d`, row by row).
fn table() -> EntityKind {
    let cell = |text: &str| TableCell { id: 0, text: text.into(), merged: None };
    let mut t = Table {
        cell_seed: 0,
        insert: kition_cad_doc::geom::Vec3::new(300.0, 0.0, 0.0),
        col_widths: vec![20.0, 20.0],
        row_heights: vec![5.0, 5.0],
        cells: vec![vec![cell("a"), cell("b")], vec![cell("c"), cell("d")]],
        style: "Standard".into(),
        text_height: 2.5,
        title: false,
        header: false,
    };
    t.identify_cells().unwrap_or_else(|e| panic!("{e}"));
    EntityKind::Table(t)
}

#[test]
fn cells_an_agent_added_to_a_table_whose_drawing_is_undone_go_with_it() {
    let mut a = first(&drawing());
    let mut h = None;
    edit(&mut a, |d| h = Some(d.add(&Space::Model, on("0"), table()).unwrap_or_else(|e| panic!("{e}"))));
    let h = h.unwrap_or_else(|| panic!("table"));
    edit_as(&mut a, &agent(), |d| {
        assert!(d.model.modify(h, |e| {
            if let EntityKind::Table(t) = &mut e.kind {
                t.row_heights.push(5.0);
                t.cells.push(vec![TableCell { id: 0, text: "e".into(), merged: None }, TableCell { id: 0, text: "f".into(), merged: None }]);
                t.identify_cells().unwrap_or_else(|e| panic!("{e}"));
            }
        }));
    });
    assert_eq!(a.replica().objects_of_type("TableCell").len(), 6, "one object per cell");
    let (undone, _) = a.undo_in(UndoScope::Person).unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    assert!(a.model().drawing.entity(h).is_none(), "the person's undo takes the table back");
    assert!(a.replica().objects_of_type("TableCell").is_empty(), "no cell outlives its table");
    assert!(a.replica().map_entries(&ObjectId::from_string("drawing"), "tableCells").unwrap_or_default().is_empty());
    assert_sound(&a);
}
