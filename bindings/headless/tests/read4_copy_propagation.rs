//! C8-cad-read-4: copying geometry does not remeasure dimensions attached to the original.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn copied_rotation_preserves_original_dimension_text() {
    check(
        |f| ("object.rotate", json!({"handles":[f.line],"base":[0,0],"angle":90,"copy":true})),
        |f| vec![("object.setProperties", json!({"handles":[f.line],"end":[110,0]}))],
        |f| vec![("text.edit", json!({"handle":f.attached_dim,"text":"Colleague"}))],
    )
    .await;
}
