//! C8-cad-read-4: referenced definitions consume shape/base, not descriptions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use serde_json::{Value, json};
use std::sync::Arc;

async fn definition(write: impl Fn(&Fixture) -> (&'static str, Value)) {
    for outside in [false, true] {
        for refuse in [false, true] {
            let f = Fixture::new();
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            let observed = f.disk();
            let d = kition_cad_io::read(&observed, FILE).unwrap();
            let id = format!("block:{}", d.record_id("block", "Marker", 0).unwrap());
            let mut block = (**d.block("Marker").unwrap()).clone();
            if refuse {
                block.base.x += 1.0;
            } else {
                block.description = "Colleague".into();
            }
            if outside {
                f.rewrite(|d| {
                    *Arc::make_mut(d.blocks.get_mut("Marker").unwrap()) = block.clone();
                });
            } else {
                tools::call_as(
                    &f.kits,
                    "cad_batch",
                    &json!({"path":FILE,"ops":[{"op":"set","target":id,"field":"data","value":block}]}),
                    CallOptions::default(),
                    "colleague",
                )
                .await
                .unwrap()
                .unwrap();
            }
            let before = f.disk();
            assert!(observed != before, "the independent definition writer must save");
            let (command, params) = write(&f);
            let expected = if refuse {
                None
            } else {
                let mut oracle = CadDocument::open(FILE, &before).unwrap();
                oracle.execute(command, params.clone()).unwrap();
                Some(kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap())
            };
            let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await;
            if refuse {
                let error = answer.expect_err("changed definition base is necessary");
                assert_eq!(error["error"]["code"], "conflict", "{error}");
                assert_eq!(before, f.disk());
            } else {
                answer.unwrap_or_else(|e| panic!("{command}, outside={outside}: unrelated description must merge: {e}"));
                assert_eq!(content(&kition_cad_io::read(&f.disk(), FILE).unwrap()), content(expected.as_ref().unwrap()));
            }
            f.kits.shutdown().await;
        }
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn definition_insert_preserves_description() {
    definition(|_| ("insert", json!({"name":"Marker","at":[700,0]}))).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn definition_sampling_preserves_description() {
    definition(|f| ("divide", json!({"handle":f.line,"segments":3,"block":"Marker"}))).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn definition_ncopy_preserves_description() {
    definition(|f| ("object.copyNested", json!({"handle":f.insert,"delta":[0,20]}))).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn definition_ncopy_preserves_insert_attributes() {
    check(
        |f| ("object.copyNested", json!({"handle":f.insert,"delta":[0,20]})),
        |f| vec![("move", json!({"handles":[f.insert],"delta":[1,0]}))],
        |f| vec![("attribute.edit", json!({"handle":f.insert,"values":{"CODE":"Colleague"}}))],
    )
    .await;
}
