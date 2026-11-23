//! Table cells: a table entity's `kind` holds its grid (each cell's id and merge span), and each
//! cell's text is the `text` of a `TableCell` object in the root's `tableCells` map under
//! `<entity object>:<cell id>` ([`crate::table_cell_key`]). So typing in one cell writes that
//! cell's characters only: two people typing in different cells, or in one, both keep their
//! typing, and neither rewrites the table's grid.
//!
//! The entity is the cell's holder in name only (its key): an entity holds no container, since a
//! drawing holds many entities and every container costs each of them an operation. The write
//! path creates a table's cells with it, keeps their texts and deletes them with it; the repair
//! deletes cells whose table is gone or no longer has their id (see `repair`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_cad_doc::{Drawing, EntityKind, EntityStore, Handle, Table};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Map, Value, json};

use crate::source::Source;
use crate::{KdmResult, root_id, table_cell_key, table_cell_tool_id};

/// The root's map of cells.
pub(crate) const FIELD: &str = "tableCells";
/// The cells' object type.
pub(crate) const TYPE: &str = "TableCell";
/// A cell's text field.
const TEXT: &str = "text";

/// The cell texts of a table entity's kind, by cell id (empty for any other kind).
fn texts(kind: &EntityKind) -> BTreeMap<u32, &str> {
    match kind {
        EntityKind::Table(t) => t.cells.iter().flatten().map(|c| (c.id, c.text.as_str())).collect(),
        _ => BTreeMap::new(),
    }
}

/// Whether a table's cells need ids (one is 0, or two share one).
fn unidentified(t: &Table) -> bool {
    let mut seen = HashSet::new();
    t.cells.iter().flatten().any(|c| c.id == 0 || !seen.insert(c.id))
}

/// Gives the cells of every table of `d` that has a cell without its own id one
/// ([`Table::identify_cells`]): a replica keys each cell's text by its id. Answers whether it
/// changed any.
pub fn identify_cells(d: &mut Drawing) -> bool {
    fn store(s: &mut EntityStore) -> bool {
        let tables: Vec<Handle> = s.iter().filter(|e| matches!(&e.kind, EntityKind::Table(t) if unidentified(t))).map(|e| e.handle).collect();
        for h in &tables {
            s.modify(*h, |e| {
                if let EntityKind::Table(t) = &mut e.kind {
                    // An exhausted id space leaves the table as it is.
                    let _ = t.identify_cells();
                }
            });
        }
        !tables.is_empty()
    }
    let mut changed = store(&mut d.model);
    for l in &mut d.layouts {
        changed |= store(&mut l.entities);
    }
    for b in d.blocks.values_mut() {
        if b.entities.iter().any(|e| matches!(&e.kind, EntityKind::Table(t) if unidentified(t))) {
            changed |= store(&mut Arc::make_mut(b).entities);
        }
    }
    changed
}

/// The live cell object a key of the root's map names.
fn live(tx: &Tx<'_>, key: &str) -> KdmResult<Option<ObjectId>> {
    let Some(object) = tx.map_get(&root_id(), FIELD, key)?.as_str().map(ObjectId::from_string) else { return Ok(None) };
    Ok((tx.exists(&object) && tx.object_type(&object).is_ok_and(|t| t == TYPE)).then_some(object))
}

/// Makes the text field of `object` read `now`, given that it reads `was`: only the changed middle
/// is deleted and inserted, so concurrent typing elsewhere in the text stays.
fn write_text(tx: &mut Tx<'_>, object: &ObjectId, was: &str, now: &str) -> KdmResult<bool> {
    if was == now {
        return Ok(false);
    }
    let a: Vec<char> = was.chars().collect();
    let b: Vec<char> = now.chars().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix =
        a.get(prefix..).unwrap_or_default().iter().rev().zip(b.get(prefix..).unwrap_or_default().iter().rev()).take_while(|(x, y)| x == y).count();
    let removed = a.len().saturating_sub(prefix + suffix);
    if removed > 0 {
        tx.text_delete(object, TEXT, prefix, removed)?;
    }
    let inserted: String = b.get(prefix..b.len().saturating_sub(suffix)).unwrap_or_default().iter().collect();
    if !inserted.is_empty() {
        tx.text_insert(object, TEXT, prefix, &inserted)?;
    }
    Ok(true)
}

/// Writes the cells of the entity `entity` (handle `handle`) going from `old` to `new` (`None`:
/// the entity is new, or gone): a cell gone from the grid is deleted, a new one created, and the
/// text of one in both changed where it differs. Any other kind holds no cells. Answers whether
/// it wrote anything.
pub(crate) fn write_cells(tx: &mut Tx<'_>, entity: &ObjectId, handle: Handle, old: Option<&EntityKind>, new: Option<&EntityKind>) -> KdmResult<bool> {
    let before = old.map(texts).unwrap_or_default();
    let after = new.map(texts).unwrap_or_default();
    if before.is_empty() && after.is_empty() {
        return Ok(false);
    }
    let root = root_id();
    let mut wrote = false;
    for id in before.keys().filter(|id| !after.contains_key(id)) {
        let key = table_cell_key(entity, *id);
        if let Some(object) = live(tx, &key)? {
            tx.delete(&object)?;
        }
        if !tx.map_get(&root, FIELD, &key)?.is_null() {
            tx.map_delete(&root, FIELD, &key)?;
        }
        wrote = true;
    }
    for (id, text) in &after {
        let key = table_cell_key(entity, *id);
        let tool_id = json!(table_cell_tool_id(handle, *id));
        match live(tx, &key)? {
            Some(object) => {
                if tx.get(&object, "id")? != tool_id {
                    tx.set(&object, "id", tool_id)?;
                    wrote = true;
                }
                match before.get(id) {
                    Some(was) => wrote |= write_text(tx, &object, was, text)?,
                    // A cell the write did not know the text of: its text replaced whole.
                    None => {
                        let len = tx.text_len(&object, TEXT)?;
                        if len > 0 {
                            tx.text_delete(&object, TEXT, 0, len)?;
                            wrote = true;
                        }
                        if !text.is_empty() {
                            tx.text_insert(&object, TEXT, 0, text)?;
                            wrote = true;
                        }
                    }
                }
            }
            None => {
                let object = tx.create(TYPE, Map::from_iter([("id".to_owned(), tool_id)]))?;
                if !text.is_empty() {
                    tx.text_insert(&object, TEXT, 0, text)?;
                }
                tx.map_set(&root, FIELD, &key, json!(object.as_str()))?;
                wrote = true;
            }
        }
    }
    Ok(wrote)
}

/// Splits a key of the root's map into the table entity's object and the cell id.
pub(crate) fn parse_key(key: &str) -> Option<(ObjectId, u32)> {
    let (entity, cell) = key.rsplit_once(':')?;
    Some((ObjectId::from_string(entity), cell.parse().ok()?))
}

/// The live cell objects of the root's map, by table entity object and cell id.
pub(crate) type Index = HashMap<ObjectId, HashMap<u32, ObjectId>>;

pub(crate) fn index<S: Source>(src: &S) -> KdmResult<Index> {
    let mut out: Index = HashMap::new();
    for (key, value) in src.entries(&root_id(), FIELD)? {
        let (Some((entity, cell)), Value::String(object)) = (parse_key(&key), value) else { continue };
        let object = ObjectId::from_string(object);
        if src.exists(&object) && src.type_of(&object).as_deref() == Some(TYPE) {
            out.entry(entity).or_default().insert(cell, object);
        }
    }
    Ok(out)
}

/// Fills the texts of a table entity's cells (read with none) from their cell objects; a cell
/// without one reads empty.
pub(crate) fn fill<S: Source>(src: &S, cells: Option<&HashMap<u32, ObjectId>>, kind: &mut EntityKind) {
    let (EntityKind::Table(t), Some(cells)) = (kind, cells) else { return };
    for cell in t.cells.iter_mut().flatten() {
        if let Some(object) = cells.get(&cell.id) {
            cell.text = src.text(object, TEXT).unwrap_or_default();
        }
    }
}
