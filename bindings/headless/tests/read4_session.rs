//! C8-cad-read-4: session variables are distinct from same-named DXF header data.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::json;
#[tokio::test(flavor = "multi_thread")]
async fn session_orthomode_does_not_assign_the_imported_header() {
    for outside in [false, true] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let observed = f.disk();
        let mut d = kition_cad_io::read(&observed, FILE).unwrap();
        d.header.set_i64("ORTHOMODE", 1);
        if outside {
            f.rewrite(|d| d.header.set_i64("ORTHOMODE", 1));
        } else {
            kition_kits::tools::call_as(&f.kits, "cad_batch", &json!({"path":FILE,"ops":[{"op":"map.set","target":"drawing","field":"header","key":"ORTHOMODE","value":d.header.get("ORTHOMODE")}]}),kition_kits::tools::CallOptions::default(),"colleague").await.unwrap().unwrap();
        }
        let before = f.disk();
        assert!(before != observed);
        let mut oracle = CadDocument::open(FILE, &before).unwrap();
        oracle.execute("variable.set", json!({"name":"ORTHOMODE","value":0})).unwrap();
        let expected = kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap();
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"variable.set","params":{"name":"ORTHOMODE","value":0}}]})).await;
        answer.unwrap_or_else(|e| panic!("unused imported header must apply, outside={outside}: {e}"));
        let actual = kition_cad_io::read(&f.disk(), FILE).unwrap();
        assert_eq!(actual.header.i64("ORTHOMODE", 0), 1, "the colleague's header is preserved");
        assert_eq!(content(&actual), content(&expected));
        f.kits.shutdown().await;
    }
}
