//! C8-cad-read-4: each excess-premise class has saved-merge and necessary-change controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn defaults_layout_without_viewport_does_not_read_current_color() {
    check(
        |_| ("layout.add", json!({"name":"Mine","viewport":false})),
        |_| vec![("units", json!({"insunits":1})), ("layout.add", json!({"name":"Other","viewport":false}))],
        |_| vec![("color", json!({"color":"red"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn defaults_viewport_limits_only_when_model_is_empty() {
    check(
        |_| ("viewport.add", json!({"layout":"Layout1","p1":[0,0],"p2":[20,20]})),
        |f| vec![("move", json!({"handles":[f.circle],"delta":[1000,0]}))],
        |_| vec![("limits", json!({"min":[0,0],"max":[999,999]}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn defaults_explicit_table_height_does_not_read_style_height() {
    check(
        |_| ("table", json!({"at":[0,300],"rows":2,"cols":2,"textHeight":3})),
        |_| vec![("tableStyle.open", json!({"name":"Standard","margin":9}))],
        |_| vec![("tableStyle.open", json!({"name":"Standard","textHeight":9}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn defaults_retained_block_uses_source_properties() {
    check(
        |f| ("block", json!({"name":"Mine","handles":[f.line],"base":[0,0],"keep":"retain"})),
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
        |_| vec![("color", json!({"color":"red"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn defaults_spline_conversion_uses_source_properties() {
    check(
        |f| ("spline", json!({"object":f.rect})),
        |f| vec![("move", json!({"handles":[f.rect],"delta":[1,0]}))],
        |_| vec![("color", json!({"color":"red"}))],
    )
    .await;
}
