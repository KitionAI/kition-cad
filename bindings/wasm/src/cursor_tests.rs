//! The cursor as the editor draws it (its state, the crosshair's place, the coordinate readout),
//! the rollover highlight, the pick box as the pick range, the selection window's fill, MOVE's
//! guide line, and the canvas: the editor's colour, colour 7 against it, and the grid.
use super::*;

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("Cursor.dxf", 0, 0).unwrap();
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

fn click(doc: &mut CadDocument, point: Vec2) -> ToolResponse {
    event(doc, PointerAction::Move, point);
    event(doc, PointerAction::Down, point);
    event(doc, PointerAction::Up, point)
}

/// The crosshair's anchor and the readout's label, the cursor's two transparent overlays.
fn cursor_of(response: &ToolResponse) -> (Option<(f64, f64, f32)>, Option<String>) {
    let anchor = response.overlays.iter().find_map(|o| match o {
        ToolOverlay::Anchor { x, y, color, size, .. } if *color == CURSOR => Some((*x, *y, *size)),
        _ => None,
    });
    let label = response.overlays.iter().find_map(|o| match o {
        ToolOverlay::Label { text, color, .. } if *color == CURSOR => Some(text.clone()),
        _ => None,
    });
    (anchor, label)
}

fn anchored(anchor: Option<(f64, f64, f32)>, x: f64, y: f64) -> bool {
    anchor.is_some_and(|(ax, ay, _)| (ax - x).abs() < 1e-6 && (ay - y).abs() < 1e-6)
}

/// The rollover highlight: thin dashed paths in the selection colour.
fn highlighted(response: &ToolResponse) -> bool {
    response.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { color, width, dashed: true, .. } if *color == SELECTED && *width == 1.0))
}

/// N1: the cursor's state follows the prompt: the crosshair with the pick box between commands,
/// the crosshair alone at a point prompt (and with PICKFIRST off), the pick box alone at "Select
/// objects", the text cursor at a free-text prompt.
#[test]
fn the_cursor_state_follows_the_prompt() {
    let mut doc = blank();
    doc.execute("circle.add", json!({ "center": [50, 50], "radius": 10 })).unwrap();
    assert_eq!(hover(&mut doc, Vec2::new(100.0, 100.0)).cursor, "default");
    doc.command_line("line").unwrap();
    assert_eq!(hover(&mut doc, Vec2::new(100.0, 100.0)).cursor, "crosshair");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.command_line("erase").unwrap();
    assert_eq!(hover(&mut doc, Vec2::new(100.0, 100.0)).cursor, "pointer");
    // An empty pick opens a window: the crosshair, as for any corner.
    assert_eq!(click(&mut doc, Vec2::new(100.0, 100.0)).cursor, "crosshair");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.session.settings.pickfirst = false;
    assert_eq!(hover(&mut doc, Vec2::new(100.0, 100.0)).cursor, "crosshair");
}

/// N1, N3: the crosshair's anchor is at the pointer and as large as the pick box; the readout is
/// the pointer's coordinates with LUPREC places; on an object snap both are the snap point.
#[test]
fn the_crosshair_and_readout_follow_the_point_a_click_gives() {
    let mut doc = blank();
    doc.execute("line.add", json!({ "points": [[0, 0], [100, 0]] })).unwrap();
    let at = Vec2::new(30.25, 40.5);
    let response = hover(&mut doc, at);
    let (anchor, label) = cursor_of(&response);
    let (x, y) = doc.placement().to_doc(at);
    assert!(anchored(anchor, x, y), "{anchor:?}");
    assert_eq!(anchor.map(|(_, _, size)| size), Some(9.0), "the pick box is PICKBOX × 1.5 pixels each side");
    assert_eq!(label.as_deref(), Some("30.2500, 40.5000, 0.0000"));
    doc.execute("drafting.objectSnap", json!({ "on": true })).unwrap();
    doc.command_line("line").unwrap();
    let response = hover(&mut doc, Vec2::new(100.4, 0.3));
    let (anchor, label) = cursor_of(&response);
    let (x, y) = doc.placement().to_doc(Vec2::new(100.0, 0.0));
    assert!(anchored(anchor, x, y), "the crosshair sits on the endpoint: {anchor:?}");
    assert_eq!(label.as_deref(), Some("100.0000, 0.0000, 0.0000"));
    // Ortho moves the rubber band, not the crosshair; the readout is the point a click gives.
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    doc.execute("drafting.ortho", json!({ "on": true })).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    let pointer = Vec2::new(50.0, 14.0);
    let (anchor, label) = cursor_of(&hover(&mut doc, pointer));
    let (x, y) = doc.placement().to_doc(pointer);
    assert!(anchored(anchor, x, y), "{anchor:?}");
    assert_eq!(label.as_deref(), Some("50.0000, 10.0000, 0.0000"));
    // Nothing for a pointer that is not anywhere.
    let response =
        doc.pointer(&ToolPointer { action: PointerAction::Move, x: f64::NAN, y: 0.0, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 });
    assert_eq!(cursor_of(&response.unwrap()), (None, None));
}

/// N4: the object under the pick box is highlighted between commands and at "Select objects",
/// not at a point prompt, not on a locked layer, not while a window is open, and no longer once
/// the pointer moves off it.
#[test]
fn the_rollover_highlight_shows_what_a_click_picks() {
    let mut doc = blank();
    doc.execute("circle.add", json!({ "center": [50, 50], "radius": 10 })).unwrap();
    let edge = Vec2::new(60.5, 50.0);
    assert!(highlighted(&hover(&mut doc, edge)), "idle, on the circle");
    assert!(!highlighted(&hover(&mut doc, Vec2::new(75.0, 50.0))), "moved off");
    doc.command_line("erase").unwrap();
    assert!(highlighted(&hover(&mut doc, edge)), "at Select objects");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.command_line("line").unwrap();
    assert!(!highlighted(&hover(&mut doc, edge)), "a point prompt picks no object");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    click(&mut doc, Vec2::new(150.0, 150.0));
    assert!(!highlighted(&hover(&mut doc, edge)), "a selection window is open");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.execute("layer.set", json!({ "name": "0", "locked": true })).unwrap();
    assert!(!highlighted(&hover(&mut doc, edge)), "a locked layer's circle");
}

/// The pick box is the pick range: PICKBOX × 1.5 screen pixels from the click, at any zoom.
#[test]
fn the_pick_box_is_the_pick_range() {
    for zoom in [1.0, 4.0] {
        let mut doc = blank();
        doc.execute("line.add", json!({ "points": [[0, 50], [100, 50]] })).unwrap();
        doc.execute("viewport.sync", json!({ "x": 0, "y": 0, "width": 1200, "height": 800, "zoom": zoom })).unwrap();
        let pixel = 1.0 / (doc.placement().k * zoom);
        let pick = |doc: &mut CadDocument, off: f64| {
            let (x, y) = doc.placement().to_doc(Vec2::new(50.0, 50.0 + off * pixel));
            for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
                doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom }).unwrap();
            }
            let picked = doc.session.selection().len();
            doc.key("Escape", ToolMods::default(), zoom).unwrap();
            doc.key("Escape", ToolMods::default(), zoom).unwrap();
            picked
        };
        assert_eq!(pick(&mut doc, 4.0), 1, "zoom {zoom}: 4 pixels off is inside the pick box");
        assert_eq!(pick(&mut doc, 6.0), 0, "zoom {zoom}: 6 pixels off is outside it");
    }
}

/// The selection window is filled, blue and see-through left to right, green right to left; the
/// line from MOVE's base point to the cursor is a guide in its own colour, LINE's rubber band is
/// what it draws.
#[test]
fn window_fills_and_preview_guides() {
    let mut doc = blank();
    doc.execute("circle.add", json!({ "center": [50, 50], "radius": 10 })).unwrap();
    click(&mut doc, Vec2::new(100.0, 100.0));
    let fill = |response: &ToolResponse| {
        response.overlays.iter().find_map(|o| match o {
            ToolOverlay::Highlight { color, .. } => Some(*color),
            _ => None,
        })
    };
    assert_eq!(fill(&hover(&mut doc, Vec2::new(150.0, 120.0))), Some(WINDOW_FILL));
    assert_eq!(fill(&hover(&mut doc, Vec2::new(20.0, 120.0))), Some(CROSSING_FILL));
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    let colours = |response: &ToolResponse| {
        response
            .overlays
            .iter()
            .filter_map(|o| match o {
                ToolOverlay::Path { color, .. } => Some(*color),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    doc.command_line("move").unwrap();
    doc.command_line("all").unwrap();
    doc.command_line("").unwrap();
    doc.command_line("50,50").unwrap();
    let paths = colours(&hover(&mut doc, Vec2::new(80.0, 70.0)));
    assert_eq!(paths.iter().filter(|c| **c == GUIDE).count(), 1, "{paths:?}");
    assert!(paths.contains(&ACCENT), "the moved circle is drawn: {paths:?}");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.command_line("line").unwrap();
    doc.command_line("0,0").unwrap();
    let paths = colours(&hover(&mut doc, Vec2::new(80.0, 70.0)));
    assert!(paths.contains(&ACCENT) && !paths.contains(&GUIDE), "{paths:?}");
}

/// Walkthrough N17: zoomed far out, the editor showed the grid shrunk with the drawing, so the
/// paper turned grey (and outside the drawing's area there was none). The editor now gives its
/// canvas colour and draws the grid itself over the whole canvas at the size shown; the drawing's
/// pixels carry no grid at any zoom. A render without that colour (a preview) keeps its grid, its
/// lines at least 8 pixels apart.
#[test]
fn issue_grid_zoomed_out() {
    let mut doc = blank();
    doc.execute("drafting.grid", json!({ "on": true })).unwrap();
    doc.session.settings.gridunit = Vec2::new(1.0, 1.0);
    let (width, height) = (doc.width(), doc.height());
    let plain = doc.render(0.0, 0.0, 1.0, 200, 40).unwrap();
    let row: Vec<bool> = plain.as_chunks::<4>().0.iter().take(200).map(|p| *p != [255, 255, 255, 255]).collect();
    let lines: Vec<usize> = (1..row.len()).filter(|&i| row[i] && !row[i - 1]).collect();
    assert!(lines.len() > 1 && lines.windows(2).all(|pair| pair[1] - pair[0] >= 8), "a preview's grid lines: {lines:?}");
    doc.set_backdrop("#f6f5f4").unwrap();
    for scale in [1.0, 1.0 / 16.0, 1.0 / 64.0] {
        let (w, h) = (((f64::from(width) * scale).ceil() as u32).max(8), ((f64::from(height) * scale).ceil() as u32).max(8));
        let pixels = doc.render(0.0, 0.0, scale, w, h).unwrap();
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| *p == [0xf6, 0xf5, 0xf4, 255]), "scale {scale}: only the canvas colour");
    }
}

/// N17: given the editor's canvas colour, the drawing is drawn on it (no paper), colour 7 in
/// black or white against it, and the grid is left to the editor, which draws it over the whole
/// canvas from the drawing's grid settings and origin.
#[test]
fn the_backdrop_is_the_editors_canvas() {
    let mut doc = blank();
    doc.execute("line.add", json!({ "points": [[0, 20], [200, 20]] })).unwrap();
    doc.execute("drafting.grid", json!({ "on": true })).unwrap();
    assert!(doc.set_backdrop("not a colour").is_err());
    assert!(doc.set_backdrop("#12345").is_err());
    doc.set_backdrop("#15171b").unwrap();
    let (x, y) = doc.placement().to_doc(Vec2::new(0.0, 40.0));
    let pixels = doc.render(x, y, 1.0, 200, 200).unwrap();
    let all = pixels.as_chunks::<4>().0;
    let canvas = all.iter().filter(|p| **p == [0x15, 0x17, 0x1b, 255]).count();
    assert!(canvas * 10 > all.len() * 9, "the canvas colour, not paper: {canvas} of {}", all.len());
    assert!(!all.contains(&[235, 233, 229, 255]), "no grid in the pixels");
    assert!(all.iter().any(|p| p[0] > 200 && p[1] > 200 && p[2] > 200), "colour 7 draws light on a dark canvas");
    let inspected = doc.inspect().unwrap();
    assert_eq!(inspected["drafting"]["gridMajor"], json!(5));
    assert!(inspected["drafting"]["gridUnit"].as_array().is_some_and(|unit| unit.len() == 2));
    let (ox, oy) = doc.placement().to_doc(Vec2::ZERO);
    assert_eq!(inspected["units"]["origin"], json!([ox, oy]));
}
