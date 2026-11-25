//! C8-cad-read-4: LEADER consumes its style name and the optional text height.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn leader_uses_text_height_but_not_dimension_override_headers() {
    check(
        |_| ("leader.add", json!({"points":[[600,0],[610,10]],"text":"Agent"})),
        |_| vec![("dimensionStyle.open", json!({"name":"ISO-25","textHeight":8}))],
        |_| vec![("variable.set", json!({"name":"DIMASZ","value":8}))],
    )
    .await;
}
