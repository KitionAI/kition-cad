//! C8-cad-pin: the public SDK keeps editor state and undo when a clean file is reread.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{KitDocument, kdm::Origin};
use serde_json::json;

const FILE: &str = "Reread.dxf";

#[test]
fn a_fresh_document_restores_the_previous_editors_selection_and_space() {
    let mut live = CadDocument::create(FILE, 420, 297).unwrap();
    let h = live.execute("line", json!({"points":[[0,0],[10,0]],"layout":"Layout1"})).unwrap()["handles"][0].clone();
    live.execute("layout.set", json!({"name":"Layout1"})).unwrap();
    live.execute("select", json!({"handles":[h]})).unwrap();
    let mut fresh = CadDocument::open(FILE, &live.save(Some(FILE)).unwrap()).unwrap();
    fresh.carry_replica_objects(&live).unwrap();
    fresh.restore_editor_state(&live.editor_state()).unwrap();
    assert_eq!(fresh.editor_state(), live.editor_state());
}

#[test]
fn a_reread_refuses_a_deleted_active_selection() {
    let mut live = CadDocument::create(FILE, 420, 297).unwrap();
    let h = live.execute("line", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    live.execute("select", json!({"handles":[h]})).unwrap();
    let saved = live.editor_state();
    let mut fresh = CadDocument::open(FILE, &live.save(Some(FILE)).unwrap()).unwrap();
    fresh.execute("erase", json!({"handles":[h]})).unwrap();
    let error = fresh.restore_editor_state(&saved).unwrap_err();
    assert!(error.contains("active selection"), "{error}");
    assert_eq!(saved, live.editor_state());
}

#[test]
fn an_outside_save_preserves_agent_undo_and_the_colleagues_edit() {
    let mut live = CadDocument::create(FILE, 420, 297).unwrap();
    let h = live.execute("line", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    live.execute_as("move", json!({"handles":[h],"delta":[3,0]}), &Origin::agent("me")).unwrap();
    live.mark_saved();
    let mut fresh = CadDocument::open(FILE, &live.save(Some(FILE)).unwrap()).unwrap();
    let other = fresh.execute("circle", json!({"center":[50,50],"radius":5})).unwrap()["handle"].clone();
    assert!(live.reconcile_reread(&fresh).unwrap());
    live.execute_as("edit.undo", json!({}), &Origin::agent("me")).unwrap();
    let d = kition_cad_io::read(&live.save(Some(FILE)).unwrap(), FILE).unwrap();
    let line = d.entity(kition_cad_doc::Handle::parse_hex(h.as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&line.kind).unwrap()["a"]["x"], 0.0);
    assert!(d.entity(kition_cad_doc::Handle::parse_hex(other.as_str().unwrap()).unwrap()).is_some());
}

#[test]
fn a_premise_fork_keeps_an_existing_clipboard_without_editing_the_live_document() {
    let mut live = CadDocument::create(FILE, 420, 297).unwrap();
    let h = live.execute("line", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    live.execute("copybase", json!({"handles":[h],"base":[2,0]})).unwrap();
    let before = live.save(Some(FILE)).unwrap();
    let mut planned = live.fork_for_premises().unwrap();
    let result = planned.execute("pasteblock", json!({"at":[50,50]})).unwrap();
    assert!(result["handle"].is_string(), "{result}");
    assert_ne!(before, planned.save(Some(FILE)).unwrap());
    assert_eq!(before, live.save(Some(FILE)).unwrap());
}
