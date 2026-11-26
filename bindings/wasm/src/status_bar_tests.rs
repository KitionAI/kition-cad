//! The status bar's menus (walkthrough N19): the editor reads the running object snap modes and
//! polar tracking's increment angle under `/drafting`, and the commands its menus run change them
//! in the middle of a command without ending it or adding an undo step.
use super::*;

fn click(doc: &mut CadDocument, point: Vec2) {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
        let (x, y) = doc.placement().to_doc(point);
        doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap();
    }
}

fn drafting(doc: &CadDocument) -> Value {
    doc.inspect().unwrap()["drafting"].clone()
}

#[test]
fn the_status_bar_reads_and_sets_object_snap_modes_and_the_polar_angle() {
    let mut doc = CadDocument::create("Status.dxf", 0, 0).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    let undo = doc.inspect().unwrap()["canUndo"].clone();

    // Endpoint and Center: OSMODE 1 + 4, and running snaps on.
    doc.execute("drafting.objectSnap", json!({ "osmode": 5 })).unwrap();
    assert_eq!(drafting(&doc)["osmode"], json!(5));
    assert_eq!(drafting(&doc)["osnap"], json!(true));
    // F3 off keeps the modes; the menu still shows them.
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    assert_eq!(drafting(&doc)["osmode"], json!(5));
    assert_eq!(drafting(&doc)["osnap"], json!(false));

    doc.execute("variable.set", json!({ "name": "POLARANG", "value": 30 })).unwrap();
    let angle = drafting(&doc)["polarAngle"].as_f64().unwrap();
    assert!((angle - 30.0).abs() < 1e-9, "{angle}");

    assert_eq!(doc.inspect().unwrap()["running"], json!("line.add"), "the menus do not end LINE");
    assert_eq!(doc.inspect().unwrap()["canUndo"], undo, "nor add an undo step");
}
