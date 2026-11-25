//! Public SDK adoption: membership, prefix planning and incremental boundary guards.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

// Removal IDs are opaque membership data; referenced additions still hold existence.
#[tokio::test(flavor = "multi_thread")]
async fn group_nonmember_handle_is_an_opaque_membership_id() {
    check(
        |f| ("group.edit", json!({"name":"Group","remove":[f.circle]})),
        |f| vec![("group.edit", json!({"name":"Group","add":[f.circle]}))],
        |f| vec![("object.delete", json!({"handles":[f.circle]}))],
    )
    .await;
}

// FIND holds matching membership; planned following consumers hold their actual inputs.
#[tokio::test(flavor = "multi_thread")]
async fn find_without_followup_does_not_hold_matching_word_values() {
    check(
        |_| ("edit.find", json!({"find":"Before"})),
        |f| vec![("text.edit", json!({"handle":f.text,"text":"Other"}))],
        |f| vec![("text.edit", json!({"handle":f.text,"text":"Before colleague"}))],
    )
    .await;
}

// Layer changes refresh the line's indirectly derived visibility before the stale write.
#[tokio::test(flavor = "multi_thread")]
async fn boundary_visibility_is_refreshed_from_layer_changes() {
    check_with_setup(
        |_| ("boundary.add", json!({"points":[[50,25]]})),
        |_| vec![("layer.set", json!({"name":"Editable","on":false}))],
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
        |_| vec![("line.add", json!({"points":[[100,50],[0,50],[0,0]]}))],
    )
    .await;
}

// Extents change a stationary curve's tessellation, requiring an indirect outline refresh.
#[tokio::test(flavor = "multi_thread")]
async fn boundary_outline_is_refreshed_from_extents_changes() {
    check(
        |_| ("hatch.add", json!({"points":[[100,100]],"pattern":"SOLID"})),
        |f| {
            let observed = f.disk();
            let mut latest = kition_cad_kit::CadDocument::open(FILE, &observed).unwrap();
            use kition_kit::KitDocument;
            latest.execute("line.add", json!({"points":[[10000,10000],[20000,20000]]})).unwrap();
            assert_ne!(
                sampled_hatch_kind(&observed),
                sampled_hatch_kind(&latest.save(Some(FILE)).unwrap()),
                "changed extents must really change the stored output loops"
            );
            vec![("line.add", json!({"points":[[10000,10000],[20000,20000]]}))]
        },
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}

fn sampled_hatch_kind(bytes: &[u8]) -> serde_json::Value {
    use kition_kit::KitDocument;
    let mut doc = kition_cad_kit::CadDocument::open(FILE, bytes).unwrap();
    let result = doc.execute("hatch.add", json!({"points":[[100,100]],"pattern":"SOLID"})).unwrap();
    let id = result.get("handle").unwrap_or(&result["handles"][0]).as_str().unwrap();
    let h = kition_cad_doc::Handle::parse_hex(id).unwrap();
    let saved = kition_cad_io::read(&doc.save(Some(FILE)).unwrap(), FILE).unwrap();
    serde_json::to_value(&saved.entity(h).unwrap().kind).unwrap()
}

// constraints.rs::run_delconstraint compares these handles only with saved refs;
// an unconstrained deleted object is not dereferenced by the native command.
#[tokio::test(flavor = "multi_thread")]
async fn delconstraint_nonmember_handle_is_an_opaque_membership_id() {
    check(
        |f| ("constraint.delete", json!({"handles":[f.circle]})),
        |f| vec![("constraint.fix", json!({"h1":f.circle}))],
        |f| vec![("object.delete", json!({"handles":[f.circle]}))],
    )
    .await;
}
