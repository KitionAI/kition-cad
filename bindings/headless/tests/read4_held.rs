//! C8-cad-read-4: concrete reasons for retained holds, with independent merge controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn held_layer_current_copies_other_layer_data_into_previous_snapshot() {
    check(
        |_| ("layer.setCurrent", json!({"name":"Secondary"})),
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_layer_deletion_discards_every_member_value() {
    check(
        |_| ("layer.deleteWithObjects", json!({"names":["Secondary"]})),
        |f| vec![("object.setProperties", json!({"handles":[f.joined],"color":"blue"}))],
        |f| vec![("object.move", json!({"handles":[f.circle],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_linetype_library_scale_uses_measurement() {
    check(
        |_| ("property.linetype", json!({"load":"HIDDEN"})),
        |_| vec![("variable.set", json!({"name":"MEASUREMENT","value":0}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_setvar_refuses_the_assigned_setting_even_at_same_destination() {
    check(
        |_| ("variable.set", json!({"name":"LTSCALE","value":2})),
        |_| vec![("variable.set", json!({"name":"LTSCALE","value":2}))],
        |_| vec![("file.setUnits", json!({"insunits":1}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn held_region_cannot_discard_a_new_constraint() {
    check(
        |f| ("region.add", json!({"handles":[f.circle]})),
        |f| vec![("constraint.fix", json!({"h1":f.circle}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
