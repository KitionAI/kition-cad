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

/// The DIM* variables are each person's own (kdm_model::is_current_var): a colleague's DIMASZ is
/// theirs, so my DIMSTYLE current, which sets my DIM* variables to the style's, applies over it;
/// the saved file carries mine (the style's 2.5 where my drawing has DIMASZ, none where it has
/// not), never the colleague's 3, and the colleague's unrelated move.
#[tokio::test(flavor = "multi_thread")]
async fn dimension_sync_sets_my_dimension_variables_not_a_colleagues() {
    for present in [false, true] {
        let f = Fixture::new();
        if present {
            f.colleague(&[("setvar", json!({"name":"DIMASZ","value":2.0}))]);
        }
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        f.colleague(&[("setvar", json!({"name":"DIMASZ","value":3})), ("move", json!({"handles":[f.circle],"delta":[1,0]}))]);
        f.call("run", json!({"basis":basis,"steps":[{"command":"dimstyle.current","params":{"name":"ISO-25"}}]}))
            .await
            .unwrap_or_else(|e| panic!("a colleague's DIMASZ is theirs: {e}"));
        let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
        assert_eq!(d.header.get("DIMASZ").and_then(kition_cad_doc::HVal::as_f64), if present { Some(2.5) } else { None }, "my DIMASZ is saved");
        let handle = kition_cad_doc::Handle::parse_hex(f.circle.as_str().unwrap()).unwrap();
        let kition_cad_doc::EntityKind::Circle(c) = &d.entity(handle).unwrap().kind else { panic!("circle") };
        assert_eq!(c.center.x, 101.0);
        f.kits.shutdown().await;
    }
}
