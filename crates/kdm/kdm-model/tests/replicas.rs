//! Two replicas through `KitSession<CadModel>`: concurrent edits of different things both
//! survive, and the repair after a merge clears references to what the other side deleted.

mod common;

use common::{
    Session, assert_no_dangling, assert_replica_clean, canonical, circle, drawing, edit, first, line, line_a, line_b, move_entity, on, peer, sync,
    the_circle,
};
use kition_cad_doc::{Constraint, ConstraintKind, EntityKind, GeomRef, Layer, Space};

fn pair() -> (Session, Session) {
    let a = first(&drawing());
    let b = peer(&a, 2);
    (a, b)
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_eq!(canonical(&a.model().drawing), canonical(&b.model().drawing), "same view");
    assert_no_dangling(&a.model().drawing);
    assert_replica_clean(a.replica());
}

fn x_of(d: &kition_cad_doc::Drawing, h: kition_cad_doc::Handle) -> f64 {
    match d.entity(h).map(|e| &e.kind) {
        Some(EntityKind::Line(l)) => l.a.x,
        Some(EntityKind::Circle(c)) => c.center.x,
        other => panic!("not a line or circle: {other:?}"),
    }
}

#[test]
fn different_entities_edited_concurrently_are_both_kept() {
    let (mut a, mut b) = pair();
    let (la, c) = (line_a(&a.model().drawing), the_circle(&a.model().drawing));
    edit(&mut a, |d| move_entity(d, la, 10.0, 0.0));
    edit(&mut b, |d| move_entity(d, c, -20.0, 0.0));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().drawing;
    assert_eq!(x_of(d, la), 10.0);
    assert_eq!(x_of(d, c), 180.0);
}

#[test]
fn a_colour_and_a_move_of_one_entity_are_both_kept() {
    let (mut a, mut b) = pair();
    let la = line_a(&a.model().drawing);
    edit(&mut a, |d| {
        d.model.modify(la, |e| e.common.color = kition_cad_doc::color::Color::Index(3));
    });
    edit(&mut b, |d| move_entity(d, la, 0.0, 7.0));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let e = a.model().drawing.entity(la).cloned().unwrap_or_else(|| panic!("line"));
    assert_eq!(e.common.color, kition_cad_doc::color::Color::Index(3));
    assert!(matches!(&e.kind, EntityKind::Line(l) if l.a.y == 7.0));
}

#[test]
fn entities_drawn_at_once_get_different_handles() {
    let (mut a, mut b) = pair();
    let mut made = Vec::new();
    edit(&mut a, |d| made.push(d.add(&Space::Model, on("0"), line((1.0, 1.0), (2.0, 2.0))).unwrap_or_else(|e| panic!("{e}"))));
    edit(&mut b, |d| made.push(d.add(&Space::Model, on("0"), line((3.0, 3.0), (4.0, 4.0))).unwrap_or_else(|e| panic!("{e}"))));
    assert_ne!(made[0], made[1], "each replica mints from its own range");
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for h in made {
        assert!(a.model().drawing.entity(h).is_some(), "{} is kept", h.hex());
    }
}

#[test]
fn an_entity_on_a_layer_deleted_elsewhere_moves_to_layer_0() {
    let (mut a, mut b) = pair();
    edit(&mut a, |d| d.layers.retain(|l| l.name != "Walls"));
    let mut made = None;
    edit(&mut b, |d| made = Some(d.add(&Space::Model, on("Walls"), line((0.0, 10.0), (50.0, 10.0))).unwrap_or_else(|e| panic!("{e}"))));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().drawing;
    assert!(d.layer("Walls").is_none(), "the layer stays deleted");
    let made = made.unwrap_or_else(|| panic!("line"));
    assert_eq!(d.entity(made).map(|e| e.common.layer.as_str()), Some("0"), "the new line is re-homed to layer 0");
}

#[test]
fn a_constraint_on_an_entity_deleted_elsewhere_is_removed() {
    let (mut a, mut b) = pair();
    let lb = line_b(&a.model().drawing);
    edit(&mut a, |d| {
        d.model.remove(lb);
        // As the engine does: what referenced it goes with it.
        d.groups.iter_mut().for_each(|g| g.members.retain(|h| *h != lb));
        d.constraints.retain(|c| c.refs.iter().all(|r| r.handle != lb));
    });
    edit(&mut b, |d| {
        let next = d.constraints.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        d.constraints.push(Constraint {
            id: next,
            kind: ConstraintKind::Fix,
            refs: vec![GeomRef::whole(lb)],
            name: String::new(),
            expr: String::new(),
        });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().drawing;
    assert!(d.entity(lb).is_none());
    assert!(d.constraints.iter().all(|c| c.refs.iter().all(|r| r.handle != lb)), "the constraint on the deleted line is gone");
    assert_eq!(d.constraints.len(), 1, "the other constraint is kept");
    // The dimension associated with the deleted line lost those associations.
    let dim = d.model.iter().find_map(|e| if let EntityKind::Dimension(dim) = &e.kind { Some(dim.clone()) } else { None });
    assert!(dim.is_some_and(|dim| dim.assoc.iter().all(|x| x.handle != lb)));
}

#[test]
fn the_same_layer_added_twice_is_one_layer() {
    let (mut a, mut b) = pair();
    edit(&mut a, |d| d.layers.push(Layer { color: kition_cad_doc::color::Color::Index(1), ..Layer::new("Furniture") }));
    edit(&mut b, |d| d.layers.push(Layer { color: kition_cad_doc::color::Color::Index(5), ..Layer::new("FURNITURE") }));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let n = a.model().drawing.layers.iter().filter(|l| l.name.eq_ignore_ascii_case("furniture")).count();
    assert_eq!(n, 1, "one record per name: the names meet on one key");
}

#[test]
fn a_draw_order_change_and_an_edit_of_the_moved_entity_are_both_kept() {
    let (mut a, mut b) = pair();
    let c = the_circle(&a.model().drawing);
    edit(&mut a, |d| {
        d.model.bring_to_front(c);
    });
    edit(&mut b, |d| {
        d.model.modify(c, |e| {
            if let EntityKind::Circle(x) = &mut e.kind {
                x.radius = 40.0;
            }
        });
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let d = &a.model().drawing;
    assert_eq!(d.model.last().map(|e| e.handle), Some(c), "on top");
    assert!(matches!(d.entity(c).map(|e| &e.kind), Some(EntityKind::Circle(x)) if x.radius == 40.0));
}

#[test]
fn an_entity_added_to_a_layout_deleted_elsewhere_goes_with_it() {
    let (mut a, mut b) = pair();
    edit(&mut a, |d| d.layouts.retain(|l| l.name != "Layout1"));
    edit(&mut b, |d| {
        d.add(&Space::Paper("Layout1".into()), on("0"), circle((5.0, 5.0), 1.0)).unwrap_or_else(|e| panic!("{e}"));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(a.model().drawing.layout("Layout1").is_none());
}

#[test]
fn a_group_member_deleted_elsewhere_leaves_the_group() {
    let (mut a, mut b) = pair();
    let la = line_a(&a.model().drawing);
    edit(&mut a, |d| {
        d.model.remove(la);
        d.constraints.retain(|c| c.refs.iter().all(|r| r.handle != la));
    });
    edit(&mut b, |d| d.groups[0].description = "edited".into());
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let g = &a.model().drawing.groups[0];
    assert_eq!(g.description, "edited");
    assert!(!g.members.contains(&la));
}

#[test]
fn undo_brings_back_a_deleted_entity_with_its_handle_and_constraint() {
    let (mut a, _b) = pair();
    let la = line_a(&a.model().drawing);
    let id = a.model().drawing.constraints[0].id;
    edit(&mut a, |d| {
        d.model.remove(la);
        d.constraints.clear();
    });
    let (undone, _) = a.undo().unwrap_or_else(|error| panic!("undo: {error}"));
    assert!(undone);
    let d = &a.model().drawing;
    assert!(d.entity(la).is_some(), "the line is back with its handle");
    assert_eq!(d.constraints.first().map(|c| c.id), Some(id), "the constraint keeps its local id");
    assert_no_dangling(d);
}
