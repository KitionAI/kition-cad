//! C8-cad-read-4: associative propagation consumes attachments and measured points.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn propagation_move_holds_rendered_dimension_text() {
    check(
        |f| ("move", json!({"handles":[f.line],"delta":[0,5]})),
        |f| vec![("textedit", json!({"handle":f.attached_dim,"text":"Colleague"}))],
        |f| vec![("properties.set", json!({"handles":[f.attached_dim],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn propagation_override_preserves_source_line_color() {
    check(
        |f| ("dimoverride", json!({"handles":[f.attached_dim],"DIMASZ":3})),
        |f| vec![("dimdisassociate", json!({"handles":[f.attached_dim]}))],
        |f| vec![("properties.set", json!({"handles":[f.line],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn propagation_remeasures_with_font_metrics() {
    check(
        |f| ("move", json!({"handles":[f.line],"delta":[0,5]})),
        |_| vec![("style", json!({"name":"Standard","height":8}))],
        |_| vec![("style", json!({"name":"Standard","annotative":true}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn propagation_holds_a_remeasured_targets_lock() {
    check_with_setup(
        |f| ("move", json!({"handles":[f.line],"delta":[0,5]})),
        |_| vec![("layer.set", json!({"name":"Secondary","locked":true}))],
        |_| vec![("layer.set", json!({"name":"Secondary","color":"blue"}))],
        |f| vec![("properties.set", json!({"handles":[f.attached_dim],"layer":"Secondary"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn rendered_dimension_uses_positive_style_scale_before_header_scale() {
    check(
        |f| ("explode", json!({"handles":[f.dim]})),
        |_| vec![("dimstyle", json!({"name":"ISO-25","scale":2,"current":false}))],
        |_| vec![("setvar", json!({"name":"DIMSCALE","value":2}))],
    )
    .await;
}
