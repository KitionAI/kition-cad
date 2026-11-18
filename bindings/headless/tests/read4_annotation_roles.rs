//! C8-cad-read-4: snap sources and ignored explicit annotation targets are read by their roles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn annotation_reassociation_ignores_source_layer_lock() {
    check_with_setup(
        |f| ("dimreassociate", json!({"handles":[f.attached_dim]})),
        |f| vec![("move", json!({"handles":[f.line],"delta":[0,5]}))],
        |_| vec![("layer.set", json!({"name":"Secondary","locked":true}))],
        |f| vec![("properties.set", json!({"handles":[f.line],"layer":"Secondary"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn annotation_disassociation_ignores_non_dimension_lock() {
    check(
        |f| ("dimdisassociate", json!({"handles":[f.circle]})),
        |f| vec![("erase", json!({"handles":[f.circle]}))],
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn annotation_style_update_ignores_unused_current_style() {
    check(
        |f| ("dimstyle.update", json!({"handles":[f.circle]})),
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
        |_| vec![("dimstyle.current", json!({"name":"Standard"}))],
    )
    .await;
}
