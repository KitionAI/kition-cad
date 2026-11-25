//! C8-land-11-cad: receipt sizes and field-sensitive carries through the actual server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;
const FILE: &str = "Plan.dxf";

async fn receipt(doc: &CadDocument, command: &str, p: Value) -> Value {
    receipt_mode(doc, json!([{"command":command,"params":p}]), true).await
}

async fn receipt_mode(doc: &CadDocument, steps: Value, all: bool) -> Value {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE), doc.save(Some(FILE)).unwrap()).unwrap();
    let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let kits = KitManager::new(Some(dir.path().into()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]);
    let mut input = json!({"path":FILE,"steps":steps});
    if all {
        input["changes"] = json!("all");
    }
    let answer = tools::call_as(&kits, "cad_run", &input, CallOptions::default(), "agent-me").await.unwrap().unwrap().output;
    kits.shutdown().await;
    answer["changes"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn offsets_use_footprints_and_a_cap_states_its_omissions() {
    let (d, ids) = drawing(200);
    let compact = receipt_mode(&d, json!([{"command":"object.move","params":{"handles":ids,"delta":[0,5]}}]), false).await;
    println!("BYTES default-offset-200: {}", compact.to_string().len());
    assert_eq!(compact["footprint"][0]["ids"].as_array().unwrap().len(), 200, "{compact}");
    assert!(compact.get("truncated").is_none(), "{compact}");
    let steps: Vec<_> = ids.iter().enumerate().map(|(i, id)| json!({"command":"object.move","params":{"handles":[id],"delta":[0,i+1]}})).collect();
    let capped = receipt_mode(&d, json!(steps), false).await;
    assert_eq!(capped["changed"].as_array().unwrap().len(), 50);
    assert_eq!(capped["truncated"]["changed"], 151, "200 entities plus the drawing's derived bounds: {capped}");
    println!("BYTES distinct-offsets-200 capped: {}; omitted changed: {}", capped.to_string().len(), capped["truncated"]["changed"]);
}

fn drawing(count: usize) -> (CadDocument, Vec<Value>) {
    let mut d = CadDocument::create(FILE, 420, 297).unwrap();
    let mut ids = Vec::new();
    for n in 0..count {
        ids.push(d.execute("line", json!({"points":[[n,0],[n,10]]})).unwrap()["handles"][0].clone());
    }
    (d, ids)
}

#[tokio::test(flavor = "multi_thread")]
async fn receipt_bytes_and_parent_carries() {
    let (d, ids) = drawing(1);
    for (label, command, p) in
        [("property", "object.setProperties", json!({"handles":ids,"color":"red"})), ("move", "move", json!({"handles":ids,"delta":[0,5]}))]
    {
        let changes = receipt(&d, command, p).await;
        println!("BYTES {label}: {}", changes.to_string().len());
    }
    for count in [5, 200] {
        let (mut d, ids) = drawing(count);
        d.execute("block", json!({"name":"Parent","base":[0,0],"handles":ids,"keep":"delete"})).unwrap();
        let parent = d.execute("insert", json!({"name":"Parent","at":[0,0]})).unwrap()["handle"].clone();
        let changes = receipt(&d, "move", json!({"handles":[parent],"delta":[0,5]})).await;
        let changed = changes["changed"].as_array().unwrap().iter().find(|e| e["id"] == parent).unwrap();
        assert_eq!(changed["carries"]["count"], count, "{changes}");
        println!("BYTES parent-{count}: {}", changes.to_string().len());
    }
    let (d, ids) = drawing(200);
    let changes = receipt(&d, "move", json!({"handles":ids,"delta":[0,5]})).await;
    println!("BYTES offset-200: {}", changes.to_string().len());
    assert!(changes.get("truncated").is_none(), "{changes}");
}
