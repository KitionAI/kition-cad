//! The KHP conformance fixtures (`conformance/`) and the check catalogue (`agent/checks.json`,
//! from [`crate::check::CATALOGUE`]): `basic.dxf` (no diagnostics), `edit.json` (steps that change
//! it), one `check-<code>.dxf` per catalogue code, and `expected/` previews of `basic`. Each fixture
//! is a small plan built in the drawing database and written as DXF; the damage AUDIT repairs (a
//! layer, a style or a block that is gone, a reference loop, a shared handle) is made the way a
//! file damaged elsewhere would hold it.
//!
//! `KITION_WRITE_CONFORMANCE=1 cargo test -p kition-cad-kit conformance` writes them; without the
//! variable the test only checks that every fixture reports its code after a DXF round trip.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_cad_doc::geom::{PolyVertex, Vec2, Vec3};
use kition_cad_doc::{Common, Drawing, Entity, EntityKind, Group, Handle, Hatch, HatchLoop, Line, LwPolyline, Space, Text};
use kition_kit::KitDocument;
use serde_json::{Value, json};

use crate::CadDocument;
use crate::check::{CATALOGUE, PER_CODE, catalogue_json};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn line(d: &mut Drawing, a: (f64, f64), b: (f64, f64)) -> Handle {
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })).expect("line")
}

fn text(value: &str) -> Text {
    serde_json::from_value(json!({ "insert": [10.0, 10.0, 0.0], "height": 2.5, "value": value })).expect("text")
}

fn vertex(x: f64, y: f64) -> PolyVertex {
    PolyVertex { p: Vec2::new(x, y), ..PolyVertex::default() }
}

/// A room: four walls of a 100 × 60 plan and a label.
fn basic() -> Drawing {
    let mut d = Drawing::new_metric();
    for (a, b) in [((0.0, 0.0), (100.0, 0.0)), ((100.0, 0.0), (100.0, 60.0)), ((100.0, 60.0), (0.0, 60.0)), ((0.0, 60.0), (0.0, 0.0))] {
        line(&mut d, a, b);
    }
    d.add(&Space::Model, Common::default(), EntityKind::Text(text("Room"))).expect("label");
    d
}

fn insert(block: &str) -> EntityKind {
    EntityKind::Insert(serde_json::from_value(json!({ "block": block, "insert": [20.0, 20.0, 0.0] })).expect("insert"))
}

fn fixture(code: &str) -> Drawing {
    let mut d = basic();
    match code {
        "cad.missing-layer" => {
            let h = line(&mut d, (10.0, 30.0), (40.0, 30.0));
            d.modify_entity(h, |e| e.common.layer = "Furniture".into()).expect("layer");
            d.layers.retain(|layer| layer.name != "Furniture");
        }
        "cad.missing-linetype" => {
            d.ensure_layer("Walls");
            d.layer_mut("Walls").expect("layer").linetype = "Lost Dashes".into();
        }
        "cad.missing-text-style" => d.dim_styles[0].text_style = "Lost Titles".into(),
        "cad.missing-block" => {
            d.add(&Space::Model, Common::default(), insert("Lost Door")).expect("insert");
        }
        // The Standard multileader style (the writer no longer invents one on save).
        "cad.missing-table-entry" => d.mleader_styles.retain(|style| !style.name.eq_ignore_ascii_case("Standard")),
        "cad.missing-reference" => {
            let h = line(&mut d, (10.0, 40.0), (40.0, 40.0));
            d.modify_entity(h, |e| e.common.linetype = "Lost Hidden".into()).expect("linetype");
        }
        "cad.dangling-reference" => {
            d.groups.push(Group { name: "Walls".into(), description: String::new(), selectable: true, members: vec![Handle(0xDEAD)] });
        }
        "cad.invalid-geometry" => {
            line(&mut d, (50.0, 30.0), (50.0, 30.0));
        }
        "cad.block-cycle" => {
            let mut block = kition_cad_doc::Block::new("Loop");
            let h = d.new_handle();
            block.entities.push(Entity::new(h, insert("Loop")));
            d.blocks.insert("Loop".into(), Arc::new(block));
            d.add(&Space::Model, Common::default(), insert("Loop")).expect("insert");
        }
        "cad.duplicate-handle" => {
            d.model.push(Entity::new(Handle(0), EntityKind::Line(Line { a: Vec3::new(40.0, 50.0, 0.0), b: Vec3::new(60.0, 50.0, 0.0) })));
        }
        "cad.missing-current" => d.header.set_str("CLAYER", "Lost Layer"),
        "cad.degenerate-polyline" => {
            let poly = LwPolyline {
                vertices: vec![vertex(30.0, 30.0), vertex(30.0, 30.0)],
                closed: false,
                const_width: 0.0,
                elevation: 0.0,
                plinegen: false,
            };
            d.add(&Space::Model, Common::default(), EntityKind::LwPolyline(poly)).expect("polyline");
        }
        "cad.degenerate-hatch" => {
            let flat = HatchLoop { vertices: vec![vertex(0.0, 0.0), vertex(50.0, 0.0), vertex(100.0, 0.0)], outer: true };
            let hatch: Hatch = serde_json::from_value(json!({ "pattern": "SOLID", "solid": true, "loops": [flat] })).expect("hatch");
            d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch)).expect("hatch");
        }
        "cad.hidden-layer" => {
            d.ensure_layer("Notes");
            d.layer_mut("Notes").expect("layer").frozen = true;
            let h = line(&mut d, (10.0, 20.0), (40.0, 20.0));
            d.modify_entity(h, |e| e.common.layer = "Notes".into()).expect("layer");
        }
        "cad.unresolved-xref" => {
            let mut block = kition_cad_doc::Block::new("Site");
            block.xref_path = Some("site.dwg".into());
            d.blocks.insert("Site".into(), Arc::new(block));
        }
        "cad.far-outside" => {
            for i in 0..10 {
                line(&mut d, (10.0, 5.0 + 5.0 * f64::from(i)), (90.0, 5.0 + 5.0 * f64::from(i)));
            }
            line(&mut d, (1e9, 1e9), (1e9 + 1.0, 1e9));
        }
        "cad.font-substituted" => {
            d.text_styles.push(kition_cad_doc::TextStyle { name: "Titles".into(), font: "Kition Missing Sans".into(), ..Default::default() });
            let mut titled = text("Plan");
            titled.style = "Titles".into();
            d.add(&Space::Model, Common::default(), EntityKind::Text(titled)).expect("text");
        }
        "cad.empty-text" => {
            d.add(&Space::Model, Common::default(), EntityKind::Text(text("  "))).expect("text");
        }
        "cad.dimension-text-overridden" => {
            let dimension = serde_json::from_value(json!({
                "kind": kition_cad_doc::DimKind::Aligned, "defpt": [0.0, -10.0, 0.0], "textMid": [50.0, -10.0, 0.0], "text": "1200"
            }))
            .expect("dimension");
            d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension)).expect("dimension");
        }
        "cad.truncated" => {
            for _ in 0..=PER_CODE {
                d.add(&Space::Model, Common::default(), EntityKind::Text(text(""))).expect("text");
            }
        }
        other => panic!("no fixture for {other}"),
    }
    d
}

fn dxf(d: &Drawing) -> Vec<u8> {
    kition_cad_io::write_dxf(d).into_bytes()
}

fn edit() -> Value {
    json!({ "steps": [
        { "command": "line", "params": { "points": [[20, 10], [80, 10]] } },
    ] })
}

fn png(doc: &CadDocument, scale: f64) -> Vec<u8> {
    let (width, height) = ((f64::from(doc.width()) * scale) as u32, (f64::from(doc.height()) * scale) as u32);
    let rgba = doc.render(0.0, 0.0, scale, width, height).expect("render");
    let picture = image::RgbaImage::from_raw(width, height, rgba).expect("pixels");
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).expect("png");
    out.into_inner()
}

fn codes(doc: &CadDocument) -> Vec<String> {
    doc.check().into_iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn every_fixture_reports_its_code_and_basic_is_clean() {
    let write = std::env::var_os("KITION_WRITE_CONFORMANCE").is_some();
    let out = repo().join("conformance");
    let basic = CadDocument::open("basic.dxf", &dxf(&basic())).expect("basic");
    assert_eq!(codes(&basic), Vec::<String>::new(), "basic.dxf is clean");
    if write {
        std::fs::create_dir_all(out.join("expected")).expect("conformance/expected");
        std::fs::create_dir_all(repo().join("agent")).expect("agent");
        std::fs::write(out.join("basic.dxf"), dxf(&self::basic())).expect("basic");
        std::fs::write(out.join("edit.json"), format!("{:#}\n", edit())).expect("edit.json");
        std::fs::write(out.join("expected/basic.png"), png(&basic, 1.0)).expect("expected preview");
        std::fs::write(out.join("expected/basic-scale-2.png"), png(&basic, 2.0)).expect("expected preview scale 2");
        let tolerance = json!({
            "maxChannelDelta": 2,
            "maxDifferentPixels": 600,
            "note": "The engine's own renderer draws the preview; anti-aliased line and glyph edges may differ by a step between platforms."
        });
        std::fs::write(out.join("expected/tolerance.json"), format!("{tolerance:#}\n")).expect("tolerance");
        std::fs::write(repo().join("agent/checks.json"), format!("{:#}\n", catalogue_json())).expect("checks.json");
    }
    let mut failures = Vec::new();
    for entry in CATALOGUE {
        let bytes = dxf(&fixture(entry.code));
        let doc = CadDocument::open("fixture.dxf", &bytes).expect("reopen");
        let found = codes(&doc);
        if !found.iter().any(|code| code == entry.code) {
            failures.push(format!("{}: {found:?}", entry.code));
        }
        if write {
            std::fs::write(out.join(format!("check-{}.dxf", entry.code.trim_start_matches("cad."))), &bytes).expect("fixture");
        }
    }
    assert!(failures.is_empty(), "fixtures that lose their code in DXF: {failures:#?}");
}
