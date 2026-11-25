//! C8-cad-read-4: CHSPACE uses the chosen viewport and transformed geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn space_change_preserves_source_color() {
    check(
        |f| ("object.changeSpace", json!({"handles":[f.line],"to":"Layout1"})),
        |f| vec![("viewport.set", json!({"handle":f.viewport,"viewHeight":10}))],
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn space_change_ignores_unselected_destination_entities() {
    check(
        |f| ("object.changeSpace", json!({"handles":[f.line],"to":"Layout1"})),
        |f| vec![("viewport.set", json!({"handle":f.viewport,"viewHeight":10}))],
        |_| vec![("line.add", json!({"points":[[1000,1000],[1100,1000]],"layout":"Layout1"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn space_change_holds_its_remeasured_dimension() {
    check(
        |f| ("object.changeSpace", json!({"handles":[f.line],"to":"Layout1"})),
        |f| vec![("text.edit", json!({"handle":f.attached_dim,"text":"Colleague"}))],
        |f| vec![("object.setProperties", json!({"handles":[f.attached_dim],"color":"blue"}))],
    )
    .await;
}
