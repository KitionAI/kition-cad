//! Every kit-env receipt-differs class on the damaged conformance files is reproduced here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_env::{EnvKit, read_snapshot};
use kition_kit::{KitDocument, session::model::ReplicaPort};
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

fn core(v: &Value) -> Value {
    let mut out = json!({"created":v["created"],"changed":v["changed"],"moved":v["moved"],"deleted":v["deleted"]});
    for key in ["created", "changed", "moved", "deleted"] {
        for item in out[key].as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove("$was");
        }
    }
    let changed = out["changed"].as_array_mut().unwrap();
    for item in changed.iter_mut() {
        item["fields"].as_array_mut().unwrap().retain(|v| !v.as_str().unwrap().starts_with('~'));
        item["edits"].as_array_mut().unwrap().retain(|v| !v["field"].as_str().unwrap().starts_with('~'));
    }
    changed.retain(|v| !v["fields"].as_array().unwrap().is_empty());
    out
}

// Map-owned records participate in the complete snapshot and saved-state comparison.
#[tokio::test(flavor = "multi_thread")]
async fn map_owned_records_are_present_in_saved_rereads() {
    let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"));
    let env = EnvKit::new(kit, Some(executable.clone()));
    let doc = CadDocument::create("Map.dxf", 420, 297).unwrap();
    let bytes = doc.save(Some("Map.dxf")).unwrap();
    let before = read_snapshot(&env, "Map.dxf", &bytes).await.unwrap();
    assert!(before.canonical()["objects"].get("layer:0").is_some());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Map.dxf"), bytes).unwrap();
    let kits = KitManager::new(Some(dir.path().into()), vec![(kit, Some(executable))]);
    let answer = tools::call_as(
        &kits,
        "cad_run",
        &json!({"path":"Map.dxf","changes":"all","steps":[{"command":"layer.set","params":{"name":"0","color":"red"}}]}),
        CallOptions::default(),
        "probe",
    )
    .await
    .unwrap()
    .unwrap()
    .output;
    kits.shutdown().await;
    let after = read_snapshot(&env, "Map.dxf", &std::fs::read(dir.path().join("Map.dxf")).unwrap()).await.unwrap();
    let schema = doc.model_schema();
    let reread = kition_kit::kdm::reread::diff_paired(&schema, &before.canonical(), &after.canonical()).to_json();
    assert!(answer["changes"]["changed"].as_array().unwrap().iter().any(|x| x["id"] == "layer:0"));
    assert!(reread["changed"].as_array().unwrap().iter().any(|x| x["id"] == "layer:0"), "a saved layer edit must appear in the reread");
    assert_eq!(core(&answer["changes"]), core(&reread));
}

#[tokio::test(flavor = "multi_thread")]
async fn damaged_fixture_receipts_equal_saved_rereads() {
    let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"));
    let env = EnvKit::new(kit, Some(executable.clone()));
    let schema = CadDocument::create("Plan.dxf", 420, 297).unwrap().model_schema();
    let mut failures = Vec::new();
    for file in [
        "check-block-cycle.dxf",
        "check-missing-block.dxf",
        "check-missing-linetype.dxf",
        "check-missing-table-entry.dxf",
        "check-missing-text-style.dxf",
    ] {
        let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance").join(file)).unwrap();
        let before = read_snapshot(&env, file, &bytes).await.unwrap();
        let entities = before.canonical()["objects"]["drawing"]["model"].as_array().unwrap().clone();
        for (index, id) in entities.iter().enumerate() {
            for delete in [false, true] {
                if !delete && entities.len() < 2 {
                    continue;
                }
                let dir = tempfile::tempdir().unwrap();
                std::fs::write(dir.path().join(file), &bytes).unwrap();
                let kits = KitManager::new(Some(dir.path().into()), vec![(kit, Some(executable.clone()))]);
                let op = if delete {
                    json!({"op":"delete","target":id})
                } else {
                    json!({"op":"list.move","target":"root","field":"model","from":index,"to":(index+1)%entities.len()})
                };
                let answer = tools::call_as(&kits, "cad_batch", &json!({"path":file,"changes":"all","ops":[op]}), CallOptions::default(), "probe")
                    .await
                    .unwrap()
                    .unwrap()
                    .output;
                let after_bytes = std::fs::read(dir.path().join(file)).unwrap();
                kits.shutdown().await;
                let after = read_snapshot(&env, file, &after_bytes).await.unwrap();
                let actual = core(&kition_kit::kdm::reread::diff_paired(&schema, &before.canonical(), &after.canonical()).to_json());
                let reported = core(&answer["changes"]);
                if reported != actual {
                    eprintln!("RECEIPT DIFF {file} index={index} delete={delete}: reported={reported} reread={actual}");
                    failures.push(format!("{file} index={index} delete={delete}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "receipt-differs: {failures:?}");
}
