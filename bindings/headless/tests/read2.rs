//! C8-cad-read-2: independent writer necessity controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn setting_assignments_keep_copied_style_dependencies() {
    check(
        |_| ("dimstyle.current", json!({"name":"Standard"})),
        |_| vec![("dimstyle", json!({"name":"Standard","textHeight":8,"current":false}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"radius":7}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dimension_sync_holds_presence_and_written_numeric_values() {
    for present in [false, true] {
        let f = Fixture::new();
        if present {
            f.colleague(&[("setvar", json!({"name":"DIMASZ","value":2.0}))]);
        }
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        f.colleague(&[("setvar", json!({"name":"DIMASZ","value":3})), ("move", json!({"handles":[f.circle],"delta":[1,0]}))]);
        let before = f.disk();
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"dimstyle.current","params":{"name":"ISO-25"}}]})).await;
        // same-field decision: synchronization replaces DIMASZ.
        assert_eq!(answer.unwrap_err()["error"]["code"], "conflict");
        assert_eq!(f.disk(), before);
        // A fresh observation explicitly accepts replacing the colleague's value.
        // Retain the original reset-value and unrelated-movement assertions on retry.
        let fresh = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        f.call("run", json!({"basis":fresh,"steps":[{"command":"dimstyle.current","params":{"name":"ISO-25"}}]})).await.unwrap();
        let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
        assert_eq!(d.header.f64("DIMASZ", 0.0), 2.5);
        let handle = kition_cad_doc::Handle::parse_hex(f.circle.as_str().unwrap()).unwrap();
        let kition_cad_doc::EntityKind::Circle(c) = &d.entity(handle).unwrap().kind else { panic!("circle") };
        assert_eq!(c.center.x, 101.0);
        f.kits.shutdown().await;
    }
}
