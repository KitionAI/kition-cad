//! A batch that writes a field the drawing keeps for itself is refused and changes
//! nothing (KHP A50, A18; KitObjectModel OM8): an entity's `handle` is the drawing's own name for
//! it, which every reference in the file uses, and no edit renames one. Before, `set` on an
//! entity's handle answered success and the drawing kept the old handle.
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kit::session::model::{Exchange, ModelErrorCode};
use serde_json::{Value, json};

/// A drawing with one line, opened from its file, as an agent opens one.
fn drawing() -> CadDocument {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    doc.execute("line", json!({ "points": [[10, 10], [100, 60]] })).unwrap();
    let bytes = doc.save(Some("Plan.dxf")).unwrap();
    CadDocument::open("Plan.dxf", &bytes).unwrap()
}

fn load(doc: &mut CadDocument, target: &Value, fields: &[&str]) -> Value {
    let answer = Exchange::new().batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": fields }] }), false).unwrap();
    answer["loaded"][0]["fields"].clone()
}

fn first_entity(doc: &mut CadDocument) -> Value {
    load(doc, &json!("root"), &["model"])["model"][0].clone()
}

#[test]
fn issue_read_only_fields_a_batch_setting_a_handle_is_refused_and_changes_nothing() {
    let mut doc = drawing();
    let entity = first_entity(&mut doc);
    let handle = load(&mut doc, &entity, &["handle"])["handle"].clone();
    assert!(handle.as_str().is_some_and(|h| !h.is_empty()), "the line has a handle: {handle}");
    let fields = load(&mut doc, &entity, &["common", "kind"]);
    let saved = doc.save(Some("Plan.dxf")).unwrap();
    let version = doc.version().unwrap();

    for value in [json!("khp check"), json!("ABC"), json!(null)] {
        // A valid edit first (a copy of the line), then the handle: the batch is one change, so
        // neither applies.
        let ops = json!({ "ops": [
            { "op": "create", "type": "Entity", "in": "root.model", "fields": { "common": fields["common"], "kind": fields["kind"] } },
            { "op": "set", "target": entity, "field": "handle", "value": value },
        ] });
        let error = Exchange::new().batch(&mut doc, &ops, false).unwrap_err();
        assert_eq!(error.code, ModelErrorCode::ReadOnly, "setting the handle to {value}: {error:?}");
        assert_eq!(error.index, Some(1), "the error names the operation: {error:?}");
        assert!(error.message.contains("Entity.handle") && error.message.contains("read-only"), "the error names the field: {error:?}");
        assert_eq!(doc.version().unwrap(), version, "the refused batch changed the replica");
        assert_eq!(doc.inspect().unwrap()["entities"], 1, "the refused batch's copy stayed");
        assert_eq!(load(&mut doc, &entity, &["handle"])["handle"], handle, "the handle changed");
        assert_eq!(doc.save(Some("Plan.dxf")).unwrap(), saved, "the refused batch changed the file");
    }
}

#[test]
fn issue_read_only_fields_describe_marks_the_handle_read_only() {
    let mut doc = drawing();
    let described = Exchange::new().describe(&mut doc).unwrap();
    assert_eq!(described["types"]["Entity"]["fields"]["handle"]["readOnly"], true, "{}", described["types"]["Entity"]);
    assert!(described["types"]["Entity"]["fields"]["common"].get("readOnly").is_none(), "an entity's properties stay writable");
}

/// The object model still makes entities: a new one gets its handle from the kit.
#[test]
fn issue_read_only_fields_a_new_entity_gets_its_handle_from_the_kit() {
    let mut doc = drawing();
    let entity = first_entity(&mut doc);
    let fields = load(&mut doc, &entity, &["handle", "common", "kind"]);
    let create = json!({ "ops": [{ "op": "create", "type": "Entity", "in": "root.model", "fields": { "common": fields["common"], "kind": fields["kind"] } }] });
    let created = Exchange::new().batch(&mut doc, &create, false).unwrap();
    assert_eq!(doc.inspect().unwrap()["entities"], 2, "{created}");
    let copy = load(&mut doc, &json!("root"), &["model"])["model"][1].clone();
    let handle = load(&mut doc, &copy, &["handle"])["handle"].clone();
    assert!(handle.as_str().is_some_and(|h| !h.is_empty()) && handle != fields["handle"], "the copy has a handle of its own: {handle}");

    let named = json!({ "ops": [{ "op": "create", "type": "Entity", "in": "root.model", "fields": { "handle": "FFFF", "common": fields["common"], "kind": fields["kind"] } }] });
    let before = doc.save(Some("Plan.dxf")).unwrap();
    let version = doc.version().unwrap();
    let error = Exchange::new().batch(&mut doc, &named, false).unwrap_err();
    // The public SDK permits creation-time register initialization. CAD's settler
    // preserves handle allocation policy; custom validation has code invalid-value.
    assert_eq!(error.code, ModelErrorCode::InvalidValue, "{error:?}");
    assert!(error.message.contains("read-only") && error.message.contains("Entity.handle"), "{error:?}");
    assert_eq!(doc.inspect().unwrap()["entities"], 2);
    assert_eq!(doc.version().unwrap(), version);
    assert_eq!(doc.save(Some("Plan.dxf")).unwrap(), before);
}
