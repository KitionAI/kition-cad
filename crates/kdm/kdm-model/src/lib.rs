//! Bridges Kition CAD's [`Drawing`] ([`kition_cad_doc`]) to a `kit.cad` KDM replica.
//!
//! [`CadModel`] implements [`KitModel`]: the engine's materialised view of a replica, the same
//! [`Drawing`] the rest of the engine already knows. [`write_document`] and [`write_changes`] are
//! the engine command bridge's entry points: writing a whole drawing in one creating transaction,
//! or only what a command changed.
//!
//! What lives where (`model/schema.json`):
//! - every entity is its own `Entity` object: its handle (a hex string), its `common` properties
//!   and its `kind` (the geometry and everything else of the entity, one JSON register, so
//!   moving an entity writes one register and two people editing different entities never
//!   meet). Every entity object gets a minted id, never one derived from its handle or the file
//!   ([`Ids::entities`] maps handles to objects); a file read again pairs its entities with the
//!   replica's by their stored `handle`;
//! - a table entity's `kind` holds its grid (row heights, column widths, each cell's id and
//!   merge span); each cell's text is a `TableCell` object's `text`, in the root's `tableCells`
//!   map under `<entity object>:<cell id>`, so two people typing in one table, or in one cell,
//!   both keep their typing ([`table_cell_key`]);
//! - draw order is a list of entity objects per entity store: the root's `model`, and each
//!   `Layout`'s and `Block`'s `entities` (so bringing an entity to the front is one list move);
//! - named tables (layers, linetypes, text, dimension, multileader and table styles, views,
//!   UCSs, layer states, groups, plot style tables), blocks and layouts are objects in maps of
//!   the root keyed by their name (folded to lower case, as the drawing compares names): two
//!   people adding the same name keep one record, and a rename is a delete and an add, with
//!   every reference to the name re-pointed in the same transaction (the engine's rename does
//!   that, so the diff writes it). A record's `order` keeps the table's order; the identities
//!   ever issued to records are the keys of the root's `recordIdsUsed` map (it only grows, so
//!   two people adding records at once both keep theirs);
//! - header variables are a map keyed by variable name, each its own register, except what each
//!   person draws with ([`CURRENT_VARS`] and the `DIM*` settings but `DIMSCALE`: the current
//!   layer, linetype, colour, styles, dimension settings, command defaults and drawing aids),
//!   which are each person's own: never written, kept by the view across reloads
//!   ([`carry_current`]);
//! - constraints are `Constraint` objects in the root's `constraints` list, their references by
//!   entity object id.
//!
//! **Handles.** Entity handles are replicated (they are the drawing's own references, and DXF
//! keeps them). A replica allocates new handles from its actor's own range ([`handle_range`]),
//! so two people drawing at once never mint the same handle. Every handle stored as data — a
//! constraint's references, a dimension's associations, group members, parametric exceptions — is
//! written as the object id of the entity it names (`refs`), or as `"#<hex>"` when it names
//! nothing (a file's dangling reference), and read back as that entity's handle.
//!
//! An entity's `handle` is the kit's to write: the schema marks it `readOnly` (KitObjectModel
//! OM8), so an object-model batch that sets one fails with `read-only`. The binding's
//! transaction settler also refuses a supplied creation handle with `invalid-value` and a
//! read-only explanation; neither changes the drawing (KHP A50). It is
//! `optional` so a batch can create an entity without one; the merge repair gives it the next
//! free handle.
//!
//! **Tool ids** (KitObjectModel OM9). The object model names an entity by its handle (`Entity`'s
//! `toolId` is its `handle`), the name inspect, commands and diagnostics use, so an entity made in
//! a session is named by the handle the saved file gives it from the moment it exists. Layers,
//! named views, blocks and layouts are named by their name in their table (`layer:Walls`,
//! `view:Front`, `block:Door`, `layout:Layout1`, a read-only `name` field the write path and the
//! repair keep equal to `data.name`): the file stores no handle for them that the drawing keeps,
//! and a name is unique within its table. A rename is a delete and an add (see above), so the
//! renamed record is named by its new name. The root's table maps own their records. Constraints
//! have `constraint:<id>` tool ids, using their persisted numeric ids. A table cell is
//! `cell:<table handle>-<cell id>` ([`table_cell_tool_id`]).
//!
//! **Constraint ids.** Numeric ids are replicated and survive saving and reopening. Repair
//! allocates missing or duplicate ids after model batches or concurrent creates.
//!
//! **Repair.** [`KitModel::validate`] clears what a merge left dangling: map entries and list
//! entries of deleted objects, objects nothing lists (the loser of two concurrent adds of one
//! name, an entity added to a layout someone deleted), duplicate handles, cells of tables that
//! are gone; an entity on a layer someone deleted goes to layer `0`, a constraint on a deleted
//! entity is removed, as are a dimension's associations and group members of deleted entities
//! (see `repair`).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod cells;
mod compare;
mod finite;
mod ids;
mod list;
mod read;
mod refs;
mod repair;
mod session;
mod source;
mod write;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use kition_cad_doc::{Drawing, Entity, EntityStore, HVal, Handle};
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitError, KitModel};

pub use cells::identify_cells;
pub use compare::{canonical, normalize};
pub use ids::{Ids, Known, Table};
pub use read::{read_document, read_document_known};
pub use refs::kind_in as read_entity_kind;
pub use repair::repair_drawing;
pub use session::CadSession;
pub use write::{write_changes, write_document};

/// Re-exported so downstream crates (the engine command bridge) share this crate's exact pin.
pub use kition_kit;

type Result<T> = kition_kit::session::Result<T>;
pub(crate) type KdmResult<T> = std::result::Result<T, KdmError>;

const SCHEMA: &str = include_str!("../model/schema.json");

/// The `Drawing` object's id: one per replica, minted by [`write_document`] with
/// [`Tx::create_with_id`], so every reader finds it without an index.
pub(crate) fn root_id() -> ObjectId {
    ObjectId::from_string("drawing")
}

fn kit_err(error: impl std::fmt::Display) -> KitError {
    KitError::new(error.to_string())
}

pub(crate) fn kdm_err(error: impl std::fmt::Display) -> KdmError {
    KdmError::Engine(error.to_string())
}

/// The header variables that are each person's own: what they draw with, not what the drawing
/// is. A replica never holds them, the view keeps its own across reloads, undo and imports
/// ([`carry_current`]), they are taken from the file when a file is opened, and a DXF or DWG file
/// saves the saving person's. Besides these names, every `DIM*` variable but `DIMSCALE` is each
/// person's own ([`is_current_var`]).
///
/// - the current layer, linetype, colour, lineweight, linetype scale, plot style, transparency,
///   material and plot style name, and the current text, dimension, multileader, table and
///   multiline styles;
/// - the other values new objects take: text height, thickness, elevation, polyline and trace
///   width, polyline linetype generation, multiline scale and justification, hatch pattern, scale
///   and angle;
/// - the drawing aids (ortho, the snap grid's base and angle) and the lineweight display;
/// - the defaults commands remember for the next run: circle radius, donut diameters, polygon
///   sides, fillet radius, chamfer distances, angle and method, trim mode, offset distance and
///   options, trim and extend modes, revision cloud arc length, spline knots, spline segments and
///   type, centre mark extension, the last inserted block and whether MIRROR mirrors text.
///
/// The styles, layers and other records are the drawing's, shared; so is everything an existing
/// entity's look depends on (units, limits, `LTSCALE`, `PDMODE`, `FILLMODE`, `DIMSCALE`…).
pub const CURRENT_VARS: &[&str] = &[
    "CLAYER",
    "CELTYPE",
    "CECOLOR",
    "CECOLOR_RGB",
    "CELWEIGHT",
    "CELTSCALE",
    "CPLOTSTYLE",
    "CETRANSPARENCY",
    "CMATERIAL",
    "CEPSNTYPE",
    "CEPSNID",
    "TEXTSTYLE",
    "DIMSTYLE",
    "CMLEADERSTYLE",
    "CTABLESTYLE",
    "CMLSTYLE",
    "TEXTSIZE",
    "THICKNESS",
    "ELEVATION",
    "PLINEWID",
    "TRACEWID",
    "PLINEGEN",
    "CMLSCALE",
    "CMLJUST",
    "HPNAME",
    "HPSCALE",
    "HPANG",
    "ORTHOMODE",
    "SNAPBASE",
    "SNAPANG",
    "LWDISPLAY",
    "CIRCLERAD",
    "DONUTID",
    "DONUTOD",
    "POLYSIDES",
    "FILLETRAD",
    "CHAMFERA",
    "CHAMFERB",
    "CHAMFERC",
    "CHAMFERD",
    "CHAMMODE",
    "TRIMMODE",
    "OFFSETDIST",
    "OFFSETERASE",
    "OFFSETLAYER",
    "TRIMEXTENDMODE",
    "EDGEMODE",
    "PROJMODE",
    "REVCLOUDARCLEN",
    "SPLKNOTS",
    "SPLINESEGS",
    "SPLINETYPE",
    "CENTEREXE",
    "INSNAME",
    "MIRRTEXT",
];

/// The one `DIM*` header variable that stays shared: a dimension or leader whose style's scale is
/// zero is drawn at the drawing's `DIMSCALE`, so existing dimensions look the way it says.
pub const SHARED_DIMVAR: &str = "DIMSCALE";

/// Whether a header variable is each person's own (names compare without case): one of
/// [`CURRENT_VARS`], or a `DIM*` variable (the current dimension settings new dimensions take
/// as their overrides) other than [`SHARED_DIMVAR`].
pub fn is_current_var(name: &str) -> bool {
    CURRENT_VARS.iter().any(|v| v.eq_ignore_ascii_case(name))
        || (name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("DIM")) && !name.eq_ignore_ascii_case(SHARED_DIMVAR))
}

/// `d`'s own header variables ([`is_current_var`]), by name.
pub fn current_vars(d: &Drawing) -> BTreeMap<&str, &HVal> {
    d.header.vars.iter().filter(|(key, _)| is_current_var(key)).map(|(key, value)| (key.as_str(), value)).collect()
}

/// Puts `from`'s own header variables ([`is_current_var`]) into `to` in place of `to`'s (a view
/// read again from the replica, which holds none), then sends every current name that names no
/// record of `to` (a layer someone else deleted) back to its default, as the repair does.
pub fn carry_current(from: &Drawing, to: &mut Drawing) {
    to.header.vars.retain(|key, _| !is_current_var(key));
    for (key, value) in &from.header.vars {
        if is_current_var(key) {
            to.header.vars.insert(key.clone(), value.clone());
        }
    }
    repair::header_names(to);
}

/// The key of a table cell's `TableCell` object in the root's `tableCells` map: the table
/// entity's object id and the cell's id within the table.
pub fn table_cell_key(entity: &ObjectId, cell: u32) -> String {
    format!("{}:{cell}", entity.as_str())
}

/// A table cell's tool id value (the `TableCell`'s `id`, named `cell:<value>`): the table's
/// handle and the cell's id within the table.
pub fn table_cell_tool_id(table: Handle, cell: u32) -> String {
    format!("{}-{cell}", table.hex())
}

/// The handles a replica of `actor` mints new entities from: `[start, end)`, a per-actor slot of
/// the handle space (Kition Film's id slots), so two actors that draw without exchanging first
/// never mint the same handle. `((actor as u64) % 2^20) << 32`: 2^20 slots, each 2^32 handles
/// (slot 0 starts above the handles a drawing reserves). Every handle stays under 2^52, so it
/// reads back exactly everywhere (handles are written as hex strings anyway).
pub fn handle_range(actor: ActorId) -> (u64, u64) {
    let base = (actor.get() % (1u64 << 20)) << 32;
    (base.max(0x100), base + (1u64 << 32))
}

/// The next handle `actor`'s replica mints in `d`: above every handle of its own range in use.
pub fn next_handle(actor: ActorId, d: &Drawing) -> u64 {
    let (start, end) = handle_range(actor);
    let used = all_entities(d).map(|e| e.handle.0).filter(|h| (start..end).contains(h)).max();
    used.map_or(start, |h| h.saturating_add(1)).max(start)
}

/// Every entity store of `d` in reading order: model space, the layouts, the blocks.
pub(crate) fn stores(d: &Drawing) -> impl Iterator<Item = &EntityStore> + '_ {
    std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities))
}

pub(crate) fn all_entities(d: &Drawing) -> impl Iterator<Item = &Arc<Entity>> + '_ {
    stores(d).flat_map(EntityStore::iter)
}

/// A file's drawing, as [`KitModel::import`] reads it: DXF (or DWG where the build reads it),
/// with duplicate handles (and handle 0) renumbered, since a replica needs every entity's handle
/// unique in the drawing, and every table cell given an id (a replica keys cell texts by it).
pub fn decode(name: &str, bytes: &[u8]) -> Result<Drawing> {
    let mut d = kition_cad_io::read(bytes, if name.is_empty() { "drawing.dxf" } else { name }).map_err(kit_err)?;
    dedupe_handles(&mut d);
    identify_cells(&mut d);
    Ok(d)
}

/// Gives every entity whose handle is 0 or already used (after its first use) a fresh handle,
/// keeping the draw order. References to a duplicated handle keep naming its first holder.
pub fn dedupe_handles(d: &mut Drawing) {
    let mut seen = HashSet::new();
    let needs = all_entities(d).any(|e| e.handle.0 == 0 || !seen.insert(e.handle));
    if !needs {
        return;
    }
    if let Some(max) = all_entities(d).map(|e| e.handle).max() {
        d.bump_handseed(max);
    }
    let mut seen = HashSet::new();
    let mut fix = |store: &EntityStore, d_seed: &mut u64| -> Option<EntityStore> {
        let dirty = store.iter().any(|e| e.handle.0 == 0 || seen.contains(&e.handle));
        let mut out = EntityStore::new();
        for e in store.iter() {
            let mut e = (**e).clone();
            if e.handle.0 == 0 || !seen.insert(e.handle) {
                e.handle = Handle(*d_seed);
                *d_seed = d_seed.saturating_add(1);
                seen.insert(e.handle);
            }
            out.push(e);
        }
        dirty.then_some(out)
    };
    let mut seed = d.handseed;
    if let Some(store) = fix(&d.model, &mut seed) {
        d.model = store;
    }
    for i in 0..d.layouts.len() {
        let fixed = d.layouts.get(i).and_then(|l| fix(&l.entities, &mut seed));
        if let (Some(store), Some(layout)) = (fixed, d.layouts.get_mut(i)) {
            layout.entities = store;
        }
    }
    let names: Vec<String> = d.blocks.keys().cloned().collect();
    for name in names {
        let fixed = d.blocks.get(&name).and_then(|b| fix(&b.entities, &mut seed));
        if let (Some(store), Some(block)) = (fixed, d.blocks.get_mut(&name)) {
            Arc::make_mut(block).entities = store;
        }
    }
    d.handseed = seed;
}

/// The engine's view of a `kit.cad` replica: the same [`Drawing`] the rest of the engine knows,
/// plus the id maps [`write_changes`] needs to find each object again.
pub struct CadModel {
    pub drawing: Drawing,
    ids: Ids,
    known: Known,
}

impl CadModel {
    /// A view built by the caller itself: `drawing` as written by [`write_document`] or
    /// [`write_changes`], with the ids they filled.
    pub fn from_parts(drawing: Drawing, ids: Ids) -> Self {
        let mut known = Known::default();
        known.remember(&drawing, &ids);
        Self { drawing, ids, known }
    }

    /// Replaces the view after the caller wrote `drawing` to the replica itself (a diff bridge's
    /// [`write_changes`]), with the ids that write left.
    pub fn adopt(&mut self, drawing: Drawing, ids: Ids) {
        self.known.remember(&drawing, &ids);
        self.drawing = drawing;
        self.ids = ids;
    }

    /// Takes the id maps out of the view, for a write that hands them back through
    /// [`CadModel::adopt`] (cheaper than cloning them for a large drawing).
    pub fn take_ids(&mut self) -> Ids {
        std::mem::take(&mut self.ids)
    }

    /// The id maps, current as of the last `load`/`apply`/`adopt`. A diff bridge clones them into
    /// its own working copy for a [`write_changes`] transaction, then hands them back through
    /// [`CadModel::adopt`].
    pub fn ids(&self) -> &Ids {
        &self.ids
    }

    /// Reads the view again; the entities of objects not in `changed` are taken from the current
    /// view (which is what the replica held before the change) rather than parsed again.
    fn reload(&mut self, replica: &Replica, changed: Option<&BTreeSet<ObjectId>>) -> Result<()> {
        let cache = read::EntityCache::of(&self.drawing, &self.ids);
        let (mut drawing, ids, _) = read::read_with(replica, Some(replica.actor()), &self.known, changed.map(|c| (&cache, c))).map_err(kit_err)?;
        carry_current(&self.drawing, &mut drawing);
        self.known.remember(&drawing, &ids);
        self.drawing = drawing;
        self.ids = ids;
        Ok(())
    }
}

impl std::fmt::Debug for CadModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CadModel").field("entities", &self.drawing.entity_count()).finish_non_exhaustive()
    }
}

impl KitModel for CadModel {
    fn schema() -> Arc<Schema> {
        #[allow(clippy::expect_used)]
        Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
    }

    fn load(replica: &Replica, _blobs: &dyn BlobSource) -> Result<Self> {
        let (drawing, ids, _) = read::read_with(replica, Some(replica.actor()), &Known::default(), None).map_err(kit_err)?;
        Ok(Self::from_parts(drawing, ids))
    }

    /// Reloads the view, parsing again only the entities that changed (the others are taken from
    /// the view), keeping every surviving constraint's local id and the view's own current
    /// properties ([`carry_current`]).
    fn apply(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>, _blobs: &dyn BlobSource) -> Result<Damage> {
        self.reload(replica, Some(changed))?;
        Ok(Damage::Full)
    }

    fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        repair::validate(tx, changed).map_err(kit_err)
    }

    fn import(name: &str, bytes: &[u8], _blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let drawing = decode(name, bytes)?;
        let mut ids = Ids::default();
        let (replica, ()) = Replica::create_with(Self::schema(), DocId::new(), actor, &Origin::system("import"), |tx| {
            write_document(tx, &drawing, &mut ids)?;
            Ok(())
        })
        .map_err(kit_err)?;
        Ok(replica)
    }

    /// In `name`'s format: DXF (the default), or an image or PDF of model space.
    fn export(&self, _replica: &Replica, name: &str, _blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        let name = if name.contains('.') { name } else { "drawing.dxf" };
        kition_cad_io::write(&self.drawing, name).map_err(kit_err)
    }
}
