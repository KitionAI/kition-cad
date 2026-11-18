//! C8-cad-read-4: FIND membership and following writes must remain distinct.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

async fn following_write(geometry: bool) {
    for outside in [false, true] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let change = if geometry {
            ("move", json!({"handles":[f.text],"delta":[1,0]}))
        } else {
            ("textedit", json!({"handle":f.text,"text":"Before colleague"}))
        };
        if outside {
            f.colleague(&[change]);
        } else {
            let (command, params) = change;
            kition_kits::tools::call_as(
                &f.kits,
                "cad_run",
                &json!({"path":FILE,"steps":[{"command":command,"params":params}]}),
                kition_kits::tools::CallOptions::default(),
                "colleague",
            )
            .await
            .unwrap()
            .unwrap();
        }
        let before = f.disk();
        let step = if geometry { json!({"command":"move","params":{"delta":[0,5]}}) } else { json!({"command":"textedit","params":{"text":"Mine"}}) };
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"find","params":{"find":"Before"}},step]})).await;
        if let Ok(out) = &answer {
            println!("unexpected applied follower, geometry={geometry}, outside={outside}: {}", out["completed"]);
            assert!(
                content(&kition_cad_io::read(&before, FILE).unwrap()) != content(&kition_cad_io::read(&f.disk(), FILE).unwrap()),
                "the unexpectedly accepted follower must really change drawing content"
            );
        }
        assert!(answer.is_err(), "FIND then a dependent writer must refuse; geometry={geometry}, outside={outside}");
        assert_eq!(answer.unwrap_err()["error"]["code"], "conflict");
        assert_eq!(before, f.disk());
        f.kits.shutdown().await;
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn search_find_followed_by_text_edit_holds_written_text() {
    following_write(false).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn search_find_followed_by_move_holds_geometry() {
    following_write(true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn search_find_then_move_merges_an_unrelated_entity_edit() {
    check_program(
        |_| vec![("find", json!({"find":"Before"})), ("move", json!({"delta":[0,5]}))],
        |f| vec![("move", json!({"handles":[f.text],"delta":[1,0]}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
