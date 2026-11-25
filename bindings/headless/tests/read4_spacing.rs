//! C8-cad-read-4: spacing uses the base geometry and baseline scale rather than unrelated style data.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn spacing_preserves_base_color() {
    check(
        |f| ("dimension.space", json!({"base":f.dim,"handles":[f.attached_dim],"spacing":12})),
        |f| vec![("object.move", json!({"handles":[f.dim],"delta":[0,5]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.dim],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn spacing_noop_preserves_base_geometry() {
    check(
        |f| ("dimension.space", json!({"base":f.dim,"handles":[f.dim],"spacing":12})),
        |f| vec![("object.delete", json!({"handles":[f.dim]}))],
        |f| vec![("object.move", json!({"handles":[f.dim],"delta":[0,5]}))],
    )
    .await;
}
