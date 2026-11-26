//! Editing in place (walkthrough N12): a double-click on a text, a paragraph, a dimension or a
//! hatch asks the editor to edit it there; TEXT and MTEXT say where their text goes; HATCH
//! previews the area under the cursor.
use super::*;
use kition_cad_doc::EntityKind;

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("InPlace.dxf", 0, 0).unwrap();
    doc.session.settings.osmode = 0;
    doc
}

fn event(doc: &mut CadDocument, action: PointerAction, point: Vec2) {
    let (x, y) = doc.placement().to_doc(point);
    doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap();
}

/// What a browser sends for a double-click: two clicks, then the double-click.
fn double_click(doc: &mut CadDocument, point: Vec2) {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up, PointerAction::Down, PointerAction::Up, PointerAction::DoubleClick] {
        event(doc, action, point);
    }
}

fn edit(doc: &CadDocument) -> Value {
    doc.inspect().unwrap()["drafting"]["edit"].clone()
}

fn prompt(doc: &CadDocument) -> Value {
    doc.inspect().unwrap()["drafting"]["commandLine"]["prompt"].clone()
}

#[test]
fn a_double_click_on_text_asks_to_edit_it_where_it_is() {
    let mut doc = blank();
    let text = doc.execute("text.add", json!({ "at": [10, 10], "height": 5, "text": "Kitchen" })).unwrap()["handle"].clone();
    double_click(&mut doc, Vec2::new(12.0, 12.0));
    let request = edit(&doc);
    assert_eq!(request["kind"], "text", "{request}");
    assert_eq!(request["handle"], text);
    assert_eq!(request["text"], "Kitchen");
    assert_eq!(request["serial"], 1);
    assert_eq!(request["box"]["height"], 5.0);
    let at = request["box"]["at"].as_array().unwrap();
    assert!((at[0].as_f64().unwrap() - 10.0).abs() < 1.0 && (at[1].as_f64().unwrap() - 10.0).abs() < 2.0, "{at:?}");
    assert!(doc.hot_grip.is_none(), "the second click's grip goes cold");
    // The editor writes it back in one step.
    doc.execute("text.edit", json!({ "handle": text, "text": "Dining" })).unwrap();
    assert!(doc.session.doc().unwrap().model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "Dining")));
    // Another double-click is another request.
    double_click(&mut doc, Vec2::new(12.0, 12.0));
    assert_eq!(edit(&doc)["serial"], 2);
    assert_eq!(edit(&doc)["text"], "Dining");
}

#[test]
fn paragraphs_dimensions_and_hatches_open_their_own_editors() {
    let mut doc = blank();
    doc.execute("multilineText.add", json!({ "at": [0, 100], "text": "One\\PTwo", "height": 5, "width": 60 })).unwrap();
    double_click(&mut doc, Vec2::new(5.0, 97.0));
    let request = edit(&doc);
    assert_eq!(request["kind"], "paragraphs", "{request}");
    assert_eq!(request["text"], "One\nTwo");
    assert_eq!(request["box"]["paragraphs"], true);
    assert_eq!(request["box"]["width"], 60.0);

    doc.execute("dimension.addLinear", json!({ "p1": [0, 0], "p2": [80, 0], "at": [40, -20] })).unwrap();
    let mid = {
        let d = doc.session.doc().unwrap();
        d.model
            .iter()
            .find_map(|e| if let EntityKind::Dimension(k) = &e.kind { Some(kition_cad_render::dimension_in(d, k).text_pos) } else { None })
            .unwrap()
    };
    doc.session.set_selection(Vec::new());
    let dimension = doc.session.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    // A point on the text's strokes, as a person double-clicks it.
    let on_text = (-40..40)
        .flat_map(|i| (-10..10).map(move |j| mid + Vec2::new(f64::from(i) * 0.25, f64::from(j) * 0.25)))
        .find(|p| doc.session.hover(*p, doc.session.pick_aperture()) == Some(dimension))
        .expect("a point that picks the dimension");
    double_click(&mut doc, on_text);
    let request = edit(&doc);
    assert_eq!(request["kind"], "dimension", "{request}");
    assert_eq!(request["text"], "<>", "the measurement stands as <>");
    assert_eq!(request["box"]["centered"], true);
    let at = request["box"]["at"].as_array().unwrap();
    assert!((at[0].as_f64().unwrap() - 40.0).abs() < 1.0 && (at[1].as_f64().unwrap() + 20.0).abs() < 5.0, "over the dimension line: {at:?}");

    let rectangle = doc.execute("rectangle.add", json!({ "p1": [200, 0], "p2": [260, 40] })).unwrap()["handle"].clone();
    doc.execute("hatch.add", json!({ "points": [[230, 20]], "pattern": "ANSI31", "scale": 2, "angle": 15 })).unwrap();
    // The hatch alone: a patterned hatch is picked by its boundary, which the rectangle shared.
    doc.execute("object.delete", json!({ "handles": [rectangle] })).unwrap();
    doc.session.set_selection(Vec::new());
    let hatch = doc.session.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Hatch(_))).unwrap().handle;
    let at = (200..260)
        .flat_map(|x| (0..40).map(move |y| Vec2::new(f64::from(x), f64::from(y) + 0.5)))
        .find(|p| doc.session.hover(*p, doc.session.pick_aperture()) == Some(hatch))
        .expect("a point that picks the hatch");
    double_click(&mut doc, at);
    let request = edit(&doc);
    assert_eq!(request["kind"], "hatch", "{request}");
    assert_eq!(request["pattern"], "ANSI31");
    assert_eq!(request["scale"], 2.0);
    assert!((request["angle"].as_f64().unwrap() - 15.0).abs() < 1e-9);
}

#[test]
fn a_double_click_elsewhere_or_in_a_command_asks_nothing() {
    let mut doc = blank();
    doc.execute("circle.add", json!({ "center": [0, 0], "radius": 10 })).unwrap();
    double_click(&mut doc, Vec2::new(10.0, 0.0));
    assert!(edit(&doc).is_null(), "a circle has no in-place edit");
    doc.execute("text.add", json!({ "at": [100, 100], "height": 5, "text": "A" })).unwrap();
    doc.execute("command.runLine", json!({ "text": "line" })).unwrap();
    double_click(&mut doc, Vec2::new(101.0, 101.0));
    assert!(edit(&doc).is_null(), "in a command a double-click is two points");
}

#[test]
fn text_and_mtext_say_where_their_text_goes() {
    let mut doc = blank();
    for entry in ["text", "20,30", "4", "0"] {
        doc.execute("command.runLine", json!({ "text": entry })).unwrap();
    }
    let place = prompt(&doc)["textBox"].clone();
    assert_eq!(place["at"], json!([20.0, 30.0]), "{place}");
    assert_eq!(place["height"], 4.0);
    assert_eq!(place["paragraphs"], false);
    doc.execute("command.runLine", json!({ "text": "First" })).unwrap();
    let next = prompt(&doc)["textBox"]["at"].clone();
    assert!(next[1].as_f64().unwrap() < 30.0 - 4.0, "the next line goes below: {next}");
    doc.execute("command.runLine", json!({ "text": "" })).unwrap();
    assert!(prompt(&doc).get("textBox").is_none());

    for entry in ["mtext", "0,100", "60,80"] {
        doc.execute("command.runLine", json!({ "text": entry })).unwrap();
    }
    let place = prompt(&doc)["textBox"].clone();
    assert_eq!(place["paragraphs"], true, "{place}");
    assert_eq!(place["at"], json!([0.0, 100.0]));
    assert_eq!(place["width"], 60.0);
    // The editor sends the lines joined by \P, then Enter.
    doc.execute("command.runLine", json!({ "text": "Line one\\PLine two" })).unwrap();
    doc.execute("command.runLine", json!({ "text": "" })).unwrap();
    assert!(doc.session.doc().unwrap().model.iter().any(|e| matches!(&e.kind, EntityKind::MText(t) if t.contents == "Line one\\PLine two")));
}

#[test]
fn hatch_previews_the_area_under_the_cursor() {
    let mut session = kition_cad_engine::Session::new();
    session.execute("rectangle.add", &json!({ "p1": [0, 0], "p2": [10, 10] })).unwrap();
    session.start("hatch.add").unwrap();
    if session.current_prompt().is_some_and(|p| p.accept.select) {
        session.cmdline("k").unwrap();
    }
    assert!(!session.preview(Vec2::new(5.0, 5.0)).is_empty(), "inside the rectangle");
    assert!(session.preview(Vec2::new(50.0, 50.0)).is_empty(), "outside any boundary");
}
