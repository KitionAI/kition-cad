//! A drawing built by the same commands saves the same DXF bytes when `SOURCE_DATE_EPOCH` is set,
//! as a showcase sample or a gating test builds it: the replica's actor (whose slot
//! numbers new handles) and the document id come from a counter in the process instead of
//! randomness. Each build runs in a process of its own: this test starts itself twice.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::json;

const OUT: &str = "KITION_CAD_DETERMINISM_OUT";

fn build() -> Vec<u8> {
    let mut doc = CadDocument::create("plan.dxf", 0, 0).unwrap();
    doc.execute("line", json!({ "points": [[0, 0], [100, 0]] })).unwrap();
    doc.execute("circle", json!({ "center": [50, 50], "radius": 20 })).unwrap();
    doc.execute("text", json!({ "at": [10, 80], "height": 5, "text": "Plan" })).unwrap();
    doc.execute("dimlinear", json!({ "p1": [0, 0], "p2": [100, 0], "at": [50, -15] })).unwrap();
    doc.save(Some("plan.dxf")).unwrap()
}

/// The child's half: builds the drawing into the file `OUT` names (nothing without it).
#[test]
fn child_builds_a_drawing() {
    if let Ok(path) = std::env::var(OUT) {
        std::fs::write(path, build()).unwrap();
    }
}

fn build_in_a_process(path: &PathBuf) -> Vec<u8> {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_builds_a_drawing", "--test-threads=1", "--quiet"])
        .env("SOURCE_DATE_EPOCH", "1794182400")
        .env(OUT, path)
        .status()
        .unwrap();
    assert!(status.success(), "the child build failed");
    std::fs::read(path).unwrap()
}

#[test]
fn the_same_commands_save_the_same_bytes() {
    let dir = std::env::temp_dir().join(format!("kition-cad-determinism-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = build_in_a_process(&dir.join("0.dxf"));
    for build in 1..4 {
        let again = build_in_a_process(&dir.join(format!("{build}.dxf")));
        assert!(first == again, "build {build} saved other bytes than build 0");
    }
    std::fs::remove_dir_all(&dir).ok();
}
