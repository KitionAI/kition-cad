//! C8-cad-read-4: source shapes do not consume source display properties.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn source_xline_uses_line_geometry() {
    check(
        |f| ("constructionLine.add", json!({"handle":f.line,"distance":5,"side":[50,10]})),
        |f| vec![("object.move", json!({"handles":[f.line],"delta":[0,2]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn source_hatch_uses_closed_geometry() {
    check(
        |f| ("hatch.add", json!({"handles":[f.rect],"pattern":"SOLID"})),
        |f| vec![("object.move", json!({"handles":[f.rect],"delta":[0,2]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.rect],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn source_spline_conversion_preserves_unassigned_common() {
    check(
        |f| ("spline.add", json!({"object":f.rect})),
        |f| vec![("object.move", json!({"handles":[f.rect],"delta":[0,2]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.rect],"color":"blue"}))],
    )
    .await;
}
