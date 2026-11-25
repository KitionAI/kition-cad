//! C8-cad-read-4: simple polyline flags do not consume preserved geometry or common properties.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn polyline_open_preserves_color() {
    check(
        |f| ("polyline.edit", json!({"handle":f.rect,"option":"open"})),
        |f| vec![("polyline.edit", json!({"handle":f.rect,"option":"open"}))],
        |f| vec![("object.setProperties", json!({"handles":[f.rect],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn polyline_open_preserves_vertex_coordinates() {
    check(
        |f| ("polyline.edit", json!({"handle":f.rect,"option":"open"})),
        |f| vec![("polyline.edit", json!({"handle":f.rect,"option":"open"}))],
        |f| vec![("polyline.edit", json!({"handle":f.rect,"option":"vertex","index":1,"action":"move","to":[512,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn polyline_close_uses_eligibility_without_vertex_coordinates() {
    check_with_setup(
        |f| ("polyline.edit", json!({"handle":f.rect,"option":"close"})),
        |f| vec![("polyline.edit", json!({"handle":f.rect,"option":"close"}))],
        |f| vec![("polyline.edit", json!({"handle":f.rect,"option":"vertex","index":1,"action":"move","to":[512,0]}))],
        |f| vec![("polyline.edit", json!({"handle":f.rect,"option":"open"}))],
    )
    .await;
}
