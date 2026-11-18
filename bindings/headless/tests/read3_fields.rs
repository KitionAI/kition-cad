//! C8-cad-read-3: dimension assignment classes and unrelated-field controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn dimension_override_replaces_only_the_named_override() {
    check(
        |f| ("dimoverride", json!({"handles":[f.dim],"DIMASZ":4})),
        |f| vec![("dimoverride", json!({"handles":[f.dim],"DIMASZ":8}))],
        |f| vec![("dimoverride", json!({"handles":[f.dim],"DIMTXT":8}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_justification_replaces_only_text_justification() {
    check(
        |f| ("dimtedit.left", json!({"handles":[f.dim]})),
        |f| vec![("dimtedit.right", json!({"handles":[f.dim]}))],
        |f| vec![("dimoverride", json!({"handles":[f.dim],"DIMASZ":8}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn flatten_holds_replaced_elevation_but_allows_planar_geometry() {
    for relevant in [false, true] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let h = kition_cad_doc::Handle::parse_hex(f.circle.as_str().unwrap()).unwrap();
        f.rewrite(|d| {
            d.modify_entity(h, |e| {
                if let kition_cad_doc::EntityKind::Circle(c) = &mut e.kind {
                    if relevant {
                        c.center.z = 4.0;
                    } else {
                        c.center.x += 1.0;
                    }
                }
            })
            .unwrap();
        });
        let before = f.disk();
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"flatten","params":{"handles":[f.circle]}}]})).await;
        if relevant {
            assert_eq!(answer.err().map(|e| e["error"]["code"].clone()), Some(json!("conflict")));
            assert_eq!(f.disk(), before);
        } else {
            answer.unwrap();
            let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
            let kition_cad_doc::EntityKind::Circle(c) = &d.entity(h).unwrap().kind else { panic!("circle") };
            assert_eq!((c.center.x, c.center.z), (101.0, 0.0));
        }
        f.kits.shutdown().await;
    }
}
