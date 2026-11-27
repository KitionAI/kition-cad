//! Drafting by hand, round two (stage 8): a rectangle's other corner free of Ortho and tracking,
//! one undo step per grip placement, the grip options Convert to Arc / Line, Radius and a
//! polyline end's Lengthen, the value boxes of a hot grip, and selecting (Select Similar, the
//! "Select objects" options, groups picked as one), driven through the tool events as the editor
//! sends them.
use super::*;
use kition_cad_doc::EntityKind;

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("Handling.dxf", 0, 0).unwrap();
    // No running object snaps, so only the mode under test moves the cursor.
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    doc
}

fn event(doc: &mut CadDocument, action: PointerAction, point: Vec2, mods: ToolMods) -> ToolResponse {
    let (x, y) = doc.placement().to_doc(point);
    doc.pointer(&ToolPointer { action, x, y, mods, pressure: 1.0, zoom: 1.0 }).unwrap()
}

fn hover(doc: &mut CadDocument, point: Vec2) -> ToolResponse {
    event(doc, PointerAction::Move, point, ToolMods::default())
}

fn click(doc: &mut CadDocument, point: Vec2) {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
        event(doc, action, point, ToolMods::default());
    }
}

fn history(doc: &CadDocument) -> Vec<String> {
    doc.history.iter().cloned().collect()
}

fn near(a: Vec2, b: Vec2) -> bool {
    a.dist(b) < 1e-9
}

fn polylines(doc: &CadDocument) -> Vec<Vec<Vec2>> {
    doc.session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::LwPolyline(p) = &e.kind { Some(p.vertices.iter().map(|v| v.p).collect()) } else { None })
        .collect()
}

/// The walkthrough's window (acceptance step 9): with Ortho on, RECTANG's other corner was squared
/// to the first ("The rectangle has no area"), and with polar tracking on a narrow rectangle drawn
/// by two clicks was pulled flat to 0°. The other corner (and any "opposite corner") now takes
/// object snap and grid snap only, so a 20 × 3 window drawn by two clicks is 20 × 3.
#[test]
fn issue_rectangle_corner_ortho() {
    for (ortho, polar) in [(true, false), (false, true), (true, true)] {
        let mut doc = blank();
        doc.execute("drafting.ortho", json!({ "on": ortho })).unwrap();
        doc.execute("drafting.polar", json!({ "on": polar })).unwrap();
        doc.select_tool("rectangle.add", 1.0).unwrap();
        let first = Vec2::new(10.0, 10.0);
        click(&mut doc, first);
        // Three units up over twenty across: within polar tracking's reach of 0° at this zoom.
        let other = first + Vec2::new(20.0, doc.session.pixel_size() * 3.0);
        hover(&mut doc, other);
        assert!(doc.aim.is_some_and(|aim| near(aim, other)), "ortho {ortho}, polar {polar}: the corner is the pointer, {:?}", doc.aim);
        click(&mut doc, other);
        let drawn = polylines(&doc);
        assert_eq!(drawn.len(), 1, "ortho {ortho}, polar {polar}: {:?}", history(&doc));
        assert!(drawn[0].iter().any(|v| near(*v, other)), "ortho {ortho}, polar {polar}: {drawn:?}");
    }
    // Grid snap still holds the corner, and LINE's next point still takes Ortho.
    let mut doc = blank();
    doc.execute("drafting.ortho", json!({ "on": true })).unwrap();
    doc.execute("drafting.snap", json!({ "on": true })).unwrap();
    doc.session.settings.snapunit = Vec2::new(1.0, 1.0);
    doc.select_tool("rectangle.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.2, 9.9));
    click(&mut doc, Vec2::new(29.8, 13.1));
    assert_eq!(polylines(&doc).len(), 1);
    assert!(polylines(&doc)[0].contains(&Vec2::new(30.0, 13.0)), "{:?}", polylines(&doc));
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(50.0, 50.0));
    hover(&mut doc, Vec2::new(70.0, 53.0));
    assert!(doc.aim.is_some_and(|aim| near(aim, Vec2::new(70.0, 50.0))), "LINE keeps Ortho: {:?}", doc.aim);
    // Object snap holds the corner too.
    let mut doc = blank();
    doc.execute("line.add", json!({ "points": [[100, 100], [140, 104]] })).unwrap();
    doc.session.settings.osmode = kition_cad_engine::snap::mode::END;
    doc.execute("drafting.ortho", json!({ "on": true })).unwrap();
    doc.select_tool("rectangle.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(90.0, 90.0));
    let beside = Vec2::new(140.0, 104.0) + Vec2::new(doc.session.pixel_size(), 0.0);
    hover(&mut doc, beside);
    assert!(doc.aim.is_some_and(|aim| near(aim, Vec2::new(140.0, 104.0))), "the endpoint holds the corner: {:?}", doc.aim);
}
