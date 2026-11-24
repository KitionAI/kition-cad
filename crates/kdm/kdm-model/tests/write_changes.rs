//! `write_changes` writes only what differs: moving one entity writes its `kind` register, a
//! draw order change moves one list entry, a property change writes `common`, a new layer is one
//! record and its map key, a renamed layer re-points its entities in the same transaction.

mod common;

use std::collections::BTreeSet;

use common::{drawing, line_a, move_entity, new_replica, person};
use kdm_model::{Table, write_changes};
use kition_cad_doc::{Drawing, EntityKind, Layer};
use kition_kit::kdm::{ObjectId, Replica};
use serde_json::Value;

fn root() -> ObjectId {
    ObjectId::from_string("drawing")
}

/// Writes `after` over `before` in one transaction; answers the objects it changed.
fn write(replica: &mut Replica, ids: &mut kdm_model::Ids, before: &Drawing, after: &Drawing) -> BTreeSet<ObjectId> {
    replica.take_changed();
    let wrote = replica.transact(&person("t"), |tx| write_changes(tx, before, after, ids)).unwrap_or_else(|error| panic!("write: {error}"));
    assert!(wrote);
    replica.take_changed()
}

#[test]
fn moving_one_entity_writes_its_kind_register_only() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let h = line_a(&before);
    let object = ids.entity(h).cloned().unwrap_or_else(|| panic!("entity object"));
    let untouched = |r: &Replica| -> Vec<Value> { ["handle", "common"].iter().map(|f| r.get(&object, f).unwrap_or(Value::Null)).collect() };
    let fields = untouched(&replica);
    let old_kind = replica.get(&object, "kind").unwrap_or_default();

    let mut after = before.clone();
    move_entity(&mut after, h, 10.0, 5.0);
    assert_eq!(write(&mut replica, &mut ids, &before, &after), BTreeSet::from([object.clone()]), "only the moved entity's object changed");
    assert_eq!(untouched(&replica), fields, "its other registers are as they were");
    assert_ne!(replica.get(&object, "kind").unwrap_or_default(), old_kind);

    // Nothing more to write.
    let wrote = replica.transact(&person("t"), |tx| write_changes(tx, &after, &after, &mut ids)).unwrap_or_else(|error| panic!("write: {error}"));
    assert!(!wrote);
}

#[test]
fn a_draw_order_change_is_one_list_move() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let order = replica.list(&root(), "model").unwrap_or_default();
    let h = line_a(&before);
    let mut after = before.clone();
    assert!(after.model.bring_to_front(h));
    assert_eq!(write(&mut replica, &mut ids, &before, &after), BTreeSet::from([root()]), "only the model space list changed");
    let mut expected = order.clone();
    let moved = expected.remove(0);
    expected.push(moved);
    assert_eq!(replica.list(&root(), "model").unwrap_or_default(), expected);
    // That it took exactly one `list_move` is `list::tests` (the list sync counts its operations).
}

#[test]
fn a_colour_change_writes_common_only() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let h = line_a(&before);
    let object = ids.entity(h).cloned().unwrap_or_else(|| panic!("entity object"));
    let kind = replica.get(&object, "kind").unwrap_or_default();
    let mut after = before.clone();
    after.model.modify(h, |e| e.common.color = kition_cad_doc::color::Color::Index(1));
    assert_eq!(write(&mut replica, &mut ids, &before, &after), BTreeSet::from([object.clone()]));
    assert_eq!(replica.get(&object, "kind").unwrap_or_default(), kind, "the geometry is untouched");
}

#[test]
fn a_new_layer_is_one_record_keyed_by_its_name() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let mut after = before.clone();
    after.layers.push(Layer::new("Furniture"));
    let changed = write(&mut replica, &mut ids, &before, &after);
    let object = ids.record(Table::Layers, "Furniture", 0).cloned().unwrap_or_else(|| panic!("record"));
    assert_eq!(changed, BTreeSet::from([root(), object.clone()]), "the new record, and the root's map entry");
    assert_eq!(replica.map_get(&root(), "layers", "furniture").unwrap_or_default(), Value::String(object.as_str().to_owned()));
}

#[test]
fn a_renamed_layer_re_points_its_entities_in_the_same_transaction() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let doors = ids.record(Table::Layers, "Doors", 0).cloned().unwrap_or_else(|| panic!("record"));
    let mut after = before.clone();
    for l in &mut after.layers {
        if l.name == "Doors" {
            l.name = "Openings".into();
        }
    }
    let on_doors: Vec<_> = after.model.iter().filter(|e| e.common.layer == "Doors").map(|e| e.handle).collect();
    assert!(!on_doors.is_empty());
    for h in &on_doors {
        after.model.modify(*h, |e| e.common.layer = "Openings".into());
    }
    let changed = write(&mut replica, &mut ids, &before, &after);
    assert!(!replica.exists(&doors), "the old record is deleted");
    assert_eq!(replica.map_get(&root(), "layers", "doors").unwrap_or_default(), Value::Null);
    let openings = ids.record(Table::Layers, "Openings", 0).cloned().unwrap_or_else(|| panic!("record"));
    assert!(changed.contains(&openings));
    for h in &on_doors {
        let object = ids.entity(*h).cloned().unwrap_or_else(|| panic!("entity"));
        assert!(changed.contains(&object), "entity {} re-pointed in the rename's transaction", h.hex());
        assert_eq!(replica.get(&object, "common").unwrap_or_default()["layer"], "Openings");
    }
    let (read, _) = kdm_model::read_document(&replica).unwrap_or_else(|error| panic!("read: {error}"));
    common::assert_same("renamed", &after, &read);
}

#[test]
fn a_header_change_writes_one_key() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let mut after = before.clone();
    after.header.set_f64("LTSCALE", 4.0);
    assert_eq!(write(&mut replica, &mut ids, &before, &after), BTreeSet::from([root()]));
    assert_eq!(replica.map_get(&root(), "header", "LTSCALE").unwrap_or_default()["r"].as_f64(), Some(4.0));
}

#[test]
fn the_current_layer_and_styles_are_never_written() {
    let mut before = drawing();
    before.header.set_str("TEXTSTYLE", "Standard");
    before.header.set_str("DIMSTYLE", "Standard");
    before.header.set_f64("DIMASZ", 0.18);
    before.header.set_i64("DIMDEC", 4);
    before.header.set_f64("FILLETRAD", 1.0);
    before.header.set_f64("TEXTSIZE", 0.2);
    before.header.set_f64("DIMSCALE", 1.0);
    let (mut replica, mut ids) = new_replica(&before);
    for var in kdm_model::CURRENT_VARS.iter().chain(&["DIMASZ", "DIMDEC"]) {
        assert!(replica.map_get(&root(), "header", var).unwrap_or_default().is_null(), "{var} is each person's own");
    }
    assert!(!replica.map_get(&root(), "header", "DIMSCALE").unwrap_or_default().is_null(), "DIMSCALE is the drawing's");
    let mut after = before.clone();
    after.header.set_str("CLAYER", "Walls");
    after.header.set_str("TEXTSTYLE", "Other");
    after.header.set_str("DIMSTYLE", "Other");
    after.header.set_f64("DIMASZ", 0.5);
    after.header.set_i64("DIMDEC", 2);
    after.header.set_f64("DIMTXT", 0.3);
    after.header.set_f64("FILLETRAD", 3.0);
    after.header.set_f64("TEXTSIZE", 0.5);
    after.header.set("SNAPBASE", kition_cad_doc::HVal::Point(kition_cad_doc::geom::Vec3::new(1.0, 1.0, 0.0)));
    replica.take_changed();
    let wrote = replica.transact(&person("t"), |tx| write_changes(tx, &before, &after, &mut ids)).unwrap_or_else(|error| panic!("write: {error}"));
    assert!(!wrote, "changing the current layer writes nothing");
    assert_eq!(replica.take_changed(), BTreeSet::new());
    let (read, _) = kdm_model::read_document(&replica).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(read.header.get("CLAYER"), None);
    assert_eq!(read.header.get("DIMSTYLE"), None, "the current dimension style is each person's own");
    for var in ["DIMASZ", "DIMDEC", "DIMTXT", "FILLETRAD", "TEXTSIZE", "SNAPBASE"] {
        assert_eq!(read.header.get(var), None, "{var} is each person's own");
    }
    assert_eq!(read.header.f64("DIMSCALE", 0.0), 1.0, "DIMSCALE is the drawing's, read back");
    assert_eq!(read.header.f64("LTSCALE", 0.0), 2.0, "the shared variables are read back");
}

#[test]
fn erasing_an_entity_deletes_its_object_and_a_new_one_gets_a_fresh_object() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let h = common::the_circle(&before);
    let object = ids.entity(h).cloned().unwrap_or_else(|| panic!("entity"));
    let mut after = before.clone();
    after.model.remove(h);
    let made = after.add(&kition_cad_doc::Space::Model, common::on("0"), common::line((5.0, 5.0), (6.0, 6.0))).unwrap_or_else(|e| panic!("{e}"));
    write(&mut replica, &mut ids, &before, &after);
    assert!(!replica.exists(&object));
    let fresh = ids.entity(made).cloned().unwrap_or_else(|| panic!("new entity"));
    assert!(replica.exists(&fresh));
    assert_ne!(fresh.as_str(), format!("h{:X}", made.0), "no entity's object id derives from its handle");
    assert!(matches!(after.model.get(made).map(|e| &e.kind), Some(EntityKind::Line(_))));
}

#[test]
fn moving_entities_into_a_block_keeps_their_objects() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let h = line_a(&before);
    let object = ids.entity(h).cloned().unwrap_or_else(|| panic!("entity"));
    let mut after = before.clone();
    let e = after.model.remove(h).unwrap_or_else(|| panic!("line"));
    let mut block = kition_cad_doc::Block::new("Edge");
    block.entities.push((*e).clone());
    after.blocks.insert("Edge".into(), std::sync::Arc::new(block));
    after.constraints.clear();
    write(&mut replica, &mut ids, &before, &after);
    assert!(replica.exists(&object), "the entity kept its object");
    let edge = ids.block("Edge").cloned().unwrap_or_else(|| panic!("block"));
    assert_eq!(replica.list(&edge, "entities").unwrap_or_default(), vec![object.clone()]);
    assert!(!replica.list(&root(), "model").unwrap_or_default().contains(&object));
}

#[test]
fn entities_written_with_the_file_get_minted_object_ids() {
    let d = drawing();
    let (first, first_ids) = new_replica(&d);
    let (second, second_ids) = new_replica(&d);
    for e in d.model.iter() {
        let a = first_ids.entity(e.handle).cloned().unwrap_or_else(|| panic!("entity {}", e.handle.hex()));
        let b = second_ids.entity(e.handle).cloned().unwrap_or_else(|| panic!("entity {}", e.handle.hex()));
        assert_ne!(a.as_str(), format!("h{:X}", e.handle.0), "the object id is not derived from the handle");
        assert_ne!(a, b, "two replicas built from one file name its entities differently");
        assert_eq!(first.get(&a, "handle").unwrap_or_default(), Value::String(e.handle.hex()), "the handle is stored");
        assert_eq!(second.get(&b, "handle").unwrap_or_default(), Value::String(e.handle.hex()));
    }
}

#[test]
fn issued_record_identities_are_one_key_each() {
    let before = drawing();
    let (mut replica, mut ids) = new_replica(&before);
    let issued: BTreeSet<String> = replica.map_entries(&root(), "recordIdsUsed").unwrap_or_default().into_iter().map(|(k, _)| k).collect();
    assert!(issued.contains("layer:Walls"), "{issued:?}");
    let mut after = before.clone();
    after.layers.push(Layer::new("Furniture"));
    after.ensure_record_ids();
    write(&mut replica, &mut ids, &before, &after);
    assert_eq!(replica.map_get(&root(), "recordIdsUsed", "layer:Furniture").unwrap_or_default(), Value::Bool(true));
    let (read, _) = kdm_model::read_document(&replica).unwrap_or_else(|error| panic!("read: {error}"));
    assert!(read.record_ids_used.contains("layer:Furniture") && read.record_ids_used.contains("layer:Walls"));
}
