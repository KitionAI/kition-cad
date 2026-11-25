//! C8-cad-read-4: dimension field assignments preserve attachments and geometry they do not consume.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn dimension_disassociation_preserves_remeasured_coordinates() {
    check(
        |f| ("dimension.disassociate", json!({"handles":[f.attached_dim]})),
        |f| vec![("dimension.disassociate", json!({"handles":[f.attached_dim]}))],
        |f| vec![("object.move", json!({"handles":[f.line],"delta":[0,5]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_text_override_holds_its_assigned_text() {
    check(
        |f| ("dimension.override", json!({"handles":[f.attached_dim],"text":"Agent"})),
        |f| vec![("text.edit", json!({"handle":f.attached_dim,"text":"Colleague"}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_assigned_arrow_override_same_server() {
    check(
        |f| ("dimension.override", json!({"handles":[f.attached_dim],"DIMASZ":3})),
        |f| vec![("dimension.override", json!({"handles":[f.attached_dim],"DIMASZ":8}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
