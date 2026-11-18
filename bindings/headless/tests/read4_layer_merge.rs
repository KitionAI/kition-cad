//! C8-cad-read-4: merging uses the destination name and conditionally thaws it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn layer_merge_preserves_destination_color() {
    check(
        |_| ("laymrg", json!({"from":["Secondary"],"to":"Editable"})),
        |_| vec![("layer.set", json!({"name":"Secondary","description":"Colleague"}))],
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn layer_merge_holds_the_freeze_state_it_assigns_when_becoming_current() {
    check_with_setup(
        |_| ("laymrg", json!({"from":["Secondary"],"to":"Editable"})),
        |_| vec![("layer.set", json!({"name":"Editable","frozen":true}))],
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
        |_| vec![("layer.current", json!({"name":"Secondary"}))],
    )
    .await;
}
