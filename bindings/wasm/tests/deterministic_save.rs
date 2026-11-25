//! A drawing built by the same commands saves the same DXF bytes in a reproducible build (the SDK's
//! `reproducible-ids` feature, which this crate enables in its dev-dependencies only, and
//! `KITION_REPRODUCIBLE_IDS` set, as a showcase sample or a gating test builds it): the replica's
//! actor (whose slot numbers new handles) and the document id come from a counter in the process
//! instead of randomness. `SOURCE_DATE_EPOCH` alone, which hosts pass to every kit for timestamps,
//! leaves them random, so two kit processes never mint the same handles. Each build runs in a
//! process of its own: this test starts itself several times.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::json;

const OUT: &str = "KITION_CAD_DETERMINISM_OUT";

fn build() -> Vec<u8> {
    let mut doc = CadDocument::create("plan.dxf", 0, 0).unwrap();
    doc.execute("line.add", json!({ "points": [[0, 0], [100, 0]] })).unwrap();
    doc.execute("circle.add", json!({ "center": [50, 50], "radius": 20 })).unwrap();
    doc.execute("text.add", json!({ "at": [10, 80], "height": 5, "text": "Plan" })).unwrap();
    doc.execute("dimension.addLinear", json!({ "p1": [0, 0], "p2": [100, 0], "at": [50, -15] })).unwrap();
    doc.save(Some("plan.dxf")).unwrap()
}

/// The child's half: builds the drawing into the file `OUT` names (nothing without it).
#[test]
fn child_builds_a_drawing() {
    if let Ok(path) = std::env::var(OUT) {
        std::fs::write(path, build()).unwrap();
    }
}

const SEED: &str = "KITION_REPRODUCIBLE_IDS";

fn build_in_a_process(path: &PathBuf, env: &[(&str, &str)]) -> Vec<u8> {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "child_builds_a_drawing", "--test-threads=1", "--quiet"])
        .env(OUT, path)
        .env_remove("SOURCE_DATE_EPOCH")
        .env_remove(SEED);
    for (name, value) in env {
        command.env(name, value);
    }
    assert!(command.status().unwrap().success(), "the child build failed");
    std::fs::read(path).unwrap()
}

/// The handles of the drawing's entities (group code 5), in file order.
fn handles(dxf: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(dxf);
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    lines.chunks(2).filter(|pair| pair.len() == 2 && pair[0] == "5").map(|pair| pair[1].to_owned()).collect()
}

#[test]
fn the_same_commands_save_the_same_bytes() {
    let dir = std::env::temp_dir().join(format!("kition-cad-determinism-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let seed = [(SEED, "1794182400"), ("SOURCE_DATE_EPOCH", "1794182400")];
    let first = build_in_a_process(&dir.join("0.dxf"), &seed);
    for build in 1..4 {
        let again = build_in_a_process(&dir.join(format!("{build}.dxf")), &seed);
        assert!(first == again, "build {build} saved other bytes than build 0");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A host passes `SOURCE_DATE_EPOCH` to every kit it launches (for timestamps): two kit processes
/// started with the same epoch still get random actors, so the handles their new entities get
/// differ.
#[test]
fn source_date_epoch_alone_leaves_ids_random() {
    let dir = std::env::temp_dir().join(format!("kition-cad-epoch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let epoch = [("SOURCE_DATE_EPOCH", "1794182400")];
    let a = build_in_a_process(&dir.join("a.dxf"), &epoch);
    let b = build_in_a_process(&dir.join("b.dxf"), &epoch);
    assert!(a != b, "SOURCE_DATE_EPOCH made two processes save the same bytes");
    assert!(handles(&a) != handles(&b), "SOURCE_DATE_EPOCH made two processes mint the same handles");
    std::fs::remove_dir_all(&dir).ok();
}
