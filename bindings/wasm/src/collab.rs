//! Kition's collaboration members (engine contract 2, Kition phase 3g): the drawing as a KDM
//! replica.
//!
//! The engine keeps its own `Session` (the command bridge to the replica,
//! `kition_cad_engine::kdm_bridge`), not a `kition_kit::session::KitSession` directly, so
//! `kition_kit::export_kit_session!` does not apply here; these members are added by hand with
//! the exact JS names and shapes `kition-sdk`'s `session::web` gives
//! (`crates/kition-kit/src/session/web.rs`), as Kition Slide's are, so Kition's host code is the
//! same for every kit. The wasm bindings live in [`crate::web`].
//!
//! A drawing has no blobs (raster images are referenced by path): [`CadDocument::blobs`] and
//! [`CadDocument::blobs_since`] answer an empty bundle, and [`CadDocument::add_blobs`] checks a
//! bundle (`(64-char ASCII-hex sha256, u64 LE length, bytes) * N`, Kition Photo's encoding) and
//! keeps nothing.

use kition_cad_engine::{Damage, Origin, Session, Version};
use kition_kit::KitDocument;
use serde_json::{Value, json};

use crate::{CadDocument, engine_error};

impl CadDocument {
    /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
    /// capability: DXF cannot carry its own replica): the drawing is the replica's, so it keeps
    /// its KDM identity across saves. The host vouches that the replica was saved with these
    /// bytes; when they disagree (an older save, a drawing edited outside the replica) the engine
    /// records the file on top as one import. A replica that does not open (damaged, a newer
    /// schema) is ignored and the file opens as [`CadDocument::open`] opens it.
    ///
    /// Empty `bytes` mean there is no file yet, only the replica (a host opening a shared drawing
    /// for the first time from its sync room's snapshot, to save it as a new file): the drawing
    /// is the replica's alone.
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        let restored = (|| -> Result<Session, String> {
            check_blobs(blobs)?;
            let mut session = Session::empty();
            session.open_with_replica(bytes, snapshot).map_err(engine_error)?;
            // The drawing is the active one, as after `open`, and is saved to `name`.
            let state = session.state_mut().map_err(engine_error)?;
            state.path = Some(name.to_owned());
            state.title = std::path::Path::new(name).file_stem().map_or_else(|| "Drawing".to_owned(), |stem| stem.to_string_lossy().into_owned());
            Ok(session)
        })();
        match restored {
            Ok(session) => Ok(Self::from_session(session)),
            Err(_) => Self::open(name, bytes),
        }
    }

    /// Whether the drawing has made its replica yet: it makes one lazily, on the first shared
    /// edit or when asked for it ([`CadDocument::version`] and the rest).
    pub fn has_replica(&self) -> bool {
        self.session.state().is_ok_and(|state| state.kdm().is_some())
    }

    /// The replica's own document id (a `kition-sdk` `DocId`, stable across saves and across
    /// opening another replica of the same document — never the DXF file's path or name): Kit
    /// UI's generic runtime reads this for the Kit UI protocol's `replica/info` message. `None`
    /// (`undefined` in JS) before the drawing has made a replica.
    pub fn doc_id(&self) -> Option<String> {
        self.session.doc_id()
    }

    /// The drawing's replica version.
    pub fn version(&mut self) -> Result<Vec<u8>, String> {
        Ok(self.session.version().map_err(engine_error)?.encode())
    }

    /// The drawing's changes since `since` (empty bytes: every change).
    pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        let changes = self.session.export_changes(&since).map_err(engine_error)?;
        if let Some(session) = self.session.kdm() {
            *self.batch_entities.lock().map_err(|_| "entity inventory unavailable")? =
                session.replica().objects_of_type("Entity").into_iter().collect();
        }
        Ok(changes)
    }

    /// Imports a peer's (or stored) changes. Answers what went stale on screen.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        self.session.import_changes(bytes).map_err(engine_error)
    }

    /// The drawing's replica as a snapshot, for a peer to open as a new replica of the same
    /// drawing ([`CadDocument::open`]'ing the saved DXF does not: DXF carries no replica).
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        self.session.snapshot().map_err(engine_error)
    }

    /// Undoes the person's last step on the drawing's replica. `done` is `false`, not an error,
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_undo();
        if done {
            self.execute("edit.undo", json!({ "scope": "person" }))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn redo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_redo();
        if done {
            self.execute("edit.redo", json!({ "scope": "person" }))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    /// Whether the person has a step to undo (engine rule E14a: the person's).
    pub fn can_undo(&self) -> bool {
        self.session.can_undo_in(kition_cad_engine::UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.session.can_redo_in(kition_cad_engine::UndoScope::Person)
    }

    /// Runs a command as `origin` (a person, an agent): the replica records it as theirs, and it
    /// undoes as that replica's own step. Contract 1's
    /// [`CadDocument::execute`](kition_kit::KitDocument::execute) is this with the local person.
    pub fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        let previous = self.session.set_origin(Some(origin.clone()));
        let result = self.execute(command, params);
        self.session.set_origin(previous);
        result
    }

    /// Every blob the drawing's replica references: none.
    pub fn blobs(&mut self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }

    /// Takes a peer's blobs (from their [`CadDocument::blobs`]): a drawing references none, so
    /// the bundle is only checked.
    pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), String> {
        check_blobs(bytes)
    }

    /// The blobs the drawing's replica has newly needed since `since`: none.
    pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        if !since.is_empty() {
            Version::decode(since).map_err(engine_error)?;
        }
        Ok(Vec::new())
    }

    /// How many times the drawing left its replica because it refused a write or an undo (a
    /// drawing it cannot hold, or a bug). The drawing keeps working, undoing from its own
    /// history; the next shared edit makes a fresh replica, which collaborators must resync (its
    /// full [`CadDocument::snapshot`]). Hosts watch this.
    pub fn kdm_resets(&self) -> u32 {
        u32::try_from(self.session.kdm_resets()).unwrap_or(u32::MAX)
    }

    /// The replicated state, the replica made first if there is none yet.
    fn replicated_state(&mut self) -> Result<Value, String> {
        self.session.version().map_err(engine_error)?;
        let kdm = self.session.kdm().ok_or("the drawing has no replica")?;
        kdm.replica().state_json().map_err(engine_error)
    }

    /// Takes a draft's changes (from a replica forked from this one) in as one undo step of the
    /// person who accepts it, whoever made the draft (`mergeChanges`, as `kition-sdk`'s
    /// `export_kit_session!` answers it; KitModel M12a): the engine's drawing follows, and one
    /// Undo of the person takes the whole draft back. Answers `{"damage",
    /// "summary"}`, `summary` the `StateDiff` of the replicated state before and after. A refused
    /// merge changes nothing and fails with the KDM's message.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Value, String> {
        let before = self.replicated_state()?;
        let damage = self.session.merge_changes(bytes, origin).map_err(engine_error)?;
        let after = self.replicated_state()?;
        Ok(json!({ "damage": damage, "summary": kition_kit::kdm::diff::diff_states(&before, &after).to_json() }))
    }

    /// What merging a draft's changes would change in the drawing now, as `StateDiff` JSON;
    /// nothing changes. Refuses what [`CadDocument::merge_changes`] refuses.
    pub fn compare_changes(&self, bytes: &[u8]) -> Result<Value, String> {
        let kdm = self.session.kdm().ok_or("the drawing has no replica, so no draft of it")?;
        let before = kdm.replica().state_json().map_err(engine_error)?;
        let after = kdm.replica().merged_state(bytes).map_err(engine_error)?;
        Ok(kition_kit::kdm::diff::diff_states(&before, &after).to_json())
    }

    /// Writes the state a snapshot holds (a checkpoint of this drawing, the Kit UI protocol's
    /// `replica/restore`) over the drawing by object id, as one undo step of `origin`'s scope: the
    /// engine's drawing follows. Nothing changes when the states are equal; another document's
    /// snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        self.session.restore_state(snapshot, origin).map_err(engine_error)
    }

    /// An agent's reply (JSON text) with every object id it holds as a value or a key named as
    /// the tool interface names it (its tool id, KitObjectModel OM9); unchanged without a replica,
    /// when it names no object of the drawing, or when the schema declares no tool ids. The
    /// drawing keeps its own ids.
    ///
    /// The editor sends every `ai/inspect`, `document/check` and `ai/run` reply through this: the
    /// tool ids are read for the objects the reply names alone (`Replica::tool_ids_of`), not for
    /// every entity of the drawing each time.
    pub fn model_reply(&self, value: &str) -> Result<String, String> {
        let value: Value = serde_json::from_str(value).map_err(|error| format!("the reply is not JSON: {error}"))?;
        let Some(kdm) = self.session.kdm() else { return Ok(value.to_string()) };
        let replica = kdm.replica();
        let mut texts = std::collections::BTreeSet::new();
        texts_of(&value, &mut texts);
        let named: std::collections::BTreeSet<kition_kit::kdm::ObjectId> =
            texts.into_iter().map(kition_kit::kdm::ObjectId::from_string).filter(|id| replica.exists(id)).collect();
        if named.is_empty() {
            return Ok(value.to_string());
        }
        let ids = replica.tool_ids_of(&named);
        Ok(if ids.is_active() { ids.show_json(&value) } else { value }.to_string())
    }

    /// What changed since the version `since` (empty bytes: since the replica began), a receipt as
    /// `kition-sdk`'s `changesSince` answers it: `{"changes": StateDiff}`, or `{"changes": null,
    /// "unavailable": "<why>"}` when the replica's log does not hold `since` (or there is no
    /// replica yet), never an empty change set in its place.
    pub fn changes_since(&self, since: &[u8]) -> Value {
        let Some(kdm) = self.session.kdm() else { return json!({ "changes": null, "unavailable": "the drawing has no replica yet" }) };
        let replica = kdm.replica();
        let since = if since.is_empty() { Ok(Version::empty()) } else { Version::decode(since) };
        match since.and_then(|since| replica.changes_since(&since)) {
            Ok(changes) => json!({ "changes": changes.to_recorded_json(replica.schema()) }),
            Err(error) => json!({ "changes": null, "unavailable": error.to_string() }),
        }
    }

    /// `model/describe` (KitObjectModel 1.0) over the drawing's change exchange, as `kition-cad`
    /// serves it headless ([`kition_kit::session::model::Exchange`]); a failure is the error's
    /// JSON (`{"code", "index", "message"}`).
    pub fn model_describe(&mut self) -> Result<Value, String> {
        kition_kit::session::model::Exchange::new().describe(self).map_err(|failure| failure.to_json().to_string())
    }

    /// `model/batch`: the request as JSON text, one undo step; a failure is the error's JSON
    /// (`{"code", "index", "message"}`).
    pub fn model_batch(&mut self, request: &str) -> Result<Value, String> {
        let request: Value = serde_json::from_str(request)
            .map_err(|failure| json!({ "code": "invalid-op", "index": null, "message": format!("the batch is not JSON: {failure}") }).to_string())?;
        kition_kit::session::model::Exchange::new().batch(self, &request, false).map_err(|failure| failure.to_json().to_string())
    }
}

/// Every string and key `value` holds, and each `:`-separated segment of them (a map key made of
/// object ids, such as a table cell's `<entity>:<cell>`): the texts that may name an object.
fn texts_of(value: &Value, out: &mut std::collections::BTreeSet<String>) {
    fn add(text: &str, out: &mut std::collections::BTreeSet<String>) {
        out.insert(text.to_owned());
        if text.contains(':') {
            out.extend(text.split(':').filter(|part| !part.is_empty()).map(str::to_owned));
        }
    }
    match value {
        Value::String(text) => add(text, out),
        Value::Array(items) => items.iter().for_each(|item| texts_of(item, out)),
        Value::Object(map) => {
            for (key, item) in map {
                add(key, out);
                texts_of(item, out);
            }
        }
        _ => {}
    }
}

/// Checks a blob bundle (`(64-byte ASCII-hex hash, u64 LE length, bytes) * N`): truncated or
/// malformed input is an error, never a panic.
fn check_blobs(bytes: &[u8]) -> Result<(), String> {
    let mut i = 0usize;
    while i < bytes.len() {
        let hash_end = i.checked_add(64).ok_or("blob bundle: overflow")?;
        let hash = bytes.get(i..hash_end).ok_or("truncated blob bundle: hash")?;
        if !hash.is_ascii() {
            return Err("blob bundle: hash is not ASCII".to_owned());
        }
        let len_end = hash_end.checked_add(8).ok_or("blob bundle: overflow")?;
        let len_bytes: [u8; 8] =
            bytes.get(hash_end..len_end).ok_or("truncated blob bundle: length")?.try_into().map_err(|_| "blob bundle: length".to_owned())?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob bundle: length too large".to_owned())?;
        let data_end = len_end.checked_add(len).ok_or("blob bundle: overflow")?;
        bytes.get(len_end..data_end).ok_or("truncated blob bundle: data")?;
        i = data_end;
    }
    Ok(())
}

/// The object model (`KitObjectModel 1.0`) over the document's change exchange: `kition-cad` serves
/// `model.describe` and `model.batch` headless through `kition_kit::session::model::Exchange`.
impl kition_kit::session::model::ReplicaPort for CadDocument {
    fn model_schema(&self) -> std::sync::Arc<kition_kit::kdm::Schema> {
        <kdm_model::CadModel as kition_kit::session::KitModel>::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.export_changes(since)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.import_changes(bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<kition_kit::KitCommand> {
        self.commands()
    }

    /// A shared drawing with no file yet (a server's headless kit opening a sync room): the
    /// replica alone, failing rather than falling back to an empty drawing.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        check_blobs(blobs)?;
        let mut session = Session::empty();
        session.open_with_replica(&[], snapshot).map_err(engine_error)?;
        let state = session.state_mut().map_err(engine_error)?;
        state.path = Some(name.to_owned());
        state.title = std::path::Path::new(name).file_stem().map_or_else(|| "Drawing".to_owned(), |stem| stem.to_string_lossy().into_owned());
        Ok(Self::from_session(session))
    }
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use serde_json::json;

    use super::*;

    fn fixture() -> Vec<u8> {
        let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
        doc.execute("line.add", json!({ "points": [[10, 10], [100, 60]] })).unwrap();
        doc.save(Some("Plan.dxf")).unwrap()
    }

    fn extra_edit(doc: &mut CadDocument) {
        doc.execute("circle.add", json!({ "center": [200, 100], "radius": 20 })).unwrap();
        doc.execute("line.add", json!({ "points": [[0, 0], [50, 0]] })).unwrap();
    }

    fn entities(doc: &CadDocument) -> u64 {
        doc.inspect().unwrap()["entities"].as_u64().unwrap()
    }

    #[test]
    fn version_and_snapshot_force_a_replica_lazily() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        assert!(!doc.has_replica(), "opening a DXF plainly makes no replica");
        let version = doc.version().unwrap();
        assert!(doc.has_replica());
        assert_eq!(doc.version().unwrap(), version, "asking twice makes no further replica");
        assert!(!doc.snapshot().unwrap().is_empty());
        assert!(!doc.dirty(), "making the replica leaves the drawing clean");
    }

    #[test]
    fn undo_redo_report_done_and_damage() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        assert!(!doc.can_undo());
        extra_edit(&mut doc);
        assert!(doc.can_undo());
        let (done, damage) = doc.undo().unwrap();
        assert!(done);
        assert_ne!(damage, Damage::None);
        assert!(doc.can_redo());
        let (done, _) = doc.redo().unwrap();
        assert!(done);
        doc.undo().unwrap();
        doc.undo().unwrap();
        let (done, damage) = doc.undo().unwrap();
        assert!(!done, "nothing more to undo is a normal false, not an error");
        assert_eq!(damage, Damage::None);
    }

    #[test]
    fn execute_as_records_the_given_origin() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let origin = Origin::person("user:agent-1");
        doc.execute_as("circle.add", json!({ "center": [5, 5], "radius": 2 }), &origin).unwrap();
        assert!(doc.can_undo());
        assert_eq!(doc.session.origin(), kition_cad_engine::kdm_bridge::local_person(), "the origin is put back");
    }

    /// `docId` is `None` before any replica exists, and the replica's own id (not the file's
    /// `name`, and stable across a save/reopen with that same replica) once one does.
    #[test]
    fn doc_id_is_none_until_a_replica_exists_then_the_replicas_own_id() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        assert_eq!(doc.doc_id(), None, "no replica yet");
        doc.version().unwrap();
        let id = doc.doc_id().unwrap_or_else(|| panic!("doc_id once a replica exists"));
        assert!(!id.is_empty());
        let bytes = doc.save(Some("Plan.dxf")).unwrap();
        let snapshot = doc.snapshot().unwrap();
        let reopened = CadDocument::open_with_replica("Plan.dxf", &bytes, &snapshot, &[]).unwrap();
        assert_eq!(reopened.doc_id(), Some(id), "the reopened replica keeps the same document id");
    }

    /// With no file yet (empty bytes), the replica alone opens: a peer's full change export
    /// becomes the same drawing, same identity, and it saves as DXF.
    #[test]
    fn a_replica_opens_without_a_file() {
        let mut a = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        extra_edit(&mut a);
        let changes = a.export_changes(&[]).unwrap();
        let mut b = CadDocument::open_with_replica("Shared.dxf", &[], &changes, &a.blobs().unwrap()).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(b.version().unwrap(), a.version().unwrap());
        assert!(!b.dirty());
        assert_eq!(entities(&b), entities(&a));
        assert!(CadDocument::open("Shared.dxf", &b.save(Some("Shared.dxf")).unwrap()).is_ok());
        assert!(CadDocument::open_with_replica("Shared.dxf", &[], b"not a replica", &[]).is_err());
    }

    #[test]
    fn blob_bundles_are_checked_without_panicking() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        assert!(doc.blobs().unwrap().is_empty());
        assert!(doc.add_blobs(&[]).is_ok());
        assert!(doc.add_blobs(&[1, 2, 3]).is_err());
        let mut malformed = "a".repeat(64).into_bytes();
        malformed.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(doc.add_blobs(&malformed).is_err(), "a length claiming more data than exists");
        let mut good = "a".repeat(64).into_bytes();
        good.extend_from_slice(&3u64.to_le_bytes());
        good.extend_from_slice(b"abc");
        assert!(doc.add_blobs(&good).is_ok());
    }

    /// A DXF saved with its replica reopens as that replica: same version, same rendered pixels,
    /// not dirty, and it takes new edits (which undo) at once.
    #[test]
    fn a_drawing_reopens_with_its_stored_replica() {
        let mut a = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(Some("Plan.dxf")).unwrap();
        let snapshot = a.snapshot().unwrap();
        let version = a.version().unwrap();

        let mut b = CadDocument::open_with_replica("Plan.dxf", &bytes, &snapshot, &[]).unwrap();
        assert!(b.has_replica());
        assert_eq!(b.version().unwrap(), version);
        assert_eq!(entities(&b), entities(&a));
        assert!(!b.dirty());
        assert!(!b.can_undo());
        extra_edit(&mut b);
        assert!(b.dirty());
        assert_ne!(b.version().unwrap(), version);
        assert!(b.undo().unwrap().0);
        assert!(CadDocument::open("Plan.dxf", &b.save(Some("Plan.dxf")).unwrap()).is_ok(), "it still saves as DXF");
    }

    /// The sorted object ids of the drawing's entities in its replica.
    fn entity_ids(doc: &CadDocument) -> Vec<String> {
        let kdm = doc.session.kdm().unwrap_or_else(|| panic!("the drawing has a replica"));
        let mut ids: Vec<String> = kdm.replica().objects_of_type("Entity").iter().map(|id| id.as_str().to_owned()).collect();
        ids.sort();
        ids
    }

    /// An unchanged drawing saves the same bytes each time, and reopened with its stored replica
    /// its entities keep their object ids; the reopened drawing, unchanged, saves the same bytes
    /// again and again too.
    #[test]
    fn an_unchanged_drawing_saves_the_same_bytes_and_reopens_with_its_object_ids() {
        let mut a = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(Some("Plan.dxf")).unwrap();
        assert_eq!(a.save(Some("Plan.dxf")).unwrap(), bytes, "saving again unchanged writes the same bytes");
        let snapshot = a.snapshot().unwrap();
        let before = entity_ids(&a);
        assert_eq!(before.len(), 3, "the line and the two edits' entities");

        let b = CadDocument::open_with_replica("Plan.dxf", &bytes, &snapshot, &[]).unwrap();
        assert_eq!(entity_ids(&b), before, "the reopened drawing's entities keep their object ids");
        assert!(!b.dirty());
        let saved = b.save(Some("Plan.dxf")).unwrap();
        assert_eq!(b.save(Some("Plan.dxf")).unwrap(), saved, "the reopened drawing saves the same bytes each time");
        assert_eq!(entities(&CadDocument::open("Plan.dxf", &saved).unwrap()), entities(&a));
    }

    /// A draft (a replica forked from the drawing's) merges in as one step of the person's Undo:
    /// the engine's drawing follows the merge, an edit made here meanwhile stays, and a refused
    /// merge changes nothing.
    #[test]
    fn a_draft_merges_in_as_one_undo_step_and_the_drawing_follows() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = CadDocument::open_with_replica("Plan.dxf", &[], &doc.snapshot().unwrap(), &[]).unwrap();
        draft.execute("circle.add", json!({ "center": [300, 200], "radius": 10 })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        // Meanwhile, here, an edit of another field (a merge refuses a field both sides changed).
        doc.execute("variable.set", json!({ "name": "DIMSCALE", "value": 3 })).unwrap();
        let before = entities(&doc);
        let origin = Origin::person("person:local");
        let answer = doc.merge_changes(&changes, &origin).unwrap();
        assert_ne!(answer["damage"], json!("none"), "{answer}");
        assert!(answer["summary"].is_object(), "{answer}");
        assert_eq!(entities(&doc), before + 1, "the engine's drawing follows the merge");
        assert!(doc.dirty());
        assert_eq!(doc.session.doc().unwrap().header.f64("DIMSCALE", 0.0), 3.0, "the edit made here stays");
        assert!(doc.undo().unwrap().0);
        assert_eq!(entities(&doc), before, "one undo takes the whole draft back");
        assert_eq!(doc.session.doc().unwrap().header.f64("DIMSCALE", 0.0), 3.0, "and only the draft");
        assert!(doc.redo().unwrap().0);
        assert_eq!(entities(&doc), before + 1, "and one redo brings it again");
        let at = doc.version().unwrap();
        assert!(doc.merge_changes(b"not changes", &origin).is_err());
        assert_eq!(doc.version().unwrap(), at, "a refused merge changes nothing");
    }

    /// The type names of the drawing's model-space entities, sorted.
    fn model_types(doc: &CadDocument) -> Vec<&'static str> {
        let mut types: Vec<&'static str> = doc.session.doc().unwrap().model.iter().map(|entity| entity.kind.type_name()).collect();
        types.sort();
        types
    }

    /// A draft that draws an entity merges into a drawing that drew another meanwhile: both
    /// append to the one model-space list (`Drawing.model`), which merges by item (data model 6.6,
    /// D5), so both entities are there, and one Undo takes the draft's back and leaves the one
    /// drawn here.
    #[test]
    fn a_draft_and_the_drawing_both_drawing_an_entity_merge() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = CadDocument::open_with_replica("Plan.dxf", &[], &doc.snapshot().unwrap(), &[]).unwrap();
        draft.execute("circle.add", json!({ "center": [300, 200], "radius": 10 })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        let start = model_types(&doc);
        doc.execute("line.add", json!({ "points": [[0, 0], [50, 0]] })).unwrap();
        let with_line = model_types(&doc);
        assert_eq!(with_line.len(), start.len() + 1);
        let origin = Origin::person("person:local");
        let answer = doc.merge_changes(&changes, &origin).unwrap();
        assert_ne!(answer["damage"], json!("none"), "{answer}");
        let mut both = with_line.clone();
        both.push("Circle");
        both.sort();
        assert_eq!(model_types(&doc), both, "the draft's circle and the line drawn here are both there");
        assert_eq!(entity_ids(&doc).len(), entity_ids(&draft).len() + 1, "the replica holds both entities");
        assert!(doc.undo().unwrap().0);
        assert_eq!(model_types(&doc), with_line, "one undo takes the draft's circle back and leaves the line");
        assert!(doc.redo().unwrap().0);
        assert_eq!(model_types(&doc), both);
    }

    /// The engine's own view of the drawing, id-blind.
    fn view(doc: &CadDocument) -> Value {
        kdm_model::canonical(doc.session.doc().unwrap())
    }

    /// KitModel M12a: a draft an agent made merges as one undo step of the person who accepts it,
    /// in the replica's history and in the engine's alike. The agent's Undo leaves it; the
    /// person's takes it back to the exact state before the merge (a line drawn here meanwhile,
    /// in the same model-space list, stays), and the person's Redo writes the exact merged state
    /// again. Before, the engine put the step in the agent's history while the replica put it in
    /// the person's.
    #[test]
    fn an_agents_draft_merges_as_one_undo_step_of_the_person() {
        use kition_cad_engine::UndoScope;
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = CadDocument::open_with_replica("Plan.dxf", &[], &doc.snapshot().unwrap(), &[]).unwrap();
        draft.execute("circle.add", json!({ "center": [300, 200], "radius": 10 })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        doc.execute("line.add", json!({ "points": [[0, 0], [50, 0]] })).unwrap();
        let (prior_hash, prior_view) = (doc.session.state_hash().unwrap(), view(&doc));
        assert_eq!(model_types(&doc), ["Line", "Line"]);
        let agent = Origin::agent("agent:drafter");
        doc.merge_changes(&changes, &agent).unwrap();
        assert_eq!(model_types(&doc), ["Circle", "Line", "Line"], "the draft's circle and the line drawn here");
        let (merged_hash, merged_view) = (doc.session.state_hash().unwrap(), view(&doc));
        let st = doc.session.state().unwrap();
        assert_eq!(st.undo.last().map(|step| step.scope), Some(UndoScope::Person), "the engine's history: a step of the person");
        assert_eq!(st.undo.last().map(|step| step.steps), Some(1), "one replica step");
        assert!(!doc.session.can_undo_in(UndoScope::Agent), "the engine's history: no step of the agent");
        let kdm = doc.session.kdm().unwrap();
        assert!(kdm.can_undo_in(UndoScope::Person), "the replica's history: a step of the person");
        assert!(!kdm.can_undo_in(UndoScope::Agent), "the replica's history: no step of the agent");
        let _ = doc.session.undo_in(UndoScope::Agent);
        assert_eq!(doc.session.state_hash().unwrap(), merged_hash, "the agent's undo leaves the draft");
        assert_eq!(view(&doc), merged_view);
        assert!(doc.undo().unwrap().0);
        assert_eq!(doc.session.state_hash().unwrap(), prior_hash, "the person's undo gives back the exact state before the merge");
        assert_eq!(view(&doc), prior_view, "and the engine's own view of it");
        assert_eq!(model_types(&doc), ["Line", "Line"]);
        assert!(doc.redo().unwrap().0);
        assert_eq!(doc.session.state_hash().unwrap(), merged_hash, "the person's redo writes the exact merged state again");
        assert_eq!(view(&doc), merged_view);
        assert_eq!(doc.session.kdm_resets(), 0);
    }

    /// `modelReply` reads the tool ids of the objects a reply names alone: in a drawing of many
    /// entities, a reply naming one gets the tool id a read of the whole drawing gives it, and a
    /// reply naming none comes back as it was.
    #[test]
    fn a_reply_reads_the_tool_ids_of_the_objects_it_names_alone() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        for i in 0..300 {
            doc.execute("line.add", json!({ "points": [[i, 0], [i, 10]] })).unwrap();
        }
        doc.version().unwrap();
        let kdm = doc.session.kdm().unwrap();
        let mut entities = kdm.replica().objects_of_type("Entity");
        entities.sort();
        assert_eq!(entities.len(), 301);
        let one = entities[150].as_str().to_owned();
        let mut changed = serde_json::Map::new();
        changed.insert(one.clone(), json!("moved"));
        let reply = json!({ "target": one, "changed": changed, "note": "one line" });
        let shown: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
        let whole = kition_kit::kdm::tool_ids::ToolIds::of_replica(kdm.replica()).show_json(&reply);
        assert_eq!(shown, whole, "the tool id the whole drawing's ids give");
        assert_ne!(shown["target"], json!(one), "the entity is named by its tool id: {shown}");
        let plain = json!({ "summary": "Plan", "layers": ["0"], "count": 301 });
        assert_eq!(serde_json::from_str::<Value>(&doc.model_reply(&plain.to_string()).unwrap()).unwrap(), plain, "a reply naming no object stays");
    }

    /// Comparing a draft names what merging it would do, and changes nothing.
    #[test]
    fn comparing_a_draft_names_what_the_merge_would_do_and_changes_nothing() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = CadDocument::open_with_replica("Plan.dxf", &[], &doc.snapshot().unwrap(), &[]).unwrap();
        draft.execute("circle.add", json!({ "center": [300, 200], "radius": 10 })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        let before = entities(&doc);
        let preview = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), base, "comparing changes nothing");
        assert_eq!(entities(&doc), before);
        let answer = doc.merge_changes(&changes, &Origin::person("person:local")).unwrap();
        assert_eq!(answer["summary"], preview, "the comparison named what the merge did: {answer}");
        assert!(doc.compare_changes(b"not changes").is_err());
    }

    /// An earlier state (a checkpoint's snapshot) is written over the drawing by object id as one
    /// undo step: the replica holds that state again, with the same ids, and Undo takes it back.
    #[test]
    fn a_restored_state_keeps_the_ids_and_undoes_as_one_step() {
        use kition_kit::kdm::{ActorId, Replica};
        use kition_kit::session::KitModel;
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        doc.version().unwrap();
        let checkpoint = doc.snapshot().unwrap();
        let before = entities(&doc);
        extra_edit(&mut doc);
        assert_eq!(entities(&doc), before + 2);
        let (prior_hash, prior_view) = (doc.session.state_hash().unwrap(), view(&doc));
        let origin = Origin::person("person:local");
        let damage = doc.restore_state(&checkpoint, &origin).unwrap();
        assert_ne!(damage, Damage::None);
        assert_eq!(entities(&doc), before, "the engine's drawing follows the restore");
        let (restored_hash, restored_view) = (doc.session.state_hash().unwrap(), view(&doc));
        let then = Replica::load(<kdm_model::CadModel as KitModel>::schema(), &checkpoint, ActorId::random()).unwrap().state_json().unwrap();
        assert_eq!(doc.replicated_state().unwrap()["objects"], then["objects"], "the checkpoint's objects, under their ids");
        let at = doc.version().unwrap();
        assert_eq!(doc.restore_state(&checkpoint, &origin).unwrap(), Damage::None, "the same state again changes nothing");
        assert_eq!(doc.version().unwrap(), at);
        assert!(doc.undo().unwrap().0);
        assert_eq!(entities(&doc), before + 2, "one undo takes the restore back");
        assert_eq!(doc.session.state_hash().unwrap(), prior_hash, "to the exact state before it");
        assert_eq!(view(&doc), prior_view, "and the engine's own view of it");
        assert!(doc.redo().unwrap().0);
        assert_eq!(doc.session.state_hash().unwrap(), restored_hash, "one redo writes the restored state again exactly");
        assert_eq!(view(&doc), restored_view);
        let mut other = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let other = other.snapshot().unwrap();
        assert!(doc.restore_state(&other, &origin).is_err(), "another document's state is refused");
    }

    /// An agent's reply names objects by their tool ids; text that merely holds an id stays.
    #[test]
    fn a_reply_names_objects_by_their_tool_ids() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let state = doc.replicated_state().unwrap();
        let layer = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, object)| object["$type"] == "Layer")
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| panic!("a layer object: {state}"));
        let shown: Value =
            serde_json::from_str(&doc.model_reply(&json!({ "target": layer, "note": format!("about {layer}") }).to_string()).unwrap()).unwrap();
        assert!(shown["target"].as_str().is_some_and(|id| id.starts_with("layer:")), "{shown}");
        assert_eq!(shown["note"], json!(format!("about {layer}")), "text that holds an id stays");
        assert!(doc.model_reply("not json").is_err());
    }

    /// A receipt names what changed since a version, or says why it cannot.
    #[test]
    fn changes_since_answers_a_receipt_or_why_not() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        assert!(doc.changes_since(&[])["changes"].is_null(), "no replica yet");
        let since = doc.version().unwrap();
        extra_edit(&mut doc);
        let receipt = doc.changes_since(&since);
        assert!(receipt["changes"].is_object(), "{receipt}");
        let now = doc.version().unwrap();
        assert_ne!(receipt["changes"], doc.changes_since(&now)["changes"], "the edit is in the receipt: {receipt}");
        let other = CadDocument::open("Plan.dxf", &fixture()).unwrap().version().unwrap();
        let refused = doc.changes_since(&other);
        assert!(refused["changes"].is_null() && refused["unavailable"].is_string(), "{refused}");
    }

    /// `modelDescribe` answers the object model and the engine's commands.
    #[test]
    fn model_describe_answers_the_object_model() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let described = doc.model_describe().unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "circle.add"));
    }

    /// `modelBatch` runs a command in the engine and reads fields back; a request that is not JSON
    /// is refused with the error's JSON.
    #[test]
    fn model_batch_runs_in_the_engine() {
        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let before = entities(&doc);
        let command = json!({ "ops": [{ "op": "command", "id": "circle.add", "params": { "center": [5, 5], "radius": 2 } }], "load": [{ "target": "root", "fields": ["model"] }] });
        let answer = doc.model_batch(&command.to_string()).unwrap();
        assert_eq!(entities(&doc), before + 1, "the command ran in the engine: {answer}");
        assert_eq!(answer["loaded"][0]["fields"]["model"].as_array().unwrap().len(), 2, "{answer}");
        let refused = doc.model_batch("not json").unwrap_err();
        assert!(refused.contains("invalid-op"), "{refused}");
    }

    /// A damaged (or missing) stored replica never stops the file from opening: it opens
    /// plainly, never as a blank drawing.
    #[test]
    fn a_damaged_stored_replica_opens_the_file_plainly() {
        let bytes = fixture();
        let a = CadDocument::open("Plan.dxf", &bytes).unwrap();
        for (snapshot, blobs) in [(&b"not a snapshot"[..], &[][..]), (&[][..], &[1u8, 2, 3][..])] {
            let doc = CadDocument::open_with_replica("Plan.dxf", &bytes, snapshot, blobs).unwrap();
            assert_eq!(entities(&doc), entities(&a));
            assert!(!doc.has_replica(), "the file opened plainly");
        }
        assert!(CadDocument::open_with_replica("Plan.dxf", b"not a drawing", b"not a snapshot", &[]).is_err());
    }

    /// `a` saves with its replica; `b` opens with it. `a` draws more;
    /// `b.importChanges(a.exportChanges(b.version()))` brings `b` to the same replicated state.
    #[test]
    fn a_second_document_follows_through_export_and_import_changes() {
        let mut a = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(Some("Plan.dxf")).unwrap();
        let snapshot = a.snapshot().unwrap();
        let mut b = CadDocument::open_with_replica("Plan.dxf", &bytes, &snapshot, &[]).unwrap();
        assert_eq!(b.version().unwrap(), a.version().unwrap(), "b starts from a's own replica");

        a.execute("circle.add", json!({ "center": [300, 200], "radius": 10 })).unwrap();
        let since = b.version().unwrap();
        let changes = a.export_changes(&since).unwrap();
        b.import_changes(&changes).unwrap();
        assert_eq!(a.version().unwrap(), b.version().unwrap());
        assert_eq!(entities(&a), entities(&b));
        assert_eq!(a.render(0.0, 0.0, 1.0, 400, 300).unwrap(), b.render(0.0, 0.0, 1.0, 400, 300).unwrap(), "both replicas render the same drawing");
    }
    /// `kition-cad` serves the object model through `ReplicaPort`: a command and a model edit in
    /// batches reach the engine, and a read-only batch refuses writes.
    #[test]
    fn serves_the_object_model_over_its_change_exchange() {
        use kition_kit::session::model::{Exchange, ModelErrorCode};
        use serde_json::{Value, json};

        let mut doc = CadDocument::open("Plan.dxf", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let described = exchange.describe(&mut doc).unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "circle.add"));

        let command = json!({ "ops": [{ "op": "command", "id": "circle.add", "params": { "center": [5, 5], "radius": 2 } }] });
        assert_eq!(exchange.batch(&mut doc, &command, true).unwrap_err().code, ModelErrorCode::ReadOnly);
        let before = doc.version().unwrap();
        exchange.batch(&mut doc, &command, false).unwrap();
        assert_ne!(doc.version().unwrap(), before, "the command ran in the engine");

        let load = |exchange: &mut Exchange, doc: &mut CadDocument, target: &str, field: &str| -> Value {
            let answer = exchange.batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": [field] }] }), false).unwrap();
            answer["loaded"][0]["fields"][field].clone()
        };
        let model = load(&mut exchange, &mut doc, "root", "model");
        assert_eq!(model.as_array().unwrap().len(), 2, "the line and the circle");
        let remove = json!({ "ops": [{ "op": "delete", "target": model[0] }] });
        exchange.batch(&mut doc, &remove, false).unwrap();
        assert_eq!(load(&mut Exchange::new(), &mut doc, "root", "model").as_array().unwrap().len(), 1);
    }
}
