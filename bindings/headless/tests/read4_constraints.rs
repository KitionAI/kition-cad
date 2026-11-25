//! C8-cad-read-4: constraint writes protect solver geometry and assigned settings.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn constraints_hideall_preserves_transparency() {
    check_with_setup(
        |_| ("constraint.hideAllBars", json!({})),
        |_| vec![("constraint.settings", json!({"barsVisible":false}))],
        |_| vec![("constraint.settings", json!({"barTransparency":40}))],
        |f| vec![("constraint.perpendicular", json!({"h1":f.line,"h2":f.joined}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn constraints_setting_preserves_other_setting() {
    check_with_setup(
        |_| ("constraint.settings", json!({"distanceTolerance":0.1})),
        |_| vec![("constraint.settings", json!({"distanceTolerance":0.2}))],
        |_| vec![("constraint.settings", json!({"barTransparency":40}))],
        |f| vec![("constraint.perpendicular", json!({"h1":f.line,"h2":f.joined}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn constraints_new_constraint_preserves_source_color() {
    check(
        |f| ("constraint.coincident", json!({"h1":f.line,"p1":"end","h2":f.joined,"p2":"start"})),
        |f| vec![("move", json!({"handles":[f.line],"delta":[10,0]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn constraints_showall_noop_preserves_geometry() {
    check_with_setup(
        |_| ("constraint.showAllBars", json!({})),
        |_| vec![("constraint.hideAllBars", json!({}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[5,0]}))],
        |f| vec![("constraint.perpendicular", json!({"h1":f.line,"h2":f.joined}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn constraints_delete_last_preserves_removed_constraint_geometry() {
    check_with_setup(
        |_| ("constraint.delete", json!({"ids":[1]})),
        |_| vec![("constraint.delete", json!({"ids":[1]}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[5,0]}))],
        |f| vec![("constraint.perpendicular", json!({"h1":f.line,"h2":f.joined}))],
    )
    .await;
}
