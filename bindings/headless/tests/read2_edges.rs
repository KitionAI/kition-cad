//! C8-cad-read-2: an explicit cutter must remain in the space whose scan consumes it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use kition_cad_doc::{EntityKind, Handle, Space};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_edge_relocated_without_geometry_changes_conflicts() {
    for relevant in [true, false] {
        let f = Fixture::new();
        let edge = Handle::parse_hex(f.joined.as_str().unwrap()).unwrap();
        f.rewrite(|d| {
            d.modify_entity(edge, |e| {
                if let EntityKind::Line(l) = &mut e.kind {
                    l.a.x = 50.0;
                    l.a.y = -10.0;
                    l.b.x = 50.0;
                    l.b.y = 10.0;
                }
            })
            .unwrap();
        });
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let moved = if relevant { edge } else { Handle::parse_hex(f.circle.as_str().unwrap()).unwrap() };
        f.rewrite(|d| {
            let e = d.remove_entity(moved).unwrap();
            d.space_mut(&Space::Paper("Layout1".into())).unwrap().push(e.as_ref().clone());
        });
        let before = f.disk();
        let params = json!({"handle":f.line,"pick":[75,0],"edges":[f.joined],"mode":"quick"});
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"object.trim","params":params}]})).await;
        if relevant {
            let error = answer.expect_err("the cutting edge left the scan space");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(f.disk(), before);
        } else {
            answer.unwrap();
            let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
            assert_eq!(d.space_of(moved), Some(Space::Paper("Layout1".into())));
            assert!(d.model.iter().any(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 0.0 && l.b.x == 50.0)), "trim result survives");
        }
        f.kits.shutdown().await;
    }
}
