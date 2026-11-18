//! A command with a parameter schema refuses a key the schema does not name, and says which
//! (platform rule E7a, kit requirement AX15): an agent that misspells a parameter is told instead
//! of the command running without it.
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::json;

/// Commands whose top level takes keys a schema cannot list, with why.
const OPEN: &[(&str, &str)] = &[
    ("dimstyle", "takes any dimension style field or DIM* variable as a key, as DIMSTYLE does"),
    ("dimstyle.dimension", "takes any dimension style field or DIM* variable as a key, as DIMSTYLE does"),
    ("dimstyle.override", "takes any dimension style field or DIM* variable as a key (per-dimension overrides), as DIMOVERRIDE does"),
    ("dimoverride", "takes any dimension style field or DIM* variable as a key (per-dimension overrides), as DIMOVERRIDE does"),
];

fn doc() -> CadDocument {
    CadDocument::create("Drawing.dxf", 800, 600).unwrap()
}

#[test]
fn every_command_refuses_an_unknown_parameter() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(doc, OPEN) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

#[test]
fn a_misspelled_parameter_names_the_one_it_resembles() {
    let error = doc().execute("commandLine", json!({ "txt": "LINE" })).unwrap_err();
    assert!(error.contains("`txt`") && error.contains("did you mean `text`"), "{error}");
}

#[test]
fn properties_set_declares_every_field_it_reads() {
    // The keys `properties.set` reads (crates/engine/engine/src/cmd/props.rs, run_set).
    let read = [
        "handles",
        "layer",
        "color",
        "linetype",
        "lineweight",
        "ltscale",
        "transparency",
        "visible",
        "dimStyle",
        "start",
        "end",
        "length",
        "angle",
        "center",
        "radius",
        "diameter",
        "circumference",
        "area",
        "startAngle",
        "endAngle",
        "text",
        "textOverride",
        "height",
        "rotation",
        "widthFactor",
        "width",
        "position",
        "closed",
        "scale",
        "pattern",
    ];
    let commands = doc().commands();
    let schema = commands.iter().find(|command| command.id == "properties.set").and_then(|command| command.schema.clone()).unwrap();
    let missing: Vec<_> = read.iter().filter(|key| schema.get(**key).is_none()).collect();
    assert!(missing.is_empty(), "properties.set reads keys its schema does not name: {missing:?}");
}

#[test]
fn measuregeom_takes_the_keys_of_the_measure_it_runs() {
    let mut drawing = doc();
    let answer = drawing.execute("measuregeom", json!({ "mode": "distance", "p1": [0, 0], "p2": [3, 4] })).unwrap();
    assert_eq!(answer["distance"], json!(5.0), "{answer}");
    let error = drawing.execute("measuregeom", json!({ "mode": "distance", "p1": [0, 0], "p3": [3, 4] })).unwrap_err();
    assert!(error.contains("`p3`"), "{error}");
}

#[test]
fn keys_written_one_or_another_are_parameters() {
    // `move {to | delta}` and `rectang {p2 | dimensions}`: agent/guide.md's examples use them.
    let mut drawing = doc();
    let rectangle = drawing.execute("rectang", json!({ "p1": [10, 10], "p2": [40, 30] })).unwrap();
    let handle = rectangle["handle"].clone();
    drawing.execute("move", json!({ "handles": [handle], "delta": [5, 5] })).unwrap();
    let error = drawing.execute("move", json!({ "handles": [rectangle["handle"].clone()], "deltta": [5, 5] })).unwrap_err();
    assert!(error.contains("did you mean `delta`"), "{error}");
}
