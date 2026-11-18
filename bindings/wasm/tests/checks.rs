//! `cad_check` against its catalogue (`agent/checks.json`, KHP A38): the catalogue file is the
//! check's own (`check::CATALOGUE`), errors first, one sentence each; every fixture in
//! `conformance/` reports its code and only catalogued codes, and every code has one; each
//! diagnostic's `fix` clears it; `basic.dxf` is clean; and checking never changes the drawing. The
//! fixtures are written by the crate's `conformance` test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_cad_kit::CadDocument;
use kition_cad_kit::check::{CATALOGUE, catalogue_json};
use kition_kit::KitDocument;
use kition_kit::diagnostics::{Severity, check_diagnostics};
use serde_json::Value;

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn open(name: &str) -> CadDocument {
    let bytes = std::fs::read(repo().join("conformance").join(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    CadDocument::open(name, &bytes).unwrap()
}

#[test]
fn the_catalogue_file_is_the_checks_own() {
    let file: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    assert_eq!(file, catalogue_json(), "regenerate with KITION_WRITE_CONFORMANCE=1 cargo test -p kition-cad-kit conformance");
    let mut previous = Severity::Error;
    for entry in CATALOGUE {
        assert!(kition_kit::diagnostics::valid_code(entry.code) && entry.code.starts_with("cad."), "{}", entry.code);
        assert!(entry.severity >= previous, "errors first: {}", entry.code);
        previous = entry.severity;
        assert!(entry.summary.ends_with('.') && !entry.summary.contains('\n'), "{}: one sentence", entry.code);
    }
    let without: Vec<&str> = file.as_array().unwrap().iter().filter(|e| e.get("fixture").is_none()).map(|e| e["code"].as_str().unwrap()).collect();
    assert_eq!(without, Vec::<&str>::new(), "every code has a fixture");
}

#[test]
fn every_fixture_reports_its_code_and_only_catalogued_codes() {
    let known: Vec<&str> = CATALOGUE.iter().map(|entry| entry.code).collect();
    for entry in CATALOGUE {
        let name = format!("check-{}.dxf", entry.code.trim_start_matches("cad."));
        let found = open(&name).check();
        check_diagnostics(&found).unwrap();
        assert!(found.iter().any(|diagnostic| diagnostic.code == entry.code), "{name}: {found:?}");
        for diagnostic in &found {
            assert!(known.contains(&diagnostic.code.as_str()), "{name}: {} is not catalogued", diagnostic.code);
        }
    }
}

#[test]
fn basic_is_clean_and_checking_changes_nothing() {
    let drawing = open("basic.dxf");
    let before = drawing.inspect().unwrap();
    assert_eq!(drawing.check(), Vec::new());
    assert_eq!(drawing.inspect().unwrap(), before);
    assert!(!drawing.dirty());
}

#[test]
fn every_fix_clears_its_code() {
    for entry in CATALOGUE.iter().filter(|entry| entry.code != "cad.truncated") {
        let name = format!("check-{}.dxf", entry.code.trim_start_matches("cad."));
        let mut drawing = open(&name);
        let found = drawing.check();
        let Some(fix) = found.iter().filter(|d| d.code == entry.code).find_map(|d| d.fix.clone()) else {
            assert!(entry.severity != Severity::Error, "{name}: an error carries a fix");
            continue;
        };
        let before = found.iter().filter(|d| d.code == entry.code).count();
        drawing.execute(&fix.command, fix.params.clone().unwrap_or(Value::Null)).unwrap_or_else(|error| panic!("{name}: {} {error}", fix.command));
        let after = drawing.check().iter().filter(|d| d.code == entry.code).count();
        assert!(after < before, "{name}: `{}` leaves {after} of {before} `{}`", fix.command, entry.code);
    }
}
