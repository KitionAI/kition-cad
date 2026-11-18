//! C8-cad-read-4: an occupied creation name changes the insertion gap without changing tab-order seed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn destination_new_layout_holds_the_empty_name_slot() {
    check(
        |_| ("layout.new", json!({"name":"Sheet","viewport":false})),
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Sheet"}))],
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Other"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn destination_layout_copy_holds_the_empty_name_slot() {
    check(
        |_| ("layout.copy", json!({"from":"Layout1","to":"Sheet"})),
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Sheet"}))],
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Other"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn destination_layout_dispatch_new_holds_the_empty_name_slot() {
    check(
        |_| ("layout", json!({"option":"new","name":"Sheet"})),
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Sheet"}))],
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Other"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn destination_layout_dispatch_copy_holds_the_empty_name_slot() {
    check(
        |_| ("layout", json!({"option":"copy","name":"Layout1","to":"Sheet"})),
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Sheet"}))],
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Other"}))],
    )
    .await;
}
