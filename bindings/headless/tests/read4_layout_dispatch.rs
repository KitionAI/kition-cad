//! C8-cad-read-4: the bracketed LAYOUT set wrapper preserves its editor state.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn layout_set_wrapper_preserves_independent_page_settings() {
    check(
        |_| ("layout.manage", json!({"option":"set","name":"Layout1"})),
        |_| vec![("layout.delete", json!({"name":"Layout1"}))],
        |_| vec![("plot.pageSetup", json!({"layout":"Layout1","width":300,"height":200}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn layout_set_wrapper_preserves_another_layouts_rename() {
    check(
        |_| ("layout.manage", json!({"option":"set","name":"Layout1"})),
        |_| vec![("layout.delete", json!({"name":"Layout1"}))],
        |_| vec![("layout.rename", json!({"from":"Layout2","to":"Other"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn layout_set_wrapper_uses_case_insensitive_address_identity() {
    check(
        |_| ("layout.manage", json!({"option":"set","name":"Layout1"})),
        |_| vec![("layout.delete", json!({"name":"Layout1"}))],
        |_| vec![("layout.rename", json!({"from":"Layout1","to":"LAYOUT1"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn layout_set_model_forms_do_not_query_paper_names() {
    for params in [json!({"option":"set"}), json!({"option":"set","name":"Model"})] {
        check_program(
            |f| vec![("layout.manage", params.clone()), ("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
            |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"red"}))],
            |_| vec![("layout.delete", json!({"name":"Layout2"}))],
        )
        .await;
    }
}
