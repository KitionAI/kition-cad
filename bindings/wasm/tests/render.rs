//! A preview by explicit view (kition-sdk `agent.render`; task card C4/C5/C6-cad): `file.render`
//! draws what its call names (a space by `layout`, a `window` in drawing units, an entity by
//! `objectId`, a named `view`), by default Model space fitted to its extents, as a function of the
//! drawing and the call alone: never the space shown, the view, the selection or the zoom, and
//! never a change. Its `window` and `unitsPerPixel` turn a pixel into drawing coordinates.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_kit::{CadDocument, EDITOR_STATE};
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn handle(answer: &Value) -> String {
    answer["handle"].as_str().or_else(|| answer["handles"][0].as_str()).unwrap_or_else(|| panic!("no handle in {answer}")).to_owned()
}

/// Model space: a 100 x 50 rectangle at the origin and a circle of radius 10 at (200, 25) on layer
/// `Marks`. Layout1: a 20 x 20 square at (10, 10) of paper space. A named view `Detail` around the
/// circle. The person is on Layout1 with the square selected and zoomed in.
fn drawing() -> (CadDocument, String) {
    let bytes = CadDocument::create("Plan.dxf", 420, 297).unwrap().save(Some("Plan.dxf")).unwrap();
    let mut d = kition_cad_io::read(&bytes, "Plan.dxf").unwrap();
    d.views.push(kition_cad_doc::NamedView {
        name: "Detail".into(),
        center: kition_cad_geom::Vec2::new(200.0, 25.0),
        height: 30.0,
        width: 30.0,
        layer_state: None,
    });
    let bytes = kition_cad_io::write(&d, "Plan.dxf").unwrap();
    let mut doc = CadDocument::open("Plan.dxf", &bytes).unwrap();
    doc.execute("rectangle.add", json!({ "p1": [0, 0], "p2": [100, 50] })).unwrap();
    doc.execute("layer.add", json!({ "name": "Marks" })).unwrap();
    let circle = handle(&doc.execute("circle.add", json!({ "center": [200, 25], "radius": 10, "layer": "Marks" })).unwrap());
    doc.execute("layout.set", json!({ "name": "Layout1" })).unwrap();
    let square = handle(&doc.execute("rectangle.add", json!({ "p1": [10, 10], "p2": [30, 30] })).unwrap());
    doc.execute("select.set", json!({ "handles": [square] })).unwrap();
    doc.execute("view.zoomRealtime", json!({ "mode": "in" })).ok();
    (doc, circle)
}

fn render(doc: &mut CadDocument, params: Value) -> Value {
    doc.execute("file.render", params.clone()).unwrap_or_else(|e| panic!("{params}: {e}"))
}

fn image(answer: &Value) -> image::RgbaImage {
    let png = kition_cad_engine::cmd::file::base64_decode(answer["png"].as_str().unwrap()).unwrap();
    image::load_from_memory(&png).unwrap().to_rgba8()
}

fn dark(px: &image::Rgba<u8>) -> bool {
    px.0[0] < 128 && px.0[1] < 128 && px.0[2] < 128
}

fn editor(doc: &CadDocument) -> Value {
    let inspected = doc.inspect().unwrap();
    json!(EDITOR_STATE.iter().map(|pointer| inspected.pointer(pointer).cloned()).collect::<Vec<_>>())
}

#[test]
fn the_default_view_is_model_space_fitted_to_its_extents_whatever_the_editor_shows() {
    let (mut doc, _) = drawing();
    let before = (editor(&doc), doc.revision(), doc.dirty());
    let answer = render(&mut doc, json!({}));
    assert_eq!((answer["layout"].clone(), answer["mimeType"].clone()), (json!("Model"), json!("image/png")), "{answer}");
    let window: Vec<f64> = answer["window"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(window[0] < 0.0 && window[1] < 0.0 && window[2] > 210.0 && window[3] > 50.0, "the extents with a margin: {window:?}");
    assert_eq!((editor(&doc), doc.revision(), doc.dirty()), before, "rendering moved nothing");
    let img = image(&answer);
    assert_eq!((u64::from(img.width()), u64::from(img.height())), (answer["width"].as_u64().unwrap(), answer["height"].as_u64().unwrap()));
    assert_eq!(img.width().max(img.height()), 1024, "the default size");
}

#[test]
fn the_same_call_answers_the_same_bytes() {
    let (mut doc, _) = drawing();
    let a = render(&mut doc, json!({ "target": { "window": [-10, -10, 110, 60] } }));
    doc.execute("layout.set", json!({ "name": "Model" })).unwrap();
    doc.execute("view.zoomRealtime", json!({ "mode": "extents" })).ok();
    let b = render(&mut doc, json!({ "target": { "window": [-10, -10, 110, 60] } }));
    assert_eq!(a, b);
}

#[test]
fn two_windows_differ_and_a_layout_differs_from_model_space() {
    let (mut doc, _) = drawing();
    let left = render(&mut doc, json!({ "target": { "window": [-10, -10, 110, 60] } }));
    let right = render(&mut doc, json!({ "target": { "window": [180, 5, 220, 45] } }));
    assert_ne!(left["png"], right["png"]);
    let model = render(&mut doc, json!({ "target": { "layout": "Model" } }));
    let layout = render(&mut doc, json!({ "target": { "layout": "Layout1" } }));
    assert_eq!(layout["layout"], "Layout1");
    assert_ne!(model["png"], layout["png"]);
}

#[test]
fn window_and_units_per_pixel_map_a_known_point_to_its_pixel() {
    let (mut doc, _) = drawing();
    // The rectangle's corner (100, 50) in a 200 x 100 window at 2 pixels per unit.
    let answer = render(&mut doc, json!({ "target": { "window": [0, 0, 200, 100] }, "scale": 2 }));
    assert_eq!((answer["width"].clone(), answer["height"].clone(), answer["unitsPerPixel"].clone()), (json!(400), json!(200), json!(0.5)));
    let window: Vec<f64> = answer["window"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(window, [0.0, 0.0, 200.0, 100.0]);
    // x = window[0] + px * unitsPerPixel, y = window[3] - py * unitsPerPixel.
    let to_pixel = |x: f64, y: f64| (((x - window[0]) / 0.5) as u32, ((window[3] - y) / 0.5) as u32);
    let img = image(&answer);
    let (px, py) = to_pixel(100.0, 50.0);
    let near = |px: u32, py: u32| {
        (px.saturating_sub(2)..=px + 2)
            .any(|x| (py.saturating_sub(2)..=py + 2).any(|y| x < img.width() && y < img.height() && dark(img.get_pixel(x, y))))
    };
    assert!(near(px, py), "the corner (100, 50) is drawn at pixel ({px}, {py})");
    let (ex, ey) = to_pixel(150.0, 80.0);
    assert!(!near(ex, ey), "nothing is drawn at (150, 80)");
}

#[test]
fn scale_and_max_size() {
    let (mut doc, _) = drawing();
    let answer = render(&mut doc, json!({ "target": { "window": [0, 0, 200, 100] }, "scale": 1 }));
    assert_eq!((answer["width"].clone(), answer["height"].clone()), (json!(200), json!(100)));
    let capped = render(&mut doc, json!({ "target": { "window": [0, 0, 200, 100] }, "scale": 100, "maxSize": 300 }));
    assert_eq!((capped["width"].clone(), capped["height"].clone(), capped["scale"].clone()), (json!(300), json!(150), json!(1.5)));
    let fitted = render(&mut doc, json!({ "target": { "window": [0, 0, 200, 100] }, "maxSize": 64 }));
    assert_eq!(fitted["width"], 64);
}

#[test]
fn an_object_a_named_view_and_a_layer_subset() {
    let (mut doc, circle) = drawing();
    let object = render(&mut doc, json!({ "target": { "objectId": circle } }));
    let window: Vec<f64> = object["window"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(window[0] < 190.0 && window[2] > 210.0 && window[0] > 150.0, "framed to the circle with a margin: {window:?}");
    assert_eq!(object["objectId"], json!(circle));
    let view = render(&mut doc, json!({ "target": { "view": "Detail" } }));
    let window: Vec<f64> = view["window"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!((window[0] - 185.0).abs() < 1e-6 && (window[3] - 40.0).abs() < 1e-6, "{window:?}");
    let all = render(&mut doc, json!({ "target": { "window": [180, 5, 220, 45] } }));
    let without = render(&mut doc, json!({ "target": { "window": [180, 5, 220, 45] }, "layers": ["0"] }));
    assert!(image(&all).pixels().any(dark) && !image(&without).pixels().any(dark), "only layer 0 drawn: no circle");
}

#[test]
fn what_cannot_be_drawn_is_refused_by_type() {
    let (mut doc, circle) = drawing();
    for (params, says) in [
        (json!({ "target": { "layout": "Nope" } }), "not-found"),
        (json!({ "target": { "objectId": "FFFFFF" } }), "not-found"),
        (json!({ "target": { "view": "Nope" } }), "not-found"),
        (json!({ "target": { "window": [0, 0, 0, 10] } }), "invalid-params"),
        (json!({ "target": { "window": [0, 0, 10] } }), "invalid-params"),
        (json!({ "target": { "window": [0, 0, 10, 10], "objectId": circle } }), "invalid-params"),
        (json!({ "target": { "objectId": circle, "layout": "Layout1" } }), "invalid-params"),
        (json!({ "target": { "slide": 1 } }), "invalid-params"),
        (json!({ "scale": 0 }), "invalid-params"),
        (json!({ "maxSize": 0 }), "invalid-params"),
        (json!({ "layers": ["Nope"] }), "not-found"),
    ] {
        let error = doc.execute("file.render", params.clone()).unwrap_err();
        assert!(error.starts_with(says), "{params}: {error}");
    }
}
