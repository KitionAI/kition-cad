//! One id space (task card C4/C5/C6-cad; KitObjectModel OM9, KHP A46, A47): an entity is named by
//! its handle everywhere, and an entity a command makes has, from the moment the command returns,
//! the handle the saved file gives it and the object model names it by. Layers, blocks, layouts
//! and named views are named by their name in their table (`layer:Walls`). Before, an entity made
//! in the session was a random object id in the object model, new on every open.
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use serde_json::{Value, json};

fn load(doc: &mut CadDocument, target: &str, fields: &[&str]) -> Value {
    let answer = Exchange::new().batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": fields }] }), false).unwrap();
    answer["loaded"][0]["fields"].clone()
}

fn handle_of(result: &Value) -> String {
    result["handle"].as_str().or_else(|| result["handles"][0].as_str()).unwrap_or_else(|| panic!("no handle in {result}")).to_owned()
}

/// A new drawing opened from its file, as an agent opens one.
fn opened() -> CadDocument {
    let bytes = CadDocument::create("Plan.dxf", 420, 297).unwrap().save(Some("Plan.dxf")).unwrap();
    CadDocument::open("Plan.dxf", &bytes).unwrap()
}

#[test]
fn an_entity_made_in_the_session_is_named_by_the_handle_it_keeps_in_the_file() {
    let mut doc = opened();
    let line = handle_of(&doc.execute("line", json!({ "points": [[10, 10], [100, 60]] })).unwrap());
    let circle = handle_of(&doc.execute("circle", json!({ "center": [50, 50], "radius": 10 })).unwrap());
    // The object model names both by the handles the commands answered.
    let model = load(&mut doc, "root", &["model"])["model"].clone();
    assert_eq!(model, json!([line, circle]), "the model space list names entities by handle");
    assert_eq!(load(&mut doc, &line, &["handle"])["handle"], json!(line), "a handle is a target");
    // The saved file keeps them, and the reopened drawing answers the same ids.
    let bytes = doc.save(Some("Plan.dxf")).unwrap();
    let mut again = CadDocument::open("Plan.dxf", &bytes).unwrap();
    assert_eq!(load(&mut again, "root", &["model"])["model"], json!([line, circle]));
    let listed = again.execute("object.query", json!({})).unwrap();
    let handles: Vec<&str> = listed["object.query"].as_array().unwrap().iter().filter_map(|e| e["handle"].as_str()).collect();
    assert_eq!(handles, [line.as_str(), circle.as_str()]);
    // A move by that handle in the reopened drawing moves that line.
    again.execute("move", json!({ "handles": [line], "delta": [5, 0] })).unwrap();
    let moved = load(&mut again, &line, &["kind"])["kind"].to_string();
    assert!(moved.contains("15.0") || moved.contains("15,"), "the line moved by 5: {moved}");
}

#[test]
fn layers_blocks_layouts_and_views_are_named_by_their_names() {
    // A drawing with a named view (read from its file: no command saves one).
    let bytes = CadDocument::create("Plan.dxf", 420, 297).unwrap().save(Some("Plan.dxf")).unwrap();
    let mut drawing = kition_cad_io::read(&bytes, "Plan.dxf").unwrap();
    drawing.views.push(kition_cad_doc::NamedView {
        name: "Front".into(),
        center: kition_cad_geom::Vec2::new(50.0, 50.0),
        height: 40.0,
        width: 60.0,
        layer_state: None,
    });
    let bytes = kition_cad_io::write(&drawing, "Plan.dxf").unwrap();
    let mut doc = CadDocument::open("Plan.dxf", &bytes).unwrap();
    doc.execute("layer.add", json!({ "name": "Walls" })).unwrap();
    for (id, name) in [("layer:Walls", "Walls"), ("layer:0", "0"), ("layout:Layout1", "Layout1"), ("view:Front", "Front")] {
        assert_eq!(load(&mut doc, id, &["name"])["name"], json!(name), "{id}");
    }
}
