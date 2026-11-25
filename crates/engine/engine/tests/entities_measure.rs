//! `object.measure` (task card C4/C5/C6-cad): what an agent that cannot see precisely needs
//! measured, in one read-only command that changes nothing and reads no editor state: each
//! entity's bounding box, the minimum distance between two entities or points, the points where
//! two entities cross, and the duplicate objects of a space (OVERKILL's report, nothing deleted).
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_cad_engine::Session;
use serde_json::{Value, json};

fn handle(answer: &Value) -> String {
    answer["handle"].as_str().or_else(|| answer["handles"][0].as_str()).unwrap_or_else(|| panic!("no handle in {answer}")).to_owned()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// A 10 x 10 square from four lines at the origin, a circle of radius 5 at (20, 5), and a line
/// from (-5, 5) to (30, 5) across both.
struct Fixture {
    s: Session,
    bottom: String,
    circle: String,
    across: String,
}

fn fixture() -> Fixture {
    let mut s = Session::new();
    let bottom = handle(&s.execute("line.add", &json!({ "points": [[0, 0], [10, 0]] })).unwrap());
    s.execute("line.add", &json!({ "points": [[10, 0], [10, 10]] })).unwrap();
    s.execute("line.add", &json!({ "points": [[10, 10], [0, 10]] })).unwrap();
    s.execute("line.add", &json!({ "points": [[0, 10], [0, 0]] })).unwrap();
    let circle = handle(&s.execute("circle.add", &json!({ "center": [20, 5], "radius": 5 })).unwrap());
    let across = handle(&s.execute("line.add", &json!({ "points": [[-5, 5], [30, 5]] })).unwrap());
    Fixture { s, bottom, circle, across }
}

fn measure(s: &mut Session, params: Value) -> Value {
    s.execute("object.measure", &params).unwrap_or_else(|e| panic!("{params}: {e}"))
}

#[test]
fn bounds_of_each_entity() {
    let mut f = fixture();
    let answer = measure(&mut f.s, json!({ "bounds": [f.bottom, f.circle] }));
    let boxes = answer["bounds"].as_array().unwrap();
    assert_eq!(boxes[0], json!({ "handle": f.bottom, "min": [0.0, 0.0], "max": [10.0, 0.0] }));
    assert_eq!(boxes[1], json!({ "handle": f.circle, "min": [15.0, 0.0], "max": [25.0, 10.0] }));
}

#[test]
fn minimum_distance_between_entities_and_points() {
    let mut f = fixture();
    // The square's bottom edge and the circle: from (10, 0) to the circle's nearest point.
    let answer = measure(&mut f.s, json!({ "distance": [f.bottom, f.circle] }));
    let expected = (10.0_f64 * 10.0 + 5.0 * 5.0).sqrt() - 5.0;
    assert!(close(answer["distance"].as_f64().unwrap(), expected, 1e-4), "{answer}");
    assert_eq!(answer["from"], json!([10.0, 0.0]), "{answer}");
    // A point and an entity, two points.
    let answer = measure(&mut f.s, json!({ "distance": [[20, 20], f.circle] }));
    assert!(close(answer["distance"].as_f64().unwrap(), 10.0, 1e-4), "{answer}");
    let answer = measure(&mut f.s, json!({ "distance": [[0, 0], [3, 4]] }));
    assert_eq!(answer["distance"], json!(5.0));
    // Crossing entities are 0 apart.
    let answer = measure(&mut f.s, json!({ "distance": [f.across, f.circle] }));
    assert_eq!(answer["distance"], json!(0.0));
}

#[test]
fn intersections_of_two_entities() {
    let mut f = fixture();
    let answer = measure(&mut f.s, json!({ "intersections": [f.across, f.circle] }));
    let mut points: Vec<(f64, f64)> = answer["points"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(points.len(), 2, "{answer}");
    assert!(close(points[0].0, 15.0, 1e-9) && close(points[0].1, 5.0, 1e-9) && close(points[1].0, 25.0, 1e-9), "{answer}");
    let none = measure(&mut f.s, json!({ "intersections": [f.bottom, f.circle] }));
    assert_eq!(none["points"], json!([]));
}

#[test]
fn overlaps_are_reported_and_nothing_is_deleted() {
    let mut f = fixture();
    let copy = handle(&f.s.execute("line.add", &json!({ "points": [[0, 0], [10, 0]] })).unwrap());
    let revision = f.s.state().unwrap().revision;
    let answer = measure(&mut f.s, json!({ "overlaps": {} }));
    assert_eq!(answer["pairs"], json!([{ "a": f.bottom, "b": copy, "kind": "duplicate" }]), "{answer}");
    assert_eq!(f.s.state().unwrap().revision, revision, "measuring changed nothing");
    assert!(f.s.doc().unwrap().entity(kition_cad_engine::doc::Handle::parse_hex(&copy).unwrap()).is_some());
    // Another space by name: Layout1 holds none.
    assert_eq!(measure(&mut f.s, json!({ "overlaps": { "layout": "Layout1" } }))["pairs"], json!([]));
}

#[test]
fn what_cannot_be_measured_is_refused_by_name() {
    let mut f = fixture();
    for (params, says) in [
        (json!({}), "one of"),
        (json!({ "bounds": ["FFFFFF"] }), "FFFFFF"),
        (json!({ "distance": [f.bottom] }), "two"),
        (json!({ "intersections": [[0, 0], f.circle] }), "entities"),
        (json!({ "overlaps": { "layout": "Nope" } }), "Nope"),
        (json!({ "bounds": [f.bottom], "distance": [f.bottom, f.circle] }), "one of"),
    ] {
        let error = f.s.execute("object.measure", &params).unwrap_err().to_string();
        assert!(error.contains(says), "{params}: {error}");
    }
}
