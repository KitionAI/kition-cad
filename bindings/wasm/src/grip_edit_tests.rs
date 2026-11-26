//! Hot grips (walkthrough N11, 别扭 4 and 5): the warm grip under the pointer and its menu, a press
//! on a grip that clicks it however far it was dragged, the modes Space and Enter cycle, the
//! options (Base point, Copy, Undo, Reference, eXit, Lengthen, Add vertex, Remove vertex), Ctrl to
//! place copies, Shift for several hot grips, and the grip menu's `grip.edit`.
use super::*;
use kition_cad_doc::EntityKind;
use kition_cad_doc::Handle;

fn blank() -> CadDocument {
    CadDocument::create("Grips.dxf", 0, 0).unwrap()
}

/// `doc` saved and opened again (clean), with nothing moving the cursor but the pointer.
fn fresh(doc: CadDocument) -> CadDocument {
    let mut doc = CadDocument::open("Grips.dxf", &doc.save(Some("Grips.dxf")).unwrap()).unwrap();
    doc.session.settings.osmode = 0;
    doc.session.settings.polarmode = false;
    doc
}

/// A clean drawing of what `setup` adds.
fn drawing(setup: &[(&str, Value)]) -> CadDocument {
    let mut doc = blank();
    for (command, params) in setup {
        doc.execute(command, params.clone()).unwrap();
    }
    fresh(doc)
}

fn event(doc: &mut CadDocument, action: PointerAction, point: Vec2, mods: ToolMods) -> ToolResponse {
    let (x, y) = doc.placement().to_doc(point);
    doc.pointer(&ToolPointer { action, x, y, mods, pressure: 1.0, zoom: 1.0 }).unwrap()
}

fn hover(doc: &mut CadDocument, point: Vec2) -> ToolResponse {
    event(doc, PointerAction::Move, point, ToolMods::default())
}

fn click_with(doc: &mut CadDocument, point: Vec2, mods: ToolMods) {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
        event(doc, action, point, mods);
    }
}

fn click(doc: &mut CadDocument, point: Vec2) {
    click_with(doc, point, ToolMods::default());
}

fn enter(doc: &mut CadDocument) {
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
}

fn circles(doc: &CadDocument) -> Vec<(Vec2, f64)> {
    doc.session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::Circle(c) = &e.kind { Some((c.center.xy(), c.radius)) } else { None })
        .collect()
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

fn polyline_vertices(doc: &CadDocument) -> Vec<Vec2> {
    doc.session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| if let EntityKind::LwPolyline(p) = &e.kind { Some(p.vertices.iter().map(|v| v.p).collect()) } else { None })
        .unwrap()
}

fn near(a: Vec2, x: f64, y: f64) -> bool {
    a.near(Vec2::new(x, y), 1e-6)
}

fn parts(doc: &CadDocument) -> Value {
    doc.inspect().unwrap()["drafting"]["commandLine"]["prompt"].clone()
}

fn history(doc: &CadDocument) -> Vec<String> {
    doc.history.iter().cloned().collect()
}

/// A circle at (100, 100), radius 30, selected.
fn selected_circle() -> CadDocument {
    let mut doc = drawing(&[("circle.add", json!({ "center": [100, 100], "radius": 30 }))]);
    click(&mut doc, Vec2::new(100.0, 130.0));
    assert_eq!(doc.session.selection().len(), 1);
    doc
}

fn anchors(response: &ToolResponse, color: [u8; 4]) -> usize {
    response.overlays.iter().filter(|o| matches!(o, ToolOverlay::Anchor { color: c, .. } if *c == color)).count()
}

fn grip_info(response: &ToolResponse) -> Option<Value> {
    response.overlays.iter().find_map(|o| match o {
        ToolOverlay::Label { text, color, .. } if *color == grip_edit::GRIP_INFO => serde_json::from_str(text).ok(),
        _ => None,
    })
}

/// 别扭 5: the grip under the pointer is warm and says what its menu offers; a hot grip is red; a
/// grip on a locked layer is drawn but never warm and not picked.
#[test]
fn grips_warm_under_the_pointer_and_hot_when_clicked() {
    let mut doc = selected_circle();
    let away = hover(&mut doc, Vec2::new(160.0, 160.0));
    assert_eq!(anchors(&away, grip_edit::GRIP_WARM), 0);
    assert_eq!(anchors(&away, SELECTED), 5, "a circle's five grips");
    assert!(grip_info(&away).is_none());
    let pixel = doc.session.pixel_size();
    let on = hover(&mut doc, Vec2::new(130.0 + 2.0 * pixel, 100.0));
    assert_eq!(anchors(&on, grip_edit::GRIP_WARM), 1, "the quadrant under the pointer is warm");
    let info = grip_info(&on).unwrap();
    assert_eq!(info["index"], 1);
    assert_eq!(info["actions"], json!([]), "a circle's grips have no options of their own");
    click(&mut doc, Vec2::new(130.0, 100.0));
    let hot = hover(&mut doc, Vec2::new(150.0, 100.0));
    assert!(anchors(&hot, grip_edit::GRIP_HOT) >= 1, "the clicked grip is hot");
    assert_eq!(anchors(&hot, grip_edit::GRIP_WARM), 0);
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(!doc.dirty());

    // Locked: drawn, not warm, not picked.
    doc.execute("layer.set", json!({ "name": "0", "locked": true })).unwrap();
    doc.session.set_selection(doc.session.doc().unwrap().model.iter().map(|e| e.handle).collect());
    let locked = hover(&mut doc, Vec2::new(130.0, 100.0));
    assert_eq!(anchors(&locked, SELECTED), 5, "a locked object's grips are drawn");
    assert_eq!(anchors(&locked, grip_edit::GRIP_WARM), 0);
    assert!(grip_info(&locked).is_none());
    click(&mut doc, Vec2::new(130.0, 100.0));
    assert!(doc.hot_grip.is_none(), "a locked object's grip is not picked");
}

/// 别扭 4 (regression): a press on a grip and a drag away opened a selection window. Now the release
/// clicks the grip however far the pointer went, and no window opens.
#[test]
fn issue_drag_from_a_grip() {
    let mut doc = selected_circle();
    event(&mut doc, PointerAction::Move, Vec2::new(130.0, 100.0), ToolMods::default());
    event(&mut doc, PointerAction::Down, Vec2::new(130.0, 100.0), ToolMods::default());
    let mut response = ToolResponse::default();
    for step in 1..=10 {
        response = event(&mut doc, PointerAction::Drag, Vec2::new(130.0 + 8.0 * f64::from(step), 100.0 - 5.0 * f64::from(step)), ToolMods::default());
    }
    assert!(doc.session.pending_window.is_none(), "a drag from a grip opens no selection window");
    assert!(!response.overlays.iter().any(|o| matches!(o, ToolOverlay::Highlight { .. })), "no window drawn");
    event(&mut doc, PointerAction::Up, Vec2::new(210.0, 50.0), ToolMods::default());
    assert!(doc.hot_grip.is_some(), "the release clicks the grip it went down on");
    assert!(!doc.dirty(), "the release does not place it");
    assert_eq!(doc.session.selection().len(), 1);
    click(&mut doc, Vec2::new(150.0, 100.0));
    assert_eq!(circles(&doc), vec![(Vec2::new(100.0, 100.0), 50.0)], "the next click places it");
}

/// N11: Space and Enter cycle STRETCH, MOVE, ROTATE, SCALE, MIRROR; the command line shows the mode
/// ("** ROTATE **") and its options; typed 90 rotates about the grip; one undo step.
#[test]
fn enter_cycles_the_modes_and_rotate_takes_a_typed_angle() {
    let mut doc = selected_circle();
    click(&mut doc, Vec2::new(130.0, 100.0));
    assert!(doc.prompt().starts_with("STRETCH Specify stretch point or [Base point/Copy/Undo/eXit]"), "{}", doc.prompt());
    assert_eq!(parts(&doc)["command"], "** STRETCH **");
    assert_eq!(parts(&doc)["grip"]["mode"], "stretch");
    enter(&mut doc);
    assert!(doc.prompt().starts_with("MOVE "), "{}", doc.prompt());
    doc.command_line("").unwrap();
    assert!(doc.prompt().starts_with("ROTATE Specify rotation angle or [Base point/Copy/Undo/Reference/eXit]"), "{}", doc.prompt());
    assert_eq!(parts(&doc)["command"], "** ROTATE **");
    assert_eq!(parts(&doc)["kind"], "angle");
    assert!(history(&doc).iter().any(|line| line == "** MOVE **") && history(&doc).iter().any(|line| line == "** ROTATE **"));
    assert!(!doc.dirty());
    doc.command_line("90").unwrap();
    assert!(doc.hot_grip.is_none(), "placed: the edit ends");
    let [(center, radius)] = circles(&doc)[..] else { panic!("one circle") };
    assert!(near(center, 130.0, 70.0) && (radius - 30.0).abs() < 1e-9, "{center:?}");
    assert_eq!(doc.session.selection().len(), 1, "the selection stays");
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty(), "one undo step");
    // Round the cycle: SCALE, MIRROR, then STRETCH again.
    doc.session.set_selection(doc.session.doc().unwrap().model.iter().map(|e| e.handle).collect());
    click(&mut doc, Vec2::new(130.0, 100.0));
    for mode in ["MOVE", "ROTATE", "SCALE", "MIRROR", "STRETCH"] {
        enter(&mut doc);
        assert!(doc.prompt().starts_with(&format!("{mode} ")), "{}", doc.prompt());
    }
    // A mode by name, and SCALE's typed factor about the grip.
    doc.command_line("scale").unwrap();
    doc.command_line("2").unwrap();
    let [(center, radius)] = circles(&doc)[..] else { panic!("one circle") };
    assert!(near(center, 70.0, 100.0) && (radius - 60.0).abs() < 1e-9, "{center:?} {radius}");
}

/// MOVE and MIRROR place at the clicked point; Base point measures from another point; Reference
/// rotates from a reference angle.
#[test]
fn move_base_point_and_reference() {
    let mut doc = selected_circle();
    click(&mut doc, Vec2::new(100.0, 100.0));
    enter(&mut doc);
    let preview = hover(&mut doc, Vec2::new(150.0, 120.0));
    assert!(preview.overlays.iter().any(|o| matches!(o, ToolOverlay::Line { color, .. } if *color == GUIDE)), "MOVE's guide from the base point");
    assert!(!doc.dirty());
    click(&mut doc, Vec2::new(150.0, 120.0));
    assert!(near(circles(&doc)[0].0, 150.0, 120.0));

    // Base point (0, 0), then the move point (10, 0): the circle moves by (10, 0).
    click(&mut doc, Vec2::new(150.0, 120.0));
    enter(&mut doc);
    doc.command_line("b").unwrap();
    assert_eq!(parts(&doc)["message"], "Specify base point");
    doc.command_line("0,0").unwrap();
    doc.command_line("10,0").unwrap();
    assert!(near(circles(&doc)[0].0, 160.0, 120.0), "{:?}", circles(&doc));

    // ROTATE about the quadrant (190, 120) with Reference 90 and new angle 180: turned by 90°.
    click(&mut doc, Vec2::new(190.0, 120.0));
    doc.command_line("rotate").unwrap();
    doc.command_line("r").unwrap();
    assert_eq!(parts(&doc)["message"], "Specify reference angle");
    doc.command_line("90").unwrap();
    assert_eq!(parts(&doc)["message"], "Specify new angle");
    doc.command_line("180").unwrap();
    assert!(near(circles(&doc)[0].0, 190.0, 90.0), "{:?}", circles(&doc));
}

/// Ctrl (or Cmd) held while placing places a copy and the edit goes on placing copies; the Copy
/// option does the same; Undo takes the last copy back; Enter after copies ends the edit.
#[test]
fn copies_with_ctrl_or_the_copy_option() {
    let mut doc = selected_circle();
    click(&mut doc, Vec2::new(100.0, 100.0));
    click_with(&mut doc, Vec2::new(200.0, 100.0), ToolMods { control: true, ..ToolMods::default() });
    assert_eq!(circles(&doc).len(), 2, "Ctrl placed a copy");
    assert!(circles(&doc).iter().any(|(c, _)| near(*c, 100.0, 100.0)), "the original stays");
    assert!(circles(&doc).iter().any(|(c, _)| near(*c, 200.0, 100.0)));
    assert!(doc.hot_grip.is_some(), "the edit goes on placing copies");
    assert_eq!(parts(&doc)["grip"]["copy"], true);
    click(&mut doc, Vec2::new(300.0, 100.0));
    assert_eq!(circles(&doc).len(), 3);
    doc.command_line("u").unwrap();
    assert_eq!(circles(&doc).len(), 2, "Undo takes the last copy back");
    enter(&mut doc);
    assert!(doc.hot_grip.is_none(), "Enter after copies ends the edit");

    // Cmd (a Mac) and a STRETCH copy of a quadrant: a bigger circle beside the original.
    let mut doc = selected_circle();
    click(&mut doc, Vec2::new(130.0, 100.0));
    click_with(&mut doc, Vec2::new(150.0, 100.0), ToolMods { command: true, ..ToolMods::default() });
    let mut found = circles(&doc);
    found.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_eq!(found, vec![(Vec2::new(100.0, 100.0), 30.0), (Vec2::new(100.0, 100.0), 50.0)]);

    // The Copy option, typed, in MOVE.
    let mut doc = selected_circle();
    click(&mut doc, Vec2::new(100.0, 100.0));
    enter(&mut doc);
    doc.command_line("c").unwrap();
    click(&mut doc, Vec2::new(100.0, 200.0));
    assert_eq!(circles(&doc).len(), 2);
    doc.command_line("x").unwrap();
    assert!(doc.hot_grip.is_none(), "eXit ends the edit");
}

/// Shift+click makes grips hot without editing; a plain click on one of them stretches all of them.
#[test]
fn shift_makes_several_grips_hot() {
    let mut doc = drawing(&[("line.add", json!({ "points": [[0, 0], [100, 0]] }))]);
    click(&mut doc, Vec2::new(50.0, 0.0));
    let shift = ToolMods { shift: true, ..ToolMods::default() };
    click_with(&mut doc, Vec2::new(0.0, 0.0), shift);
    assert!(doc.hot_grip.is_none(), "Shift makes it hot and waits");
    assert_eq!(doc.session.selection().len(), 1, "Shift on a grip does not deselect");
    let marked = hover(&mut doc, Vec2::new(0.0, 0.0));
    assert!(anchors(&marked, grip_edit::GRIP_HOT) >= 1);
    click(&mut doc, Vec2::new(100.0, 0.0));
    assert!(doc.hot_grip.is_some(), "a plain click on a grip that was not hot starts on it alone");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.session.selection().len(), 1, "Escape cools the grips first");
    click_with(&mut doc, Vec2::new(0.0, 0.0), shift);
    click_with(&mut doc, Vec2::new(100.0, 0.0), shift);
    click(&mut doc, Vec2::new(100.0, 0.0));
    hover(&mut doc, Vec2::new(110.0, 20.0));
    click(&mut doc, Vec2::new(110.0, 20.0));
    let [(a, b)] = lines(&doc)[..] else { panic!("one line") };
    assert!(near(a, 10.0, 20.0) && near(b, 110.0, 20.0), "both hot ends moved: {a:?} {b:?}");
}

/// The grip's own options: a line's end lengthens, a polyline's vertex is removed or gets a new
/// vertex after it; the grip menu starts them with `grip.edit`.
#[test]
fn grip_options_and_the_grip_menu() {
    let mut doc = drawing(&[("line.add", json!({ "points": [[0, 0], [100, 0]] }))]);
    click(&mut doc, Vec2::new(50.0, 0.0));
    let info = grip_info(&hover(&mut doc, Vec2::new(100.0, 0.0))).unwrap();
    assert_eq!(info["actions"], json!([{ "label": "Lengthen", "keyword": "Lengthen" }]));
    let handle = info["handle"].as_str().unwrap().to_owned();
    click(&mut doc, Vec2::new(100.0, 0.0));
    assert!(doc.prompt().contains("[Base point/Copy/Undo/Lengthen/eXit]"), "{}", doc.prompt());
    doc.command_line("l").unwrap();
    assert_eq!(parts(&doc)["command"], "** LENGTHEN **");
    let preview = hover(&mut doc, Vec2::new(140.0, 30.0));
    assert!(preview.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { color, .. } if *color == ACCENT)), "the lengthened line previews");
    click(&mut doc, Vec2::new(140.0, 30.0));
    assert!(doc.hot_grip.is_none());
    let [(a, b)] = lines(&doc)[..] else { panic!("one line") };
    assert!(near(a, 0.0, 0.0) && near(b, 140.0, 0.0), "lengthened along itself: {b:?}");
    // From the grip menu, with a typed change.
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 2, "option": "Lengthen" })).unwrap();
    doc.command_line("-40").unwrap();
    assert!(near(lines(&doc)[0].1, 100.0, 0.0), "{:?}", lines(&doc));
    // The grip menu's modes.
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 1, "mode": "rotate" })).unwrap();
    assert!(doc.prompt().starts_with("ROTATE "));
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();

    let mut doc = drawing(&[("polyline.add", json!({ "points": [[0, 0], [100, 0], [100, 100]] }))]);
    click(&mut doc, Vec2::new(50.0, 0.0));
    let handle = doc.session.selection()[0].hex();
    // Grips: vertex 0, mid 0, vertex 1, mid 1, vertex 2.
    let info = grip_info(&hover(&mut doc, Vec2::new(100.0, 0.0))).unwrap();
    assert_eq!(info["index"], 2);
    assert_eq!(
        info["actions"].as_array().unwrap().iter().map(|a| a["keyword"].as_str().unwrap()).collect::<Vec<_>>(),
        ["Add vertex", "Remove vertex"]
    );
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 2, "option": "Remove vertex" })).unwrap();
    assert_eq!(polyline_vertices(&doc), vec![Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0)]);
    assert!(doc.hot_grip.is_none());
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(polyline_vertices(&doc).len(), 3);
    // Add vertex on the first segment (its midpoint grip).
    doc.session.set_selection(vec![Handle::parse_hex(&handle).unwrap()]);
    doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 1, "option": "Add vertex" })).unwrap();
    assert_eq!(parts(&doc)["command"], "** ADD VERTEX **");
    click(&mut doc, Vec2::new(50.0, -20.0));
    assert_eq!(polyline_vertices(&doc), vec![Vec2::new(0.0, 0.0), Vec2::new(50.0, -20.0), Vec2::new(100.0, 0.0), Vec2::new(100.0, 100.0)]);
    // A two-vertex polyline's vertex is not removed.
    let mut doc = drawing(&[("polyline.add", json!({ "points": [[0, 0], [100, 0]] }))]);
    click(&mut doc, Vec2::new(50.0, 0.0));
    let handle = doc.session.selection()[0].hex();
    assert!(doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 0, "option": "Remove vertex" })).is_err());
    assert!(doc.hot_grip.is_none() && !doc.dirty());
}

/// The grip menu's command refuses what is not a selected, unlocked object's grip, and nothing
/// hostile panics; a refused entry leaves the hot grip as it was.
#[test]
fn grip_edit_refuses_hostile_calls() {
    let mut doc = selected_circle();
    let handle = doc.session.selection()[0].hex();
    for params in [
        json!({}),
        json!({ "handle": "zz", "index": 0 }),
        json!({ "handle": handle, "index": 99 }),
        json!({ "handle": handle, "index": u64::MAX }),
        json!({ "handle": handle, "index": -1 }),
        json!({ "handle": handle, "index": 0, "mode": "spin" }),
        json!({ "handle": handle, "index": 0, "option": "Lengthen" }),
        json!({ "handle": "FFFFFF", "index": 0 }),
    ] {
        assert!(doc.execute(grip_edit::EDIT, params.clone()).is_err(), "{params}");
        assert!(doc.hot_grip.is_none(), "{params}");
    }
    assert!(!doc.dirty());
    click(&mut doc, Vec2::new(100.0, 100.0));
    for text in ["nowhere", "1e999,0", "@nan,0", "<<<", "∞"] {
        assert!(doc.command_line(text).is_err(), "{text}");
        assert!(doc.hot_grip.is_some(), "{text}");
    }
    doc.command_line("rotate").unwrap();
    assert!(doc.command_line("1e999").is_err());
    doc.command_line("scale").unwrap();
    assert!(doc.command_line("-2").is_err() && doc.command_line("0").is_err());
    assert!(doc.command_line("u").is_err(), "nothing to undo");
    assert!(!doc.dirty());
    doc.session.set_selection(Vec::new());
    let unselected = doc.execute(grip_edit::EDIT, json!({ "handle": handle, "index": 0 }));
    assert!(unselected.is_err());
}
