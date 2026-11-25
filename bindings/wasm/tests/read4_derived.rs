//! Computed guard facts must be described so same-server receipts refresh them.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{KitDocument, session::model::Exchange};
use serde_json::json;
#[test]
fn every_computed_guard_fact_has_a_model_declaration() {
    let mut d = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    d.execute("line", json!({"points":[[0,0],[10,0]]})).unwrap();
    let schema = Exchange::new().describe(&mut d).unwrap();
    let mut missing = Vec::new();
    for fact in d.derived(&[], &[]).unwrap() {
        let id = fact.id.as_str().unwrap();
        let ty = match id.split(':').next().unwrap() {
            "drawing" => "Drawing",
            "layout" => "Layout",
            "block" => "Block",
            "layer" => "Layer",
            "textstyle" => "TextStyle",
            "dimensionStyle.open" => "DimStyle",
            _ => "Entity",
        };
        for name in fact.values.keys() {
            if schema["types"][ty]["derived"].get(name).is_none() {
                missing.push(format!("{ty}.{name}"));
            }
        }
    }
    assert!(missing.is_empty(), "computed premises absent from the description: {missing:?}");
}
