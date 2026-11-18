//! C8-cad-read-4: copied records consume stored payloads; group removal ignores other group positions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn layout_copy_ignores_source_layer_lock() {
    check(
        |_| ("layout.copy", json!({"from":"Layout1","to":"Sheet"})),
        |f| vec![("viewport.set", json!({"handle":f.viewport,"viewHeight":10}))],
        |_| vec![("layer.set", json!({"name":"0","locked":true}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn block_reference_ignores_member_layer_lock() {
    check(
        |_| ("insert", json!({"name":"Marker","at":[700,0]})),
        |f| vec![("block", json!({"name":"Marker","base":[500,0],"handles":[f.rect],"keep":"retain"}))],
        |_| vec![("layer.set", json!({"name":"0","locked":true}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ungroup_ignores_a_preceding_group_deletion() {
    check_with_setup(
        |_| ("ungroup", json!({"name":"Group"})),
        |f| vec![("groupedit", json!({"name":"Group","add":[f.rect]}))],
        |_| vec![("ungroup", json!({"name":"Before"}))],
        |f| vec![("groupedit", json!({"name":"Group","rename":"Before"})), ("group", json!({"name":"Group","handles":[f.circle]}))],
    )
    .await;
}
