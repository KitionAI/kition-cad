//! C8-cad-read-4: concrete reasons for retained holds, with independent merge controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn held_layer_current_copies_other_layer_data_into_previous_snapshot() {
    check(
        |_| ("layer.current", json!({"name":"Secondary"})),
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_layer_deletion_discards_every_member_value() {
    check(
        |_| ("laydel", json!({"names":["Secondary"]})),
        |f| vec![("properties.set", json!({"handles":[f.joined],"color":"blue"}))],
        |f| vec![("move", json!({"handles":[f.circle],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_linetype_library_scale_uses_measurement() {
    check(
        |_| ("linetype", json!({"load":"HIDDEN"})),
        |_| vec![("setvar", json!({"name":"MEASUREMENT","value":0}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_setvar_refuses_the_assigned_setting_even_at_same_destination() {
    check(
        |_| ("setvar", json!({"name":"LTSCALE","value":2})),
        |_| vec![("setvar", json!({"name":"LTSCALE","value":2}))],
        |_| vec![("units", json!({"insunits":1}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_region_cannot_discard_a_new_constraint() {
    check(
        |f| ("region", json!({"handles":[f.circle]})),
        |f| vec![("gcfix", json!({"h1":f.circle}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
