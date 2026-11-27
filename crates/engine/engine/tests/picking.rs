//! Picking takes what is drawn under the cursor, and the rollover highlight shows the same object.

use kition_cad_engine::doc::{EntityKind, entity_bounds};
use kition_cad_engine::geom::Vec2;
use kition_cad_engine::{Input, Session, select};
use serde_json::json;

/// The acceptance drawing in WebKit (stage 7): BLOCK's "Select objects" on the bed's lower edge
/// took the 120 dimension below the bed instead. The pointer lands on whole pixels there, 0.43
/// units inside the bed, and the dimension was hit along the outline of its bounding box (its
/// definition points with room for the text and arrows), which is not drawn and ran just inside the
/// bed, closer to the pointer than the bed's edge. A dimension is now hit by its lines, arrows and
/// the box of its text: the bed is picked, highlighted and made the block.
#[test]
fn issue_webkit_block_pick_takes_the_object_under_the_cursor() {
    let mut s = Session::new();
    s.cmdline("dimlinear").unwrap();
    for point in ["100,60", "220,60", "160,48.1"] {
        s.cmdline(point).unwrap();
    }
    assert!(s.running.is_none(), "{}", s.prompt_text());
    let doc = s.doc().unwrap();
    let dimension = doc.model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().clone();
    // The bed's lower edge a quarter unit under the top of the dimension's bounding box.
    let top = entity_bounds(doc, &dimension, 0).max.y;
    assert!(top > 60.0, "the bounds keep room above the definition points: {top}");
    let edge = top - 0.25;
    s.execute("rectangle.add", &json!({ "p1": [195, edge], "p2": [213, edge + 14.0] })).unwrap();
    let bed = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::LwPolyline(_))).unwrap().handle;
    let at = Vec2::new(203.65, edge + 0.43);
    let aperture = 1.0;
    let doc = s.doc().unwrap();
    assert!(select::entity_distance(doc, &dimension, at, 0.1) > 1.0, "nothing of the dimension is drawn there");
    assert_eq!(select::pick(doc, &s.space(), at, aperture), Some(bed), "the pick");
    assert_eq!(s.hover(at, aperture), Some(bed), "the rollover highlight");
    // The dimension's text still picks it, inside its box.
    let text = kition_cad_engine::render::dimension_in(
        doc,
        match &dimension.kind {
            EntityKind::Dimension(dm) => dm,
            _ => unreachable!(),
        },
    )
    .text_pos;
    assert_eq!(select::pick(doc, &s.space(), text, 0.01), Some(dimension.handle), "the text at {text:?}");
    // BLOCK, as typed in the acceptance drawing.
    s.pick_tolerance = Some(aperture);
    for entry in ["block", "Bed", "195,66"] {
        s.cmdline(entry).unwrap();
    }
    assert!(s.prompt_text().contains("Select objects"), "{}", s.prompt_text());
    s.input(Input::Point(at)).unwrap();
    assert_eq!(s.selection(), vec![bed]);
}

/// A block reference is hit by what it draws: its lines and arcs, and the box of each text in it
/// (stage 9's insert dialog places blocks by hand). Before, it was also hit along the outline of
/// its bounding box, which is not drawn: a round chair's box corner, far from the chair, took the
/// pick from the wall line next to it, and on empty paper near the corner the chair was picked.
#[test]
fn issue_block_pick_outline_takes_what_is_drawn() {
    let mut s = Session::new();
    let made = s.execute("circle.add", &json!({ "center": [400, 250], "radius": 4 })).unwrap();
    let chair = made["handle"].as_str().unwrap().to_owned();
    let label = s.execute("text.add", &json!({ "at": [398, 249], "height": 1, "text": "CH" })).unwrap();
    let label = label["handle"].as_str().unwrap().to_owned();
    s.execute("block.add", &json!({ "name": "Chair", "base": [400, 250], "handles": [chair, label] })).unwrap();
    s.execute("block.insert", &json!({ "name": "Chair", "at": [400, 250] })).unwrap();
    let doc = s.doc().unwrap();
    let insert = doc.model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().clone();
    let corner = entity_bounds(doc, &insert, 0).max;
    assert!((corner.x - 404.0).abs() < 0.5 && (corner.y - 254.0).abs() < 0.5, "the box's corner: {corner:?}");
    // Nothing is drawn at the box's corner: the circle is (4·√2 − 4) ≈ 1.66 away.
    let at = Vec2::new(corner.x - 0.1, corner.y - 0.1);
    let aperture = 1.0;
    assert!(select::entity_distance(doc, &insert, at, 0.01) > 1.0, "nothing of the block is drawn there");
    assert_eq!(select::pick(doc, &s.space(), at, aperture), None, "empty paper by the corner");
    assert_eq!(s.hover(at, aperture), None, "no rollover there either");
    // A wall line just outside the box's corner is what the pick takes.
    s.execute("line.add", &json!({ "points": [[corner.x + 0.6, 240], [corner.x + 0.6, 270]] })).unwrap();
    let doc = s.doc().unwrap();
    let wall = doc.model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
    assert_eq!(select::pick(doc, &s.space(), at, aperture), Some(wall), "the wall beside the corner");
    assert_eq!(s.hover(at, aperture), Some(wall), "the rollover shows the wall");
    // The chair's circle and the text inside it still pick the chair.
    assert_eq!(select::pick(doc, &s.space(), Vec2::new(404.0, 250.0), 0.1), Some(insert.handle), "on the circle");
    assert_eq!(select::pick(doc, &s.space(), Vec2::new(399.0, 249.5), 0.05), Some(insert.handle), "inside the text's box");
}
