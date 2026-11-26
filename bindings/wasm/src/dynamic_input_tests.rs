//! Dynamic input (N2): the base point and the mode the editor reads, a value locked in the boxes
//! holding the cursor, the text Enter sends for a box entry, and the live values beside the cursor.
use super::*;
use kition_cad_doc::EntityKind;

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("Dynamic.dxf", 0, 0).unwrap();
    // No running object snaps or polar tracking: only the boxes move the cursor.
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    doc.execute("drafting.polar", json!({ "on": false })).unwrap();
    doc
}

fn event(doc: &mut CadDocument, action: PointerAction, point: Vec2) -> ToolResponse {
    let (x, y) = doc.placement().to_doc(point);
    doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap()
}

fn hover(doc: &mut CadDocument, point: Vec2) -> ToolResponse {
    event(doc, PointerAction::Move, point)
}

fn click(doc: &mut CadDocument, point: Vec2) {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
        event(doc, action, point);
    }
}

fn entry(doc: &mut CadDocument, text: &str, second_locked: bool) -> Value {
    doc.execute(dynamic_input::SET_ENTRY, json!({ "text": text, "secondLocked": second_locked })).unwrap()
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

fn near(a: Vec2, b: Vec2) -> bool {
    a.dist(b) < 1e-9
}

/// The live values' label in a tool answer, read as the editor reads it.
fn live(response: &ToolResponse) -> Option<Value> {
    response.overlays.iter().find_map(|overlay| match overlay {
        ToolOverlay::Label { text, color, .. } if *color == dynamic_input::DYNAMIC => serde_json::from_str(text).ok(),
        _ => None,
    })
}

/// LINE's second point starting at (10, 10).
fn second_point() -> CadDocument {
    let mut doc = blank();
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    doc
}

/// inspect() tells the editor whether dynamic input is on and the prompt's base point (the last
/// point), under the existing `/drafting` entry.
#[test]
fn the_editor_reads_the_mode_and_the_base_point() {
    let mut doc = blank();
    assert_eq!(doc.inspect().unwrap()["drafting"]["dynamicInput"], json!(true), "DYNMODE is on by default");
    doc.select_tool("line.add", 1.0).unwrap();
    let prompt = |doc: &CadDocument| doc.inspect().unwrap()["drafting"]["commandLine"]["prompt"].clone();
    assert_eq!(prompt(&doc)["base"], Value::Null, "the first point has no base");
    click(&mut doc, Vec2::new(10.0, 10.0));
    assert_eq!(prompt(&doc)["base"], json!([10.0, 10.0]));
    doc.execute("drafting.dynamicInput", json!({ "on": false })).unwrap();
    assert_eq!(doc.inspect().unwrap()["drafting"]["dynamicInput"], json!(false));
}

/// `50<` locks the distance: the cursor only turns the direction, the preview and the click keep
/// the length, and the click uses the entry up.
#[test]
fn a_locked_distance_holds_the_cursor() {
    let mut doc = second_point();
    let answer = entry(&mut doc, "50<", false);
    assert_eq!((answer["entry"].clone(), answer["format"].clone(), answer["holds"].clone()), (json!(true), json!("polar"), json!(true)), "{answer}");
    hover(&mut doc, Vec2::new(70.0, 90.0));
    let aim = doc.aim.unwrap();
    assert!(near(aim, Vec2::new(40.0, 50.0)), "{aim:?}");
    hover(&mut doc, Vec2::new(200.0, 10.0));
    assert!(near(doc.aim.unwrap(), Vec2::new(60.0, 10.0)), "{:?}", doc.aim);
    click(&mut doc, Vec2::new(10.0, 300.0));
    assert!(near(lines(&doc)[0].1, Vec2::new(10.0, 60.0)), "{:?}", lines(&doc));
    assert_eq!(doc.dynamic_entry, dynamic_input::DynamicEntry::default(), "the click used the entry up");
    hover(&mut doc, Vec2::new(30.0, 70.0));
    assert!(near(doc.aim.unwrap(), Vec2::new(30.0, 70.0)), "nothing holds the next point");
}

/// `20,` locks the relative X: the cursor only moves Y. A Tab-locked second value alone holds the
/// direction (`<90`), and both locked pin the point.
#[test]
fn locked_values_hold_x_y_and_the_direction() {
    let mut doc = second_point();
    entry(&mut doc, "20,", false);
    hover(&mut doc, Vec2::new(100.0, 44.0));
    assert!(near(doc.aim.unwrap(), Vec2::new(30.0, 44.0)), "{:?}", doc.aim);
    entry(&mut doc, "<90", true);
    hover(&mut doc, Vec2::new(13.0, 14.0));
    assert!(doc.aim.unwrap().dist(Vec2::new(10.0, 15.0)) < 1e-9, "{:?}", doc.aim);
    entry(&mut doc, "<90", false);
    hover(&mut doc, Vec2::new(13.0, 14.0));
    assert!(near(doc.aim.unwrap(), Vec2::new(13.0, 14.0)), "an angle typed but not locked does not hold");
    entry(&mut doc, "20<90", true);
    hover(&mut doc, Vec2::new(300.0, 300.0));
    assert!(doc.aim.unwrap().dist(Vec2::new(10.0, 30.0)) < 1e-9, "{:?}", doc.aim);
    // A held point wins over an object snap.
    doc.execute("drafting.objectSnap", json!({ "on": true })).unwrap();
    entry(&mut doc, "5,", false);
    hover(&mut doc, Vec2::new(10.0, 40.0));
    assert!(doc.snapped.is_none() && near(doc.aim.unwrap(), Vec2::new(15.0, 40.0)), "{:?} {:?}", doc.snapped, doc.aim);
}

/// Enter sends what the boxes show measured from the last point: typed values as typed, an empty
/// box from the cursor; a plain number stays direct distance; a first point is absolute.
#[test]
fn enter_sends_the_point_the_boxes_show() {
    let mut doc = second_point();
    let answer = entry(&mut doc, "50<30", false);
    assert_eq!(answer["complete"], json!("@50<30"));
    doc.command_line("@50<30").unwrap();
    let end = lines(&doc)[0].1;
    assert!(end.dist(Vec2::new(10.0, 10.0) + Vec2::from_angle(30f64.to_radians()) * 50.0) < 1e-9, "{end:?}");
    assert_eq!(doc.dynamic_entry, dynamic_input::DynamicEntry::default(), "the sent entry is used up");
    // The distance typed, the angle from the cursor (straight up from the new base).
    hover(&mut doc, end + Vec2::new(0.0, 25.0));
    let answer = entry(&mut doc, "40<", false);
    let text = answer["complete"].as_str().unwrap().to_owned();
    assert!(text.starts_with("@40<<"), "{text}");
    doc.command_line(&text).unwrap();
    assert!(lines(&doc)[1].1.dist(end + Vec2::new(0.0, 40.0)) < 1e-9, "{:?}", lines(&doc));
    // Y typed, X from the cursor.
    let base = lines(&doc)[1].1;
    hover(&mut doc, base + Vec2::new(7.0, 100.0));
    let answer = entry(&mut doc, ",3", false);
    assert_eq!(answer["format"], json!("cartesian"));
    doc.command_line(answer["complete"].as_str().unwrap()).unwrap();
    assert!(lines(&doc)[2].1.dist(base + Vec2::new(7.0, 3.0)) < 1e-9, "{:?}", lines(&doc));
    // A plain number is direct distance entry: sent as typed.
    assert_eq!(entry(&mut doc, "12", false)["complete"], Value::Null);
    // Not box entries: sent as typed.
    for text in ["@5,5", "#5,5", "u", "close", "1,2,3"] {
        assert_eq!(entry(&mut doc, text, false)["entry"], json!(false), "{text}");
    }
    // A first point is absolute: both values typed go as typed, an empty one from the cursor.
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.dynamic_entry, dynamic_input::DynamicEntry::default(), "Escape forgets the entry");
    doc.select_tool("line.add", 1.0).unwrap();
    hover(&mut doc, Vec2::new(3.0, 7.0));
    assert_eq!(entry(&mut doc, "10,20", false)["complete"], Value::Null);
    let answer = entry(&mut doc, "10,", false);
    assert_eq!((answer["relative"].clone(), answer["complete"].clone()), (json!(false), json!("10,7")), "{answer}");
}

/// The live values come with the cursor's overlays at a point prompt, dynamic input on: X and Y
/// from the base and the distance and angle from it; none between commands or with it off.
#[test]
fn the_live_values_follow_the_cursor() {
    let mut doc = second_point();
    let values = live(&hover(&mut doc, Vec2::new(13.0, 14.0))).unwrap();
    assert_eq!(values["cartesian"], json!(["3.0000", "4.0000"]), "{values}");
    assert_eq!(values["polar"][0], json!("5.0000"), "{values}");
    assert_eq!(values["polar"][1], json!("53.1°"), "{values}");
    assert_eq!(values["relative"], json!(true));
    // A value prompt that takes a point too (CIRCLE's radius) has them, for its one box.
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.select_tool("circle.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(0.0, 0.0));
    let values = live(&hover(&mut doc, Vec2::new(0.0, -8.0))).unwrap();
    assert_eq!((values["polar"][0].clone(), values["polar"][1].clone()), (json!("8.0000"), json!("270°")), "{values}");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(live(&hover(&mut doc, Vec2::new(5.0, 5.0))).is_none(), "nothing between commands");
    doc.execute("drafting.dynamicInput", json!({ "on": false })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    assert!(live(&hover(&mut doc, Vec2::new(5.0, 5.0))).is_none(), "nothing with dynamic input off");
}

/// With dynamic input off (F12) nothing typed holds the cursor; the entry does not end a hot grip;
/// hostile text does not panic and is cut.
#[test]
fn off_grips_and_hostile_entries() {
    let mut doc = second_point();
    doc.execute("drafting.dynamicInput", json!({ "on": false })).unwrap();
    let answer = entry(&mut doc, "50<", false);
    assert_eq!(answer["entry"], json!(false));
    hover(&mut doc, Vec2::new(70.0, 90.0));
    assert!(near(doc.aim.unwrap(), Vec2::new(70.0, 90.0)));
    doc.execute("drafting.dynamicInput", json!({ "on": true })).unwrap();
    for text in ["1e400<", "<1e400", "nan,nan", "inf<inf", ",", "<", "1,<2", "--1,2", &"9".repeat(10_000)] {
        entry(&mut doc, text, true);
        hover(&mut doc, Vec2::new(70.0, 90.0));
        assert!(doc.aim.unwrap().is_finite(), "{text}");
    }
    assert!(doc.dynamic_entry.text.chars().count() <= 200);
    assert!(doc.execute(dynamic_input::SET_ENTRY, json!({})).is_err(), "text is required");
    // A hot grip stays hot: the entry is no command.
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    for text in ["line", "0,0", "30,0", ""] {
        doc.command_line(text).unwrap();
    }
    click(&mut doc, Vec2::new(15.0, 0.0));
    click(&mut doc, Vec2::new(30.0, 0.0));
    assert!(doc.hot_grip.is_some(), "a click on the selected line's end grip makes it hot");
    entry(&mut doc, "5", false);
    assert!(doc.hot_grip.is_some());
    assert!(!doc.commands().into_iter().find(|c| c.id == dynamic_input::SET_ENTRY).unwrap().mutates);
}
