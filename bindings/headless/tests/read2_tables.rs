//! C8-cad-read-2: independent writer necessity controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn table_assignment_keeps_identity_and_merge_redirects() {
    check(
        |f| ("table.set", json!({"handle":f.table,"row":1,"col":1,"text":"Mine"})),
        |f| vec![("table.merge", json!({"handle":f.table,"row":0,"col":0,"rows":2,"cols":2}))],
        // same-field decision: only another cell is unrelated.
        |f| vec![("table.set", json!({"handle":f.table,"row":2,"col":2,"text":"Kept"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn table_insertion_keeps_the_gap_but_ignores_other_cell_text() {
    check(
        |f| ("table.insertRow", json!({"handle":f.table,"row":1,"height":8})),
        |f| vec![("table.insertRow", json!({"handle":f.table,"row":0,"height":7}))],
        |f| vec![("table.set", json!({"handle":f.table,"row":2,"col":2,"text":"Kept"}))],
    )
    .await;
}
