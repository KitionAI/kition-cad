//! `object.setProperties` places what the Properties panel shows a place for (stage 9): one vertex
//! of a polyline, an ellipse's centre, a point's position, a paragraph's and a block reference's
//! insertion point (its attributes go with it), each as one undo step.

use kition_cad_engine::Session;
use kition_cad_engine::doc::{EntityKind, Handle};
use serde_json::{Value, json};

fn handle(answer: &Value) -> String {
    answer.get("handle").or_else(|| answer["handles"].get(0)).and_then(Value::as_str).unwrap().to_owned()
}

fn kind(s: &Session, h: &str) -> EntityKind {
    s.doc().unwrap().entity(Handle::parse_hex(h).unwrap()).unwrap().kind.clone()
}

#[test]
fn one_polyline_vertex_moves_and_the_others_stay() {
    let mut s = Session::new();
    let poly = handle(&s.execute("polyline.add", &json!({ "points": [[0, 0], [10, 0], [10, 5]] })).unwrap());
    s.execute("object.setProperties", &json!({ "handles": [poly], "vertex": 1, "vertexAt": [12, -2] })).unwrap();
    let EntityKind::LwPolyline(pl) = kind(&s, &poly) else { panic!("not a polyline") };
    let points: Vec<(f64, f64)> = pl.vertices.iter().map(|v| (v.p.x, v.p.y)).collect();
    assert_eq!(points, vec![(0.0, 0.0), (12.0, -2.0), (10.0, 5.0)]);
    // A vertex it does not have, or one key without the other, is refused and changes nothing.
    assert!(s.execute("object.setProperties", &json!({ "handles": [poly], "vertex": 3, "vertexAt": [1, 1] })).is_err());
    assert!(s.execute("object.setProperties", &json!({ "handles": [poly], "vertex": 0 })).is_err());
    assert!(s.execute("object.setProperties", &json!({ "handles": [poly], "vertexAt": [1, 1] })).is_err());
    assert!(s.execute("object.setProperties", &json!({ "handles": [poly], "vertex": -1, "vertexAt": [1, 1] })).is_err());
    assert!(s.execute("object.setProperties", &json!({ "handles": [poly], "vertex": 0, "vertexAt": "nowhere" })).is_err());
    let EntityKind::LwPolyline(after) = kind(&s, &poly) else { panic!("not a polyline") };
    assert_eq!(after.vertices, pl.vertices);
    // One undo step takes the move back.
    s.execute("edit.undoStep", &json!({})).unwrap();
    let EntityKind::LwPolyline(undone) = kind(&s, &poly) else { panic!("not a polyline") };
    assert_eq!((undone.vertices[1].p.x, undone.vertices[1].p.y), (10.0, 0.0));
}

#[test]
fn an_ellipse_a_point_and_a_paragraph_move_to_where_they_are_told() {
    let mut s = Session::new();
    let ellipse = handle(&s.execute("ellipse.add", &json!({ "center": [0, 0], "major": [10, 0], "ratio": 0.5 })).unwrap());
    let point = handle(&s.execute("point.add", &json!({ "at": [1, 1] })).unwrap());
    let paragraph = handle(&s.execute("multilineText.add", &json!({ "at": [0, 0], "text": "Room", "height": 2.5 })).unwrap());
    s.execute("object.setProperties", &json!({ "handles": [ellipse], "center": [30, 40] })).unwrap();
    s.execute("object.setProperties", &json!({ "handles": [point], "position": [5, 6] })).unwrap();
    s.execute("object.setProperties", &json!({ "handles": [paragraph], "position": [7, 8] })).unwrap();
    let EntityKind::Ellipse(el) = kind(&s, &ellipse) else { panic!("not an ellipse") };
    assert_eq!((el.center.x, el.center.y, el.major.x, el.ratio), (30.0, 40.0, 10.0, 0.5));
    let EntityKind::Point(pt) = kind(&s, &point) else { panic!("not a point") };
    assert_eq!((pt.p.x, pt.p.y), (5.0, 6.0));
    let EntityKind::MText(t) = kind(&s, &paragraph) else { panic!("not a paragraph") };
    assert_eq!((t.insert.x, t.insert.y, t.contents.as_str()), (7.0, 8.0, "Room"));
}

#[test]
fn a_block_reference_moves_with_its_attributes_and_keeps_scale_and_rotation_writable() {
    let mut s = Session::new();
    let circle = handle(&s.execute("circle.add", &json!({ "center": [0, 0], "radius": 2 })).unwrap());
    let tag = handle(&s.execute("attribute.define", &json!({ "tag": "NO", "default": "1", "at": [3, 0], "height": 1 })).unwrap());
    s.execute("block.add", &json!({ "name": "Seat", "base": [0, 0], "handles": [circle, tag] })).unwrap();
    let seat = handle(&s.execute("block.insert", &json!({ "name": "Seat", "at": [10, 10], "attribs": { "NO": "7" } })).unwrap());
    let EntityKind::Insert(before) = kind(&s, &seat) else { panic!("not a block reference") };
    let label = before.attribs.first().map(|a| (a.text.insert.x, a.text.insert.y)).unwrap();
    s.execute("object.setProperties", &json!({ "handles": [seat], "position": [20, 15], "scale": 2, "rotation": 90 })).unwrap();
    let EntityKind::Insert(after) = kind(&s, &seat) else { panic!("not a block reference") };
    assert_eq!((after.insert.x, after.insert.y), (20.0, 15.0));
    assert_eq!((after.scale.x, after.scale.y), (2.0, 2.0));
    assert!((after.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    let moved = after.attribs.first().map(|a| (a.text.insert.x, a.text.insert.y)).unwrap();
    assert_eq!(moved, (label.0 + 10.0, label.1 + 5.0), "the attribute goes with the reference");
}
