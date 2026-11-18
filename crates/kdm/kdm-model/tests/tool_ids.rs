//! Tool ids (KitObjectModel OM9, task card C4/C5/C6-cad): the object model names an entity by its
//! handle (the hex string inspect, commands and diagnostics use) and a layer, a block, a layout
//! and a named view by its name in its table (`layer:Walls`, `block:Door`, `layout:Layout1`,
//! `view:Front`), so receipts, queries, batches and commands speak one id space. Handles are the
//! file's own (DXF group 5) and names are unique within their table, so the ids survive a read,
//! a rewrite and a new replica made from the saved file; an entity made in a session is named by
//! the handle it will have in the file from the moment it exists.

mod common;

use std::collections::BTreeSet;

use common::{actor, drawing, new_replica, on, person};
use kdm_model::{CadModel, Ids, Table, decode, next_handle, write_changes};
use kition_cad_doc::geom::Vec2;
use kition_cad_doc::{Drawing, NamedView, Space};
use kition_kit::kdm::Replica;
use kition_kit::kdm::tool_ids::ToolIds;
use kition_kit::session::KitModel;

/// `drawing()` with a named view `Front`.
fn with_view() -> Drawing {
    let mut d = drawing();
    d.views.push(NamedView { name: "Front".into(), center: Vec2::new(50.0, 25.0), height: 60.0, width: 120.0, layer_state: None });
    d
}

/// Every tool id the object model renders for the entities, layers, blocks, layouts and views of
/// `d` (as `ids` maps them); an object without one fails.
fn named(replica: &Replica, ids: &Ids, d: &Drawing) -> BTreeSet<String> {
    let tools = ToolIds::of_replica(replica);
    let mut objects = Vec::new();
    for store in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities)) {
        objects.extend(store.iter().map(|e| ids.entity(e.handle).cloned().unwrap_or_else(|| panic!("no object for {}", e.handle.hex()))));
    }
    objects.extend(d.layers.iter().map(|l| ids.record(Table::Layers, &l.name, 0).cloned().unwrap_or_else(|| panic!("layer {}", l.name))));
    objects.extend(d.views.iter().map(|v| ids.record(Table::Views, &v.name, 0).cloned().unwrap_or_else(|| panic!("view {}", v.name))));
    objects.extend(d.blocks.keys().map(|b| ids.block(b).cloned().unwrap_or_else(|| panic!("block {b}"))));
    objects.extend(d.layouts.iter().map(|l| ids.layout(&l.name).cloned().unwrap_or_else(|| panic!("layout {}", l.name))));
    objects.iter().map(|object| tools.tool_id(object).unwrap_or_else(|| panic!("{object} has no tool id")).to_owned()).collect()
}

/// What `d` should be named by: its handles, and its layers, views, blocks and layouts by name.
fn expected(d: &Drawing) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for store in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities)) {
        out.extend(store.iter().map(|e| e.handle.hex()));
    }
    out.extend(d.layers.iter().map(|l| format!("layer:{}", l.name)));
    out.extend(d.views.iter().map(|v| format!("view:{}", v.name)));
    out.extend(d.blocks.keys().map(|b| format!("block:{b}")));
    out.extend(d.layouts.iter().map(|l| format!("layout:{}", l.name)));
    out
}

#[test]
fn entities_are_named_by_handle_and_table_records_by_name() {
    let d = with_view();
    assert!(d.layouts.iter().any(|l| !l.entities.is_empty()) && !d.blocks.is_empty(), "the fixture has layout and block entities");
    let (replica, ids) = new_replica(&d);
    let names = named(&replica, &ids, &d);
    assert_eq!(names, expected(&d));
    assert!(names.contains("layer:Walls") && names.contains("block:Door") && names.contains("layout:Layout1") && names.contains("view:Front"));
}

#[test]
fn an_entity_made_in_a_session_is_named_by_its_handle_and_keeps_it_in_the_file() {
    let before = with_view();
    let (mut replica, mut ids) = new_replica(&before);
    let mut after = before.clone();
    // The engine bridge mints a new entity's handle from the replica's range, as a command does.
    after.handseed = next_handle(actor(1), &after);
    let made = after.add(&Space::Model, on("Walls"), common::line((5.0, 5.0), (25.0, 5.0))).unwrap_or_else(|e| panic!("{e}"));
    replica.transact(&person("t"), |tx| write_changes(tx, &before, &after, &mut ids)).unwrap_or_else(|e| panic!("write: {e}"));
    let object = ids.entity(made).cloned().unwrap_or_else(|| panic!("no object for the new line"));
    assert_eq!(ToolIds::of_replica(&replica).tool_id(&object), Some(made.hex().as_str()), "named by its handle at once");
    // The saved file keeps the handle, and a replica made from it names the line the same.
    let bytes = kition_cad_io::write(&after, "drawing.dxf").unwrap_or_else(|e| panic!("{e}"));
    let reopened = decode("drawing.dxf", &bytes).unwrap_or_else(|e| panic!("{e}"));
    assert!(reopened.model.get(made).is_some(), "the saved file holds the line under handle {}", made.hex());
    let (again, again_ids) = new_replica(&reopened);
    let again_names = named(&again, &again_ids, &reopened);
    // saved-identity contract (6423c54c), orchestrator decision
    assert_eq!(again_names, expected(&reopened).into_iter().map(|id| if id == "block:*D1" { "block:dxf:10000103D".into() } else { id }).collect());
    // Every id the session's replica gave survives (the file adds the dimension's own block).
    let lost: Vec<String> = expected(&after).difference(&again_names).cloned().collect();
    assert!(lost.is_empty(), "ids lost in the saved file: {lost:?}");
}

#[test]
fn ids_survive_a_rewrite_and_a_rename_names_the_record_anew() {
    let before = with_view();
    let (mut replica, mut ids) = new_replica(&before);
    let mut after = before.clone();
    if let Some(layer) = after.layers.iter_mut().find(|l| l.name == "Doors") {
        layer.name = "Openings".into();
    }
    for e in after.model.iter().map(|e| e.handle).collect::<Vec<_>>() {
        after.model.modify(e, |e| {
            if e.common.layer == "Doors" {
                e.common.layer = "Openings".into();
            }
        });
    }
    replica.transact(&person("t"), |tx| write_changes(tx, &before, &after, &mut ids)).unwrap_or_else(|e| panic!("write: {e}"));
    let names = named(&replica, &ids, &after);
    assert_eq!(names, expected(&after));
    assert!(names.contains("layer:Openings") && !names.contains("layer:Doors"), "a renamed layer is named by its new name");
    // A view of the replica reads the same names.
    let model = CadModel::load(&replica, &kition_kit::session::MemoryBlobs::default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(named(&replica, model.ids(), &model.drawing), expected(&after));
}

#[test]
fn a_layer_a_batch_creates_without_its_name_field_is_named_by_the_repair() {
    let d = with_view();
    let (mut replica, _) = new_replica(&d);
    let made = kition_kit::kdm::ObjectId::from_string("batch-layer");
    replica
        .transact(&person("t"), |tx| {
            // As a model batch creates one: `name` is read-only to batches.
            let data = serde_json::to_value(kition_cad_doc::Layer::new("Batch")).unwrap_or_else(|e| panic!("{e}"));
            let mut fields = serde_json::Map::new();
            fields.insert("order".into(), serde_json::json!(99.0));
            fields.insert("data".into(), data);
            tx.create_with_id("Layer", &made, fields)?;
            tx.map_set(&kition_kit::kdm::ObjectId::from_string("drawing"), "layers", "batch", serde_json::json!(made.as_str()))?;
            CadModel::validate(tx, &BTreeSet::from([made.clone()])).map_err(|e| kition_kit::kdm::KdmError::Engine(e.to_string()))?;
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    // saved-identity contract (6423c54c), orchestrator decision
    assert_eq!(ToolIds::of_replica(&replica).tool_id(&made), Some("layer:batch-layer"));
}
