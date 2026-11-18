//! C8-cad-read-2: named dimension sources ignore coincident neighbours.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn named_dimension_sources_merge_neighbour_edits_and_hold_the_named_geometry() {
    check(
        |f| ("dimlinear", json!({"object":f.line,"at":[50,-10]})),
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
        |f| vec![("move", json!({"handles":[f.joined],"delta":[0,10]})), ("move", json!({"handles":[f.circle],"delta":[1,0]}))],
    )
    .await;
}
