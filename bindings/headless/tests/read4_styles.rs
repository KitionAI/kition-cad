//! C8-cad-read-4: named style identity differs from unused style data.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn style_reference_uses_only_font_geometry_fields() {
    check_with_setup(
        |_| ("dimension.addLinear", json!({"p1":[700,0],"p2":[710,0],"at":[705,-10]})),
        |_| vec![("style", json!({"name":"S","height":8,"current":false}))],
        |_| vec![("style", json!({"name":"S","annotative":true,"current":false}))],
        |_| vec![("style", json!({"name":"S","current":false})), ("variable.set", json!({"name":"DIMTXSTY","value":"S"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn style_copy_preserves_font_data() {
    check(
        |_| ("dimensionStyle.open", json!({"name":"Fresh","current":false})),
        |_| vec![("dimensionStyle.open", json!({"name":"ISO-25","arrowSize":8,"current":false}))],
        |_| vec![("style", json!({"name":"Standard","height":8,"current":false}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn style_record_rename_preserves_data() {
    check(
        |_| ("rename", json!({"table":"layer","from":"Editable","to":"Mine"})),
        |_| vec![("layer.set", json!({"name":"Editable","newName":"Theirs"}))],
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn style_rename_with_constraints_preserves_display_settings() {
    check_with_setup(
        |_| ("rename", json!({"table":"layer","from":"Editable","to":"Mine"})),
        |_| vec![("layer.set", json!({"name":"Editable","newName":"Theirs"}))],
        |_| vec![("constraint.settings", json!({"barTransparency":40}))],
        |f| vec![("constraint.perpendicular", json!({"h1":f.line,"h2":f.joined}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn style_explosion_uses_font_geometry_fields() {
    check(
        |f| ("explode", json!({"handles":[f.attached_dim]})),
        |_| vec![("style", json!({"name":"Standard","height":8,"current":false}))],
        |_| vec![("style", json!({"name":"Standard","annotative":true,"current":false}))],
    )
    .await;
}
