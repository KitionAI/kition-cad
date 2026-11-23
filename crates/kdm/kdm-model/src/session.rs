//! [`CadSession`]: one drawing's replica and its [`CadModel`] view, for the engine's command
//! bridge (Kition phase 3g).
//!
//! It is `kition_kit::session::KitSession<CadModel>`'s flow (transactions, per-actor undo, the
//! change exchange, the post-merge repair through [`KitModel::validate`], the view patched
//! through [`KitModel::apply`]), with the undo steps the engine's history can still reach counted
//! here, as Kition Slide's `SlideSession` does. Edits are diff-bridged: the engine changes its
//! own [`Drawing`], and [`CadSession::record`] writes what changed as one transaction
//! ([`write_changes`]), then adopts it as the view. A drawing has no blobs (raster images are
//! referenced by path). Everything here is `Send`.

use kition_cad_doc::Drawing;
use kition_kit::kdm::{ActorId, DocId, ObjectId, Origin, OriginKind, Replica, UndoScope, Version};
use kition_kit::session::{Damage, KitModel, KitSession, MemoryBlobs};

use crate::{CadModel, Ids, Result, canonical, handle_range, identify_cells, kit_err, next_handle, read_document, write_changes, write_document};

/// One open drawing as a KDM replica and the engine's view of it. See the module docs.
pub struct CadSession {
    inner: KitSession<CadModel>,
    /// This replica's undo and redo steps the engine's history can still take
    /// ([`CadSession::retain_steps`]): the replica keeps more.
    undo_steps: usize,
    redo_steps: usize,
}

impl std::fmt::Debug for CadSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CadSession")
            .field("doc", &self.inner.replica().doc_id())
            .field("actor", &self.inner.replica().actor())
            .finish_non_exhaustive()
    }
}

impl CadSession {
    /// A new replica holding `drawing`, written in its creating transaction (not an undo step).
    /// The view is `drawing` itself (its table cells given ids where they had none).
    pub fn create(drawing: &Drawing) -> Result<Self> {
        let mut drawing = drawing.clone();
        identify_cells(&mut drawing);
        let drawing = &drawing;
        let mut ids = Ids::default();
        let (replica, ()) =
            Replica::create_with(CadModel::schema(), crate::fresh::document(), crate::fresh::actor(), &Origin::system("create"), |tx| {
                write_document(tx, drawing, &mut ids).map(|_| ())
            })
            .map_err(kit_err)?;
        let mut inner: KitSession<CadModel> = KitSession::from_replica(replica, Box::new(MemoryBlobs::default()))?;
        // `from_replica` read the view back; put back the exact drawing and the ids
        // `write_document` minted for it.
        inner.model_mut().adopt(drawing.clone(), ids);
        Ok(Self { inner, undo_steps: 0, redo_steps: 0 })
    }

    /// Opens a snapshot (or a full change export) of another replica as a new replica of a fresh
    /// actor.
    pub fn open_snapshot(bytes: &[u8]) -> Result<Self> {
        let inner = KitSession::open_snapshot(bytes, Box::new(MemoryBlobs::default()), crate::fresh::actor())?;
        Ok(Self { inner, undo_steps: 0, redo_steps: 0 })
    }

    /// The view: the drawing as of the last record, undo, redo or import. Its next handle is this
    /// replica's own ([`handle_range`]).
    pub fn drawing(&self) -> &Drawing {
        &self.inner.model().drawing
    }

    pub fn model(&self) -> &CadModel {
        self.inner.model()
    }

    /// The handles this replica mints new entities from.
    pub fn handle_range(&self) -> (u64, u64) {
        handle_range(self.inner.replica().actor())
    }

    /// The next handle this replica mints in `d`.
    pub fn next_handle(&self, d: &Drawing) -> u64 {
        next_handle(self.inner.replica().actor(), d)
    }

    /// Writes what differs between the view and `after` as one transaction of `origin`, then
    /// makes `after` the view. Answers how many undo steps that made: 1, or 0 when nothing
    /// replicated changed or `origin` is a `system`/`import` one (never an undo step). On failure
    /// nothing is written and the view is the replica's.
    pub fn record(&mut self, origin: &Origin, after: &Drawing) -> Result<u32> {
        let mut identified = after.clone();
        identify_cells(&mut identified);
        let after = &identified;
        let before = self.inner.model().drawing.clone();
        let mut ids = self.inner.model_mut().take_ids();
        let version = self.inner.version();
        let result = self.inner.record(origin, |tx| write_changes(tx, &before, after, &mut ids).map(|_| ()).map_err(Into::into));
        if let Err(error) = result {
            // The id maps may name objects the failed transaction never made: read them again.
            let model = CadModel::load(self.inner.replica(), &MemoryBlobs::default())?;
            *self.inner.model_mut() = model;
            return Err(error);
        }
        self.inner.model_mut().adopt(after.clone(), ids);
        let wrote = self.inner.version() != version;
        let undoable = wrote && !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        if undoable {
            self.undo_steps = self.undo_steps.saturating_add(1);
            self.redo_steps = 0;
        }
        Ok(u32::from(undoable))
    }

    /// Import only the file difference, without adding or replacing either undo history.
    /// `before` is the saved-file form, so regenerated DXF caches do not become outside edits.
    /// The file's entities pair with the replica's by handle (the ids map), never by object id.
    pub fn reread(&mut self, before: &Drawing, after: &Drawing) -> Result<Damage> {
        let mut ids = self.inner.model().ids.clone();
        self.inner.reread(|_, tx| write_changes(tx, before, after, &mut ids).map(|_| ()).map_err(Into::into))
    }

    /// Whether the replica holds `drawing`'s content, compared id-blind ([`canonical`]) as
    /// [`read_document`] reads both back. Errs if `drawing` cannot be written to a replica.
    pub fn holds(&self, drawing: &Drawing) -> Result<bool> {
        let mut ids = Ids::default();
        let (scratch, ()) =
            Replica::create_with(CadModel::schema(), crate::fresh::document(), crate::fresh::actor(), &Origin::system("create"), |tx| {
                write_document(tx, drawing, &mut ids).map(|_| ())
            })
            .map_err(kit_err)?;
        let (theirs, _) = read_document(&scratch).map_err(kit_err)?;
        let (mine, _) = read_document(self.inner.replica()).map_err(kit_err)?;
        Ok(canonical(&theirs) == canonical(&mine))
    }

    /// Whether the replica holds what a file saved from it holds: `file` (a drawing read from
    /// DXF) against this replica's view saved as DXF and read back the same way, so what DXF does
    /// not keep (or writes from the next handle, `$HANDSEED`) does not count. A host reopening a
    /// file with the replica it stored beside it asks this before recording the file on top.
    pub fn holds_file(&self, file: &Drawing) -> Result<bool> {
        let bytes = kition_cad_io::write(self.drawing(), "drawing.dxf").map_err(kit_err)?;
        let mine = crate::decode("drawing.dxf", &bytes)?;
        Ok(file_form(&mine) == file_form(file))
    }

    /// Undoes this replica's last step of the person (never an import or a repair), repairs the
    /// result and patches the view. Answers whether there was one, and the damage.
    pub fn undo(&mut self) -> Result<(bool, Damage)> {
        self.undo_in(UndoScope::Person)
    }

    pub fn redo(&mut self) -> Result<(bool, Damage)> {
        self.redo_in(UndoScope::Person)
    }

    /// [`CadSession::undo`] in `scope` (KitModel M12a): the person's and the agents' steps on this
    /// replica are separate histories.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if !self.can_undo_in(scope) {
            return Ok((false, Damage::None));
        }
        let (done, damage) = self.inner.undo_in(scope)?;
        if done {
            self.undo_steps = self.undo_steps.saturating_sub(1);
            self.redo_steps = self.redo_steps.saturating_add(1);
        }
        Ok((done, damage))
    }

    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if !self.can_redo_in(scope) {
            return Ok((false, Damage::None));
        }
        let (done, damage) = self.inner.redo_in(scope)?;
        if done {
            self.redo_steps = self.redo_steps.saturating_sub(1);
            self.undo_steps = self.undo_steps.saturating_add(1);
        }
        Ok((done, damage))
    }

    /// Whether the person has an undo step to take: one of this replica's, not trimmed by
    /// [`CadSession::retain_steps`].
    pub fn can_undo(&self) -> bool {
        self.can_undo_in(UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_in(UndoScope::Person)
    }

    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.inner.can_undo_in(scope) && self.undo_steps > 0
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.inner.can_redo_in(scope) && self.redo_steps > 0
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps reachable (the
    /// engine history's own).
    pub fn retain_steps(&mut self, undo: usize, redo: usize) {
        self.undo_steps = self.undo_steps.min(undo);
        self.redo_steps = self.redo_steps.min(redo);
    }

    /// Undo and redo steps reachable, as [`CadSession::retain_steps`] left them.
    pub fn steps(&self) -> (usize, usize) {
        (self.undo_steps, self.redo_steps)
    }

    /// Imports remote or stored changes, repairs the merged state and patches the view. An
    /// import whose repair or view load fails has still imported its changes:
    /// [`CadSession::view_loaded`] tells whether the view followed.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        self.inner.import_changes(bytes)
    }

    /// Whether the replica as it is now reads into a view.
    pub fn view_loaded(&self) -> bool {
        read_document(self.inner.replica()).is_ok()
    }

    pub fn export_changes(&self, since: &Version) -> Result<Vec<u8>> {
        self.inner.export_changes(since)
    }

    pub fn version(&self) -> Version {
        self.inner.version()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.inner.snapshot()
    }

    pub fn state_hash(&self) -> Result<String> {
        self.inner.state_hash()
    }

    pub fn doc_id(&self) -> DocId {
        self.inner.replica().doc_id()
    }

    pub fn actor(&self) -> ActorId {
        self.inner.replica().actor()
    }

    pub fn replica(&self) -> &Replica {
        self.inner.replica()
    }

    /// The object of the entity with handle `h` in the view, for tests and tools.
    pub fn entity_object(&self, h: kition_cad_doc::Handle) -> Option<ObjectId> {
        self.inner.model().ids().entities.object(h).cloned()
    }
}

/// A drawing read from DXF, compared id-blind and without `$HANDSEED` (the writer numbers its
/// structural handles from the drawing's next handle, which is each replica's own).
fn file_form(d: &Drawing) -> serde_json::Value {
    let mut d = d.clone();
    d.header.vars.remove("HANDSEED");
    canonical(&d)
}
