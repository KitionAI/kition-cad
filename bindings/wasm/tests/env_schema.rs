//! Environment words describe actual CAD data; order is an internal ordinal.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{KitDocument, session::model::Exchange};

#[test]
fn named_records_have_labels_and_internal_order_is_not_sampled() {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let schema = Exchange::new().describe(&mut doc).unwrap();
    for ty in [
        "Layer",
        "Linetype",
        "TextStyle",
        "DimStyle",
        "MLeaderStyle",
        "TableStyle",
        "View",
        "Ucs",
        "LayerState",
        "Group",
        "PlotStyleTable",
        "Block",
        "Layout",
    ] {
        assert_eq!(schema["types"][ty]["label"], "name", "{ty}");
        if schema["types"][ty]["fields"].get("order").is_some() {
            assert_eq!(schema["types"][ty]["fields"]["order"]["sample"], false, "{ty}");
        }
        assert_eq!(schema["types"][ty]["fields"]["name"]["readOnly"], true, "{ty}");
    }
}

#[test]
fn json_member_and_command_numeric_limits_are_validated() {
    let schema: serde_json::Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    let parsed = kition_kit::kdm::Schema::parse(&schema.to_string()).unwrap();
    assert_eq!(
        parsed.type_def("Entity").unwrap().fields["common"].members["property.linetypeScale"].minimum.as_ref().and_then(serde_json::Number::as_f64),
        Some(0.0)
    );
    kition_kit::schema::validate(&serde_json::json!({"radius":{"type":"number","minimum":0.0}})).unwrap();
    assert!(kition_kit::schema::validate(&serde_json::json!({"radius":{"type":"string","minimum":0.0}})).is_err());
    let mut doc = CadDocument::create("Limits.dxf", 420, 297).unwrap();
    let h = doc.execute("line", serde_json::json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    let before = doc.save(Some("Limits.dxf")).unwrap();
    let error = Exchange::new()
        .batch(&mut doc, &serde_json::json!({"ops":[{"op":"set","target":h,"field":"common.ltscale","value":-1}]}), false)
        .unwrap_err();
    assert_eq!(error.code, kition_kit::session::model::ModelErrorCode::InvalidValue);
    assert_eq!(doc.save(Some("Limits.dxf")).unwrap(), before);
}
