//! C8-cad-read-2: independent writer necessity controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn lock_conflicts_for_discarded_join_sources() {
    for command in ["join", "pedit"] {
        check(
            |f| {
                (
                    command,
                    if command == "join" {
                        json!({"handles":[f.line,f.joined]})
                    } else {
                        json!({"handle":f.line,"option":"join","handles2":[f.joined]})
                    },
                )
            },
            |_| vec![("layer.set", json!({"name":"Secondary","locked":true}))],
            |f| vec![("move", json!({"handles":[f.circle],"delta":[1,0]}))],
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lock_conflicts_for_field_writes() {
    for command in ["properties.set", "dimoverride", "attedit"] {
        check(
            |f| {
                (
                    command,
                    match command {
                        "properties.set" => json!({"handles":[f.line],"color":"red"}),
                        "dimoverride" => json!({"handles":[f.dim],"DIMASZ":8}),
                        _ => json!({"handle":f.insert,"values":{"CODE":"After"}}),
                    },
                )
            },
            |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
            |f| vec![("move", json!({"handles":[f.circle],"delta":[1,0]}))],
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn locks_of_solver_neighbours_are_guarded() {
    let f = Fixture::new();
    f.colleague(&[("gccoincident", json!({"h1":f.line,"p1":"end","h2":f.joined,"p2":"start"}))]);
    let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
    f.colleague(&[("layer.set", json!({"name":"Secondary","locked":true}))]);
    let before = f.disk();
    let error = f.call("run", json!({"basis":basis,"steps":[{"command":"move","params":{"handles":[f.line],"delta":[2,3]}}]})).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert_eq!(f.disk(), before);
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn find_holds_both_matching_words_and_lock_eligibility() {
    check(
        |_| ("find", json!({"find":"Before","replace":"After"})),
        |f| vec![("textedit", json!({"handle":f.text,"text":"Before colleague"}))],
        |f| vec![("move", json!({"handles":[f.circle],"delta":[1,0]}))],
    )
    .await;
}
