//! C8-cad-read-3: scan dependencies with an independent writer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};
fn result_kind(bytes: &[u8], command: &str) -> Value {
    let mut doc = CadDocument::open(FILE, bytes).unwrap();
    let result = doc.execute(command, json!({"points":[[100,100]]})).unwrap();
    let id = result.get("handle").unwrap_or(&result["handles"][0]).as_str().unwrap();
    let h = kition_cad_doc::Handle::parse_hex(id).unwrap();
    let d = kition_cad_io::read(&doc.save(Some(FILE)).unwrap(), FILE).unwrap();
    serde_json::to_value(&d.entity(h).unwrap().kind).unwrap()
}

async fn boundary_tolerance(command: &'static str) {
    for relevant in [false, true] {
        let f = Fixture::new();
        let observed = f.disk();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        f.colleague(&[if relevant {
            ("line", json!({"points":[[10000,10000],[20000,20000]]}))
        } else {
            ("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))
        }]);
        let before = f.disk();
        if relevant {
            assert_ne!(result_kind(&observed, command), result_kind(&before, command), "the changed extents must actually change the output loops");
        }
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":{"points":[[100,100]]}}]})).await;
        if relevant {
            assert_eq!(
                answer.err().map(|e| e["error"]["code"].clone()),
                Some(json!("conflict")),
                "{command}: changed extents alter the boundary tolerance"
            );
            assert_eq!(f.disk(), before);
        } else {
            answer.unwrap_or_else(|e| panic!("{command}: {e}"));
            let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
            assert!(d.model.len() > kition_cad_io::read(&before, FILE).unwrap().model.len());
        }
        f.kits.shutdown().await;
    }
}
macro_rules! probe {
    ($name:ident, $command:literal) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            boundary_tolerance($command).await;
        }
    };
}
probe!(boundary_tolerance_dependency, "boundary");
probe!(hatch_tolerance_dependency, "hatch");
probe!(gradient_tolerance_dependency, "gradient");

#[tokio::test(flavor = "multi_thread")]
async fn layer_state_save_holds_its_destination_name() {
    check(
        |_| ("layerState.save", json!({"name":"Mine"})),
        |_| vec![("layerState.save", json!({"name":"Mine"}))],
        |_| vec![("layerState.save", json!({"name":"Other"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn point_dimension_scans_follow_new_sources_and_allow_unrelated_properties() {
    check(
        |_| ("dimension.addLinear", json!({"p1":[700,700],"p2":[710,700],"at":[705,690]})),
        |_| vec![("line", json!({"points":[[700,700],[710,700]]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn find_holds_matching_words_and_allows_nonmatching_words() {
    check(
        |_| ("find", json!({"find":"Before","replace":"Mine"})),
        |f| vec![("text.edit", json!({"handle":f.text,"text":"Before colleague"}))],
        |f| vec![("attribute.edit", json!({"handle":f.insert,"values":{"CODE":"Kept"}}))],
    )
    .await;
}
