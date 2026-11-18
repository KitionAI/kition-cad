//! C8-cad-read-4: simple polyline flags do not consume preserved geometry or common properties.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn polyline_open_preserves_color() {
    check(
        |f| ("pedit", json!({"handle":f.rect,"option":"open"})),
        |f| vec![("pedit", json!({"handle":f.rect,"option":"open"}))],
        |f| vec![("properties.set", json!({"handles":[f.rect],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn polyline_open_preserves_vertex_coordinates() {
    check(
        |f| ("pedit", json!({"handle":f.rect,"option":"open"})),
        |f| vec![("pedit", json!({"handle":f.rect,"option":"open"}))],
        |f| vec![("pedit", json!({"handle":f.rect,"option":"vertex","index":1,"action":"move","to":[512,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn polyline_close_uses_eligibility_without_vertex_coordinates() {
    check_with_setup(
        |f| ("pedit", json!({"handle":f.rect,"option":"close"})),
        |f| vec![("pedit", json!({"handle":f.rect,"option":"close"}))],
        |f| vec![("pedit", json!({"handle":f.rect,"option":"vertex","index":1,"action":"move","to":[512,0]}))],
        |f| vec![("pedit", json!({"handle":f.rect,"option":"open"}))],
    )
    .await;
}
