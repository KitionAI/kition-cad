//! A command that fails changes nothing (task card C4/C5/C6-cad): `Session::execute` puts the
//! drawing and the selection back when a command answers an error, as it does when one panics,
//! records no undo step and moves no revision. Before, `properties.set` given a new layer and an
//! unknown dimension style made the layer, then failed on the style, and the layer stayed (with an
//! undo step for a command that reported failure).
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_cad_engine::{Session, UndoScope};
use serde_json::json;

#[test]
fn issue_partial_change_a_failing_command_leaves_the_drawing_as_it_was() {
    let mut s = Session::new();
    let line = s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    let handle = line["handle"].as_str().or_else(|| line["handles"][0].as_str()).unwrap().to_owned();
    s.execute("select", &json!({ "handles": [handle] })).unwrap();
    let (revision, layers, undo) = (s.state().unwrap().revision, s.doc().unwrap().layers.len(), s.state().unwrap().undo.len());
    let doc = s.doc().unwrap().clone();
    let error = s.execute("properties.set", &json!({ "handles": [handle], "layer": "Fresh", "dimStyle": "Nope" })).unwrap_err();
    assert!(error.to_string().contains("Nope"), "{error}");
    assert!(s.doc().unwrap().layer("Fresh").is_none(), "the failed command left its new layer");
    assert_eq!(s.doc().unwrap().layers.len(), layers);
    assert_eq!(*s.doc().unwrap(), doc, "the drawing is as it was");
    assert_eq!((s.state().unwrap().revision, s.state().unwrap().undo.len()), (revision, undo), "no revision, no undo step");
    assert!(!s.can_redo_in(UndoScope::Person));
    assert_eq!(s.selection().len(), 1, "the selection is as it was");
}
