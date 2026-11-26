//! The Properties panel's description (walkthrough N13, N14, 别扭 8): rows by group with their
//! kinds, rounded values, disagreement and writers; the current properties with nothing
//! selected; the layers with why one cannot be deleted; and typed values read as numbers.
use super::*;
use kition_cad_doc::EntityKind;

fn blank() -> CadDocument {
    CadDocument::create("Properties.dxf", 0, 0).unwrap()
}

fn added(doc: &mut CadDocument, command: &str, params: Value) -> String {
    let answer = doc.execute(command, params).unwrap();
    answer.get("handle").or_else(|| answer["handles"].get(0)).and_then(Value::as_str).unwrap_or_else(|| panic!("{answer}")).to_owned()
}

fn select(doc: &mut CadDocument, handles: &[&str]) {
    doc.session.set_selection(handles.iter().map(|h| kition_cad_doc::Handle::parse_hex(h).unwrap()).collect());
}

fn color_of(doc: &CadDocument, handle: &str) -> kition_cad_color::Color {
    doc.session.doc().unwrap().entity(kition_cad_doc::Handle::parse_hex(handle).unwrap()).unwrap().common.color
}

fn describe(doc: &mut CadDocument) -> Value {
    doc.execute(properties::DESCRIBE, json!({})).unwrap()
}

fn row<'a>(described: &'a Value, key: &str) -> &'a Value {
    described["rows"].as_array().unwrap().iter().find(|row| row["key"] == key).unwrap_or_else(|| panic!("no row {key} in {described}"))
}

fn has_row(described: &Value, key: &str) -> bool {
    described["rows"].as_array().unwrap().iter().any(|row| row["key"] == key)
}

/// Runs a row's writer with `value`, as the panel does.
fn write(doc: &mut CadDocument, row: &Value, value: Value) {
    let write = &row["write"];
    let mut params = write["params"].clone();
    let key = write["key"].as_str().unwrap();
    let value = match write.get("point").and_then(Value::as_array) {
        Some(point) => {
            let mut point = point.clone();
            point[write["index"].as_u64().unwrap() as usize] = value;
            Value::Array(point)
        }
        None => value,
    };
    params[key] = value;
    doc.execute(write["command"].as_str().unwrap(), params).unwrap();
}

#[test]
fn a_line_is_described_by_group_and_its_end_x_is_written_in_one_step() {
    let mut doc = blank();
    let line = added(&mut doc, "line.add", json!({ "points": [[10, 20], [40, 60]] }));
    select(&mut doc, &[&line]);
    let described = describe(&mut doc);
    assert_eq!(described["count"], 1);
    assert_eq!(described["current"], false);
    for (key, group) in [
        ("color", "General"),
        ("layer", "General"),
        ("linetype", "General"),
        ("ltscale", "General"),
        ("lineweight", "General"),
        ("transparency", "General"),
    ] {
        assert_eq!(row(&described, key)["group"], group, "{key}");
    }
    assert_eq!(row(&described, "color")["kind"], "color");
    assert!(row(&described, "color")["options"].as_array().unwrap().len() >= 9, "ByLayer, ByBlock and the 7 standard colours");
    assert_eq!(row(&described, "start.x")["display"], "10");
    assert_eq!(row(&described, "end.y")["display"], "60");
    assert_eq!(row(&described, "length")["display"], "50");
    assert_eq!(row(&described, "length")["unit"], "length");
    assert_eq!(row(&described, "angle")["unit"], "angle");
    assert_eq!(row(&described, "delta.x")["editable"], false);
    assert_eq!(row(&described, "end.x")["editable"], true);
    write(&mut doc, row(&described, "end.x"), json!(100.0));
    let handle = kition_cad_doc::Handle::parse_hex(&line).unwrap();
    let end = |doc: &CadDocument| match &doc.session.doc().unwrap().entity(handle).unwrap().kind {
        EntityKind::Line(l) => (l.b.x, l.b.y),
        _ => panic!("not a line"),
    };
    assert_eq!(end(&doc), (100.0, 60.0), "the end's Y is kept");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(end(&doc), (40.0, 60.0), "one undo step");
}

#[test]
fn numbers_are_rounded_to_the_drawings_precision() {
    let mut doc = blank();
    let circle = added(&mut doc, "circle.add", json!({ "center": [0, 0], "radius": 340.411569987389 }));
    select(&mut doc, &[&circle]);
    let described = describe(&mut doc);
    assert_eq!(row(&described, "radius")["display"], "340.4116");
    assert_eq!(row(&described, "radius")["value"].as_f64(), Some(340.411569987389), "the value itself stays exact");
    doc.execute("file.setUnits", json!({ "luprec": 2 })).unwrap();
    select(&mut doc, &[&circle]);
    assert_eq!(row(&describe(&mut doc), "radius")["display"], "340.41");
}

#[test]
fn several_objects_show_what_they_share_and_mark_what_differs() {
    let mut doc = blank();
    let a = added(&mut doc, "circle.add", json!({ "center": [0, 0], "radius": 5 }));
    let b = added(&mut doc, "circle.add", json!({ "center": [0, 20], "radius": 8 }));
    let line = added(&mut doc, "line.add", json!({ "points": [[0, 0], [1, 1]] }));
    select(&mut doc, &[&a, &b]);
    let described = describe(&mut doc);
    assert_eq!(row(&described, "radius")["varies"], true);
    assert_eq!(row(&described, "radius")["display"], "");
    assert_eq!(row(&described, "center.x")["varies"], false);
    assert_eq!(row(&described, "center.x")["editable"], false, "X is written with Y, on which they disagree");
    assert_eq!(row(&described, "color")["varies"], false);
    // A colour for both in one step.
    write(&mut doc, row(&described, "color"), json!("Red"));
    assert!([&a, &b].iter().all(|h| color_of(&doc, h) == kition_cad_color::Color::Index(1)));
    doc.execute("edit.undo", json!({})).unwrap();
    assert!([&a, &b].iter().all(|h| color_of(&doc, h) == kition_cad_color::Color::ByLayer), "one undo step");
    // Of several kinds, only the general rows.
    select(&mut doc, &[&a, &line]);
    let described = describe(&mut doc);
    assert_eq!(described["types"].as_array().unwrap().len(), 2);
    assert!(has_row(&described, "layer") && !has_row(&described, "radius") && !has_row(&described, "length"));
}

#[test]
fn nothing_selected_describes_the_current_properties_with_their_own_commands() {
    let mut doc = blank();
    doc.execute("layer.add", json!({ "name": "Walls" })).unwrap();
    let described = describe(&mut doc);
    assert_eq!(described["current"], true);
    assert_eq!(row(&described, "color")["write"]["command"], "property.color");
    assert_eq!(row(&described, "layer")["write"]["command"], "layer.setCurrent");
    assert_eq!(row(&described, "linetype")["write"]["command"], "property.linetype");
    assert_eq!(row(&described, "lineweight")["write"]["command"], "property.lineweight");
    write(&mut doc, row(&described, "layer"), json!("Walls"));
    write(&mut doc, row(&described, "color"), json!("Blue"));
    write(&mut doc, row(&described, "lineweight"), json!(0.5));
    write(&mut doc, row(&described, "ltscale"), json!(2.0));
    let described = describe(&mut doc);
    assert_eq!(row(&described, "layer")["value"], "Walls");
    assert_eq!(row(&described, "color")["value"], "Blue");
    assert_eq!(row(&described, "lineweight")["value"].as_f64(), Some(0.5));
    assert_eq!(row(&described, "ltscale")["display"], "2");
    // New objects get them.
    let circle = added(&mut doc, "circle.add", json!({ "center": [0, 0], "radius": 5 }));
    let d = doc.session.doc().unwrap();
    let e = d.entity(kition_cad_doc::Handle::parse_hex(&circle).unwrap()).unwrap();
    assert_eq!(e.common.layer, "Walls");
    assert_eq!(color_of(&doc, &circle), kition_cad_color::Color::Index(5));
}

#[test]
fn layers_say_why_one_cannot_be_deleted() {
    let mut doc = blank();
    doc.execute("layer.add", json!({ "name": "Walls", "color": "red" })).unwrap();
    doc.execute("layer.add", json!({ "name": "Empty" })).unwrap();
    doc.execute("layer.add", json!({ "name": "Here", "current": true })).unwrap();
    doc.execute("circle.add", json!({ "center": [0, 0], "radius": 5, "layer": "Walls" })).unwrap();
    let described = describe(&mut doc);
    let layer = |name: &str| described["layers"].as_array().unwrap().iter().find(|l| l["name"] == name).unwrap().clone();
    assert_eq!(layer("0")["deleteBlocked"], "reserved");
    assert_eq!(layer("Walls")["deleteBlocked"], "used");
    assert_eq!(layer("Here")["deleteBlocked"], "current");
    assert!(layer("Empty")["deleteBlocked"].is_null());
    assert_eq!(layer("Walls")["color"], "Red");
    assert_eq!(layer("Walls")["rgb"], "#ff0000");
    assert!(described["library"].as_array().unwrap().iter().any(|name| name == "DASHED"), "{}", described["library"]);
}

#[test]
fn typed_values_read_with_arithmetic_and_units() {
    let mut doc = blank();
    let parsed = doc
        .execute(
            properties::DESCRIBE,
            json!({ "parse": [
                { "text": "2*30+5", "unit": "length" },
                { "text": "1'6\"", "unit": "length" },
                { "text": "90", "unit": "angle" },
                { "text": "0.5r", "unit": "angle" },
                { "text": "nonsense", "unit": "length" },
                { "text": "3/4", "unit": "number" },
            ] }),
        )
        .unwrap()["parsed"]
        .clone();
    let values: Vec<Option<f64>> = parsed.as_array().unwrap().iter().map(Value::as_f64).collect();
    assert_eq!(values[0], Some(65.0));
    assert_eq!(values[1], Some(18.0));
    assert_eq!(values[2], Some(90.0));
    assert!((values[3].unwrap() - 0.5f64.to_degrees()).abs() < 1e-9);
    assert_eq!(values[4], None);
    assert_eq!(values[5], Some(0.75));
    // A unit after a number when the drawing says its own (INSUNITS 4, millimetres).
    doc.execute("file.setUnits", json!({ "insunits": 4 })).unwrap();
    let parsed = doc.execute(properties::DESCRIBE, json!({ "parse": [{ "text": "2.5cm", "unit": "length" }] })).unwrap()["parsed"].clone();
    assert_eq!(parsed[0].as_f64(), Some(25.0));
}

#[test]
fn describing_changes_nothing_and_keeps_a_hot_grip() {
    let mut doc = blank();
    let circle = added(&mut doc, "circle.add", json!({ "center": [0, 0], "radius": 5 }));
    select(&mut doc, &[&circle]);
    doc.execute("grip.edit", json!({ "handle": circle, "index": 0 })).unwrap();
    assert!(doc.hot_grip.is_some());
    let dirty = doc.dirty();
    describe(&mut doc);
    assert!(doc.hot_grip.is_some(), "a read ends no grip edit");
    assert_eq!(doc.dirty(), dirty);
}

#[test]
fn a_locked_member_makes_every_row_read_only() {
    let mut doc = blank();
    doc.execute("layer.add", json!({ "name": "Locked" })).unwrap();
    let circle = added(&mut doc, "circle.add", json!({ "center": [0, 0], "radius": 5, "layer": "Locked" }));
    doc.execute("layer.set", json!({ "name": "Locked", "locked": true })).unwrap();
    select(&mut doc, &[&circle]);
    let described = describe(&mut doc);
    assert_eq!(described["locked"], true);
    assert!(described["rows"].as_array().unwrap().iter().all(|row| row["editable"] == false));
}
