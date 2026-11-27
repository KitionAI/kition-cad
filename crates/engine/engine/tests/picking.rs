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
