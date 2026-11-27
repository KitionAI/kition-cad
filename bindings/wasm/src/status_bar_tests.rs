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

/// How many pixels of a drawn area are not the paper (the canvas's white): how much is drawn.
fn inked(pixels: &[u8]) -> usize {
    pixels
        .chunks(4)
        .filter(|p| p.first().zip(p.get(1)).zip(p.get(2)).is_some_and(|((r, g), b)| u16::from(*r) + u16::from(*g) + u16::from(*b) < 600))
        .count()
}

/// The darkest pixel of a drawn area (its smallest red + green + blue): an opaque black line is 0.
fn darkest(pixels: &[u8]) -> u16 {
    pixels.chunks(4).filter_map(|p| Some(u16::from(*p.first()?) + u16::from(*p.get(1)?) + u16::from(*p.get(2)?))).min().unwrap_or(765)
}

/// Stage 9: the status bar's Lineweight and Transparency show the objects' lineweights and
/// transparency on the canvas while they are on; Transparency runs in the middle of a command
/// without ending it, and the editor reads it under `/drafting/transparency`.
#[test]
fn lineweight_and_transparency_are_drawn_while_their_toggles_are_on() {
    let mut doc = CadDocument::create("Shown.dxf", 0, 0).unwrap();
    let line = doc.execute("line.add", json!({ "points": [[100, 100], [300, 100]] })).unwrap();
    let handle = line["handle"].as_str().or_else(|| line["handles"][0].as_str()).unwrap().to_owned();
    doc.execute("object.setProperties", json!({ "handles": [handle], "color": "7", "lineweight": 1.0, "transparency": 70 })).unwrap();
    let (x, y) = doc.placement().to_doc(Vec2::new(200.0, 100.0));
    let area = |doc: &CadDocument| doc.draw(x - 10.0, y - 10.0, 1.0, 20, 20).unwrap();
    let (thin, opaque) = (inked(&area(&doc)), darkest(&area(&doc)));

    doc.execute("drafting.lineweightDisplay", json!({ "on": true })).unwrap();
    assert_eq!(drafting(&doc)["lwdisplay"], json!(true));
    let wide = inked(&area(&doc));
    assert!(wide >= thin * 3, "a 1 mm line is drawn wider with lineweights shown: {thin} → {wide} pixels");
    doc.execute("drafting.lineweightDisplay", json!({ "on": false })).unwrap();
    assert_eq!(inked(&area(&doc)), thin, "and as a hairline again when they are hidden");

    // Transparency, in the middle of LINE.
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    let undo = doc.inspect().unwrap()["canUndo"].clone();
    assert_eq!(drafting(&doc)["transparency"], json!(false));
    doc.execute("drafting.transparencyDisplay", json!({ "on": true })).unwrap();
    assert_eq!(drafting(&doc)["transparency"], json!(true));
    assert_eq!(doc.inspect().unwrap()["running"], json!("line.add"), "Transparency does not end LINE");
    assert_eq!(doc.inspect().unwrap()["canUndo"], undo, "nor add an undo step");
    let faint = darkest(&area(&doc));
    assert!(faint > opaque + 200, "a 70% transparent line is drawn faint: darkest {opaque} → {faint}");
    doc.execute("drafting.transparencyDisplay", json!({ "on": false })).unwrap();
    assert_eq!(darkest(&area(&doc)), opaque, "and opaque again when transparency is hidden");
}
