//! Deterministic repair after a merge, an undo or a redo ([`kition_kit::session::KitModel::validate`]):
//! state only, so every replica writes the same repair for the same merged state.
//!
//! First at the replica's level ([`cleanup`]):
//! - map entries naming deleted objects (or objects of another type) are removed, and objects no
//!   map names are deleted: the loser of two concurrent adds of one name;
//! - list entries naming deleted objects are removed, and entities no store lists are deleted
//!   (one added to a layout or block someone deleted), as are constraints the drawing does not
//!   list;
//! - an entity whose handle an earlier entity (in reading order) already holds gets a new one,
//!   above every handle in use (a safety net: replicas mint handles from their own ranges);
//! - a table cell whose table is gone, is no table, or no longer has the cell's id is deleted,
//!   and every cell's tool id follows its table's handle.
//!
//! Then on the drawing itself ([`repair_drawing`], the merge-relevant part of the engine's AUDIT):
//! - layer `0`, the `ByBlock`, `ByLayer` and `Continuous` linetypes and the `Standard` styles
//!   exist; the current layer, linetype and styles name records that exist;
//! - an entity on a layer someone deleted goes to layer `0`; a linetype or style that is gone
//!   falls back to `ByLayer` or `Standard`; a block reference whose block is gone gets an empty
//!   definition (the reference and its attributes are kept until the block is redefined); a
//!   dimension's associations with deleted entities are removed; a reference that would make a
//!   block contain itself (two people nesting blocks in each other at once) is removed;
//! - a constraint on a deleted entity is removed, group members that are gone are removed (and a
//!   group left empty), as are parametric exceptions of deleted entities.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_cad_doc::{Block, DimStyle, Drawing, Entity, EntityKind, EntityStore, Handle, Layer, Linetype, MLeaderStyle, TableStyle, TextStyle};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::Value;

use crate::cells;
use crate::ids::{Known, Table};
use crate::read::{entries, read_with};
use crate::refs::{drop_gone, drop_gone_assoc, each_parametric_list, refs_gone};
use crate::{KdmResult, all_entities, root_id, table_cell_tool_id, write_changes};

pub(crate) fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let root = root_id();
    if !tx.exists(&root) || changed.is_empty() {
        return Ok(false);
    }
    let deleted = changed.iter().any(|o| !tx.exists(o));
    let (mut wrote, removed) = cleanup(tx, changed, deleted)?;
    wrote |= name_records(tx, changed)?;
    wrote |= constraint_ids(tx)?;
    if changed.iter().any(|o| tx.object_type(o).is_ok_and(|t| t == "Entity")) {
        let (drawing, ids, _) = read_with(&*tx, None, &Known::default(), None)?;
        for e in all_entities(&drawing) {
            if let Some(object) = ids.entities.object(e.handle).filter(|o| changed.contains(*o)) {
                let description = Value::String(crate::write::entity_description(e));
                if tx.get(object, "description")? != description {
                    tx.set(object, "description", description)?;
                    wrote = true;
                }
            }
        }
    }
    // Only entities changed (none deleted) and each names records that exist: nothing the
    // drawing-level repair could fix, so the drawing is not read for it.
    if !deleted && !removed && entities_only_and_sound(tx, changed)? {
        return Ok(wrote);
    }
    let (before, mut ids, _) = read_with(&*tx, None, &Known::default(), None)?;
    let mut after = before.clone();
    if repair_drawing(&mut after) {
        wrote |= write_changes(tx, &before, &after, &mut ids)?;
    }
    wrote |= crate::write::refresh_constraint_state(tx, &after, &ids)?;
    Ok(wrote)
}

/// Readable names are copied from data; tool identities are assigned once and never renamed.
const NAMED_TYPES: [&str; 13] = [
    "Layer",
    "Linetype",
    "TextStyle",
    "DimStyle",
    "MLeaderStyle",
    "TableStyle",
    "View",
    "Ucs",
    "LayerState",
    "Group",
    "PlotStyleTable",
    "Block",
    "Layout",
];

/// Sets `name` of every changed record of [`NAMED_TYPES`] whose `name` is not its `data.name`.
fn name_records(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let mut wrote = false;
    for object in changed {
        if !tx.exists(object) || !tx.object_type(object).is_ok_and(|ty| NAMED_TYPES.contains(&ty.as_str())) {
            continue;
        }
        let data = if tx.object_type(object)? == "Group" {
            Value::Object(tx.map_entries(object, "data")?.into_iter().collect())
        } else {
            tx.get(object, "data")?
        };
        let Some(name) = data.get("name").and_then(Value::as_str).map(str::to_owned) else { continue };
        if tx.get(object, "id")?.as_str().is_none() {
            tx.set(object, "id", Value::String(object.as_str().to_owned()))?;
            wrote = true;
        }
        if tx.get(object, "name").ok().as_ref().and_then(Value::as_str) != Some(name.as_str()) {
            tx.set(object, "name", Value::String(name))?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// Constraints loaded from DXF keep their numbers. A batch-created constraint or a merge of
/// independently allocated equal numbers gets a fresh number in deterministic list order.
fn constraint_ids(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let constraints = tx.list_items(&root_id(), "constraints")?;
    let number = |tx: &Tx<'_>, o: &ObjectId| tx.get(o, "id").ok().and_then(|v| v.as_u64()).and_then(|n| u32::try_from(n).ok());
    let mut next = constraints.iter().filter_map(|o| number(tx, o)).max().unwrap_or(0);
    let mut used = HashSet::new();
    let mut wrote = false;
    for object in constraints {
        if !tx.exists(&object) {
            continue;
        }
        if number(tx, &object).is_some_and(|n| n > 0 && used.insert(n)) {
            continue;
        }
        next = next.checked_add(1).ok_or_else(|| crate::kdm_err("constraint id space exhausted"))?;
        used.insert(next);
        tx.set(&object, "id", Value::from(next))?;
        wrote = true;
    }
    Ok(wrote)
}

/// The replica-level repair: dangling map and list entries, unnamed and unlisted objects,
/// references to deleted entities, duplicate handles. `deleted`: the change deleted objects
/// (references anywhere may dangle; otherwise only those of changed objects can). Answers
/// whether it wrote anything, and whether it deleted objects itself.
fn cleanup(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>, deleted: bool) -> KdmResult<(bool, bool)> {
    let root = root_id();
    let mut wrote = false;
    let mut removed = false;
    let maps: Vec<(&str, &str)> =
        Table::ALL.iter().map(|t| (t.field(), t.ty())).chain([("blocks", "Block"), ("layouts", "Layout"), (cells::FIELD, cells::TYPE)]).collect();
    let mut stores: Vec<ObjectId> = vec![root.clone()];
    for (field, ty) in maps {
        let mut named: HashSet<ObjectId> = HashSet::new();
        for (key, value) in tx.map_entries(&root, field)? {
            let object = value.as_str().map(ObjectId::from_string);
            match object.filter(|o| tx.exists(o) && tx.object_type(o).is_ok_and(|t| t == ty)) {
                Some(o) => {
                    named.insert(o);
                }
                None => {
                    tx.map_delete(&root, field, &key)?;
                    wrote = true;
                }
            }
        }
        for object in tx.objects_of_type(ty) {
            if !named.contains(&object) {
                tx.delete(&object)?;
                wrote = true;
                removed = true;
            }
        }
        if ty == "Block" || ty == "Layout" {
            stores.extend(named);
        }
    }
    let mut listed: HashSet<ObjectId> = HashSet::new();
    for owner in &stores {
        let field = if *owner == root { "model" } else { "object.query" };
        wrote |= clean_dangling(tx, owner, field)?;
        listed.extend(tx.list_items(owner, field)?);
    }
    for object in tx.objects_of_type("Entity") {
        if !listed.contains(&object) {
            tx.delete(&object)?;
            wrote = true;
            removed = true;
        }
    }
    // References to entities that are gone (the reader drops them from the view, so the repair
    // of the view would not see them).
    let gone = |tx: &Tx<'_>, o: &ObjectId| !(tx.exists(o) && tx.object_type(o).is_ok_and(|t| t == "Entity"));
    let referring: Vec<ObjectId> = if deleted || removed {
        tx.objects_of_type("Entity")
    } else {
        changed.iter().filter(|o| tx.object_type(o).is_ok_and(|t| t == "Entity")).cloned().collect()
    };
    for object in referring {
        let mut kind = tx.get(&object, "kind")?;
        if drop_gone_assoc(&mut kind, &|o| gone(tx, o)) {
            tx.set(&object, "kind", kind)?;
            wrote = true;
        }
    }
    for object in tx.objects_of_type("Group") {
        let mut members = tx.map_get(&object, "data", "members")?;
        if drop_gone(&mut members, &|o| gone(tx, o)) {
            tx.map_set(&object, "data", "members", members)?;
            wrote = true;
        }
    }
    let mut parametric = tx.get(&root, "parametric")?;
    let mut dropped = false;
    each_parametric_list(&mut parametric, &mut |list| dropped |= drop_gone(list, &|o| gone(tx, o)));
    if dropped {
        tx.set(&root, "parametric", parametric)?;
        wrote = true;
    }
    for object in tx.objects_of_type("Constraint") {
        if refs_gone(&tx.get(&object, "refs")?, &|o| gone(tx, o)) {
            tx.delete(&object)?;
            wrote = true;
            removed = true;
        }
    }
    wrote |= clean_dangling(tx, &root, "constraints")?;
    let constraints: HashSet<ObjectId> = tx.list_items(&root, "constraints")?.into_iter().collect();
    for object in tx.objects_of_type("Constraint") {
        if !constraints.contains(&object) {
            tx.delete(&object)?;
            wrote = true;
        }
    }

    // Duplicate handles, in the reader's order: model space, the layouts, the blocks.
    let mut order: Vec<ObjectId> = Vec::new();
    let mut seen = HashSet::new();
    let mut owners = vec![(root.clone(), "model")];
    owners.extend(entries(&*tx, "layouts", "Layout")?.into_iter().map(|e| (e.object, "object.query")));
    owners.extend(entries(&*tx, "blocks", "Block")?.into_iter().map(|e| (e.object, "object.query")));
    for (owner, field) in &owners {
        for object in tx.list_items(owner, field)? {
            if tx.exists(&object) && seen.insert(object.clone()) {
                order.push(object);
            }
        }
    }
    let mut handles: Vec<(ObjectId, Option<Handle>)> = Vec::with_capacity(order.len());
    for object in order {
        let h = tx.get(&object, "handle")?.as_str().and_then(Handle::parse_hex);
        handles.push((object, h));
    }
    let mut next = handles.iter().filter_map(|(_, h)| h.map(|h| h.0)).max().unwrap_or(0x100).saturating_add(1).max(0x100);
    let mut used = HashSet::new();
    for (object, h) in handles {
        if let Some(h) = h
            && used.insert(h)
        {
            continue;
        }
        tx.set(&object, "handle", Value::String(Handle(next).hex()))?;
        used.insert(Handle(next));
        next = next.saturating_add(1);
        wrote = true;
        // References by handle into this entity now name another: read the drawing again.
        removed = true;
    }
    // Cells name nothing the drawing-level repair reads: deleting one needs no read of it.
    wrote |= clean_cells(tx)?;
    Ok((wrote, removed))
}

/// Table cells whose table is gone, is no table, or no longer has their id are deleted (a cell
/// typed in while someone deleted its row, or its table); every other cell's tool id names its
/// table's handle (which a duplicate-handle repair may have changed).
fn clean_cells(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = root_id();
    let mut wrote = false;
    let mut tables: HashMap<ObjectId, Option<(Handle, HashSet<u32>)>> = HashMap::new();
    for (key, value) in tx.map_entries(&root, cells::FIELD)? {
        let Some(object) = value.as_str().map(ObjectId::from_string) else { continue };
        let table = match cells::parse_key(&key) {
            Some((entity, cell)) => {
                if !tables.contains_key(&entity) {
                    let grid = table_grid(tx, &entity)?;
                    tables.insert(entity.clone(), grid);
                }
                tables.get(&entity).and_then(Option::as_ref).filter(|(_, ids)| ids.contains(&cell)).map(|(h, _)| (*h, cell))
            }
            None => None,
        };
        match table {
            Some((handle, cell)) => {
                let id = Value::String(table_cell_tool_id(handle, cell));
                if tx.exists(&object) && tx.get(&object, "id")? != id {
                    tx.set(&object, "id", id)?;
                    wrote = true;
                }
            }
            None => {
                tx.map_delete(&root, cells::FIELD, &key)?;
                if tx.exists(&object) {
                    tx.delete(&object)?;
                }
                wrote = true;
            }
        }
    }
    Ok(wrote)
}

/// A live table entity's handle and the ids of its cells (`None`: no such table).
fn table_grid(tx: &Tx<'_>, entity: &ObjectId) -> KdmResult<Option<(Handle, HashSet<u32>)>> {
    if !(tx.exists(entity) && tx.object_type(entity).is_ok_and(|t| t == "Entity")) {
        return Ok(None);
    }
    let kind = tx.get(entity, "kind")?;
    if kind.get("type").and_then(Value::as_str) != Some("table") {
        return Ok(None);
    }
    let Some(handle) = tx.get(entity, "handle")?.as_str().and_then(Handle::parse_hex) else { return Ok(None) };
    let ids = kind
        .get("cells")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|rows| rows.values())
        .filter_map(Value::as_object)
        .flat_map(|cols| cols.values())
        .filter_map(|cell| cell.get("id").and_then(Value::as_u64).and_then(|id| u32::try_from(id).ok()))
        .collect();
    Ok(Some((handle, ids)))
}

/// Whether every changed object is an existing entity of a kind that names no block, dimension
/// style or other entity, on a layer and with a linetype (and a text style) that exist, or the
/// root (entities added to model space; its header names no record): then the drawing-level
/// repair has nothing to do.
fn entities_only_and_sound(tx: &Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let root = root_id();
    let names = |table: Table| -> KdmResult<Names> {
        let mut out = HashSet::new();
        for entry in entries(tx, table.field(), table.ty())? {
            if let Some(name) = tx.get(&entry.object, "data")?.get("name").and_then(Value::as_str) {
                out.insert(name.to_ascii_lowercase());
            }
        }
        Ok(Names(out))
    };
    let mut tables: HashMap<Table, Names> = HashMap::new();
    let mut has = |table: Table, name: &str| -> KdmResult<bool> {
        if let std::collections::hash_map::Entry::Vacant(slot) = tables.entry(table) {
            slot.insert(names(table)?);
        }
        Ok(tables.get(&table).is_some_and(|n| n.has(name)))
    };
    for object in changed {
        // The header names no record: the current layer and styles are each person's own,
        // never replicated (`crate::CURRENT_VARS`).
        if *object == root {
            continue;
        }
        let ty = tx.object_type(object).ok();
        // A cell's text names nothing (`clean_cells` already dropped cells of tables that are gone).
        if ty.as_deref() == Some(cells::TYPE) {
            continue;
        }
        if ty.as_deref() != Some("Entity") {
            return Ok(false);
        }
        let common = tx.get(object, "common")?;
        let kind = tx.get(object, "kind")?;
        let ty = kind.get("type").and_then(Value::as_str).unwrap_or_default();
        let style = match ty {
            "line" | "point" | "circle" | "arc" | "ellipse" | "lwPolyline" | "polyline3d" | "spline" | "ray" | "xLine" | "hatch" | "solid"
            | "trace" | "face3d" | "image" | "wipeout" | "unknown" => None,
            "text" | "mText" => Some(kind.get("style").and_then(Value::as_str).unwrap_or("Standard").to_owned()),
            _ => return Ok(false),
        };
        let layer = common.get("layer").and_then(Value::as_str).unwrap_or("0");
        let linetype = common.get("linetype").and_then(Value::as_str).unwrap_or("ByLayer");
        if layer.trim().is_empty() || !has(Table::Layers, layer)? {
            return Ok(false);
        }
        if !linetype.eq_ignore_ascii_case("ByLayer") && !linetype.eq_ignore_ascii_case("ByBlock") && !has(Table::Linetypes, linetype)? {
            return Ok(false);
        }
        if let Some(style) = style
            && !has(Table::TextStyles, &style)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Removes list entries naming an object that no longer exists.
fn clean_dangling(tx: &mut Tx<'_>, owner: &ObjectId, field: &str) -> KdmResult<bool> {
    let items = tx.list_items(owner, field)?;
    let mut wrote = false;
    for (index, object) in items.iter().enumerate().rev() {
        if !tx.exists(object) {
            tx.list_remove(owner, field, index)?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// Names compared as the drawing compares them (ASCII case folded).
struct Names(HashSet<String>);

impl Names {
    fn of<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        Names(names.map(str::to_ascii_lowercase).collect())
    }
    fn has(&self, name: &str) -> bool {
        self.0.contains(&name.to_ascii_lowercase())
    }
}

/// Repairs what a merge can leave inconsistent in `d` (see the module docs). Answers whether it
/// changed anything; a consistent drawing is left as it is.
pub fn repair_drawing(d: &mut Drawing) -> bool {
    let mut changed = standard_records(d);
    changed |= header_names(d);
    changed |= missing_blocks(d);
    changed |= entity_references(d);
    changed |= block_cycles(d);
    let all: HashSet<Handle> = all_entities(d).map(|e| e.handle).collect();
    changed |= handle_references(d, &all);
    changed
}

/// Layer 0, the standard linetypes and styles, and the names table records refer to.
fn standard_records(d: &mut Drawing) -> bool {
    let mut changed = false;
    if d.layer("0").is_none() {
        d.layers.insert(0, Layer::default());
        changed = true;
    }
    for name in ["ByBlock", "ByLayer"] {
        if d.linetype(name).is_none() {
            d.linetypes.push(Linetype { name: name.into(), description: String::new(), pattern: Vec::new() });
            changed = true;
        }
    }
    if d.linetype("Continuous").is_none() {
        d.linetypes.push(Linetype::continuous());
        changed = true;
    }
    if d.text_style("Standard").is_none() {
        d.text_styles.insert(0, TextStyle::default());
        changed = true;
    }
    if d.dim_style("Standard").is_none() {
        d.dim_styles.insert(0, DimStyle::default());
        changed = true;
    }
    if !d.mleader_styles.iter().any(|s| s.name.eq_ignore_ascii_case("Standard")) {
        d.mleader_styles.insert(0, MLeaderStyle::default());
        changed = true;
    }
    if !d.table_styles.iter().any(|s| s.name.eq_ignore_ascii_case("Standard")) {
        d.table_styles.insert(0, TableStyle::default());
        changed = true;
    }
    let linetypes = Names::of(d.linetypes.iter().map(|l| l.name.as_str()));
    for l in &mut d.layers {
        if !linetypes.has(&l.linetype) {
            l.linetype = "Continuous".into();
            changed = true;
        }
    }
    let styles = Names::of(d.text_styles.iter().map(|s| s.name.as_str()));
    for s in &mut d.dim_styles {
        if !styles.has(&s.text_style) {
            s.text_style = "Standard".into();
            changed = true;
        }
    }
    for s in &mut d.mleader_styles {
        if !styles.has(&s.text_style) {
            s.text_style = "Standard".into();
            changed = true;
        }
    }
    changed
}

/// The current layer, linetype and styles (CLAYER…) name records that exist.
pub(crate) fn header_names(d: &mut Drawing) -> bool {
    let mut changed = false;
    let vars: [(&str, &str); 6] = [
        ("CLAYER", "0"),
        ("CELTYPE", "ByLayer"),
        ("TEXTSTYLE", "Standard"),
        ("DIMSTYLE", "Standard"),
        ("CMLEADERSTYLE", "Standard"),
        ("CTABLESTYLE", "Standard"),
    ];
    for (var, default) in vars {
        let Some(name) = d.header.get(var).map(|v| v.as_str().map(str::to_owned)) else { continue };
        let exists = name.as_deref().is_some_and(|n| match var {
            "CLAYER" => d.layer(n).is_some(),
            "CELTYPE" => n.eq_ignore_ascii_case("ByLayer") || n.eq_ignore_ascii_case("ByBlock") || d.linetype(n).is_some(),
            "TEXTSTYLE" => d.text_style(n).is_some(),
            "DIMSTYLE" => d.dim_style(n).is_some(),
            "CMLEADERSTYLE" => d.mleader_styles.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
            _ => d.table_styles.iter().any(|s| s.name.eq_ignore_ascii_case(n)),
        });
        if !exists {
            d.header.set_str(var, default);
            changed = true;
        }
    }
    changed
}

/// Block references whose definition is gone get an empty one.
fn missing_blocks(d: &mut Drawing) -> bool {
    let mut missing: BTreeSet<String> = BTreeSet::new();
    for e in all_entities(d) {
        if let EntityKind::Insert(i) = &e.kind
            && d.block(&i.block).is_none()
        {
            missing.insert(i.block.clone());
        }
    }
    let changed = !missing.is_empty();
    for name in missing {
        d.blocks.insert(name.clone(), Arc::new(Block::new(&name)));
    }
    changed
}

/// Applies `fix` to every entity of every store, replacing those it changed (in place: the draw
/// order stays). Answers whether it changed any.
fn fix_entities(d: &mut Drawing, fix: &dyn Fn(&Entity) -> Option<Entity>) -> bool {
    fn store(s: &mut EntityStore, fix: &dyn Fn(&Entity) -> Option<Entity>) -> bool {
        let fixed: Vec<Entity> = s.iter().filter_map(|e| fix(e)).collect();
        let changed = !fixed.is_empty();
        for e in fixed {
            s.replace(e);
        }
        changed
    }
    let mut changed = store(&mut d.model, fix);
    for l in &mut d.layouts {
        changed |= store(&mut l.entities, fix);
    }
    for b in d.blocks.values_mut() {
        if b.entities.iter().any(|e| fix(e).is_some()) {
            changed |= store(&mut Arc::make_mut(b).entities, fix);
        }
    }
    changed
}

/// Entities' layers, linetypes, styles and dimension blocks name records that exist.
fn entity_references(d: &mut Drawing) -> bool {
    let layers = Names::of(d.layers.iter().map(|l| l.name.as_str()));
    let linetypes = Names::of(d.linetypes.iter().map(|l| l.name.as_str()));
    let text = Names::of(d.text_styles.iter().map(|s| s.name.as_str()));
    let dims = Names::of(d.dim_styles.iter().map(|s| s.name.as_str()));
    let mleaders = Names::of(d.mleader_styles.iter().map(|s| s.name.as_str()));
    let tables = Names::of(d.table_styles.iter().map(|s| s.name.as_str()));
    let blocks = Names::of(d.blocks.keys().map(String::as_str));
    let fix = |e: &Entity| -> Option<Entity> {
        let mut f = e.clone();
        if f.common.layer.trim().is_empty() || !layers.has(&f.common.layer) {
            f.common.layer = "0".into();
        }
        let lt = &f.common.linetype;
        if !lt.eq_ignore_ascii_case("ByLayer") && !lt.eq_ignore_ascii_case("ByBlock") && !linetypes.has(lt) {
            f.common.linetype = "ByLayer".into();
        }
        let style = |s: &mut String| {
            if !text.has(s) {
                *s = "Standard".into();
            }
        };
        match &mut f.kind {
            EntityKind::Text(t) => style(&mut t.style),
            EntityKind::MText(t) => style(&mut t.style),
            EntityKind::AttDef(a) => style(&mut a.text.style),
            EntityKind::Insert(i) => i.attribs.iter_mut().for_each(|a| style(&mut a.text.style)),
            EntityKind::MLeader(m) => {
                if let Some(t) = &mut m.text {
                    style(&mut t.style);
                }
                if !mleaders.has(&m.style) {
                    m.style = "Standard".into();
                }
            }
            EntityKind::Table(t) if !tables.has(&t.style) => {
                t.style = "Standard".into();
            }
            EntityKind::Leader(l) if !dims.has(&l.style) => {
                l.style = "Standard".into();
            }
            EntityKind::Dimension(dim) => {
                if !dims.has(&dim.style) {
                    dim.style = "Standard".into();
                }
                if dim.block.as_ref().is_some_and(|b| !blocks.has(b)) {
                    dim.block = None;
                }
            }
            EntityKind::Viewport(vp) => {
                vp.frozen_layers.retain(|n| layers.has(n));
                vp.layer_colors.retain(|(n, _)| layers.has(n));
            }
            _ => {}
        }
        (f != *e).then_some(f)
    };
    fix_entities(d, &fix)
}

/// References to entities that are gone: dimension associations, constraints, group members and
/// parametric exceptions.
fn handle_references(d: &mut Drawing, all: &HashSet<Handle>) -> bool {
    let fix = |e: &Entity| -> Option<Entity> {
        let EntityKind::Dimension(dim) = &e.kind else { return None };
        let gone = |a: &kition_cad_doc::DimAssoc| {
            !all.contains(&a.handle) || matches!(a.snap, kition_cad_doc::AssocSnap::Intersection { other } if !all.contains(&other))
        };
        if !dim.assoc.iter().any(gone) {
            return None;
        }
        let mut f = e.clone();
        if let EntityKind::Dimension(dim) = &mut f.kind {
            dim.assoc.retain(|a| !gone(a));
        }
        Some(f)
    };
    let mut changed = fix_entities(d, &fix);
    let before = d.constraints.len();
    d.constraints.retain(|c| c.refs.iter().all(|r| all.contains(&r.handle)));
    changed |= d.constraints.len() != before;
    for g in &mut d.groups {
        let n = g.members.len();
        g.members.retain(|h| all.contains(h));
        changed |= g.members.len() != n;
    }
    let n = d.groups.len();
    d.groups.retain(|g| !g.members.is_empty());
    changed |= d.groups.len() != n;
    let settings = &mut d.parametric.settings;
    for list in [&mut settings.bar_exceptions, &mut settings.dim_exceptions] {
        let n = list.len();
        list.retain(|h| all.contains(h));
        changed |= list.len() != n;
    }
    changed
}

/// Block definitions that contain themselves (directly or through other blocks) lose the
/// reference that closes the loop (the engine's AUDIT rule; names resolve as `Drawing::block`
/// does: exactly, else without case).
fn block_cycles(d: &mut Drawing) -> bool {
    let names: Vec<String> = d.blocks.keys().cloned().collect();
    let exact: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let folded: HashMap<String, usize> = names.iter().enumerate().rev().map(|(i, n)| (n.to_ascii_lowercase(), i)).collect();
    let lookup = |n: &str| exact.get(n).or_else(|| folded.get(&n.to_ascii_lowercase())).copied();
    let edges: Vec<Vec<(Handle, usize)>> = names
        .iter()
        .map(|n| {
            d.blocks
                .get(n)
                .map(|b| {
                    b.entities
                        .iter()
                        .filter_map(|e| match &e.kind {
                            EntityKind::Insert(i) => lookup(&i.block).map(|t| (e.handle, t)),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    // Iterative depth-first search: 0 unvisited, 1 on the current path, 2 done.
    let mut state = vec![0u8; names.len()];
    let mut cuts: Vec<(usize, Handle)> = Vec::new();
    for root in 0..names.len() {
        if state.get(root) != Some(&0) {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        if let Some(s) = state.get_mut(root) {
            *s = 1;
        }
        while let Some((node, next)) = stack.last_mut() {
            let node = *node;
            match edges.get(node).and_then(|es| es.get(*next)) {
                Some(&(h, to)) => {
                    *next += 1;
                    match state.get(to) {
                        Some(1) => cuts.push((node, h)),
                        Some(0) => {
                            if let Some(s) = state.get_mut(to) {
                                *s = 1;
                            }
                            stack.push((to, 0));
                        }
                        _ => {}
                    }
                }
                None => {
                    if let Some(s) = state.get_mut(node) {
                        *s = 2;
                    }
                    stack.pop();
                }
            }
        }
    }
    let changed = !cuts.is_empty();
    for (from, h) in cuts {
        if let Some(block) = names.get(from).and_then(|n| d.blocks.get_mut(n)) {
            Arc::make_mut(block).entities.remove(h);
        }
    }
    changed
}
