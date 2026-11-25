//! C8-cad-pin: narrower declarations must merge unrelated writes and refuse necessary changes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn repeated_block_inserts_hold_copied_attribute_definitions_only() {
    for command in ["point.divide", "point.measure"] {
        check_with_setup(
            |f| {
                (
                    command,
                    if command == "point.divide" {
                        json!({"handle":f.circle,"segments":3,"block":"Repeated"})
                    } else {
                        json!({"handle":f.circle,"length":3,"block":"Repeated"})
                    },
                )
            },
            |f| {
                let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
                let attdef =
                    d.model.iter().find(|e| matches!(&e.kind, kition_cad_doc::EntityKind::AttDef(ad) if ad.tag == "NEW")).unwrap().handle.hex();
                vec![("block.addDialog", json!({"name":"Repeated","base":[0,0],"handles":[attdef],"keep":"retain"}))]
            },
            |f| vec![("block.addDialog", json!({"name":"Repeated","base":[10,10],"handles":[f.rect],"keep":"retain"}))],
            |f| {
                vec![
                    ("block.addDialog", json!({"name":"Repeated","base":[0,0],"handles":[f.joined],"keep":"retain"})),
                    ("attribute.define", json!({"tag":"NEW","default":"copied","at":[200,200],"height":3})),
                ]
            },
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn multileader_style_reads_follow_the_content_branch() {
    for block in [false, true] {
        check(
            |_| {
                (
                    "multileader.add",
                    if block {
                        json!({"points":[[0,0],[10,10]],"block":"Marker","blockAttach":"insertion"})
                    } else {
                        json!({"points":[[10,10]],"text":"label","leaderType":"none"})
                    },
                )
            },
            |_| {
                vec![(
                    "multileaderStyle.open",
                    if block { json!({"name":"Standard","dogleg":99}) } else { json!({"name":"Standard","textHeight":9}) },
                )]
            },
            |_| {
                vec![(
                    "multileaderStyle.open",
                    if block { json!({"name":"Standard","textHeight":9}) } else { json!({"name":"Standard","dogleg":99}) },
                )]
            },
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn attribute_sync_preserves_reference_appearance() {
    check(
        |_| ("attribute.sync", json!({"name":"Marker"})),
        |f| vec![("attribute.edit", json!({"handle":f.insert,"values":{"CODE":"colleague"}}))],
        |f| vec![("object.setProperties", json!({"handles":[f.insert],"color":"blue"}))],
    )
    .await;
}
