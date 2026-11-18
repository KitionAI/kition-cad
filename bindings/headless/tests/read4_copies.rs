//! C8-cad-read-4: copies consume stored source values and source space.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn copy_preserves_unrelated_source_group_membership() {
    check(
        |f| ("copy", json!({"handles":[f.line],"delta":[0,20]})),
        |f| vec![("properties.set", json!({"handles":[f.line],"color":"blue"}))],
        |f| vec![("groupedit", json!({"name":"Group","add":[f.circle]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn array_preserves_unrelated_source_layer_lock() {
    check(
        |f| ("arrayrect", json!({"handles":[f.line],"rows":2,"cols":2,"rowSpacing":20,"colSpacing":20})),
        |f| vec![("properties.set", json!({"handles":[f.line],"color":"blue"}))],
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
    )
    .await;
}
