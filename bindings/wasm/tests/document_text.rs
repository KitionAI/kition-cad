//! `document.text` (KitText 1.0): the words of a drawing's TEXT, MTEXT, attribute
//! values and dimension text overrides, model space then each layout, within the limit, with
//! parts inside the text, changing nothing (`kition_kit::testing::check_text`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_cad_doc::geom::Vec3;
use kition_cad_doc::{
    Attrib, AttribProps, Block, Common, DimKind, Dimension, Drawing, Entity, EntityKind, HAlign, Insert, Layer, MText, Space, Text, VAlign,
};
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kit::testing::check_text;
use kition_kit::text::{DEFAULT_LIMIT, DocumentText, TextPart};

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn open(path: &str) -> CadDocument {
    let bytes = std::fs::read(repo().join(path)).unwrap_or_else(|error| panic!("{path}: {error}"));
    CadDocument::open(path, &bytes).unwrap()
}

fn text(value: &str, x: f64) -> Text {
    Text {
        insert: Vec3::new(x, 0.0, 0.0),
        align_pt: None,
        height: 2.5,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
    }
}

fn mtext(contents: &str) -> MText {
    MText {
        insert: Vec3::new(0.0, 20.0, 0.0),
        height: 2.5,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: contents.into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
    }
}

fn attrib(tag: &str, value: &str, invisible: bool) -> Attrib {
    Attrib { tag: tag.into(), text: text(value, 0.0), invisible, constant: false, prompt: String::new(), props: AttribProps::default() }
}

fn on(layer: &str) -> Common {
    Common { layer: layer.into(), ..Common::default() }
}

/// A drawing with every kind of words, written to DXF and opened as a document.
fn drawing() -> CadDocument {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { on: false, ..Layer::new("OFF") });
    let mut title = Block::new("TITLE");
    title.entities.push(Entity::new(d.new_handle(), EntityKind::Text(text("DRAWN BY", 0.0))));
    d.blocks.insert("TITLE".into(), std::sync::Arc::new(title));
    let model = Space::Model;
    d.add(&model, on("0"), EntityKind::Text(text("BORE %%c10 %%p0.1", 0.0))).unwrap();
    d.add(&model, on("0"), EntityKind::MText(mtext("{\\fArial|b1;NOTES:}\\P1. FINISH \\S1/2;"))).unwrap();
    d.add(&model, on("OFF"), EntityKind::Text(text("hidden layer", 0.0))).unwrap();
    let insert = Insert {
        block: "TITLE".into(),
        insert: Vec3::new(100.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![attrib("NAME", "Ada", false), attrib("SECRET", "invisible", true)],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    d.add(&model, on("0"), EntityKind::Insert(insert)).unwrap();
    let dimension = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(0.0, 10.0, 0.0),
        text_mid: Vec3::new(50.0, 10.0, 0.0),
        p13: Vec3::new(0.0, 0.0, 0.0),
        p14: Vec3::new(100.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: "<> TYP".into(),
        style: "ISO-25".into(),
        measurement: 100.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
    };
    d.add(&model, on("0"), EntityKind::Dimension(dimension.clone())).unwrap();
    d.add(&model, on("0"), EntityKind::Dimension(Dimension { text: String::new(), ..dimension })).unwrap();
    let layout = Space::Paper(d.layouts[0].name.clone());
    d.add(&layout, on("0"), EntityKind::Text(text("SHEET 1", 0.0))).unwrap();
    let bytes = kition_cad_io::write(&d, "words.dxf").unwrap();
    CadDocument::open("words.dxf", &bytes).unwrap()
}

const WORDS: &str = "BORE ⌀10 ±0.1\nNOTES:\n1. FINISH 1/2\nDRAWN BY\nAda\n100 TYP\nSHEET 1";

fn part(label: &str, start: usize, end: usize) -> TextPart {
    TextPart { label: label.into(), start, end }
}

#[test]
fn text_mtext_attributes_and_dimension_overrides_are_read_space_by_space() {
    let text = check_text(&drawing()).unwrap();
    assert_eq!((text.text.as_str(), text.truncated), (WORDS, false), "{text:?}");
    let model_end = WORDS.find("\nSHEET").map(|end| WORDS[..end].chars().count()).unwrap();
    assert_eq!(text.parts, [part("Model", 0, model_end), part("Layout1", model_end + 1, WORDS.chars().count())]);
}

#[test]
fn the_example_answers_its_labels_notes_and_schedule() {
    let text = check_text(&open("examples/apartment.dxf")).unwrap();
    assert!(
        text.text.starts_with("LIVING / DINING\n34.6 m²\nBEDROOM\n16.4 m²\nBATH\n7.7 m²\nDouble glazing, U = 1.1\nROOM SCHEDULE\nRoom\tArea\n"),
        "{text:?}"
    );
    assert!(text.text.ends_with("Total\t58.7 m²"), "{text:?}");
    assert!(!text.text.contains("\\P") && !text.text.contains("7000"), "no codes, no measured values: {text:?}");
    assert_eq!(text.parts[0].label, "Model");

    let overridden = check_text(&open("conformance/check-dimension-text-overridden.dxf")).unwrap();
    assert_eq!(overridden.text, "Room\n1200");
}

#[test]
fn the_limit_truncates_by_characters() {
    let document = drawing();
    let cut = document.text(8).unwrap();
    assert_eq!(cut, DocumentText { text: "BORE ⌀10".into(), truncated: true, parts: vec![part("Model", 0, 8)] });
    let exact = document.text(WORDS.chars().count()).unwrap();
    assert_eq!((exact.text.as_str(), exact.truncated), (WORDS, false));
}

#[test]
fn every_example_and_fixture_passes_the_text_check() {
    for folder in ["examples", "conformance"] {
        for entry in std::fs::read_dir(repo().join(folder)).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            if !name.ends_with(".dxf") {
                continue;
            }
            let Ok(bytes) = std::fs::read(repo().join(folder).join(&name)) else { continue };
            let Ok(document) = CadDocument::open(&name, &bytes) else { continue };
            check_text(&document).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }
}

#[test]
fn reading_changes_nothing() {
    let document = open("examples/apartment.dxf");
    let (revision, dirty, bytes) = (document.revision(), document.dirty(), document.save(None).unwrap());
    let state = document.inspect().unwrap();
    document.text(DEFAULT_LIMIT).unwrap();
    document.text(3).unwrap();
    assert_eq!((document.revision(), document.dirty()), (revision, dirty));
    assert_eq!(document.save(None).unwrap(), bytes);
    assert_eq!(document.inspect().unwrap(), state);
}

#[test]
fn an_empty_drawing_has_no_text() {
    let document = CadDocument::create("untitled.dxf", 420, 297).unwrap();
    assert_eq!(check_text(&document).unwrap(), DocumentText::default());
}
