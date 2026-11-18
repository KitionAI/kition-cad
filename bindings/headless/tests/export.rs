//! `kition-cad` exports as its manifest declares (KHP A41, Kition platform K21): `kit.initialize`
//! reports the formats, the agent may run `file.export` (what `cad_export` runs), and
//! `document.export` writes a new file of each format.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::types::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::{Map, json};

#[test]
fn issue_export_the_headless_kit_reports_and_writes_its_exports() {
    let mut kit = Headless::<CadDocument>::with_replica_port(KitInfo::from_manifest(include_str!("../../../kit.json"), &[], &[]).unwrap());
    assert_eq!(kit.capabilities()["formats"]["export"], json!(["pdf", "svg", "png"]));
    let root = std::env::temp_dir().join(format!("kition-cad-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.dxf"), root.join("basic.dxf")).unwrap();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("basic.dxf").unwrap(), read_only: false, create: None }).unwrap();

    let listed = kit.commands(Some(&opened.id)).unwrap();
    assert!(listed.iter().any(|command| command.id == "file.export"), "the agent may run the export command");
    let executed = kit.execute(&opened.id, "file.export", Map::from_iter([("format".into(), json!("svg"))]), Origin::Agent).unwrap();
    assert!(!executed.changed, "an export never edits the drawing");
    let result = executed.result.unwrap();
    assert_eq!(result["format"], "svg");
    assert!(result["data"].as_str().is_some_and(|data| !data.is_empty()), "{result}");

    for format in ["pdf", "svg", "png"] {
        let name = format!("basic.{format}");
        kit.export(&opened.id, &folder.export(&name).unwrap(), format, None).unwrap();
        let bytes = std::fs::read(root.join(&name)).unwrap();
        let magic: &[u8] = match format {
            "pdf" => b"%PDF-",
            "svg" => b"<svg ",
            _ => b"\x89PNG\r\n\x1a\n",
        };
        assert!(bytes.starts_with(magic), "{name} is a {format} file");
    }
    assert!(!kit.document(&opened.id).unwrap().dirty);
    let _ = std::fs::remove_dir_all(root);
}
