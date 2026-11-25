//! C8-cad-read-4: a common property does not invoke the geometry solver.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn common_color_preserves_new_constraint_but_holds_written_color() {
    check(
        |f| ("object.setProperties", json!({"handles":[f.circle],"color":"blue"})),
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"red"}))],
        |f| vec![("constraint.fix", json!({"h1":f.circle}))],
    )
    .await;
}
