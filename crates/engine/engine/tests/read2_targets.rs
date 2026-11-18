//! C8-cad-read-2: handlers must resolve the named object's kind and owning space.
use kition_cad_doc::{EntityKind, Handle, Space};
use kition_cad_engine::Session;
use serde_json::{Value, json};
fn run(s: &mut Session, c: &str, p: Value) -> Value {
    s.execute(c, &p).unwrap()
}
fn h(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}
fn line(s: &mut Session) -> Value {
    run(s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone()
}

#[test]
fn reassociation_scans_the_dimensions_own_space() {
    let mut s = Session::new();
    s.state_mut().unwrap().space = Space::Paper("Layout1".into());
    let source = line(&mut s);
    let dim = run(&mut s, "dimlinear", json!({"object":source,"at":[5,-5]}))["handle"].clone();
    run(&mut s, "dimdisassociate", json!({"handles":[dim]}));
    s.state_mut().unwrap().space = Space::Model;
    run(&mut s, "dimreassociate", json!({"handles":[dim]}));
    let EntityKind::Dimension(dm) = &s.doc().unwrap().entity(h(&dim)).unwrap().kind else { panic!("dimension") };
    assert_eq!(dm.assoc.len(), 2);
    assert!(dm.assoc.iter().all(|a| a.handle == h(&source)));
}

#[test]
fn hatch_edit_cannot_rotate_a_named_line() {
    let mut s = Session::new();
    let a = line(&mut s);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("hatchedit", &json!({"handles":[a],"angle":30})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn chspace_rejects_a_viewport_from_another_layout() {
    let mut s = Session::new();
    let a = line(&mut s);
    let vp = run(&mut s, "mview", json!({"layout":"Layout2","p1":[0,0],"p2":[100,100]}))["viewports"][0].clone();
    let before = s.doc().unwrap().clone();
    assert!(s.execute("chspace", &json!({"handles":[a],"to":"Layout1","viewport":vp})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn chspace_does_not_silently_ignore_a_paper_destination() {
    let mut s = Session::new();
    s.state_mut().unwrap().space = Space::Paper("Layout1".into());
    let a = line(&mut s);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("chspace", &json!({"handles":[a],"to":"Layout2"})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn draw_order_uses_the_addressed_entities_space() {
    let mut s = Session::new();
    let a = line(&mut s);
    let b = line(&mut s);
    s.state_mut().unwrap().space = Space::Paper("Layout1".into());
    run(&mut s, "draworder.front", json!({"handles":[a]}));
    assert_eq!(s.doc().unwrap().model.last().unwrap().handle, h(&a));
    run(&mut s, "draworder.back", json!({"handles":[a]}));
    assert_eq!(s.doc().unwrap().model.last().unwrap().handle, h(&b));
}

#[test]
fn an_invalid_viewport_selection_never_changes_the_active_viewport() {
    let mut s = Session::new();
    let a = line(&mut s);
    let vp = run(&mut s, "mview", json!({"layout":"Layout1","p1":[0,0],"p2":[100,100]}))["viewports"][0].clone();
    s.state_mut().unwrap().space = Space::Paper("Layout1".into());
    run(&mut s, "mspace", json!({"handle":vp}));
    s.set_selection(vec![h(&a)]);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("viewport.set", &json!({"scale":2})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn page_setup_inside_mspace_edits_the_current_sheet() {
    let mut s = Session::new();
    let vp = run(&mut s, "mview", json!({"layout":"Layout1","p1":[0,0],"p2":[100,100]}))["viewports"][0].clone();
    s.state_mut().unwrap().space = Space::Paper("Layout1".into());
    run(&mut s, "mspace", json!({"handle":vp}));
    let answer = s.execute("pagesetup", &json!({"paper":"A3"}));
    assert!(answer.is_ok(), "{answer:?}");
    assert_eq!(s.doc().unwrap().layout("Layout1").unwrap().page.paper, "ISO A3 (297.00 x 420.00 MM)");
}
