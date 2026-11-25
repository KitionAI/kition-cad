//! Command-level round trips: a realistic drawing made only with commands survives SAVEAS and
//! OPEN of DXF, the kit's native format, through the real reader and writer (`kition-cad-io`).
//! (DWG is saved only by the native app; its losses are listed in `cad.file.drawings`.) The whole
//! drawing is compared in its id-blind canonical form (`kdm_model::canonical`: constraint ids
//! renumbered and the next handle zeroed), so any entity, table or property the file drops fails
//! the test.
//!
//! The first save adds what the writer derives from the drawing, which [`as_saved`] sets aside:
//! the file's own header values (`ACADVER`: the writer writes R2000, `AC1015`; `DWGCODEPAGE`,
//! `EXTMIN`/`EXTMAX`, `HANDSEED`), each dimension's drawn geometry as an anonymous `*D` block
//! (kept as the dimension's `block` until an edit regenerates it), the text position of a
//! dimension whose text the user did not move (`text_mid` is authoritative only with
//! `user_text_pos`). The LAYERP command's previous-layer snapshot (`KITION_CAD_LAYERP`) is one
//! person's editing state, not drawing content: each person's own (`kdm_model::CURRENT_VARS`, which
//! `canonical` leaves out), and the writer does not write it (AutoCAD keeps LAYERP's history for
//! the session only, and the DXF Reference has no header variable for it). Saving the reopened drawing again
//! must then reproduce it exactly, apart from the header values every save computes afresh.

use kition_cad_engine::Session;
use kition_cad_engine::cmd::file::{IoHooks, set_io};
use serde_json::{Value, json};

fn real_files() {
    set_io(IoHooks {
        read: |bytes, name| kition_cad_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |d, name| kition_cad_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

fn ok(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn handle(r: &Value) -> Value {
    r.get("handle").cloned().unwrap_or_else(|| r["handles"][0].clone())
}

/// The first JSON pointer at which two drawings differ, for a readable failure.
fn first_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => x
            .keys()
            .chain(y.keys())
            .find_map(|k| first_difference(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &format!("{at}/{k}"))),
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (u, v))| first_difference(u, v, &format!("{at}/{i}")))
        }
        _ if a == b => None,
        _ => Some(format!("{at}: {a} != {b}")),
    }
}

/// Header values the writer computes for the file it writes (see the module comment).
const FILE_HEADER: &[&str] = &["ACADVER", "DWGCODEPAGE", "EXTMIN", "EXTMAX", "HANDSEED"];

/// `canonical` without the header values the writer computes afresh on every save.
fn without_file_header(mut v: Value) -> Value {
    if let Some(vars) = v.pointer_mut("/header/vars").and_then(Value::as_object_mut) {
        vars.retain(|k, _| !FILE_HEADER.contains(&k.as_str()));
    }
    v
}

/// `canonical` without what the first save derives from the drawing (see the module comment).
fn as_saved(v: Value) -> Value {
    let mut v = without_file_header(v);
    if let Some(blocks) = v.get_mut("blocks").and_then(Value::as_object_mut) {
        blocks.retain(|name, b| !(name.starts_with("*D") && b.pointer("/block/anonymous") == Some(&json!(true))));
    }
    fn dimensions(v: &mut Value) {
        match v {
            Value::Object(o) => {
                if o.get("type") == Some(&json!("dimension")) && o.contains_key("userTextPos") {
                    o.remove("block");
                    if o.get("userTextPos") == Some(&json!(false)) {
                        o.remove("textMid");
                    }
                }
                o.values_mut().for_each(dimensions);
            }
            Value::Array(a) => a.iter_mut().for_each(dimensions),
            _ => {}
        }
    }
    dimensions(&mut v);
    v
}

fn reopen(s: &mut Session, path: &str) -> Session {
    ok(s, "file.saveAs", json!({ "path": path }));
    let mut reopened = Session::empty();
    ok(&mut reopened, "file.open", json!({ "path": path }));
    reopened
}

/// SAVEAS the drawing as DXF, OPEN it in a fresh session, and compare; then save the reopened
/// drawing again and require it to come back exactly.
fn survives_save_and_reopen(s: &mut Session, name: &str) {
    real_files();
    let before = as_saved(kdm_model::canonical(s.doc().unwrap()));
    let dir = std::env::temp_dir().join(format!("kition-cad-roundtrip-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ext = "dxf";
    let first = dir.join(format!("{name}.{ext}")).to_string_lossy().into_owned();
    let mut reopened = reopen(s, &first);
    let after = kdm_model::canonical(reopened.doc().unwrap());
    if let Some(d) = first_difference(&before, &as_saved(after.clone()), "") {
        panic!("saving as .{ext} and opening it again changed the drawing at {d}");
    }
    let second = dir.join(format!("{name}-again.{ext}")).to_string_lossy().into_owned();
    let again = kdm_model::canonical(reopen(&mut reopened, &second).doc().unwrap());
    if let Some(d) = first_difference(&without_file_header(after), &without_file_header(again), "") {
        panic!("saving the reopened .{ext} drawing again changed it at {d}");
    }

    std::fs::remove_dir_all(&dir).ok();
}

fn kinds(s: &Session) -> Vec<String> {
    s.doc().unwrap().model.iter().map(|e| format!("{:?}", e.kind).split(['(', ' ', '{']).next().unwrap().to_string()).collect()
}

#[test]
fn lines_curves_and_polylines_survive_save_and_reopen() {
    let mut s = Session::new();
    ok(&mut s, "line.add", json!({ "points": [[0, 0], [100, 0], [100, 50]] }));
    ok(&mut s, "circle.add", json!({ "center": [20, 20], "radius": 7.5 }));
    ok(&mut s, "arc.add", json!({ "center": [60, 20], "radius": 10, "start": 0, "end": 135 }));
    ok(&mut s, "polyline.add", json!({ "vertices": [[0, 60], [30, 60], [30, 90], [0, 90]], "closed": true }));
    ok(&mut s, "rectangle.add", json!({ "p1": [50, 60], "p2": [80, 80] }));
    let k = kinds(&s);
    assert_eq!(k.iter().filter(|k| *k == "Line").count(), 2, "{k:?}");
    assert!(["Circle", "Arc", "LwPolyline"].iter().all(|w| k.iter().any(|k| k == w)), "{k:?}");
    survives_save_and_reopen(&mut s, "geometry");
}

#[test]
fn text_and_mtext_survive_save_and_reopen() {
    let mut s = Session::new();
    ok(&mut s, "text.add", json!({ "at": [5, 5], "text": "Plan A", "height": 2.5, "justify": "TL" }));
    ok(
        &mut s,
        "multilineText.add",
        json!({ "at": [5, 20], "text": "Line one\nLine two", "justify": "BR", "lineSpacing": 1.25, "lineSpacingExact": true }),
    );
    let k = kinds(&s);
    assert!(k.contains(&"Text".to_string()) && k.contains(&"MText".to_string()), "{k:?}");
    survives_save_and_reopen(&mut s, "text");
}

#[test]
fn layers_survive_save_and_reopen() {
    let mut s = Session::new();
    ok(&mut s, "layer.add", json!({ "name": "Walls", "color": "red", "current": true }));
    ok(&mut s, "line.add", json!({ "points": [[0, 0], [50, 0]] }));
    ok(&mut s, "layer.add", json!({ "name": "Furniture" }));
    ok(&mut s, "layer.set", json!({ "name": "Furniture", "frozen": true, "color": 3 }));
    ok(&mut s, "layer.setCurrent", json!({ "name": "0" }));
    ok(&mut s, "circle.add", json!({ "center": [10, 10], "radius": 2 }));
    let d = s.doc().unwrap();
    assert!(d.model.iter().any(|e| e.common.layer == "Walls"));
    survives_save_and_reopen(&mut s, "layers");
}

#[test]
fn blocks_and_inserts_survive_save_and_reopen() {
    let mut s = Session::new();
    let a = handle(&ok(&mut s, "line.add", json!({ "points": [[0, 0], [10, 0]] })));
    let b = handle(&ok(&mut s, "circle.add", json!({ "center": [5, 5], "radius": 3 })));
    ok(&mut s, "block.addDialog", json!({ "name": "Washer", "base": [0, 0], "handles": [a, b], "keep": "convert" }));
    ok(&mut s, "block.insert", json!({ "name": "Washer", "at": [100, 0] }));
    ok(&mut s, "block.insert", json!({ "name": "Washer", "at": [200, 50] }));
    // The originals became an insert too (`keep: convert`).
    assert_eq!(kinds(&s).iter().filter(|k| *k == "Insert").count(), 3, "{:?}", kinds(&s));
    survives_save_and_reopen(&mut s, "blocks");
}

#[test]
fn dimensions_survive_save_and_reopen() {
    let mut s = Session::new();
    ok(&mut s, "line.add", json!({ "points": [[0, 0], [40, 0]] }));
    ok(&mut s, "dimension.addLinear", json!({ "p1": [0, 0], "p2": [40, 0], "at": [20, -6] }));
    ok(&mut s, "dimension.addAligned", json!({ "p1": [0, 10], "p2": [30, 30], "at": [10, 30] }));
    let c = handle(&ok(&mut s, "circle.add", json!({ "center": [80, 20], "radius": 10 })));
    ok(&mut s, "dimension.addRadius", json!({ "handle": c, "at": [95, 25] }));
    assert_eq!(kinds(&s).iter().filter(|k| *k == "Dimension").count(), 3, "{:?}", kinds(&s));
    survives_save_and_reopen(&mut s, "dimensions");
}

#[test]
fn modified_and_arrayed_objects_survive_save_and_reopen() {
    let mut s = Session::new();
    let l = handle(&ok(&mut s, "line.add", json!({ "points": [[0, 0], [10, 0]] })));
    let c = handle(&ok(&mut s, "circle.add", json!({ "center": [20, 0], "radius": 2 })));
    ok(&mut s, "object.move", json!({ "handles": [l], "delta": [3, 2] }));
    ok(&mut s, "object.rotate", json!({ "handles": [l], "base": [0, 0], "angle": 30 }));
    ok(&mut s, "object.scale", json!({ "handles": [c], "base": [20, 0], "factor": 2 }));
    ok(&mut s, "object.copy", json!({ "handles": [c], "delta": [0, 30] }));
    ok(&mut s, "object.mirror", json!({ "handles": [l], "p1": [-1, 0], "p2": [-1, 1] }));
    ok(&mut s, "object.array", json!({ "type": "polar", "handles": [c], "center": [0, 0], "count": 3 }));
    let r = handle(&ok(&mut s, "rectangle.add", json!({ "p1": [40, 40], "p2": [60, 60] })));
    ok(&mut s, "hatch.add", json!({ "handles": [r] }));
    assert!(kinds(&s).iter().any(|k| k == "Hatch"), "{:?}", kinds(&s));
    survives_save_and_reopen(&mut s, "modify");
}

/// `$EXTMIN`/`$EXTMAX` of a saved DXF: (x, y) pairs after the variable's name.
fn saved_extents(path: &str) -> [(f64, f64); 2] {
    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let at = |name: &str| {
        let i = lines.iter().position(|l| *l == name).unwrap_or_else(|| panic!("no {name} in {path}"));
        // The variable's point follows as `10 x 20 y 30 z`.
        let value = |k: usize| lines[i + k].parse::<f64>().unwrap();
        assert_eq!((lines[i + 1], lines[i + 3]), ("10", "20"), "{name} in {path}");
        (value(2), value(4))
    };
    [at("$EXTMIN"), at("$EXTMAX")]
}

/// A drawing's extents cover each dimension as drawn, text included, on the first save as on
/// every later one (the reference writes the extents of the drawn geometry): before, a new
/// dimension was bounded by its points plus a margin until a reopen gave it its `*D` block, so
/// saving the reopened drawing recorded larger extents.
#[test]
fn saved_extents_cover_dimensions_as_drawn_on_every_save() {
    real_files();
    let mut s = Session::new();
    let c = handle(&ok(&mut s, "circle.add", json!({ "center": [80, 20], "radius": 10 })));
    ok(&mut s, "dimension.addRadius", json!({ "handle": c, "at": [95, 25] }));
    ok(&mut s, "dimension.addLinear", json!({ "p1": [0, 0], "p2": [40, 0], "at": [20, -6], "text": "Overall length of the bracket <>" }));
    let dir = std::env::temp_dir().join(format!("kition-cad-extents-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.dxf").to_string_lossy().into_owned();
    let second = dir.join("second.dxf").to_string_lossy().into_owned();
    let mut reopened = reopen(&mut s, &first);
    ok(&mut reopened, "file.saveAs", json!({ "path": second }));
    let (a, b) = (saved_extents(&first), saved_extents(&second));
    for (p, q) in a.iter().zip(&b) {
        assert!((p.0 - q.0).abs() < 1e-6 && (p.1 - q.1).abs() < 1e-6, "first save {a:?}, after reopening {b:?}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
