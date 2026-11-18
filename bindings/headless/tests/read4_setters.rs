//! C8-cad-read-4: setters hold assigned fields and preserve unrelated fields.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn setter_viewport_scale_preserves_frozen_layers() {
    check(
        |f| ("viewport.set", json!({"handle":f.viewport,"scale":0.1})),
        |f| vec![("viewport.set", json!({"handle":f.viewport,"scale":0.2}))],
        |f| vec![("viewport.set", json!({"handle":f.viewport,"freeze":["Secondary"]}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn setter_table_style_preserves_margin() {
    check(
        |_| ("tablestyle", json!({"name":"Standard","textHeight":3,"current":false})),
        |_| vec![("tablestyle", json!({"name":"Standard","textHeight":9,"current":false}))],
        |_| vec![("tablestyle", json!({"name":"Standard","margin":9,"current":false}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn setter_mleader_style_preserves_text_height() {
    check(
        |_| ("mleaderstyle", json!({"name":"Standard","arrowSize":3,"current":false})),
        |_| vec![("mleaderstyle", json!({"name":"Standard","arrowSize":9,"current":false}))],
        |_| vec![("mleaderstyle", json!({"name":"Standard","textHeight":9,"current":false}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn setter_rename_layer_preserves_color() {
    check(
        |_| ("layer.set", json!({"name":"Editable","newName":"Mine"})),
        |_| vec![("layer.set", json!({"name":"Editable","newName":"Theirs"}))],
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn setter_draw_order_preserves_color() {
    check(
        |f| ("draworder.front", json!({"handles":[f.circle]})),
        |f| vec![("draworder.back", json!({"handles":[f.circle]}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn setter_hatch_pattern_preserves_color() {
    check(
        |f| ("hatchedit", json!({"handles":[f.hatch],"pattern":"ANSI31"})),
        |f| vec![("hatchedit", json!({"handles":[f.hatch],"pattern":"ANSI37"}))],
        |f| vec![("properties.set", json!({"handles":[f.hatch],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn setter_group_rename_preserves_members() {
    check(
        |_| ("groupedit", json!({"name":"Group","rename":"Mine"})),
        |_| vec![("groupedit", json!({"name":"Group","rename":"Theirs"}))],
        |f| vec![("groupedit", json!({"name":"Group","add":[f.circle]}))],
    )
    .await;
}
