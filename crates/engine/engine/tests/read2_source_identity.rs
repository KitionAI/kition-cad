//! C8-cad-read-2: explicit source handles must survive coincident geometry.
use kition_cad_doc::{EntityKind, Handle};
use kition_cad_engine::Session;
use serde_json::{Value, json};
fn run(s: &mut Session, c: &str, p: Value) -> Value {
    s.execute(c, &p).unwrap()
}
fn handle(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}
fn sources(s: &Session, dim: &Value) -> Vec<Handle> {
    let EntityKind::Dimension(dm) = &s.doc().unwrap().entity(handle(dim)).unwrap().kind else { panic!("dimension") };
    assert!(!dm.assoc.is_empty());
    dm.assoc.iter().map(|a| a.handle).collect()
}
#[test]
fn a_named_line_wins_over_another_lines_coincident_start() {
    let mut s = Session::new();
    let a = run(&mut s, "line.add", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    run(&mut s, "line.add", json!({"points":[[10,0],[10,10]]}));
    let dim = run(&mut s, "dimension.addLinear", json!({"object":a,"at":[5,-5]}))["handle"].clone();
    assert_eq!(sources(&s, &dim), vec![handle(&a); 2]);
}
#[test]
fn a_named_radial_source_wins_over_an_earlier_coincident_circle() {
    for command in ["dimension.addRadius", "dimension.addDiameter"] {
        let mut s = Session::new();
        run(&mut s, "circle.add", json!({"center":[0,0],"radius":5}));
        let a = run(&mut s, "circle.add", json!({"center":[0,0],"radius":5}))["handle"].clone();
        let dim = run(&mut s, command, json!({"handle":a,"at":[10,10]}))["handle"].clone();
        assert_eq!(sources(&s, &dim), vec![handle(&a); 2], "{command}");
    }
}
#[test]
fn a_named_arc_wins_over_coincident_line_endpoints() {
    for command in ["dimension.addAngular", "dimension.addArcLength"] {
        let mut s = Session::new();
        run(&mut s, "line.add", json!({"points":[[5,0],[0,5]]}));
        let a = run(&mut s, "arc.add", json!({"center":[0,0],"radius":5,"start":0,"end":90}))["handle"].clone();
        let params = if command == "dimension.addAngular" { json!({"arc":a,"at":[10,10]}) } else { json!({"handle":a,"at":[10,10]}) };
        let dim = run(&mut s, command, params)["handle"].clone();
        assert_eq!(sources(&s, &dim), vec![handle(&a); 3], "{command}");
    }
}
