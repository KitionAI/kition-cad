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

/// The dynamic input boxes' live values in a tool answer, read as the editor reads them.
fn live(response: &ToolResponse) -> Option<Value> {
    response.overlays.iter().find_map(|overlay| match overlay {
        ToolOverlay::Label { text, color, .. } if *color == dynamic_input::DYNAMIC => serde_json::from_str(text).ok(),
        _ => None,
    })
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

/// A hot grip has the dynamic input boxes (before, they showed only while a command ran): STRETCH
/// and MOVE take a distance and an angle from the grip, ROTATE an angle and SCALE a factor, with
/// the live values beside the cursor; a locked distance holds the grip's point, Enter sends the
/// boxes as `@d<a` from the grip, and a plain number goes that far toward the cursor.
#[test]
fn hot_grips_have_the_dynamic_input_boxes() {
    let mut doc = drawing(&[("circle.add", json!({ "center": [100, 100], "radius": 30 }))]);
    doc.execute("drafting.dynamicInput", json!({ "on": true })).unwrap();
    click(&mut doc, Vec2::new(130.0, 100.0));
    assert_eq!(doc.session.selection().len(), 1);
    // The right quadrant's grip, hot.
    click(&mut doc, Vec2::new(130.0, 100.0));
    assert!(doc.hot_grip.is_some());
    assert_eq!(parts(&doc)["kind"], "point");
    assert_eq!(parts(&doc)["base"], json!([130.0, 100.0]));
    let values = live(&hover(&mut doc, Vec2::new(140.0, 100.0))).expect("the live values");
    assert_eq!(values["relative"], true);
    assert_eq!(values["polar"], json!(["10.0000", "0°"]), "{values}");
    // A locked distance holds the grip's point to it; the click places it there.
    let answer = doc.execute(dynamic_input::SET_ENTRY, json!({ "text": "20<", "secondLocked": false })).unwrap();
    assert_eq!(answer["holds"], true, "{answer}");
    hover(&mut doc, Vec2::new(135.0, 100.0));
    assert!(doc.aim.is_some_and(|aim| near(aim, Vec2::new(150.0, 100.0))), "{:?}", doc.aim);
    click(&mut doc, Vec2::new(135.0, 100.0));
    assert_eq!(circles(&doc), vec![(Vec2::new(100.0, 100.0), 50.0)]);
    // Enter sends the boxes from the grip: `@d<a`.
    click(&mut doc, Vec2::new(150.0, 100.0));
    assert!(doc.hot_grip.is_some());
    let answer = doc.execute(dynamic_input::SET_ENTRY, json!({ "text": "10<0", "secondLocked": true })).unwrap();
    assert_eq!(answer["complete"], "@10<0", "{answer}");
    doc.command_line("@10<0").unwrap();
    assert_eq!(circles(&doc), vec![(Vec2::new(100.0, 100.0), 60.0)]);
    // A plain number: direct distance toward the cursor.
    click(&mut doc, Vec2::new(160.0, 100.0));
    hover(&mut doc, Vec2::new(200.0, 100.0));
    doc.command_line("5").unwrap();
    assert_eq!(circles(&doc), vec![(Vec2::new(100.0, 100.0), 65.0)]);
    // ROTATE takes an angle, SCALE a factor: one box each, its live value from the grip.
    click(&mut doc, Vec2::new(165.0, 100.0));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(parts(&doc)["kind"], "angle");
    let values = live(&hover(&mut doc, Vec2::new(165.0, 120.0))).expect("ROTATE's live angle");
    assert_eq!(values["polar"][1], "90°", "{values}");
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(parts(&doc)["kind"], "distance");
    let values = live(&hover(&mut doc, Vec2::new(167.0, 100.0))).expect("SCALE's live factor");
    assert_eq!(values["polar"][0], "2.0000", "{values}");
    // Off (F12), no boxes.
    doc.execute("drafting.dynamicInput", json!({ "on": false })).unwrap();
    assert!(live(&hover(&mut doc, Vec2::new(167.0, 100.0))).is_none());
}

fn selected(doc: &CadDocument) -> Vec<Handle> {
    let mut out = doc.session.selection();
    out.sort();
    out
}

fn sorted(mut handles: Vec<Handle>) -> Vec<Handle> {
    handles.sort();
    handles
}

/// The drawing menu's Select Similar (`select.similar`): the objects of the selected ones' type and
/// layer, in the space shown; only the selection changes (no undo step, the drawing clean); with
/// nothing selected it says what to do.
#[test]
fn select_similar_takes_the_type_and_the_layer() {
    let mut doc = drawing(&[
        ("layer.add", json!({ "name": "Walls" })),
        ("line.add", json!({ "points": [[0, 0], [10, 0]], "layer": "Walls" })),
        ("line.add", json!({ "points": [[0, 20], [10, 20]], "layer": "Walls" })),
        ("line.add", json!({ "points": [[0, 40], [10, 40]] })),
        ("circle.add", json!({ "center": [50, 50], "radius": 5, "layer": "Walls" })),
    ]);
    let all: Vec<(Handle, String, String)> =
        doc.session.doc().unwrap().model.iter().map(|e| (e.handle, e.kind.type_name().to_owned(), e.common.layer.clone())).collect();
    let walls_lines: Vec<Handle> = all.iter().filter(|(_, t, l)| t == "Line" && l == "Walls").map(|(h, ..)| *h).collect();
    assert_eq!(walls_lines.len(), 2);
    assert!(doc.execute(selection::SIMILAR, json!({})).is_err(), "nothing selected");
    click(&mut doc, Vec2::new(5.0, 0.0));
    assert_eq!(doc.session.selection().len(), 1);
    let steps = undo_steps(&doc);
    let answer = doc.execute(selection::SIMILAR, json!({})).unwrap();
    assert_eq!(answer["selected"], 2, "{answer}");
    assert_eq!(selected(&doc), sorted(walls_lines.clone()));
    assert_eq!(undo_steps(&doc), steps, "only the selection changes");
    assert!(!doc.dirty());
    assert!(!doc.commands().into_iter().find(|c| c.id == selection::SIMILAR).unwrap().mutates);
    // A line and the circle: both kinds on Walls.
    click(&mut doc, Vec2::new(55.0, 50.0));
    doc.execute(selection::SIMILAR, json!({})).unwrap();
    assert_eq!(doc.session.selection().len(), 3);
}

/// "Select objects" lists the selection modes it knows as options (ALL, Last, Previous, Add,
/// Remove), typed as before; the engine has no W, C, F, WP or CP, which are not offered.
#[test]
fn select_objects_offers_its_modes() {
    let mut doc = drawing(&[("line.add", json!({ "points": [[0, 0], [10, 0]] })), ("line.add", json!({ "points": [[0, 20], [10, 20]] }))]);
    doc.command_line("move").unwrap();
    assert_eq!(parts(&doc)["kind"], "select");
    assert_eq!(parts(&doc)["keywords"], json!(["ALL", "Last", "Previous", "Add", "Remove"]));
    doc.command_line("ALL").unwrap();
    assert_eq!(doc.session.selection().len(), 2);
    doc.command_line("r").unwrap();
    assert_eq!(parts(&doc)["message"], "Remove objects");
    click(&mut doc, Vec2::new(5.0, 20.0));
    assert_eq!(doc.session.selection().len(), 1);
    doc.command_line("Add").unwrap();
    doc.command_line("l").unwrap();
    assert_eq!(doc.session.selection().len(), 2, "{:?}", history(&doc));
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
}

/// A group is picked as one (PICKSTYLE 1, the default): a click on a member selects the group, at
/// "Select objects" too, Shift+click takes it out again, the rollover highlight shows the whole
/// group, and an unselectable group or PICKSTYLE 0 picks the member alone.
#[test]
fn groups_are_picked_as_one() {
    let mut doc = drawing(&[
        ("line.add", json!({ "points": [[0, 0], [10, 0]] })),
        ("line.add", json!({ "points": [[0, 20], [10, 20]] })),
        ("line.add", json!({ "points": [[0, 40], [10, 40]] })),
    ]);
    let handles: Vec<Handle> = doc.session.doc().unwrap().model.iter().map(|e| e.handle).collect();
    let pair = vec![handles[0].hex(), handles[1].hex()];
    doc.execute("arrange.group", json!({ "name": "Pair", "handles": pair })).unwrap();
    doc.session.set_selection(Vec::new());
    // The highlight: the member under the pointer and the rest of its group.
    let alone = hover(&mut doc, Vec2::new(5.0, 40.0)).overlays.iter().filter(|o| matches!(o, ToolOverlay::Path { .. })).count();
    let group = hover(&mut doc, Vec2::new(5.0, 0.0)).overlays.iter().filter(|o| matches!(o, ToolOverlay::Path { .. })).count();
    assert!(group > alone, "the group's two lines highlight: {group} paths against {alone}");
    click(&mut doc, Vec2::new(5.0, 0.0));
    assert_eq!(selected(&doc), sorted(handles[..2].to_vec()));
    let shift = ToolMods { shift: true, ..ToolMods::default() };
    // (Off the grips, which Shift makes hot.)
    click_with(&mut doc, Vec2::new(2.0, 20.0), shift);
    assert!(doc.session.selection().is_empty(), "Shift takes the group out");
    // At "Select objects".
    doc.command_line("erase").unwrap();
    click(&mut doc, Vec2::new(5.0, 20.0));
    assert_eq!(selected(&doc), sorted(handles[..2].to_vec()));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.session.doc().unwrap().model.iter().count(), 1, "the group erased together");
    doc.execute("edit.undo", json!({})).unwrap();
    // PICKSTYLE 0: the member alone.
    doc.execute("variable.set", json!({ "name": "PICKSTYLE", "value": 0 })).unwrap();
    doc.session.set_selection(Vec::new());
    click(&mut doc, Vec2::new(5.0, 0.0));
    assert_eq!(selected(&doc), vec![handles[0]]);
    // A group made unselectable: the member alone too.
    doc.execute("variable.set", json!({ "name": "PICKSTYLE", "value": 1 })).unwrap();
    doc.execute("arrange.group", json!({ "name": "Loose", "handles": [handles[2].hex(), handles[1].hex()], "selectable": false })).unwrap();
    doc.session.set_selection(Vec::new());
    click(&mut doc, Vec2::new(5.0, 40.0));
    assert_eq!(selected(&doc), vec![handles[2]]);
}
