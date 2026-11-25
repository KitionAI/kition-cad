//! C8-cad-pin: definition consumers hold the inputs used by their native result.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use kition_cad_doc::EntityKind;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use serde_json::json;

async fn definition_input(command: &str) {
    for outside in [false, true] {
        for refuse in [false, true] {
            let f = Fixture::new();
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            let observed = f.disk();
            let drawing = kition_cad_io::read(&observed, FILE).unwrap();
            let member = drawing
                .block("Marker")
                .unwrap()
                .entities
                .iter()
                .find(|e| {
                    if command == "attribute.sync" && refuse {
                        matches!(e.kind, EntityKind::AttDef(_))
                    } else {
                        matches!(e.kind, EntityKind::Line(_))
                    }
                })
                .unwrap();
            let mut changed = (**member).clone();
            let (field, value) = if command == "multileader.add" && !refuse {
                changed.common.color = serde_json::from_value(json!({"kind":"index","value":5})).unwrap();
                ("common", serde_json::to_value(&changed.common).unwrap())
            } else {
                match &mut changed.kind {
                    EntityKind::Line(line) => line.b.x += 100.0,
                    EntityKind::AttDef(att) => att.text.height += 8.0,
                    _ => panic!("fixture member kind"),
                }
                ("kind", serde_json::to_value(&changed.kind).unwrap())
            };
            if outside {
                f.rewrite(|d| {
                    std::sync::Arc::make_mut(d.blocks.get_mut("Marker").unwrap()).entities.modify(member.handle, |e| *e = changed.clone());
                });
            } else {
                tools::call_as(
                    &f.kits,
                    "cad_batch",
                    &json!({"path":FILE,"ops":[{"op":"set","target":member.handle.hex(),"field":field,"value":value}]}),
                    CallOptions::default(),
                    "colleague",
                )
                .await
                .unwrap()
                .unwrap();
            }
            let before = f.disk();
            assert_ne!(observed, before, "the colleague must save the change");
            let params = if command == "attribute.sync" { json!({"name":"Marker"}) } else { json!({"points":[[0,0],[10,10]],"block":"Marker"}) };
            let mut oracle = CadDocument::open(FILE, &before).unwrap();
            oracle.execute(command, params.clone()).unwrap();
            let expected = kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap();
            let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await;
            if refuse {
                let error = answer.expect_err("the block input used by the native result changed");
                assert_eq!(error["error"]["code"], "conflict", "{error}");
                assert_eq!(before, f.disk());
            } else {
                answer.unwrap_or_else(|e| panic!("{command} outside={outside}: unrelated block input must apply: {e}"));
                assert_eq!(content(&kition_cad_io::read(&f.disk(), FILE).unwrap()), content(&expected));
            }
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn attribute_sync_ignores_ordinary_block_geometry() {
    definition_input("attribute.sync").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn centered_multileader_ignores_member_appearance() {
    definition_input("multileader.add").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn centered_multileader_holds_member_layer_visibility() {
    check_with_setup(
        |_| ("multileader.add", json!({"points":[[0,0],[10,10]],"block":"Marker"})),
        |_| vec![("layer.set", json!({"name":"Secondary","on":false}))],
        |_| vec![("layer.set", json!({"name":"Secondary","locked":true}))],
        |f| vec![("block", json!({"name":"Marker","handles":[f.joined],"base":[0,0],"keep":"retain"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn attribute_sync_holds_reference_layer_lock() {
    check(
        |_| ("attribute.sync", json!({"name":"Marker"})),
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
        |f| vec![("object.setProperties", json!({"handles":[f.insert],"color":"blue"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn gradient_ignores_pattern_default_used_only_by_hatch() {
    check(
        |f| ("gradient", json!({"handles":[f.rect],"angle":0})),
        |f| vec![("move", json!({"handles":[f.rect],"delta":[1,0]}))],
        |f| vec![("variable.set", json!({"name":"HPNAME","value":"SOLID"})), ("object.setProperties", json!({"handles":[f.text],"color":"blue"}))],
    )
    .await;
}
