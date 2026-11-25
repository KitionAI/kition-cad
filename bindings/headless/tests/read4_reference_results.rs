//! C8-cad-read-4: a copied reference can consume definition geometry or an external path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use serde_json::json;
use std::sync::Arc;
#[tokio::test(flavor = "multi_thread")]
async fn nonrotating_array_holds_definition_geometry() {
    check(
        |f| ("object.arrayPolar", json!({"handles":[f.insert],"center":[0,0],"count":3,"rotate":false})),
        |f| vec![("block", json!({"name":"Marker","base":[5,0],"handles":[f.rect],"keep":"retain"}))],
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn insertion_holds_external_path_but_preserves_description() {
    for outside in [false, true] {
        for refuse in [false, true] {
            let f = Fixture::new();
            f.rewrite(|d| Arc::make_mut(d.blocks.get_mut("Marker").unwrap()).xref_path = Some("site.dxf".into()));
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            let observed = f.disk();
            let d = kition_cad_io::read(&observed, FILE).unwrap();
            let id = format!("block:{}", d.record_id("block", "Marker", 0).unwrap());
            let mut block = (**d.block("Marker").unwrap()).clone();
            if refuse {
                block.xref_path = Some("other.dxf".into());
            } else {
                block.description = "Colleague".into();
            }
            if outside {
                f.rewrite(|d| *Arc::make_mut(d.blocks.get_mut("Marker").unwrap()) = block.clone());
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
            assert_ne!(observed, before, "the independent writer must really save");
            let params = json!({"name":"Marker","at":[700,0]});
            let mut oracle = CadDocument::open(FILE, &before).unwrap();
            oracle.execute("insert", params.clone()).unwrap();
            let expected = kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap();
            let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"block.insert","params":params}]})).await;
            if refuse {
                let e = answer.expect_err("changed external path must refuse");
                assert_eq!(e["error"]["code"], "conflict", "{e}");
                assert_eq!(before, f.disk());
            } else {
                answer.unwrap_or_else(|e| panic!("unrelated description must merge, outside={outside}: {e}"));
                assert_eq!(content(&kition_cad_io::read(&f.disk(), FILE).unwrap()), content(&expected));
            }
            f.kits.shutdown().await;
        }
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn nonrotating_array_holds_member_layer_visibility() {
    check_with_setup(
        |f| ("object.arrayPolar", json!({"handles":[f.insert],"center":[0,0],"count":3,"rotate":false})),
        |_| vec![("layer.set", json!({"name":"Secondary","on":false}))],
        |_| vec![("layer.set", json!({"name":"Secondary","locked":true}))],
        |f| vec![("block", json!({"name":"Marker","handles":[f.joined],"base":[200,200],"keep":"retain"}))],
    )
    .await;
}
