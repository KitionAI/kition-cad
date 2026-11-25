use super::*;
use kition_kit::testing::check_contract;

/// A metric drawing with one line from (10, 10) to (100, 60).
fn fixture() -> Vec<u8> {
    let mut doc = CadDocument::create("Plan.dxf", 0, 0).unwrap();
    doc.command_line("line").unwrap();
    doc.command_line("10,10").unwrap();
    doc.command_line("100,60").unwrap();
    doc.command_line("").unwrap();
    doc.save(Some("Plan.dxf")).unwrap()
}

fn click(doc: &mut CadDocument, x: f64, y: f64) -> ToolResponse {
    for action in [PointerAction::Move, PointerAction::Down, PointerAction::Up] {
        doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap();
    }
    doc.state(x, y, ToolMods::default(), 1.0)
}

fn pixel(doc: &CadDocument, x: f64, y: f64) -> [u8; 4] {
    let rgba = doc.render(x - 2.0, y - 2.0, 1.0, 5, 5).unwrap();
    // The darkest pixel of the 5 x 5 area around (x, y).
    let mut darkest = [255u8; 4];
    for px in rgba.as_chunks::<4>().0 {
        if u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2]) < u32::from(darkest[0]) + u32::from(darkest[1]) + u32::from(darkest[2]) {
            darkest.copy_from_slice(px);
        }
    }
    darkest
}

#[test]
fn draws_lines_in_black_on_white_paper() {
    let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
    doc.execute("grid", json!({"on": false})).unwrap();
    let place = doc.placement();
    assert!(doc.width() > 1000 && doc.width() <= 2000, "{}", doc.width());
    let (mx, my) = place.to_doc(Vec2::new(55.0, 35.0));
    assert!(pixel(&doc, mx, my)[..3].iter().all(|channel| *channel < 64), "colour 7 plots black on paper");
    let (ex, ey) = place.to_doc(Vec2::new(300.0, 250.0));
    assert_eq!(pixel(&doc, ex, ey), [255, 255, 255, 255]);
}

fn circle_document() -> CadDocument {
    let mut doc = CadDocument::create("Grips.dxf", 0, 0).unwrap();
    doc.execute("layer.add", json!({"name": "Furniture", "current": true})).unwrap();
    doc.execute("circle", json!({"center": [100, 100], "radius": 30})).unwrap();
    doc.execute("layer.setCurrent", json!({"name": "0"})).unwrap();
    CadDocument::open("Grips.dxf", &doc.save(Some("Grips.dxf")).unwrap()).unwrap()
}

fn click_world(doc: &mut CadDocument, point: Vec2) -> ToolResponse {
    let (x, y) = doc.placement().to_doc(point);
    click(doc, x, y)
}

#[test]
fn circle_grip_previews_commits_and_undoes_without_editing_on_cancel() {
    let mut doc = circle_document();
    click_world(&mut doc, Vec2::new(100.0, 130.0));
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["layer"], "Furniture");
    click_world(&mut doc, Vec2::new(100.0, 100.0));
    let (x, y) = doc.placement().to_doc(Vec2::new(140.0, 110.0));
    let preview = doc.state(x, y, ToolMods::default(), 1.0);
    assert!(preview.busy);
    assert!(doc.prompt().contains("STRETCH"));
    assert!(!doc.dirty(), "starting and previewing a grip must not edit the drawing");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(!doc.dirty());
    click_world(&mut doc, Vec2::new(100.0, 100.0));
    click_world(&mut doc, Vec2::new(140.0, 110.0));
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([140.0, 110.0]));
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["radius"], 30.0);
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty(), "one undo returns to the saved geometry");
    doc.execute("edit.redo", json!({})).unwrap();
    let saved = CadDocument::open("Grips.dxf", &doc.save(Some("Grips.dxf")).unwrap()).unwrap();
    let circle = saved
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| if let kition_cad_doc::EntityKind::Circle(c) = &e.kind { Some(c) } else { None })
        .unwrap();
    assert_eq!(circle.center.xy(), Vec2::new(140.0, 110.0));
}

#[test]
fn grip_coordinates_and_locked_layers_follow_the_engine_contract() {
    let mut doc = circle_document();
    click_world(&mut doc, Vec2::new(100.0, 130.0));
    click_world(&mut doc, Vec2::new(130.0, 100.0));
    assert!(doc.command_line("invalid").is_err());
    assert!(!doc.dirty());
    doc.command_line("150,100").unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["radius"], 50.0);
    doc.execute("layer.set", json!({"name": "Furniture", "locked": true})).unwrap();
    click_world(&mut doc, Vec2::new(100.0, 100.0));
    assert!(doc.hot_grip.is_none(), "locked objects cannot start grip edits");
}

fn start_circle_grip(doc: &mut CadDocument) {
    click_world(doc, Vec2::new(100.0, 130.0));
    click_world(doc, Vec2::new(100.0, 100.0));
    assert!(doc.hot_grip.is_some());
}

#[test]
fn escape_cancels_a_grip_before_clearing_its_selection() {
    for multiple in [false, true] {
        let mut doc = circle_document();
        if multiple {
            doc.execute("line", json!({"points": [[100, 100], [180, 160]]})).unwrap();
            doc = CadDocument::open("Grips.dxf", &doc.save(Some("Grips.dxf")).unwrap()).unwrap();
        }
        doc.execute("edit.selectAll", json!({})).unwrap();
        let selected = doc.session.selection();
        click_world(&mut doc, Vec2::new(100.0, 100.0));
        let (x, y) = doc.placement().to_doc(Vec2::new(145.0, 101.0));
        doc.state(x, y, ToolMods::default(), 1.0);
        assert!(doc.hot_grip.is_some());
        let response = doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        assert!(doc.hot_grip.is_none());
        assert!(doc.grip_overlays().is_empty());
        assert!(doc.snapped.is_none());
        assert_eq!(doc.session.selection(), selected, "the first Escape retains the complete selection");
        assert!(response.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Anchor { .. })));
        assert!(!doc.dirty());
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        assert!(doc.session.selection().is_empty(), "the second Escape clears selection");
        assert!(!doc.dirty());
    }
}

fn pointer_world(doc: &mut CadDocument, action: PointerAction, point: Vec2) {
    let (x, y) = doc.placement().to_doc(point);
    doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap();
}

#[test]
fn escape_during_a_held_grip_ignores_release_and_allows_the_next_edit() {
    let mut doc = circle_document();
    start_circle_grip(&mut doc);
    let base = Vec2::new(100.0, 100.0);
    pointer_world(&mut doc, PointerAction::Down, base);
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    pointer_world(&mut doc, PointerAction::Up, base);
    assert!(doc.session.pending_window.is_none(), "release after Escape must not open a new selection window");
    assert!(doc.hot_grip.is_none(), "release after Escape must not reactivate the grip");
    assert!(!doc.dirty());
    click_world(&mut doc, base);
    assert!(doc.hot_grip.is_some(), "a fresh click can restart the retained grip");
    doc.command_line("@45,12").unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([145.0, 112.0]));
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty(), "cancellation adds no history entry");
}

#[test]
fn escape_during_selection_drag_ignores_release_without_reopening_the_window() {
    let mut doc = circle_document();
    pointer_world(&mut doc, PointerAction::Down, Vec2::new(50.0, 50.0));
    pointer_world(&mut doc, PointerAction::Drag, Vec2::new(150.0, 150.0));
    assert!(doc.session.pending_window.is_some());
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    pointer_world(&mut doc, PointerAction::Up, Vec2::new(150.0, 150.0));
    assert!(doc.session.pending_window.is_none(), "release must not revive a cancelled selection window");
    assert!(doc.session.selection().is_empty());
    click_world(&mut doc, Vec2::new(100.0, 130.0));
    assert_eq!(doc.session.selection().len(), 1, "the next click selects normally");
    assert!(!doc.dirty());
}

#[test]
fn grip_drafting_toggles_and_view_changes_keep_the_pending_edit() {
    let mut doc = circle_document();
    start_circle_grip(&mut doc);
    for id in [
        "ortho",
        "drafting.objectSnap",
        "grid",
        "snap",
        "polar",
        "drafting.objectSnapTracking",
        "drafting.lineweightDisplay",
        "view.zoomIn",
        "view.zoomToFit",
    ] {
        doc.execute(id, json!({})).unwrap();
        assert!(doc.hot_grip.is_some(), "{id} must not cancel a pending grip");
        assert!(!doc.dirty());
    }
    doc.command_line("140,110").unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([140.0, 110.0]));
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty(), "view and drafting changes add no geometry undo steps");
}

#[test]
fn grip_pointer_snaps_to_endpoints_before_ortho_and_previews_the_same_destination() {
    let mut doc = circle_document();
    doc.execute("line", json!({"points": [[160, 120], [200, 180]]})).unwrap();
    doc.session.settings.osmode = kition_cad_engine::snap::mode::END;
    doc.session.settings.orthomode = true;
    start_circle_grip(&mut doc);
    let near = Vec2::new(160.5, 120.5);
    let (x, y) = doc.placement().to_doc(near);
    doc.state(x, y, ToolMods::default(), 1.0);
    assert_eq!(doc.snapped.map(|(point, _)| point), Some(Vec2::new(160.0, 120.0)));
    let preview = serde_json::to_value(doc.grip_overlays()).unwrap();
    let (x, y) = doc.placement().to_doc(Vec2::new(160.0, 120.0));
    doc.state(x, y, ToolMods::default(), 1.0);
    assert_eq!(serde_json::to_value(doc.grip_overlays()).unwrap(), preview);
    click_world(&mut doc, near);
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([160.0, 120.0]));
}

#[test]
fn grip_pointer_obeys_ortho_but_explicit_coordinates_remain_exact() {
    for (raw, expected) in [(Vec2::new(145.0, 112.0), json!([145.0, 100.0])), (Vec2::new(112.0, 145.0), json!([100.0, 145.0]))] {
        let mut doc = circle_document();
        doc.session.settings.osmode = 0;
        doc.session.settings.orthomode = true;
        start_circle_grip(&mut doc);
        click_world(&mut doc, raw);
        assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], expected);
    }
    let mut doc = circle_document();
    doc.session.settings.orthomode = true;
    start_circle_grip(&mut doc);
    doc.command_line("@45,12").unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([145.0, 112.0]));
}

#[test]
fn grip_pointer_obeys_grid_snap_without_changing_the_typed_destination() {
    let mut doc = circle_document();
    doc.session.settings.osmode = 0;
    doc.session.settings.snapmode = true;
    doc.session.settings.snapunit = Vec2::new(10.0, 10.0);
    start_circle_grip(&mut doc);
    click_world(&mut doc, Vec2::new(143.0, 117.0));
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([140.0, 120.0]));
}

#[test]
fn delete_cancels_the_hot_grip_and_undo_restores_the_object() {
    let mut doc = circle_document();
    start_circle_grip(&mut doc);
    doc.key("Delete", ToolMods::default(), 1.0).unwrap();
    assert!(doc.hot_grip.is_none());
    assert_eq!(doc.inspect().unwrap()["object.query"], 0);
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["object.query"], 1);
    assert!(!doc.dirty());
}

fn has_polar_guide(overlays: &[ToolOverlay]) -> bool {
    overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Label { text, .. } if text == "Polar"))
}

#[test]
fn polar_grip_preview_commit_and_undo_share_the_constrained_destination() {
    let mut doc = circle_document();
    doc.session.settings.osmode = 0;
    start_circle_grip(&mut doc);
    let raw = Vec2::new(145.0, 100.0 + doc.session.pixel_size() * 3.0);
    let (x, y) = doc.placement().to_doc(raw);
    doc.state(x, y, ToolMods::default(), 1.0);
    let preview = serde_json::to_value(doc.grip_overlays()).unwrap();
    assert!(has_polar_guide(&doc.grip_overlays()));
    let (x, y) = doc.placement().to_doc(Vec2::new(145.0, 100.0));
    doc.state(x, y, ToolMods::default(), 1.0);
    assert_eq!(serde_json::to_value(doc.grip_overlays()).unwrap(), preview);
    assert!(!doc.dirty());
    click_world(&mut doc, raw);
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([145.0, 100.0]));
    assert!(!has_polar_guide(&doc.overlays()));
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty());
}

#[test]
fn polar_grip_honors_increment_additional_angles_and_angle_base() {
    for additional in [false, true] {
        let mut doc = circle_document();
        doc.session.settings.osmode = 0;
        if additional {
            doc.execute("variable.set", json!({"name": "ANGBASE", "value": 15})).unwrap();
            doc.execute("variable.set", json!({"name": "POLARMODE", "value": 4})).unwrap();
            doc.execute("variable.set", json!({"name": "POLARADDANG", "value": "30"})).unwrap();
        } else {
            doc.execute("variable.set", json!({"name": "POLARANG", "value": 45})).unwrap();
        }
        start_circle_grip(&mut doc);
        click_world(&mut doc, Vec2::new(140.0, 139.0));
        let object = &doc.inspect().unwrap()["selectedObjects"][0];
        for axis in [0, 1] {
            assert!((object["center"][axis].as_f64().unwrap() - 139.5).abs() < 1e-9);
        }
    }
}

#[test]
fn polar_grip_tolerance_scales_with_zoom_and_modes_clear_the_guide() {
    let mut doc = circle_document();
    doc.session.settings.osmode = 0;
    start_circle_grip(&mut doc);
    for zoom in [0.5, 2.0] {
        doc.sync_view(zoom);
        let pixel = doc.session.pixel_size();
        for (offset, expected) in [(3.0, true), (12.0, false)] {
            let (x, y) = doc.placement().to_doc(Vec2::new(145.0, 100.0 + pixel * offset));
            doc.state(x, y, ToolMods::default(), zoom);
            assert_eq!(has_polar_guide(&doc.grip_overlays()), expected, "zoom {zoom}, offset {offset}");
        }
    }
    doc.sync_view(1.0);
    let (x, y) = doc.placement().to_doc(Vec2::new(145.0, 100.0 + doc.session.pixel_size() * 3.0));
    doc.state(x, y, ToolMods::default(), 1.0);
    assert!(has_polar_guide(&doc.grip_overlays()));
    doc.execute("polar", json!({"on": false})).unwrap();
    assert!(doc.hot_grip.is_some());
    assert!(!has_polar_guide(&doc.grip_overlays()));
    doc.execute("polar", json!({"on": true})).unwrap();
    assert!(has_polar_guide(&doc.grip_overlays()));
    doc.execute("ortho", json!({"on": true})).unwrap();
    assert!(!has_polar_guide(&doc.grip_overlays()), "Ortho suppresses polar tracking");
    doc.execute("ortho", json!({"on": false})).unwrap();
    assert!(has_polar_guide(&doc.grip_overlays()));
    doc.command_line("145,101").unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([145.0, 101.0]), "typed coordinates stay exact");
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty());
}

#[test]
fn endpoint_snap_takes_priority_over_a_nearby_polar_grip_path() {
    let mut doc = circle_document();
    doc.execute("line", json!({"points": [[160, 101], [190, 130]]})).unwrap();
    doc.session.settings.osmode = kition_cad_engine::snap::mode::END;
    start_circle_grip(&mut doc);
    let raw = Vec2::new(160.2, 100.9);
    let (x, y) = doc.placement().to_doc(raw);
    doc.state(x, y, ToolMods::default(), 1.0);
    assert_eq!(doc.snapped.map(|(point, _)| point), Some(Vec2::new(160.0, 101.0)));
    assert!(!has_polar_guide(&doc.grip_overlays()));
    click_world(&mut doc, raw);
    assert_eq!(doc.inspect().unwrap()["selectedObjects"][0]["center"], json!([160.0, 101.0]));
}

#[test]
fn coincident_grip_preview_matches_all_committed_objects_and_skips_locked_members() {
    use kition_cad_doc::EntityKind;
    for locked in [false, true] {
        let mut doc = CadDocument::create("Connected.dxf", 0, 0).unwrap();
        for end in [[160, 100], [100, 160]] {
            doc.execute("line", json!({"points": [[100, 100], end]})).unwrap();
        }
        doc.execute("line", json!({"points": [[40, 40], [70, 40]]})).unwrap();
        doc.execute("layer.add", json!({"name": "Protected", "current": true})).unwrap();
        doc.execute("line", json!({"points": [[100, 100], [60, 140]]})).unwrap();
        doc.execute("layer.set", json!({"name": "Protected", "locked": locked})).unwrap();
        let mut doc = CadDocument::open("Connected.dxf", &doc.save(Some("Connected.dxf")).unwrap()).unwrap();
        doc.session.settings.osmode = 0;
        // Preserve locked members as a selection made before their layer was locked would.
        let handles = doc.session.doc().unwrap().model.iter().map(|entity| entity.handle).collect();
        doc.session.set_selection(handles);
        assert_eq!(doc.session.selection().len(), 4);
        click_world(&mut doc, Vec2::new(100.0, 100.0));
        assert!(doc.hot_grip.is_some());
        let destination = Vec2::new(120.0, 115.0);
        let (x, y) = doc.placement().to_doc(destination);
        doc.state(x, y, ToolMods::default(), 1.0);
        let preview: Vec<String> = doc
            .grip_overlays()
            .into_iter()
            .filter_map(|o| match o {
                ToolOverlay::Path { d, .. } => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(preview.len(), if locked { 2 } else { 3 }, "every editable coincident object must be previewed");
        assert!(!doc.dirty(), "previews must not mutate the document");
        click_world(&mut doc, destination);
        let changed: Vec<_> = doc
            .session
            .doc()
            .unwrap()
            .model
            .iter()
            .filter(|e| matches!(&e.kind, EntityKind::Line(line) if line.a.xy().near(destination, 1e-9)))
            .map(|e| &**e)
            .collect();
        assert_eq!(changed.len(), preview.len());
        let list = kition_cad_render::build_entities(
            doc.session.doc().unwrap(),
            changed,
            &kition_cad_render::Options { tolerance: doc.tolerance(), ..Default::default() },
        );
        let committed: Vec<String> = doc
            .paths(&list, &doc.placement(), ACCENT, 1.5, false)
            .into_iter()
            .filter_map(|o| match o {
                ToolOverlay::Path { d, .. } => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(preview, committed, "preview geometry must match the full committed edit");
        doc.execute("edit.undo", json!({})).unwrap();
        assert!(!doc.dirty(), "one Undo restores the whole connected edit");
        doc.execute("edit.redo", json!({})).unwrap();
        let reopened = CadDocument::open("Connected.dxf", &doc.save(Some("Connected.dxf")).unwrap()).unwrap();
        assert_eq!(
            reopened
                .session
                .doc()
                .unwrap()
                .model
                .iter()
                .filter(|e| { matches!(&e.kind, EntityKind::Line(line) if line.a.xy().near(destination, 1e-9)) })
                .count(),
            preview.len()
        );
    }
}

#[test]
fn invalid_primary_grip_has_no_partial_preview_or_edit_and_escape_cancels_it() {
    let mut doc = circle_document();
    doc.execute("line", json!({"points": [[130, 100], [160, 140]]})).unwrap();
    let mut doc = CadDocument::open("Grips.dxf", &doc.save(Some("Grips.dxf")).unwrap()).unwrap();
    doc.execute("edit.selectAll", json!({})).unwrap();
    click_world(&mut doc, Vec2::new(130.0, 100.0));
    let (x, y) = doc.placement().to_doc(Vec2::new(100.0, 100.0));
    doc.state(x, y, ToolMods::default(), 1.0);
    assert!(doc.grip_overlays().is_empty(), "a zero-radius primary must not preview edits to connected objects");
    assert!(doc.command_line("100,100").is_err());
    assert!(!doc.dirty());
    assert!(doc.hot_grip.is_some(), "invalid input allows a corrected destination");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(doc.hot_grip.is_none());
    assert!(doc.snapped.is_none());
    assert!(doc.grip_overlays().is_empty());
    assert!(!doc.dirty());
}

#[test]
fn viewport_commands_publish_a_view_without_marking_the_drawing_dirty() {
    let mut doc = circle_document();
    let viewport = json!({"x": 30, "y": 40, "width": 1200, "height": 700, "zoom": 2});
    doc.execute("viewport.sync", viewport.clone()).unwrap();
    assert_eq!(doc.session.viewport_px, (1200.0, 700.0));
    assert!(doc.view_request.is_none());
    let original = doc.current_view();
    doc.command_line("view.zoomToFit").unwrap();
    assert_ne!(doc.current_view(), original);
    assert_eq!(doc.inspect().unwrap()["viewRequest"]["serial"], 1);
    assert!(!doc.dirty());
    doc.execute("viewport.sync", viewport).unwrap();
    assert_eq!(doc.inspect().unwrap()["viewRequest"]["serial"], 1, "synchronizing the host must not request another view");
    doc.execute("view.zoomIn", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["viewRequest"]["serial"], 2);
    for zoom in [0.0, -1.0, f64::INFINITY] {
        assert!(doc.execute("viewport.sync", json!({"x": 0, "y": 0, "width": 800, "height": 600, "zoom": zoom})).is_err());
    }
}

#[test]
fn grid_toggle_changes_only_the_background_and_keeps_geometry_and_dirty_state() {
    let mut doc = circle_document();
    let view = doc.placement();
    let (x, y) = view.to_doc(Vec2::new(0.0, 20.0));
    doc.execute("grid", json!({"on": false})).unwrap();
    let blank = doc.render(x, y, 1.0, 80, 80).unwrap();
    doc.execute("grid", json!({"on": true})).unwrap();
    let grid = doc.render(x, y, 1.0, 80, 80).unwrap();
    assert_ne!(grid, blank);
    assert!(grid.as_chunks::<4>().0.iter().all(|pixel| *pixel == [255, 255, 255, 255] || *pixel == [235, 233, 229, 255]));
    assert!(!doc.dirty());
    assert_eq!(doc.inspect().unwrap()["object.query"], 1);
}

#[test]
fn draws_a_line_with_the_pointer_and_snaps_to_its_end() {
    let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
    let place = doc.placement();
    doc.select_tool("line", 1.0).unwrap();
    assert_eq!(doc.tool(), "line");
    // Close to the first line's end (100, 60): the endpoint snap takes the click there.
    let (sx, sy) = place.to_doc(Vec2::new(100.4, 60.3));
    let response = doc.state(sx, sy, ToolMods::default(), 1.0);
    assert!(
        response.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Label { text, .. } if text.contains("ndpoint"))),
        "{:?}",
        response.overlays
    );
    click(&mut doc, sx, sy);
    let (tx, ty) = place.to_doc(Vec2::new(200.0, 60.0));
    let response = doc.state(tx, ty, ToolMods::default(), 1.0);
    assert!(response.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Path { .. })), "the rubber band follows the cursor");
    assert!(matches!(response.requests.first(), Some(ToolRequest::Status { text }) if !text.is_empty()));
    click(&mut doc, tx, ty);
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.tool(), "select");
    assert_eq!(doc.inspect().unwrap()["object.query"], 2);
    assert!(doc.dirty());
    let saved = CadDocument::open("Plan.dxf", &doc.save(Some("Plan.dxf")).unwrap()).unwrap();
    assert_eq!(saved.inspect().unwrap()["object.query"], 2);
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["object.query"], 1);
}

#[test]
fn picks_and_window_selects_with_no_command_running() {
    let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
    let place = doc.placement();
    let (x, y) = place.to_doc(Vec2::new(55.0, 35.0));
    let response = click(&mut doc, x, y);
    assert_eq!(doc.inspect().unwrap()["selection"], 1);
    assert!(response.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Anchor { .. })), "the selection shows its grips");
    assert!(response.busy, "editing keys go to the engine while objects are selected");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.inspect().unwrap()["selection"], 0);
    // Clicking empty paper twice opens and closes a window around the line.
    let (ax, ay) = place.to_doc(Vec2::new(0.0, 80.0));
    let (bx, by) = place.to_doc(Vec2::new(150.0, 0.0));
    click(&mut doc, ax, ay);
    click(&mut doc, bx, by);
    assert_eq!(doc.inspect().unwrap()["selection"], 1);
    doc.key("Delete", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.inspect().unwrap()["object.query"], 0);
}

#[test]
fn the_command_line_runs_aliases_coordinates_and_enter() {
    let mut doc = CadDocument::create("New.dxf", 0, 0).unwrap();
    // As the host does: the command line through `execute`, and the rail following it.
    doc.execute("command.runLine", json!({ "text": "line" })).unwrap();
    let switched = doc.state(0.0, 0.0, ToolMods::default(), 1.0);
    assert!(switched.requests.iter().any(|request| matches!(request, ToolRequest::SwitchTool { id } if id == "line")));
    assert!(doc.select_tool("line", 1.0).is_ok());
    assert_eq!(doc.tool(), "line", "following the switch keeps the command running");
    let ended = doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(ended.requests.iter().any(|request| matches!(request, ToolRequest::SwitchTool { id } if id == "select")));
    assert!(doc.commands().iter().any(|command| command.id == "command.runLine" && command.schema.is_some()));
    assert!(!doc.dirty());
    assert!(doc.disabled_commands().contains(&"edit.undo".to_owned()));
    doc.command_line("c").unwrap();
    assert_eq!(doc.tool(), "circle");
    doc.command_line("50,50").unwrap();
    doc.command_line("20").unwrap();
    assert_eq!(doc.inspect().unwrap()["object.query"], 1);
    assert!(!doc.disabled_commands().contains(&"edit.undo".to_owned()));
    assert!(doc.save(Some("New.dwg")).unwrap_err().contains(".dxf"));
}

#[test]
fn keeps_the_kit_contract() {
    check_contract::<CadDocument>(&fixture(), "Plan.dxf", |doc| {
        doc.command_line("circle")?;
        doc.command_line("200,150")?;
        doc.command_line("40").map(|_| ())
    })
    .unwrap();
}

#[test]
fn the_command_line_cannot_read_or_write_files() {
    let mut doc = CadDocument::create("New.dxf", 0, 0).unwrap();
    for entry in ["file.saveAs", "w", "open", "insert", "exportpdf {\"path\": \"/tmp/x.pdf\"}", "recover"] {
        assert!(doc.command_line(entry).is_err(), "{entry}");
        assert_eq!(doc.tool(), "select", "{entry}");
    }
    assert!(names_a_file("{path, format?: dxf|dwg}"));
    assert!(!names_a_file("{center: [x,y], radius}"));
    assert!(doc.command_line("circle").is_ok());
}

#[test]
fn reports_the_drafting_modes() {
    use kition_kit::KitDocument;
    let mut doc = CadDocument::create("Drawing.dxf", 420, 297).unwrap();
    doc.execute("ortho", json!({ "on": true })).unwrap();
    doc.execute("grid", json!({ "on": false })).unwrap();
    let drafting = &doc.inspect().unwrap()["drafting"];
    assert_eq!(drafting["ortho"], json!(true));
    assert_eq!(drafting["grid"], json!(false));
    doc.execute("drafting.objectSnap", json!({ "on": true })).unwrap();
    assert_eq!(doc.inspect().unwrap()["drafting"]["drafting.objectSnap"], json!(true));
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    assert_eq!(doc.inspect().unwrap()["drafting"]["drafting.objectSnap"], json!(false));
}

#[test]
fn the_catalog_flags_as_mutating_exactly_what_changes_the_drawing() {
    let doc = CadDocument::create("Flags.dxf", 0, 0).unwrap();
    let mutates = |id: &str| doc.commands().into_iter().find(|command| command.id == id).unwrap_or_else(|| panic!("no {id}")).mutates;
    for id in ["line", "erase", "layer.add", "edit.undo", "edit.redo", "u", "undo", "redo", "new", "open", "file.save", "command.runLine"] {
        assert!(mutates(id), "{id} changes the drawing");
    }
    for id in ["edit.selectAll", "view.zoomIn", "view.panLeft", "grid", "list", "plot", "file.exportPdf", "file.bytes", "edit.copy"] {
        assert!(!mutates(id), "{id} does not change the drawing");
    }
}

#[test]
fn commands_flagged_as_not_mutating_never_commit_to_the_replica() {
    let bytes = fixture();
    let commands = CadDocument::open("Plan.dxf", &bytes).unwrap().commands();
    // Commands that read or write files are left out: the test must not touch any.
    for command in commands.iter().filter(|command| !command.mutates && !names_a_file(command.params.as_deref().unwrap_or_default())) {
        let mut doc = CadDocument::open("Plan.dxf", &bytes).unwrap();
        doc.execute("edit.selectAll", json!({})).unwrap();
        let before = doc.version().unwrap();
        let _ = doc.execute(&command.id, json!({}));
        assert_eq!(doc.version().unwrap(), before, "{} committed to the replica", command.id);
    }
}
