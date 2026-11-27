//! Drafting by hand, round two (stage 8): a rectangle's other corner free of Ortho and tracking,
//! one undo step per grip placement, the grip options Convert to Arc / Line, Radius and a
//! polyline end's Lengthen, the value boxes of a hot grip, and selecting (Select Similar, the
//! "Select objects" options, groups picked as one), driven through the tool events as the editor
//! sends them.
use super::*;
use kition_cad_doc::{EntityKind, Handle};

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("Handling.dxf", 0, 0).unwrap();
    // No running object snaps, so only the mode under test moves the cursor.
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    doc
}

/// A clean drawing of what `setup` adds, with nothing moving the cursor but the pointer.
fn drawing(setup: &[(&str, Value)]) -> CadDocument {
    let mut doc = blank();
    for (command, params) in setup {
        doc.execute(command, params.clone()).unwrap();
    }
    let mut doc = CadDocument::open("Handling.dxf", &doc.save(Some("Handling.dxf")).unwrap()).unwrap();
    doc.session.settings.osmode = 0;
    doc.session.settings.polarmode = false;
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

fn click_with(doc: &mut CadDocument, point: Vec2, mods: ToolMods) {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
        event(doc, action, point, mods);
    }
}

fn lines(doc: &CadDocument) -> Vec<(Vec2, Vec2)> {
    doc.session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some((l.a.xy(), l.b.xy())) } else { None })
        .collect()
}

fn undo_steps(doc: &CadDocument) -> usize {
    doc.session.state().unwrap().undo.len()
}

fn grip_info(response: &ToolResponse) -> Option<Value> {
    response.overlays.iter().find_map(|o| match o {
        ToolOverlay::Label { text, color, .. } if *color == grip_edit::GRIP_INFO => serde_json::from_str(text).ok(),
        _ => None,
    })
}

fn parts(doc: &CadDocument) -> Value {
    doc.inspect().unwrap()["drafting"]["commandLine"]["prompt"].clone()
}

fn kind(doc: &CadDocument, handle: &str) -> EntityKind {
    doc.session.doc().unwrap().entity(Handle::parse_hex(handle).unwrap()).unwrap().kind.clone()
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

/// One grip placement is one undo step: a STRETCH copy (a copy made in place, then stretched) was
/// two, and several hot grips stretched together one each. Copies stay one step each, as in CAD,
/// and the Undo option takes the last copy back whole.
#[test]
fn one_grip_placement_is_one_undo_step() {
    // A STRETCH copy.
    let mut doc = drawing(&[("line.add", json!({ "points": [[0, 0], [50, 0]] }))]);
    click(&mut doc, Vec2::new(25.0, 0.0));
    click(&mut doc, Vec2::new(50.0, 0.0));
    doc.command_line("c").unwrap();
    let before = undo_steps(&doc);
    click(&mut doc, Vec2::new(60.0, 10.0));
    assert_eq!(undo_steps(&doc), before + 1, "one copy, one step");
    click(&mut doc, Vec2::new(70.0, 20.0));
    assert_eq!(undo_steps(&doc), before + 2, "a second copy, a second step");
    doc.command_line("u").unwrap();
    assert_eq!(lines(&doc).len(), 2, "Undo takes the last copy back whole: {:?}", lines(&doc));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert!(doc.hot_grip.is_none(), "Enter after copies ends the edit");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(lines(&doc), vec![(Vec2::new(0.0, 0.0), Vec2::new(50.0, 0.0))], "one Undo takes the first copy back");

    // Several hot grips: one step for all of them.
    let mut doc = drawing(&[("line.add", json!({ "points": [[0, 0], [50, 0]] })), ("line.add", json!({ "points": [[0, 10], [50, 10]] }))]);
    click(&mut doc, Vec2::new(25.0, 0.0));
    click(&mut doc, Vec2::new(25.0, 10.0));
    assert_eq!(doc.session.selection().len(), 2);
    let shift = ToolMods { shift: true, ..ToolMods::default() };
    click_with(&mut doc, Vec2::new(50.0, 0.0), shift);
    click_with(&mut doc, Vec2::new(50.0, 10.0), shift);
    click(&mut doc, Vec2::new(50.0, 0.0));
    let before = undo_steps(&doc);
    click(&mut doc, Vec2::new(60.0, 0.0));
    let mut drawn = lines(&doc);
    drawn.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    assert_eq!(drawn, vec![(Vec2::new(0.0, 0.0), Vec2::new(60.0, 0.0)), (Vec2::new(0.0, 10.0), Vec2::new(60.0, 10.0))]);
    assert_eq!(undo_steps(&doc), before + 1, "both grips in one step");
    doc.execute("edit.undo", json!({})).unwrap();
    let mut drawn = lines(&doc);
    drawn.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    assert_eq!(drawn, vec![(Vec2::new(0.0, 0.0), Vec2::new(50.0, 0.0)), (Vec2::new(0.0, 10.0), Vec2::new(50.0, 10.0))], "one Undo");
    doc.execute("edit.redo", json!({})).unwrap();
    assert_eq!(lines(&doc).iter().filter(|(_, b)| b.x == 60.0).count(), 2, "one Redo");
}

/// The grip menu's new options: an arc's midpoint offers Radius (the new radius picked or typed,
/// the rubber band from the centre), a polyline arc segment's midpoint Convert to Line, which
/// changes the drawing at once and so is answered at the prompt; each is one undo step.
#[test]
fn grip_menu_radius_and_convert_to_line() {
    let mut doc = drawing(&[("arc.add", json!({ "center": [0, 0], "radius": 50, "start": 0, "end": 90 }))]);
    let mid = Vec2::from_angle(std::f64::consts::FRAC_PI_4) * 50.0;
    click(&mut doc, mid);
    let handle = doc.session.selection()[0].hex();
    let info = grip_info(&hover(&mut doc, mid)).expect("the midpoint grip's menu");
    assert_eq!(info["index"], 1);
    assert_eq!(info["actions"], json!([{ "label": "Radius", "keyword": "Radius", "immediate": false }]));
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 1, "option": "Radius" })).unwrap();
    assert_eq!(parts(&doc)["command"], "** RADIUS **");
    assert_eq!(parts(&doc)["base"], json!([0.0, 0.0]), "measured from the centre");
    assert_eq!(parts(&doc)["kind"], "distance");
    doc.command_line("80").unwrap();
    assert!(matches!(kind(&doc, &handle), EntityKind::Arc(a) if (a.radius - 80.0).abs() < 1e-9));
    assert!(doc.hot_grip.is_none());
    // Picked: the distance from the centre.
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 1, "option": "Radius" })).unwrap();
    click(&mut doc, Vec2::new(0.0, 60.0));
    assert!(matches!(kind(&doc, &handle), EntityKind::Arc(a) if (a.radius - 60.0).abs() < 1e-9));
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(matches!(kind(&doc, &handle), EntityKind::Arc(a) if (a.radius - 80.0).abs() < 1e-9), "one undo step");

    let mut doc = drawing(&[("polyline.add", json!({ "points": [{ "p": [0, 0], "bulge": 1 }, [100, 0], [100, 100]] }))]);
    click(&mut doc, Vec2::new(100.0, 50.0));
    let handle = doc.session.selection()[0].hex();
    // The arc segment's midpoint grip: the bulge of 1 is a half circle below the chord.
    let arc_mid = Vec2::new(50.0, -50.0);
    let info = grip_info(&hover(&mut doc, arc_mid)).expect("the arc segment's midpoint grip");
    assert_eq!(info["index"], 1);
    let keywords: Vec<&str> = info["actions"].as_array().unwrap().iter().map(|a| a["keyword"].as_str().unwrap()).collect();
    assert_eq!(keywords, ["Add vertex", "convert to Line"]);
    assert!(
        doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 1, "option": "convert to Line" })).is_err(),
        "it changes the drawing at once"
    );
    assert!(!doc.dirty());
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 1 })).unwrap();
    doc.command_line("l").unwrap();
    assert!(matches!(kind(&doc, &handle), EntityKind::LwPolyline(p) if p.vertices.iter().all(|v| v.bulge == 0.0)));
    assert!(doc.hot_grip.is_none());
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(matches!(kind(&doc, &handle), EntityKind::LwPolyline(p) if p.vertices[0].bulge == 1.0), "one undo step");
}
