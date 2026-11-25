//! C8-cad-read-4: conditional drawing inputs and copied properties.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn drawing_wipeout_preserves_source_color() {
    check(
        |f| ("wipeout", json!({"handle":f.rect})),
        |f| vec![("move", json!({"handles":[f.rect],"delta":[0,2]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.rect],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn drawing_offset_current_does_not_copy_source_layer() {
    check(
        |f| ("offset", json!({"handle":f.line,"distance":2,"side":[50,5],"layer":"current"})),
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
        |f| vec![("object.setProperties", json!({"handles":[f.line],"layer":"Secondary"}))],
    )
    .await;
}
