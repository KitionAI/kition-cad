//! C8-cad-read-4: large namespace snapshots retain small named-record before values.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn record_name_style_copy_preserves_an_unrelated_layer_rename() {
    check(
        |_| ("dimstyle", json!({"name":"Fresh","current":false})),
        |_| vec![("dimstyle", json!({"name":"ISO-25","arrowSize":8,"current":false}))],
        |_| vec![("layer.set", json!({"name":"Secondary","newName":"Other"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn record_name_rename_preserves_an_unrelated_layout_rename() {
    check(
        |_| ("rename", json!({"table":"layer","from":"Editable","to":"Mine"})),
        |_| vec![("layer.set", json!({"name":"Editable","newName":"Theirs"}))],
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Other"}))],
    )
    .await;
}
