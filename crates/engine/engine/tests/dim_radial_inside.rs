//! DIMRADIUS / DIMDIAMETER: a dimension line location puts the text there: inside the circle on
//! the dimension line, outside at the end of the line led out from the arrowhead.

use kition_cad_engine::Session;
use kition_cad_engine::doc::{Dimension, EntityKind};
use kition_cad_engine::geom::Vec2;
use kition_cad_engine::render::{DimGeometry, LineRole};
use serde_json::json;

fn last_dim(s: &Session) -> (Dimension, DimGeometry) {
    let d = s.doc().unwrap();
    match &d.model.last().unwrap().kind {
        EntityKind::Dimension(dm) => (dm.clone(), kition_cad_engine::render::dimension_in(d, dm)),
        other => panic!("not a dimension: {other:?}"),
    }
}

/// Whether a dimension line passes over `p` (on the x axis).
fn covers(g: &DimGeometry, x: f64) -> bool {
    g.lines_of(LineRole::Dim)
        .any(|l| l.windows(2).any(|w| w[0].y.abs() < 1e-9 && w[1].y.abs() < 1e-9 && w[0].x.min(w[1].x) < x && x < w[0].x.max(w[1].x)))
}

#[test]
fn radial_text_goes_inside_when_the_location_is_inside() {
    let mut s = Session::new();
    s.execute("circle", &json!({"center": [0, 0], "radius": 10})).unwrap();

    // Typed: pick the circle, then a location inside.
    s.script("dimradius 10,0 4,0\n").unwrap();
    assert!(s.running.is_none());
    let (dm, g) = last_dim(&s);
    assert!(dm.user_text_pos);
    assert!(g.text_pos.dist(Vec2::new(4.0, 0.0)) < 1e-9, "{:?}", g.text_pos);
    // The dimension line runs from beyond the text to the arrowhead and stays inside the circle.
    assert!(covers(&g, 9.0) && !covers(&g, 4.0) && !covers(&g, 1.0));
    assert!(g.lines_of(LineRole::Dim).flatten().all(|p| p.len() <= 10.0 + 1e-9));

    // Outside: the text at the location, the dimension line led out to it.
    s.script("dimradius 0,10 0,15\n").unwrap();
    assert!(s.running.is_none());
    let (dm, g) = last_dim(&s);
    assert!(dm.user_text_pos);
    assert!(g.text_pos.dist(Vec2::new(0.0, 15.0)) < 1e-9, "{:?}", g.text_pos);

    // JSON form, diameter: the line spans the circle, broken around the text.
    let c = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
    s.execute("dimension.addDiameter", &json!({"handle": c, "at": [-3, 0]})).unwrap();
    let (_, g) = last_dim(&s);
    assert!(g.text_pos.dist(Vec2::new(-3.0, 0.0)) < 1e-9, "{:?}", g.text_pos);
    assert!(covers(&g, -9.5) && covers(&g, 9.5) && !covers(&g, -3.0));
}
