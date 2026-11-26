//! The cursor pipeline while a command runs, direct distance entry, tracking guides and Enter
//! between commands, driven through the tool events as the editor sends them.
use super::*;
use kition_cad_doc::EntityKind;

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("Drafting.dxf", 0, 0).unwrap();
    // No running object snaps, so only the mode under test moves the cursor.
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
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

/// B1: with the pointer to the right of the first point, a typed 60 draws 60 units to the right
/// (before, the binding never told the session where the cursor was, so it went toward the origin);
/// with Ortho on and the pointer 10° above the horizontal it is still 60 to the right.
#[test]
fn issue_direct_distance_follows_the_cursor() {
    for ortho in [false, true] {
        let mut doc = blank();
        doc.execute("drafting.ortho", json!({ "on": ortho })).unwrap();
        doc.select_tool("line.add", 1.0).unwrap();
        let start = Vec2::new(40.0, 30.0);
        click(&mut doc, start);
        let toward = if ortho { start + Vec2::from_angle(10f64.to_radians()) * 50.0 } else { start + Vec2::new(50.0, 0.0) };
        hover(&mut doc, toward);
        KitTools::text(&mut doc, "60", 1.0).unwrap();
        doc.key("Enter", ToolMods::default(), 1.0).unwrap();
        let drawn = lines(&doc);
        assert_eq!(drawn.len(), 1, "ortho {ortho}: {drawn:?}");
        assert!(near(drawn[0].0, start) && near(drawn[0].1, start + Vec2::new(60.0, 0.0)), "ortho {ortho}: {drawn:?}");
    }
    // Off every tracking path, the typed distance goes along the rubber band.
    let mut doc = blank();
    doc.execute("drafting.polar", json!({ "on": false })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    hover(&mut doc, Vec2::new(40.0, 50.0));
    KitTools::text(&mut doc, "25", 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert!(near(lines(&doc)[0].1, Vec2::new(25.0, 30.0)), "{:?}", lines(&doc));
}

/// B2: during a command the pointer goes through the whole pipeline, not only object snap: Ortho
/// (F8) keeps the second point level with the first, grid snap (F9) puts points on the grid, and
/// polar tracking at 45° makes a line drawn near that angle exactly 45°.
#[test]
fn issue_command_cursor_pipeline() {
    // Ortho.
    let mut doc = blank();
    doc.execute("drafting.ortho", json!({ "on": true })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    click(&mut doc, Vec2::new(60.0, 13.0));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(lines(&doc), vec![(Vec2::new(10.0, 10.0), Vec2::new(60.0, 10.0))]);

    // Grid snap.
    let mut doc = blank();
    doc.execute("drafting.snap", json!({ "on": true })).unwrap();
    doc.session.settings.snapunit = Vec2::new(10.0, 10.0);
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(43.0, 17.0));
    click(&mut doc, Vec2::new(88.0, 61.0));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(lines(&doc), vec![(Vec2::new(40.0, 20.0), Vec2::new(90.0, 60.0))]);

    // Polar tracking at 45°: the preview and the click agree.
    let mut doc = blank();
    doc.execute("variable.set", json!({ "name": "POLARANG", "value": 45 })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    let start = Vec2::new(10.0, 10.0);
    click(&mut doc, start);
    let off = start + Vec2::new(40.0, 40.0 + doc.session.pixel_size() * 2.0);
    let response = hover(&mut doc, off);
    assert!(
        response.overlays.iter().any(|o| matches!(o, ToolOverlay::Line { dashed: true, .. })),
        "the polar path is drawn: {:?}",
        response.overlays
    );
    assert!(
        response.overlays.iter().any(|o| matches!(o, ToolOverlay::Measure { text, .. } if text.starts_with("Polar: ") && text.ends_with("< 45°"))),
        "the tooltip names the path, its distance and angle: {:?}",
        response.overlays
    );
    click(&mut doc, off);
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    let (a, b) = lines(&doc)[0];
    assert!(near(a, start));
    assert!(((b - a).x - (b - a).y).abs() < 1e-9 && (b - a).x > 39.0, "exactly 45°: {:?}", b - a);
}

/// N10: resting on an object snap point acquires it for tracking (the page repeats the move after
/// the dwell); its alignment path then holds a point level with it, and the guide shows the `+`
/// and the tooltip "Endpoint: … < 0°". Moving the pointer without resting acquires nothing.
#[test]
fn tracking_acquires_a_rested_point_and_aligns_with_it() {
    let mut doc = blank();
    doc.execute("line.add", json!({ "points": [[100, 50], [100, 90]] })).unwrap();
    doc.session.settings.osmode = kition_cad_engine::snap::mode::END;
    doc.execute("drafting.polar", json!({ "on": false })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(10.0, 10.0));
    let end = Vec2::new(100.0, 90.0);
    hover(&mut doc, end);
    let off = Vec2::new(40.0, 90.0 + doc.session.pixel_size() * 2.0);
    let passing = hover(&mut doc, off);
    assert!(doc.session.tracking.acquired.is_empty(), "passing over the endpoint acquires nothing");
    assert!(!passing.overlays.iter().any(|o| matches!(o, ToolOverlay::Measure { .. })));
    hover(&mut doc, end);
    hover(&mut doc, end);
    assert_eq!(doc.session.tracking.acquired.len(), 1, "resting acquires the endpoint");
    let aligned = hover(&mut doc, off);
    assert!(
        aligned
            .overlays
            .iter()
            .any(|o| matches!(o, ToolOverlay::Measure { text, .. } if text.starts_with("Endpoint: 60") && text.ends_with("< 180°"))),
        "{:?}",
        aligned.overlays
    );
    click(&mut doc, off);
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    let drawn = lines(&doc);
    assert!(near(drawn[1].1, Vec2::new(40.0, 90.0)), "the click lands on the alignment path: {drawn:?}");
}

/// A snap override typed at a point prompt (END, from the command menu too) holds the cursor for
/// one pick, with running snaps off; NON lets it go free.
#[test]
fn a_snap_override_holds_the_cursor_for_one_pick() {
    let mut doc = blank();
    doc.execute("line.add", json!({ "points": [[100, 50], [100, 90]] })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    KitTools::text(&mut doc, "_end", 1.0).unwrap();
    let response = hover(&mut doc, Vec2::new(101.0, 89.0));
    assert_eq!(doc.snapped.map(|(point, _)| point), Some(Vec2::new(100.0, 90.0)));
    assert!(response.overlays.iter().any(|o| matches!(o, ToolOverlay::Label { text, .. } if text == "Endpoint")));
    click(&mut doc, Vec2::new(101.0, 89.0));
    KitTools::text(&mut doc, "_non", 1.0).unwrap();
    hover(&mut doc, Vec2::new(100.5, 50.5));
    assert!(doc.snapped.is_none());
    click(&mut doc, Vec2::new(100.5, 50.5));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    let drawn = lines(&doc);
    assert_eq!(drawn[1].0, Vec2::new(100.0, 90.0));
    assert!(near(drawn[1].1, Vec2::new(100.5, 50.5)), "{drawn:?}");
}

/// B5: Enter between commands repeats the last command, also with objects selected (the page
/// then hands Enter to the engine, which before swallowed it).
#[test]
fn issue_enter_repeats_with_a_selection() {
    let mut doc = blank();
    doc.command_line("circle").unwrap();
    doc.command_line("50,50").unwrap();
    doc.command_line("10").unwrap();
    assert!(doc.session.running.is_none());
    doc.execute("edit.selectAll", json!({})).unwrap();
    assert_eq!(doc.session.selection().len(), 1);
    let response = doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.session.running.as_ref().map(|r| r.id.as_str()), Some("circle.add"));
    assert_eq!(response.cursor, "crosshair");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    // A free-text prompt shows the text cursor: there Space types a space on the command line.
    doc.command_line("text").unwrap();
    doc.command_line("10,10").unwrap();
    for _ in 0..5 {
        if doc.session.current_prompt().is_none_or(|p| !(p.accept.number || p.accept.point)) {
            break;
        }
        doc.command_line("").unwrap();
    }
    assert_eq!(doc.key("Tab", ToolMods::default(), 1.0).unwrap().cursor, "text", "{}", doc.prompt());
}

/// Hostile pointers (far off, non-finite) never panic the pipeline or the guides.
#[test]
fn hostile_pointers_never_panic_the_pipeline() {
    let mut doc = blank();
    doc.execute("drafting.objectSnap", json!({ "on": true })).unwrap();
    doc.execute("line.add", json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    doc.select_tool("line.add", 1.0).unwrap();
    click(&mut doc, Vec2::new(1.0, 1.0));
    for (x, y) in [(f64::NAN, 0.0), (f64::INFINITY, 1.0), (1e300, -1e300), (0.0, 0.0), (0.0, 0.0)] {
        let _ = doc.pointer(&ToolPointer { action: PointerAction::Move, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 });
        let _ = doc.state(x, y, ToolMods::default(), 1.0);
    }
    KitTools::text(&mut doc, "1e308", 1.0).ok();
}
