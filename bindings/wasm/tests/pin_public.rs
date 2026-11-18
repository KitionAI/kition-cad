//! C8-cad-pin: guard implementation facts are private; public eligibility and bounds remain observable.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{KitDocument, session::model::Exchange};

#[test]
fn guard_implementation_facts_are_premise_only() {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let schema = Exchange::new().describe(&mut doc).unwrap();
    for (ty, name) in [
        ("Drawing", "queries"),
        ("Drawing", "history"),
        ("Entity", "pick"),
        ("Entity", "boundaryOutline"),
        ("Block", "memberOrder"),
        ("Block", "blockCenter"),
        ("Block", "insertAttributes"),
    ] {
        assert_eq!(schema["types"][ty]["derived"][name]["premiseOnly"], true, "{ty}.{name}");
    }
    for (ty, name) in [("Drawing", "modelBounds"), ("Layer", "canFreeze"), ("DimStyle", "canDelete"), ("TextStyle", "canDelete")] {
        assert_ne!(schema["types"][ty]["derived"][name]["premiseOnly"], true, "{ty}.{name}");
    }
}

#[test]
fn exchange_follows_a_new_drawing_and_subsequent_model_edits() {
    use serde_json::json;
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let h = doc.execute("circle", json!({"center":[10,10],"radius":5})).unwrap()["handle"].clone();
    let mut exchange = Exchange::new();
    let before = exchange.describe(&mut doc).unwrap();
    let result = exchange
        .batch(
            &mut doc,
            &json!({"ops":[
        {"op":"command","id":"new","params":{}},
        {"op":"command","id":"line","params":{"points":[[0,0],[10,0]]}}
    ],"load":[{"target":"root","fields":["model"]}]}),
            false,
        )
        .unwrap();
    assert_eq!(result["loaded"][0]["fields"]["model"].as_array().unwrap().len(), 1);
    let model = exchange.describe(&mut doc).unwrap();
    assert_eq!(model["rootId"], before["rootId"], "CAD uses a fixed public root across replacements");
    assert_eq!(doc.inspect().unwrap()["entities"], 1);
    let old = exchange.batch(&mut doc, &json!({"ops":[],"load":[{"target":h}]}), false).unwrap();
    assert_ne!(old["loaded"][0]["fields"]["kind"]["type"], "circle");
}
