//! `kition-cad` opens a sync room's state with no file, as a server's headless kit pool does
//! (`model.open`, `model.import`, `model.export`, `model.save`; kition-sdk `docs/platform/reference/object-model.md`).

use std::sync::Arc;

use kition_cad_kit::CadDocument;
use kition_headless::{Headless, KitInfo};
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Origin, Replica, Schema};
use kition_kit_server::{ErrorReason, KitBackend};
use serde_json::{Value, json};

fn kit() -> Headless<CadDocument> {
    Headless::with_replica_port(KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_cad_kit::EDITOR_STATE).unwrap())
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for k in 0..4 {
            out.push(if k <= chunk.len() { char::from(BASE64[((n >> (18 - 6 * k)) & 63) as usize]) } else { '=' });
        }
    }
    out
}

fn decode(text: &str) -> Vec<u8> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = BASE64.iter().position(|known| *known == byte).expect("base64");
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

fn entities(doc: &CadDocument) -> u64 {
    doc.inspect().unwrap()["entities"].as_u64().unwrap()
}

/// The room's drawing, as a collaborator holds it: one line.
fn peer() -> CadDocument {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    doc.execute("line.add", json!({ "points": [[10, 10], [100, 60]] })).unwrap();
    doc.version().unwrap();
    doc
}

fn open(kit: &mut Headless<CadDocument>, snapshot: &[u8]) -> String {
    let opened = kit.model("model.open", json!({ "name": "Plan.dxf", "snapshot": encode(snapshot) })).unwrap();
    assert_eq!(opened["format"], "cad");
    opened["documentId"].as_str().unwrap().to_owned()
}

#[test]
fn opens_a_room_without_a_file_and_imports_exports_and_saves_it() {
    let mut kit = kit();
    let features = kit.capabilities()["features"].clone();
    for feature in ["model.open", "model.import", "model.export", "model.save"] {
        assert!(features.as_array().unwrap().iter().any(|listed| listed == feature), "{feature} in {features}");
    }

    let mut peer = peer();
    let id = open(&mut kit, &peer.snapshot().unwrap());

    // A collaborator's change comes in.
    let since = peer.version().unwrap();
    peer.execute("circle.add", json!({ "center": [200, 100], "radius": 20 })).unwrap();
    let change = peer.export_changes(&since).unwrap();
    kit.model("model.import", json!({ "documentId": id, "changes": encode(&change) })).unwrap();

    // The kit edits on top of it, and the collaborator takes what it exports.
    let batch = json!({ "documentId": id, "ops": [{ "op": "command", "id": "line.add", "params": { "points": [[0, 0], [50, 0]] } }] });
    kit.model("model.batch", batch).unwrap();
    let exported = kit.model("model.export", json!({ "documentId": id, "since": encode(&peer.version().unwrap()) })).unwrap();
    peer.import_changes(&decode(exported["changes"].as_str().unwrap())).unwrap();
    assert_eq!(entities(&peer), 3);
    let everything = kit.model("model.export", json!({ "documentId": id })).unwrap();
    let mut joined = CadDocument::open_with_replica("Plan.dxf", &[], &decode(everything["changes"].as_str().unwrap()), &[]).unwrap();
    assert_eq!(joined.version().unwrap(), peer.version().unwrap(), "every change the document has");

    let saved = kit.model("model.save", json!({ "documentId": id })).unwrap();
    assert_eq!((saved["name"].as_str(), saved["format"].as_str()), (Some("Plan.dxf"), Some("cad")));
    let file = CadDocument::open("Plan.dxf", &decode(saved["data"].as_str().unwrap())).unwrap();
    assert_eq!(entities(&file), 3);
}

#[test]
fn never_opens_an_empty_drawing_for_a_replica_it_cannot_read() {
    let mut kit = kit();
    for snapshot in [&b"not a replica"[..], &b""[..]] {
        let error = kit.model("model.open", json!({ "name": "Plan.dxf", "snapshot": encode(snapshot) })).unwrap_err();
        assert_eq!(error.code, ErrorReason::UnsupportedFormat.code(), "{error:?}");
    }
    let snapshot = peer().snapshot().unwrap();
    let error = kit.model("model.open", json!({ "name": "Plan.dxf", "snapshot": encode(&snapshot), "blobs": encode(&[1, 2, 3]) })).unwrap_err();
    assert_eq!(error.code, ErrorReason::UnsupportedFormat.code(), "damaged blobs: {error:?}");
}

fn schema() -> Arc<Schema> {
    Arc::new(Schema::parse(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap())
}

/// Imports `change` into a freshly opened replica of `snapshot`, which the kit refuses: it answers
/// `data.refused` and closes the document.
fn assert_refused(snapshot: &[u8], change: &[u8]) {
    let mut kit = kit();
    let id = open(&mut kit, snapshot);
    let error = kit.model("model.import", json!({ "documentId": id, "changes": encode(change) })).unwrap_err();
    assert_eq!(error.code, ErrorReason::InvalidParams.code(), "{error:?}");
    assert_eq!(error.data.as_ref().unwrap()["refused"], true);
    let closed = kit.model("model.export", json!({ "documentId": id })).unwrap_err();
    assert_eq!(closed.code, ErrorReason::DocumentNotFound.code());
}

/// A change that deletes the drawing itself: the merge repair has nothing to repair it from (it
/// makes no drawing up), so the change is refused.
#[test]
fn refuses_a_change_it_cannot_repair_and_closes_the_document() {
    let snapshot = peer().snapshot().unwrap();
    let mut other = Replica::load(schema(), &snapshot, ActorId::from_u64(77).unwrap()).unwrap();
    let since = other.version();
    other
        .transact(&Origin::person("user:b"), |tx| {
            let drawing = tx.objects_of_type("Drawing").into_iter().next().ok_or_else(|| kition_kit::kdm::KdmError::Engine("no drawing".into()))?;
            tx.delete(&drawing)
        })
        .unwrap();
    assert_refused(&snapshot, &other.export_changes(&since).unwrap());
}

/// Changes from a newer build, whose schema this kit does not read, are refused too.
#[test]
fn refuses_changes_needing_a_newer_schema() {
    let snapshot = peer().snapshot().unwrap();
    let mut raw: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    let newer = raw["schemaVersion"].as_u64().unwrap() + 1;
    raw["schemaVersion"] = json!(newer);
    let mut other = Replica::load(Arc::new(Schema::parse(&raw.to_string()).unwrap()), &snapshot, ActorId::from_u64(77).unwrap()).unwrap();
    let since = other.version();
    other.transact(&Origin::system("migrate"), |tx| tx.raise_schema_version(u32::try_from(newer).unwrap()).map(|_| ())).unwrap();
    assert_refused(&snapshot, &other.export_changes(&since).unwrap());
}
