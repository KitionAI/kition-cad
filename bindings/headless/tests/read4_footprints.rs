//! C8-cad-read-4: finite-line boundary guards preserve numeric receipt compaction.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::json;
use std::path::PathBuf;
#[tokio::test(flavor = "multi_thread")]
async fn finite_line_boundary_still_holds_endpoints() {
    check_with_setup(
        |_| ("boundary", json!({"points":[[50,25]]})),
        |f| vec![("move", json!({"handles":[f.line],"delta":[0,5]}))],
        |f| vec![("properties.set", json!({"handles":[f.line],"color":"blue"}))],
        |_| vec![("line", json!({"points":[[100,50],[0,50],[0,0]]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn equal_finite_line_offsets_keep_their_receipt_footprint() {
    let mut d = CadDocument::create(FILE, 420, 297).unwrap();
    let mut ids = Vec::new();
    for x in 0..4 {
        ids.push(d.execute("line", json!({"points":[[x,0],[x,10]]})).unwrap()["handles"][0].clone());
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE), d.save(Some(FILE)).unwrap()).unwrap();
    let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
    let kits = KitManager::new(Some(dir.path().into()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]);
    let answer = tools::call_as(
        &kits,
        "cad_run",
        &json!({"path":FILE,"steps":[{"command":"move","params":{"handles":ids,"delta":[0,5]}}]}),
        CallOptions::default(),
        "agent-me",
    )
    .await
    .unwrap()
    .unwrap()
    .output;
    assert_eq!(answer["changes"]["footprint"][0]["ids"].as_array().map(Vec::len), Some(4), "{answer}");
    let actual = kition_cad_io::read(&std::fs::read(dir.path().join(FILE)).unwrap(), FILE).unwrap();
    for e in actual.model.iter() {
        if let kition_cad_doc::EntityKind::Line(l) = &e.kind {
            assert_eq!((l.a.y, l.b.y), (5.0, 15.0));
        }
    }
    kits.shutdown().await;
}
