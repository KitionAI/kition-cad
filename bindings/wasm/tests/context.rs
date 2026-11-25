//! Explicit call context (Kit Process Protocol section 7a; task card C4/C5/C6-cad): the editor state
//! a command reads besides its parameters (the space shown and MSPACE, the views, the selection) is
//! taken and put back without an edit, set for one call from a part (`{"layout": "<name>"}`) and
//! the entities a call acts on (`on`, handles), and an agent's command leaves it as it was. The
//! current layer is the drawing's (its `CLAYER` header variable): a drawing command takes an
//! explicit `layer` and `layout` instead, and leaves the current layer and the space shown alone.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_kit::{AGENT_TOLERANCE, CadDocument, EDITOR_STATE};
use kition_kit::kdm::Origin;
use kition_kit::{CallContext, KitDocument};
use serde_json::{Value, json};

fn handle(answer: &Value) -> String {
    answer["handle"].as_str().or_else(|| answer["handles"][0].as_str()).unwrap_or_else(|| panic!("no handle in {answer}")).to_owned()
}

/// Model space: lines A (0,0)-(100,0) and B (0,50)-(100,50). Layout1: a line C. Layout2: empty.
/// A layer `Walls`. The person is in Model space with A selected, zoomed in on A, on layer 0.
struct Drawing {
    doc: CadDocument,
    a: String,
    b: String,
    c: String,
}

fn drawing() -> Drawing {
    let bytes = CadDocument::create("Plan.dxf", 420, 297).unwrap().save(Some("Plan.dxf")).unwrap();
    let mut doc = CadDocument::open("Plan.dxf", &bytes).unwrap();
    let a = handle(&doc.execute("line.add", json!({ "points": [[0, 0], [100, 0]] })).unwrap());
    let b = handle(&doc.execute("line.add", json!({ "points": [[0, 50], [100, 50]] })).unwrap());
    doc.execute("layout.set", json!({ "name": "Layout1" })).unwrap();
    let c = handle(&doc.execute("line.add", json!({ "points": [[10, 10], [60, 10]] })).unwrap());
    doc.execute("layout.set", json!({ "name": "Model" })).unwrap();
    doc.execute("layer.add", json!({ "name": "Walls" })).unwrap();
    doc.execute("select.set", json!({ "handles": [a] })).unwrap();
    doc.execute("view.set", json!({ "center": [50, 0], "height": 20 })).unwrap();
    Drawing { doc, a, b, c }
}

/// The editor as the person sees it: what inspect reports of the editing session.
fn editor(doc: &CadDocument) -> Value {
    let inspected = doc.inspect().unwrap();
    json!(EDITOR_STATE.iter().map(|pointer| (pointer.to_string(), inspected.pointer(pointer).cloned())).collect::<Vec<_>>())
}

fn agent() -> Origin {
    Origin::agent("agent:test")
}

fn context(on: &[&str], part: Option<Value>) -> CallContext {
    CallContext { on: on.iter().map(|h| json!(h)).collect(), part, time: None }
}

fn content(doc: &CadDocument) -> Value {
    kition_kit::document_content(doc.inspect().unwrap(), EDITOR_STATE)
}

/// Each entity of a space: handle → (layer, start x).
fn space(doc: &mut CadDocument, layout: &str) -> Vec<(String, String)> {
    let listed = doc.execute("object.query", json!({ "layout": layout })).unwrap();
    listed["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["handle"].as_str().unwrap().to_owned(), e["layer"].as_str().unwrap().to_owned()))
        .collect()
}

fn start_x(doc: &mut CadDocument, h: &str) -> f64 {
    let listed = doc.execute("object.list", json!({ "handles": [h] })).unwrap().to_string();
    let found = doc.execute("object.measure", json!({ "bounds": [h] })).unwrap();
    found["bounds"][0]["min"][0].as_f64().unwrap_or_else(|| panic!("{listed}"))
}

#[test]
fn editor_state_changes_nothing_in_what_an_agent_reads() {
    let mut d = drawing();
    let before = content(&d.doc);
    d.doc.execute("layout.set", json!({ "name": "Layout1" })).unwrap();
    d.doc.execute("select.set", json!({ "handles": [d.c] })).unwrap();
    d.doc.execute("view.zoomRealtime", json!({ "mode": "extents" })).unwrap();
    d.doc.execute("drafting.ortho", json!({ "on": true })).unwrap();
    assert_eq!(content(&d.doc), before, "the space shown, the selection, the view and the drafting modes are editor state");
    let listed = content(&d.doc);
    assert_eq!(listed["layouts"][0], json!({ "name": "Model", "part": { "layout": "Model" }, "entities": 2 }));
    assert_eq!(listed["layouts"][1]["id"], "layout:Layout1");
    assert!(listed["layers"].as_array().unwrap().iter().any(|l| l["id"] == "layer:Walls"), "{listed}");
}

#[test]
fn check_context_with_a_move_to_a_layout_and_a_selection_there() {
    let mut d = drawing();
    let on = context(&[&d.c], Some(json!({ "layout": "Layout1" })));
    kition_kit::testing::check_context(&mut d.doc, &on, EDITOR_STATE).unwrap();
    let model = context(&[&d.b], Some(json!({ "layout": "Model" })));
    kition_kit::testing::check_context(&mut d.doc, &model, EDITOR_STATE).unwrap();
}

#[test]
fn c8_context_checks_the_full_editor_state_inventory() {
    assert_eq!(
        EDITOR_STATE,
        &["/space", "/viewport", "/view", "/selection", "/selectedObjects", "/viewRequest", "/prompt", "/running", "/drafting", "/units"]
    );
    let mut d = drawing();
    for part in ["Model", "Layout1", "Layout2"] {
        let call = context(&[], Some(json!({"layout":part})));
        kition_kit::testing::check_context(&mut d.doc, &call, EDITOR_STATE).unwrap();
    }
    let command = d.doc.commands().into_iter().find(|c| c.id == "object.setProperties").unwrap();
    assert_eq!(command.reads, &[kition_kit::EditorState::Selection]);
    let command = d.doc.commands().into_iter().find(|c| c.id == "line.add").unwrap();
    assert_eq!(command.reads, &[kition_kit::EditorState::Active, kition_kit::EditorState::Part]);
}

#[test]
fn hooks_are_no_edit() {
    let mut d = drawing();
    let (revision, dirty, undo) = (d.doc.revision(), d.doc.dirty(), d.doc.inspect().unwrap()["canUndo"].clone());
    let saved = d.doc.editor_state();
    assert!(!saved.is_null());
    d.doc.set_context(&context(&[&d.c], Some(json!({ "layout": "Layout1" })))).unwrap();
    assert_eq!((d.doc.inspect().unwrap()["space"].clone(), d.doc.inspect().unwrap()["selection"].clone()), (json!("Layout1"), json!(1)));
    d.doc.restore_editor_state(&saved).unwrap();
    assert_eq!((d.doc.revision(), d.doc.dirty(), d.doc.inspect().unwrap()["canUndo"].clone()), (revision, dirty, undo));
}

#[test]
fn on_alone_names_its_space() {
    let mut d = drawing();
    d.doc.set_context(&context(&[&d.c], None)).unwrap();
    assert_eq!(d.doc.inspect().unwrap()["space"], json!("Layout1"), "an entity's space follows from its handle");
    assert_eq!(d.doc.inspect().unwrap()["selection"], json!(1));
}

#[test]
fn refused_contexts_change_nothing() {
    let mut d = drawing();
    let before = editor(&d.doc);
    let refusals = [
        (context(&["FFFFFF"], None), "no entity"),
        (context(&[&d.a, &d.c], None), "two spaces"),
        (context(&[&d.a], Some(json!({ "layout": "Layout1" }))), "is in Model"),
        (context(&[], Some(json!({ "layout": "Nope" }))), "no layout"),
        (context(&[], Some(json!({ "layout": "layout:Layout1" }))), "by its name"),
        (context(&[], Some(json!({ "slide": 1 }))), "layout"),
        (CallContext { on: vec![json!(256)], part: None, time: None }, "hex string"),
    ];
    for (context, says) in refusals {
        let error = d.doc.set_context(&context).expect_err(&format!("{context:?} is refused"));
        assert!(error.contains(says), "{context:?}: {error}");
        assert!(!error.contains("not available"), "an invalid context is invalid_params, not command_disabled: {error}");
        assert_eq!(editor(&d.doc), before, "{context:?} changed the editor");
    }
    let time = d.doc.set_context(&CallContext { on: Vec::new(), part: None, time: Some(1.0) }).unwrap_err();
    assert!(time.contains("not available"), "{time}");
}

#[test]
fn a_selection_command_driven_by_on_changes_exactly_those_entities() {
    let mut d = drawing();
    let before = editor(&d.doc);
    let saved = d.doc.editor_state();
    // What a host does for `{command: "object.move", on: [C]}` with C in Layout1 (`on` into the
    // selection, as for a command without `handles`): the context, the command, the restore.
    d.doc.set_context(&context(&[&d.c], None)).unwrap();
    d.doc.execute_as("object.move", json!({ "delta": [5, 0] }), &agent()).unwrap();
    d.doc.restore_editor_state(&saved).unwrap();
    assert_eq!(start_x(&mut d.doc, &d.c), 15.0, "C moved");
    assert_eq!((start_x(&mut d.doc, &d.a), start_x(&mut d.doc, &d.b)), (0.0, 0.0), "A, the person's selection, and B stayed");
    assert_eq!(editor(&d.doc), before, "the space shown, the view and the selection are the person's");
    // `on` into the command's `handles`, as hosts do for commands that declare them.
    d.doc.execute_as("object.delete", json!({ "handles": [d.b] }), &agent()).unwrap();
    d.doc.execute_as("object.setProperties", json!({ "handles": [d.a], "layer": "Walls" }), &agent()).unwrap();
    assert_eq!(space(&mut d.doc, "Model"), [(d.a.clone(), "Walls".to_owned())]);
    assert_eq!(editor(&d.doc)[3], before[3], "the selection is still A");
}

#[test]
fn a_drawing_command_with_an_explicit_layer_and_layout_draws_there_and_moves_nothing() {
    let mut d = drawing();
    let before = editor(&d.doc);
    for who in [Some(agent()), None] {
        let params = json!({ "points": [[0, 90], [40, 90]], "layer": "Walls", "layout": "Layout2" });
        let made = match &who {
            Some(origin) => d.doc.execute_as("line.add", params, origin).unwrap(),
            None => d.doc.execute("line.add", params).unwrap(),
        };
        let made = handle(&made);
        assert!(space(&mut d.doc, "Layout2").contains(&(made, "Walls".to_owned())), "drawn in Layout2 on Walls");
        assert_eq!(d.doc.inspect().unwrap()["currentLayer"], "0", "the current layer stays");
        assert_eq!(editor(&d.doc), before, "the space shown stays");
    }
    let error = d.doc.execute_as("line.add", json!({ "points": [[0, 0], [1, 1]], "layer": "Nope" }), &agent()).unwrap_err();
    assert!(error.contains("no layer `Nope`") && error.contains("layer.add"), "{error}");
    let error = d.doc.execute_as("circle.add", json!({ "center": [0, 0], "radius": 1, "layout": "Nope" }), &agent()).unwrap_err();
    assert!(error.contains("no layout `Nope`"), "{error}");
}

#[test]
fn two_layouts_in_one_run() {
    let mut d = drawing();
    let saved = d.doc.editor_state();
    for layout in ["Layout1", "Layout2"] {
        d.doc.set_context(&context(&[], Some(json!({ "layout": layout })))).unwrap();
        d.doc.execute_as("circle.add", json!({ "center": [50, 50], "radius": 5 }), &agent()).unwrap();
    }
    d.doc.restore_editor_state(&saved).unwrap();
    assert_eq!((space(&mut d.doc, "Layout1").len(), space(&mut d.doc, "Layout2").len(), space(&mut d.doc, "Model").len()), (2, 1, 2));
    assert_eq!(d.doc.inspect().unwrap()["space"], "Model");
}

#[test]
fn an_agents_failing_command_leaves_the_editor_and_the_drawing() {
    let mut d = drawing();
    let before = (d.doc.revision(), d.doc.save(None).unwrap());
    let person = d.doc.editor_state();
    d.doc.set_context(&context(&[], Some(json!({ "layout": "Layout1" })))).unwrap();
    let saved = d.doc.editor_state();
    let error = d.doc.execute_as("object.setProperties", json!({ "handles": [d.c], "layer": "Fresh", "dimStyle": "Nope" }), &agent()).unwrap_err();
    assert!(error.contains("Nope"), "{error}");
    assert_eq!(d.doc.editor_state(), saved, "the failed command left the editor as the step's context set it");
    d.doc.restore_editor_state(&person).unwrap();
    assert_eq!(d.doc.editor_state(), person);
    assert_eq!((d.doc.revision(), d.doc.save(None).unwrap()), before, "no partial change: no layer Fresh");
}

#[test]
fn create_then_act_in_one_run() {
    let mut d = drawing();
    let saved = d.doc.editor_state();
    let made = handle(&d.doc.execute_as("rectangle.add", json!({ "p1": [200, 200], "p2": [220, 210] }), &agent()).unwrap());
    // `select` keeps its effect for the rest of the call; `move` acts on it.
    d.doc.execute_as("select.set", json!({ "window": [[190, 190], [230, 230]] }), &agent()).unwrap();
    d.doc.execute_as("object.move", json!({ "delta": [0, 10] }), &agent()).unwrap();
    d.doc.restore_editor_state(&saved).unwrap();
    let bounds = d.doc.execute("object.measure", json!({ "bounds": [made] })).unwrap();
    assert_eq!(bounds["bounds"][0]["min"], json!([200.0, 210.0]));
}

#[test]
fn picking_by_a_point_with_an_explicit_tolerance_is_independent_of_zoom() {
    let mut d = drawing();
    // A point 0.3 units above A: picked at a tolerance of 0.5 whatever the zoom.
    for height in [1.0, 10_000.0] {
        d.doc.execute("view.set", json!({ "center": [50, 0], "height": height })).unwrap();
        let picked = d.doc.execute_as("select.set", json!({ "at": [50, 0.3], "aperture": 0.5 }), &agent()).unwrap();
        assert_eq!(picked["handles"], json!([d.a]), "at zoom height {height}");
        let missed = d.doc.execute_as("select.set", json!({ "at": [50, 0.3], "aperture": 0.1 }), &agent()).unwrap();
        assert_eq!(missed["handles"], json!([]), "at zoom height {height}");
        // The agent's default: 0.5% of the larger side of the extents (100 x 50 here: 0.5 units).
        let default = d.doc.execute_as("select.set", json!({ "at": [50, 0.4] }), &agent()).unwrap();
        assert_eq!(default["handles"], json!([d.a]), "the default tolerance at zoom height {height}");
        let beyond = d.doc.execute_as("select.set", json!({ "at": [50, 100.0 * AGENT_TOLERANCE + 0.2] }), &agent()).unwrap();
        assert_eq!(beyond["handles"], json!([]), "beyond the default at zoom height {height}");
    }
}

#[test]
fn an_agents_zoom_inside_a_layout_viewport_is_refused_and_writes_nothing() {
    let mut d = drawing();
    d.doc.execute("layout.set", json!({ "name": "Layout1" })).unwrap();
    d.doc.execute("viewport.add", json!({ "p1": [100, 100], "p2": [200, 180] })).unwrap();
    d.doc.execute("viewport.modelSpace", json!({})).unwrap();
    let saved = d.doc.save(None).unwrap();
    for (command, params) in [
        ("view.zoomRealtime", json!({ "mode": "extents" })),
        ("view.zoomIn", json!({})),
        ("view.pan", json!({ "delta": [5, 5] })),
        ("view.set", json!({ "center": [0, 0], "height": 5 })),
    ] {
        let error = d.doc.execute_as(command, params, &agent()).unwrap_err();
        assert!(error.contains("MSPACE") && error.contains("viewport.paperSpace"), "{command}: {error}");
    }
    assert_eq!(d.doc.save(None).unwrap(), saved, "the saved drawing is unchanged");
    // In paper space an agent navigates freely, and the drawing does not change.
    d.doc.execute_as("viewport.paperSpace", json!({}), &agent()).unwrap();
    d.doc.execute_as("view.zoomRealtime", json!({ "mode": "extents" }), &agent()).unwrap();
    assert_eq!(d.doc.save(None).unwrap(), saved);
}

#[test]
fn entities_lists_any_space_by_name_with_paging_without_going_there() {
    let mut d = drawing();
    let before = editor(&d.doc);
    let listed = d.doc.execute_as("object.query", json!({ "layout": "Layout1" }), &agent()).unwrap();
    assert_eq!((listed["count"].clone(), listed["entities"][0]["handle"].clone()), (json!(1), json!(d.c)));
    let page = d.doc.execute_as("object.query", json!({ "layout": "Model", "limit": 1, "offset": 1 }), &agent()).unwrap();
    assert_eq!((page["count"].clone(), page["entities"].as_array().unwrap().len(), page["entities"][0]["handle"].clone()), (json!(2), 1, json!(d.b)));
    assert_eq!(editor(&d.doc), before);
    let error = d.doc.execute_as("object.query", json!({ "layout": "Nope" }), &agent()).unwrap_err();
    assert!(error.contains("no layout"), "{error}");
}
