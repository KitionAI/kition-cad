//! `document.identity` and `document.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-cad` process: a `.dxf` is the drawing alone (`kition_cad_io::write`; the
//! kit's own xdata and XRECORDs hold associativity, constraints and record ids, never a replica)
//! and embeds no replica; the kit makes one under a new id each time it opens a file
//! (`CadModel::import`). So a file carries no identity, and a fork is the same bytes with no id,
//! written to a new file only.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kit_host::{HeadlessKit, LaunchOptions};

const LIMIT: Duration = Duration::from_secs(30);

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-cad-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-cad"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// A saved drawing: a new one, as a person's first save writes it.
fn drawing() -> Vec<u8> {
    CadDocument::create("Plan.dxf", 1920, 1080).unwrap().save(Some("Plan.dxf")).unwrap()
}

#[tokio::test]
async fn a_drawing_carries_no_identity_and_forks_into_the_same_bytes() {
    let dir = workspace("plain");
    let bytes = drawing();
    std::fs::write(dir.join("Plan.dxf"), &bytes).unwrap();
    // An empty `.dxf` (a file made for the kit before it saved a drawing there).
    std::fs::write(dir.join("New.dxf"), b"").unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("document.identity"), "{}", kit.capabilities());

    // No file carries an identity, a byte copy neither.
    std::fs::copy(dir.join("Plan.dxf"), dir.join("Copy.dxf")).unwrap();
    for path in ["Plan.dxf", "Copy.dxf", "New.dxf"] {
        assert_eq!(kit.identity(path, LIMIT).await.unwrap().doc_id, None, "{path} embeds no replica");
    }

    // A fork is the same bytes, with no id, in a new file that opens as the drawing.
    let fork = kit.fork("Plan.dxf", "Fork.dxf", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(fork.name, "Fork.dxf");
    assert!(!fork.version.is_empty());
    assert_eq!(std::fs::read(dir.join("Fork.dxf")).unwrap(), bytes, "a file without a replica forks unchanged");
    assert_eq!(std::fs::read(dir.join("Plan.dxf")).unwrap(), bytes, "the source is unchanged");
    let state = kit.open("Fork.dxf", true, LIMIT).await.unwrap();
    assert!(state["documentId"].is_string(), "{state}");
    let empty = kit.fork("New.dxf", "NewCopy.dxf", LIMIT).await.unwrap();
    assert_eq!(empty.doc_id, None);
    assert_eq!(std::fs::read(dir.join("NewCopy.dxf")).unwrap(), b"");

    // A fork never replaces a file.
    let taken = kit.fork("Copy.dxf", "Fork.dxf", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Fork.dxf")).unwrap(), bytes, "the first fork is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
