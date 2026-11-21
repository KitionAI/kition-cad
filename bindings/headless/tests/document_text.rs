//! `document.text` (KitText 1.0) through `kition-cad` as `kition-headless` serves it: the
//! manifest's capability reaches `kit.initialize`, and the executable answers the drawing's words
//! without changing it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_cad_kit::CadDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::DOCUMENT_TEXT_LIMIT;
use kition_kit_server::{ErrorReason, KitBackend, OpenRequest};
use serde_json::json;

#[test]
fn answers_document_text_as_the_manifest_declares() {
    let mut kit = Headless::<CadDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_cad_kit::EDITOR_STATE).unwrap(),
    );
    assert_eq!(kit.capabilities()["capabilities"], json!([kition_kit::text::CAPABILITY]));
    let root = std::env::temp_dir().join(format!("kition-cad-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let path = root.join("room.dxf");
    std::fs::copy(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/check-dimension-text-overridden.dxf")), &path).unwrap();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("room.dxf").unwrap(), read_only: false, create: None }).unwrap();
    assert_eq!(
        kit.text(&opened.id, DOCUMENT_TEXT_LIMIT).unwrap(),
        json!({ "text": "Room\n1200", "truncated": false, "parts": [{ "label": "Model", "start": 0, "end": 9 }] })
    );
    assert_eq!(kit.text(&opened.id, 4).unwrap(), json!({ "text": "Room", "truncated": true, "parts": [{ "label": "Model", "start": 0, "end": 4 }] }));
    assert!(!kit.document(&opened.id).unwrap().dirty);
    assert_eq!(kit.text("no-such-document", 10).unwrap_err().reason(), Some(ErrorReason::DocumentNotFound));
    let _ = std::fs::remove_dir_all(root);
}
