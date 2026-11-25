//! `Drawing` -> replica: a full write (a new replica, an import) and an incremental diff (the
//! engine command bridge) that writes only what changed.
//!
//! Both run the same passes, a full write being a diff from nothing:
//! 1. every new entity gets its object first (ids chosen up front, so a dimension associated
//!    with a line drawn in the same command can name it);
//! 2. entities `after` no longer holds anywhere are deleted (one moved to another store, a block
//!    made from model space objects, keeps its object); then every named table, block, layout,
//!    header variable and constraint is written: new records created and keyed in their map,
//!    gone ones removed, changed ones rewritten;
//! 3. every entity register that differs is set, and every store's list made to hold `after`'s
//!    draw order (lists are aligned by object identity, so bringing an entity to the front is
//!    one move, not a rewrite).
//!
//! The entity stores are copy-on-write: a store `after` shares with `before` is skipped whole,
//! and within a changed store only the chunks it no longer shares are compared, so a command
//! that moves one entity of a large drawing looks at that entity's chunk only.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_cad_doc::{Block, Constraint, Drawing, Entity, EntityStore, Handle, Layout};
use kition_kit::kdm::{ObjectId, Tx};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::cells::{identify_cells, write_cells};
use crate::ids::{Ids, Name, Slot, Table, fold_key, names, new_key};
use crate::list::{assign_orders, sync_list};
use crate::refs::{each_parametric_list, each_ref_handle, handle_out, handles_out, hval_out, kind_out};
use crate::{KdmResult, all_entities, is_current_var, kdm_err, root_id};

/// Writes a whole drawing into a fresh replica's creating transaction. Answers the `Drawing`
/// object's id. Every entity gets a minted object id (never one derived from its handle: two
/// replicas built from one file are two documents). Errs if the drawing holds a handle twice
/// ([`crate::dedupe_handles`] fixes that) or a number that is not finite.
pub fn write_document(tx: &mut Tx<'_>, d: &Drawing, ids: &mut Ids) -> KdmResult<ObjectId> {
    let root = root_id();
    tx.create_with_id("Drawing", &root, Map::new())?;
    *ids = Ids::default();
    check_unique(d)?;
    Writer { tx, ids, full: true, fresh: HashSet::new(), wrote: false }.write(None, d)?;
    Ok(root)
}

/// Writes only what differs between `before` (the drawing `ids` describe) and `after`. Answers
/// whether it wrote anything. Errs, writing nothing usable, if `after` gives one handle to two
/// entities or holds a number that is not finite.
pub fn write_changes(tx: &mut Tx<'_>, before: &Drawing, after: &Drawing, ids: &mut Ids) -> KdmResult<bool> {
    let mut w = Writer { tx, ids, full: false, fresh: HashSet::new(), wrote: false };
    w.write(Some(before), after)?;
    Ok(w.wrote)
}

/// Errs if two entities of `d` share a handle: a replica names entities by handle.
pub(crate) fn check_unique(d: &Drawing) -> KdmResult<()> {
    let mut seen = HashSet::new();
    for e in all_entities(d) {
        if !seen.insert(e.handle) {
            return Err(kdm_err(format!("the drawing holds handle {} twice; refusing to write it", e.handle.hex())));
        }
    }
    Ok(())
}

/// A value as written, refusing NaN and infinity (JSON has neither: they would read back as
/// `null` and the value would be lost).
pub(crate) fn value_of(v: &impl Serialize) -> KdmResult<Value> {
    if !crate::finite::is_finite(v) {
        return Err(kdm_err("the drawing holds an infinite or undefined number; it cannot be shared"));
    }
    serde_json::to_value(v).map(crate::compare::numbers).map_err(kdm_err)
}

fn fields(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

/// Which store a list of entities is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Owner {
    Model,
    Layout(Name),
    Block(String),
}

/// One entity store, before and after.
struct Pair<'d> {
    owner: Owner,
    before: Option<&'d EntityStore>,
    after: Option<&'d EntityStore>,
}

impl Pair<'_> {
    fn changed(&self) -> bool {
        match (self.before, self.after) {
            (Some(b), Some(a)) => !b.same_as(a),
            _ => true,
        }
    }
}

/// The entities of `store` in chunks `other` does not share (all of them without `other`).
fn unshared<'a>(other: Option<&EntityStore>, store: &'a EntityStore) -> impl Iterator<Item = &'a Arc<Entity>> + 'a {
    let shared: HashSet<usize> = other.map(|o| o.chunk_slices().map(|(key, _)| key).collect()).unwrap_or_default();
    store.chunk_slices().filter(move |(key, _)| !shared.contains(key)).flat_map(|(_, items)| items.iter())
}

fn same_order(a: &EntityStore, b: &EntityStore) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.handle == y.handle)
}

/// A layout's `data`: everything but its entities (serde skips them).
fn layout_data(l: &Layout) -> KdmResult<Value> {
    value_of(l)
}

/// A block's `data`: everything but its entities (serde skips them).
fn block_data(b: &Block) -> KdmResult<Value> {
    value_of(b)
}

pub(crate) fn common_out(e: &Entity) -> KdmResult<Value> {
    value_of(&e.common)
}

pub(crate) fn entity_description(e: &Entity) -> String {
    // The text body has its own schema path. Duplicating it in the stored label makes
    // one text replacement into two field edits and prevents exact environment replay.
    format!("{} on layer {}", e.kind.type_name(), e.common.layer)
}

/// A constraint edit can change an entity's solve premises without changing its geometry.
/// Store the witness on that entity so incremental derived-value readers refresh it too.
pub(crate) fn refresh_constraint_state(tx: &mut Tx<'_>, d: &Drawing, ids: &Ids) -> KdmResult<bool> {
    let mut wrote = false;
    for e in all_entities(d) {
        let Some(object) = ids.entities.object(e.handle) else { continue };
        let fact = kition_cad_constraints::premise(d, &[e.handle]);
        if tx.get(object, "constraintState")? != fact {
            tx.set(object, "constraintState", fact)?;
            wrote = true;
        }
    }
    Ok(wrote)
}

pub(crate) fn parametric_out(p: &kition_cad_doc::Parametric, ids: &Ids) -> KdmResult<Value> {
    let mut v = value_of(p)?;
    each_parametric_list(&mut v, &mut |list| handles_out(list, ids));
    Ok(v)
}

fn constraint_refs(c: &Constraint, ids: &Ids) -> KdmResult<Value> {
    let mut v = value_of(&c.refs)?;
    each_ref_handle(&mut v, &mut |slot| {
        if let Some(h) = slot.as_u64() {
            *slot = handle_out(Handle(h), ids);
        }
    });
    Ok(v)
}

fn constraint_fields(c: &Constraint, ids: &Ids) -> KdmResult<Vec<(&'static str, Value)>> {
    Ok(vec![("id", json!(c.id)), ("kind", value_of(&c.kind)?), ("refs", constraint_refs(c, ids)?), ("name", json!(c.name)), ("expr", json!(c.expr))])
}

/// A named record's `data` (a group's members as object ids).
fn record_data<T: Serialize>(table: Table, record: &T, ids: &Ids) -> KdmResult<Value> {
    let mut v = value_of(record)?;
    if table == Table::Groups
        && let Some(members) = v.get_mut("members")
    {
        handles_out(members, ids);
    }
    Ok(v)
}

struct Writer<'w, 'tx> {
    tx: &'w mut Tx<'tx>,
    ids: &'w mut Ids,
    /// A full write: everything is new.
    full: bool,
    /// Objects this write created: written in full, whatever `before` held.
    fresh: HashSet<ObjectId>,
    wrote: bool,
}

impl Writer<'_, '_> {
    fn write(&mut self, before: Option<&Drawing>, after: &Drawing) -> KdmResult<()> {
        let mut identified = after.clone();
        identified.ensure_record_ids();
        identify_cells(&mut identified);
        let after = &identified;
        self.rekey_records(after)?;
        let pairs = store_pairs(before, after);
        let changed: Vec<&Pair<'_>> = pairs.iter().filter(|p| p.changed()).collect();

        // 1. Objects for new entities.
        let mut visited: HashSet<Handle> = HashSet::new();
        let mut new: Vec<&Arc<Entity>> = Vec::new();
        for pair in &changed {
            let Some(a) = pair.after else { continue };
            for e in unshared(pair.before, a) {
                if !visited.insert(e.handle) {
                    return Err(kdm_err(format!("handle {} is given to two objects; refusing to write the change", e.handle.hex())));
                }
                if self.ids.entities.object(e.handle).is_none() {
                    new.push(e);
                }
            }
        }
        let mut created = Vec::with_capacity(new.len());
        for e in &new {
            let object = ObjectId::new();
            self.ids.entities.insert(object.clone(), e.handle);
            created.push(object);
        }
        for (e, object) in new.iter().zip(created) {
            let f = fields(vec![
                ("handle", json!(e.handle.hex())),
                ("description", json!(entity_description(e))),
                ("common", common_out(e)?),
                ("kind", kind_out(&e.kind, self.ids)?),
            ]);
            self.tx.create_with_id("Entity", &object, f)?;
            write_cells(self.tx, &object, e.handle, None, Some(&e.kind))?;
            self.fresh.insert(object);
            self.wrote = true;
        }

        // 2. Entities gone from every store.
        let mut everywhere: Option<HashSet<Handle>> = None;
        for pair in &changed {
            let Some(b) = pair.before else { continue };
            for e in unshared(pair.after, b) {
                if pair.after.is_some_and(|a| a.contains(e.handle)) {
                    continue;
                }
                let all = everywhere.get_or_insert_with(|| all_entities(after).map(|x| x.handle).collect());
                if all.contains(&e.handle) {
                    continue;
                }
                if let Some(object) = self.ids.entities.remove_local(e.handle) {
                    write_cells(self.tx, &object, e.handle, Some(&e.kind), None)?;
                    if self.tx.exists(&object) {
                        self.tx.delete(&object)?;
                    }
                    self.wrote = true;
                }
            }
        }

        // Everything that names entities, now that the ids of deleted ones are gone (a reference to
        // one is written as the handle it was).
        self.write_tables(before, after)?;
        self.write_blocks(before.map(|b| &b.blocks), &after.blocks)?;
        self.write_layouts(before.map(|b| b.layouts.as_slice()), &after.layouts)?;
        self.write_header(before, after)?;
        self.write_constraints(before.map(|b| b.constraints.as_slice()), &after.constraints)?;
        self.wrote |= refresh_constraint_state(self.tx, after, self.ids)?;

        // 3. Entity registers and draw order.
        let mut before_index: Option<HashMap<Handle, &Arc<Entity>>> = None;
        for pair in &changed {
            let Some(a) = pair.after else { continue };
            for e in unshared(pair.before, a) {
                let Some(object) = self.ids.entities.object(e.handle).cloned() else { continue };
                if self.fresh.contains(&object) {
                    continue;
                }
                let old = pair.before.and_then(|b| b.get(e.handle)).or_else(|| {
                    let b = before?;
                    before_index.get_or_insert_with(|| all_entities(b).map(|x| (x.handle, x)).collect()).get(&e.handle).copied()
                });
                if old.is_some_and(|o| Arc::ptr_eq(o, e)) {
                    continue;
                }
                if old.is_none_or(|o| o.common != e.common) {
                    self.tx.set(&object, "common", common_out(e)?)?;
                    self.wrote = true;
                }
                if old.is_none_or(|o| o.kind != e.kind) {
                    // A table whose cell texts alone changed keeps its grid register as it is, so
                    // a concurrent change of the grid is not overwritten.
                    let kind = kind_out(&e.kind, self.ids)?;
                    if old.is_none_or(|o| kind_out(&o.kind, self.ids).ok().as_ref() != Some(&kind)) {
                        self.tx.set(&object, "kind", kind)?;
                    }
                    write_cells(self.tx, &object, e.handle, old.map(|o| &o.kind), Some(&e.kind))?;
                    self.wrote = true;
                }
                if old.is_none_or(|o| entity_description(o) != entity_description(e)) {
                    self.tx.set(&object, "description", json!(entity_description(e)))?;
                    self.wrote = true;
                }
            }
            if pair.before.is_some_and(|b| same_order(b, a)) {
                continue;
            }
            let (owner, field) = match &pair.owner {
                Owner::Model => (root_id(), "model"),
                Owner::Layout(name) => (self.slot_object(self.ids.layouts.get(name))?, "object.query"),
                Owner::Block(name) => (self.slot_object(self.ids.blocks.get(name))?, "object.query"),
            };
            let target: Vec<ObjectId> = a.iter().filter_map(|e| self.ids.entities.object(e.handle).cloned()).collect();
            self.wrote |= sync_list(self.tx, &owner, field, &target)? > 0;
        }
        for ((table, (name, nth)), slot) in &self.ids.records {
            if self.tx.get(&slot.object, "name")?.as_str() != Some(name) {
                self.tx.set(&slot.object, "name", json!(name))?;
                self.wrote = true;
            }
            let id = after.record_id(&table.ty().to_ascii_lowercase(), name, *nth);
            if self.tx.get(&slot.object, "id")?.as_str() != id {
                self.tx.set(&slot.object, "id", json!(id))?;
                self.wrote = true;
            }
        }
        for (name, slot) in &self.ids.blocks {
            if self.tx.get(&slot.object, "name")?.as_str() != Some(name) {
                self.tx.set(&slot.object, "name", json!(name))?;
                self.wrote = true;
            }
            let id = after.record_id("block", name, 0);
            if self.tx.get(&slot.object, "id")?.as_str() != id {
                self.tx.set(&slot.object, "id", json!(id))?;
                self.wrote = true;
            }
        }
        for ((name, nth), slot) in &self.ids.layouts {
            if self.tx.get(&slot.object, "name")?.as_str() != Some(name) {
                self.tx.set(&slot.object, "name", json!(name))?;
                self.wrote = true;
            }
            let id = after.record_id("layout", name, *nth);
            if self.tx.get(&slot.object, "id")?.as_str() != id {
                self.tx.set(&slot.object, "id", json!(id))?;
                self.wrote = true;
            }
        }
        // Issued record identities only grow: each is its own key, so identities two people
        // issue at once both stay issued.
        if before.is_none_or(|b| b.record_ids_used != after.record_ids_used) {
            let root = root_id();
            for id in &after.record_ids_used {
                if before.is_some_and(|b| b.record_ids_used.contains(id)) {
                    continue;
                }
                if self.tx.map_get(&root, "recordIdsUsed", id)?.is_null() {
                    self.tx.map_set(&root, "recordIdsUsed", id, json!(true))?;
                    self.wrote = true;
                }
            }
        }
        Ok(())
    }

    /// Match by persisted identity before comparing names. A rename keeps its KDM object,
    /// even when two equal-valued records exchange names.
    fn rekey_records(&mut self, after: &Drawing) -> KdmResult<()> {
        let mut nth = HashMap::<(String, String), usize>::new();
        let mut wanted = HashMap::new();
        for (ty, name) in after.record_names() {
            let n = nth.entry((ty.clone(), name.clone())).or_default();
            if let Some(id) = after.record_id(&ty, &name, *n) {
                wanted.insert((ty, id.to_owned()), (name, *n));
            }
            *n += 1;
        }
        // Two people making an unnamed group at once both name it `*A1` and both store the
        // identity `*A1`, so a merged replica can hold two records with one stored identity. The
        // drawing keeps that identity on the first of them and gives the others fresh ones
        // (`ensure_record_ids`). Each record keeps its own object: the slot read at the place
        // the identity now names keeps it, and a slot whose identity another slot took moves to
        // the fresh identity at its own place. Neither is dropped or written a second time.
        let mut slots = Vec::new();
        for ((table, name), slot) in std::mem::take(&mut self.ids.records) {
            let ty = table.ty().to_ascii_lowercase();
            let id = self.tx.get(&slot.object, "id")?.as_str().unwrap_or(&name.0).to_owned();
            slots.push((table, name, slot, ty, id));
        }
        // The slot already at its target goes first, whatever order the map gave.
        slots.sort_by_key(|(_, name, _, ty, id)| wanted.get(&(ty.clone(), id.clone())) != Some(name));
        let stored: HashSet<(String, String)> = slots.iter().map(|(_, _, _, ty, id)| (ty.clone(), id.clone())).collect();
        let mut shared = Vec::new();
        for (table, name, slot, ty, id) in slots {
            match wanted.get(&(ty, id)) {
                Some(new) if !self.ids.records.contains_key(&(table, new.clone())) => {
                    self.ids.records.insert((table, new.clone()), slot);
                }
                Some(_) => shared.push((table, name, slot)),
                None => self.remove_entry(table.field(), &slot)?,
            }
        }
        for (table, name, slot) in shared {
            let ty = table.ty().to_ascii_lowercase();
            let fresh = after.record_id(&ty, &name.0, name.1).is_some_and(|id| !stored.contains(&(ty.clone(), id.to_owned())));
            if fresh && !self.ids.records.contains_key(&(table, name.clone())) {
                self.ids.records.insert((table, name), slot);
            } else {
                self.remove_entry(table.field(), &slot)?;
            }
        }
        for (name, slot) in std::mem::take(&mut self.ids.blocks) {
            let id = self.tx.get(&slot.object, "id")?.as_str().unwrap_or(&name).to_owned();
            if let Some((new, _)) = wanted.get(&("block".into(), id)) {
                self.ids.blocks.insert(new.clone(), slot);
            } else {
                self.remove_entry("blocks", &slot)?;
            }
        }
        for (name, slot) in std::mem::take(&mut self.ids.layouts) {
            let id = self.tx.get(&slot.object, "id")?.as_str().unwrap_or(&name.0).to_owned();
            if let Some(new) = wanted.get(&("layout".into(), id)) {
                self.ids.layouts.insert(new.clone(), slot);
            } else {
                self.remove_entry("layouts", &slot)?;
            }
        }
        Ok(())
    }

    fn slot_object(&self, slot: Option<&Slot>) -> KdmResult<ObjectId> {
        slot.map(|s| s.object.clone()).ok_or_else(|| kdm_err("a store the write just made is missing from the id maps"))
    }

    /// Removes a record's map entry (if it still names the record) and its object.
    fn remove_entry(&mut self, field: &str, slot: &Slot) -> KdmResult<()> {
        let root = root_id();
        if self.tx.map_get(&root, field, &slot.key)?.as_str() == Some(slot.object.as_str()) {
            self.tx.map_delete(&root, field, &slot.key)?;
        }
        if self.tx.exists(&slot.object) {
            self.tx.delete(&slot.object)?;
        }
        self.wrote = true;
        Ok(())
    }

    fn write_tables(&mut self, before: Option<&Drawing>, after: &Drawing) -> KdmResult<()> {
        let b = before;
        self.write_table(Table::Layers, b.map(|d| d.layers.as_slice()), &after.layers, |r| &r.name)?;
        self.write_table(Table::Linetypes, b.map(|d| d.linetypes.as_slice()), &after.linetypes, |r| &r.name)?;
        self.write_table(Table::TextStyles, b.map(|d| d.text_styles.as_slice()), &after.text_styles, |r| &r.name)?;
        self.write_table(Table::DimStyles, b.map(|d| d.dim_styles.as_slice()), &after.dim_styles, |r| &r.name)?;
        self.write_table(Table::MLeaderStyles, b.map(|d| d.mleader_styles.as_slice()), &after.mleader_styles, |r| &r.name)?;
        self.write_table(Table::TableStyles, b.map(|d| d.table_styles.as_slice()), &after.table_styles, |r| &r.name)?;
        self.write_table(Table::Views, b.map(|d| d.views.as_slice()), &after.views, |r| &r.name)?;
        self.write_table(Table::Ucss, b.map(|d| d.ucss.as_slice()), &after.ucss, |r| &r.name)?;
        self.write_table(Table::LayerStates, b.map(|d| d.layer_states.as_slice()), &after.layer_states, |r| &r.name)?;
        self.write_table(Table::Groups, b.map(|d| d.groups.as_slice()), &after.groups, |r| &r.name)?;
        self.write_table(Table::PlotStyleTables, b.map(|d| d.plot_style_tables.as_slice()), &after.plot_style_tables, |r| &r.name)
    }

    /// One named table: records matched by name (a rename is a delete and an add), each record's
    /// `data` rewritten when it changed, `order` numbers kept where the order allows.
    fn write_table<T: Serialize + PartialEq>(&mut self, table: Table, before: Option<&[T]>, after: &[T], name: fn(&T) -> &String) -> KdmResult<()> {
        let before = before.unwrap_or(&[]);
        let root = root_id();
        let after_names = names(after.iter().map(|r| name(r).as_str()));
        let before_names = names(before.iter().map(|r| name(r).as_str()));
        let wanted: HashSet<&Name> = after_names.iter().collect();
        for n in &before_names {
            if !wanted.contains(n)
                && let Some(slot) = self.ids.records.remove(&(table, n.clone()))
            {
                self.remove_entry(table.field(), &slot)?;
            }
        }
        let current: Vec<Option<f64>> = after_names.iter().map(|n| self.ids.records.get(&(table, n.clone())).map(|s| s.order)).collect();
        let orders = assign_orders(&current);
        let mut used = self.ids.keys(table);
        for ((n, record), order) in after_names.iter().zip(after).zip(orders) {
            match self.ids.records.get(&(table, n.clone())).cloned() {
                Some(mut slot) => {
                    let data = record_data(table, record, self.ids)?;
                    self.write_record_data(table, &slot.object, data)?;
                    if slot.order != order {
                        self.tx.set(&slot.object, "order", json!(order))?;
                        slot.order = order;
                        self.ids.records.insert((table, n.clone()), slot);
                        self.wrote = true;
                    }
                }
                None => {
                    let object = ObjectId::new();
                    let key = new_key(table, &n.0, &object, &mut used);
                    let data = record_data(table, record, self.ids)?;
                    let mut created = vec![("order", json!(order))];
                    if table != Table::Groups {
                        created.push(("data", data.clone()));
                    }
                    if table.named() {
                        created.push(("name", json!(name(record))));
                    }
                    self.tx.create_with_id(table.ty(), &object, fields(created))?;
                    if table == Table::Groups {
                        self.write_record_data(table, &object, data)?;
                    }
                    self.tx.map_set(&root, table.field(), &key, json!(object.as_str()))?;
                    self.fresh.insert(object.clone());
                    self.ids.records.insert((table, n.clone()), Slot { object, key, order });
                    self.wrote = true;
                }
            }
        }
        Ok(())
    }

    /// Group fields merge independently: pruning members must never rewrite a description.
    fn write_record_data(&mut self, table: Table, object: &ObjectId, data: Value) -> KdmResult<()> {
        if table == Table::Groups {
            let fields = data.as_object().ok_or_else(|| kdm_err("group data must be an object"))?;
            for (key, value) in fields {
                if self.tx.map_get(object, "data", key)? != *value {
                    self.tx.map_set(object, "data", key, value.clone())?;
                    self.wrote = true;
                }
            }
            for (key, _) in self.tx.map_entries(object, "data")? {
                if !fields.contains_key(&key) {
                    self.tx.map_delete(object, "data", &key)?;
                    self.wrote = true;
                }
            }
        } else if self.tx.get(object, "data")? != data {
            self.tx.set(object, "data", data)?;
            self.wrote = true;
        }
        Ok(())
    }

    /// Block definitions by name: new ones created and keyed, gone ones removed, changed
    /// properties rewritten. Their entity lists are written with the other stores.
    fn write_blocks(&mut self, before: Option<&BTreeMap<String, Arc<Block>>>, after: &BTreeMap<String, Arc<Block>>) -> KdmResult<()> {
        let root = root_id();
        if let Some(before) = before {
            for name in before.keys() {
                if !after.contains_key(name)
                    && let Some(slot) = self.ids.blocks.remove(name)
                {
                    self.remove_entry("blocks", &slot)?;
                }
            }
        }
        let mut used: HashSet<String> = self.ids.blocks.values().map(|s| s.key.clone()).collect();
        for (name, block) in after {
            let old = before.and_then(|b| b.get(name));
            match self.ids.blocks.get(name).cloned() {
                Some(slot) => {
                    if old.is_some_and(|o| Arc::ptr_eq(o, block)) {
                        continue;
                    }
                    let data = block_data(block)?;
                    if self.tx.get(&slot.object, "data")? != data {
                        self.tx.set(&slot.object, "data", data)?;
                        self.wrote = true;
                    }
                }
                None => {
                    let object = ObjectId::new();
                    let key = fold_key(name, &mut used);
                    self.tx.create_with_id("Block", &object, fields(vec![("data", block_data(block)?), ("name", json!(name))]))?;
                    self.tx.map_set(&root, "blocks", &key, json!(object.as_str()))?;
                    self.fresh.insert(object.clone());
                    self.ids.blocks.insert(name.clone(), Slot { object, key, order: 0.0 });
                    self.wrote = true;
                }
            }
        }
        Ok(())
    }

    /// Layouts by name, like a named table; their entity lists are written with the other stores.
    fn write_layouts(&mut self, before: Option<&[Layout]>, after: &[Layout]) -> KdmResult<()> {
        let root = root_id();
        let before = before.unwrap_or(&[]);
        let after_names = names(after.iter().map(|l| l.name.as_str()));
        let before_names = names(before.iter().map(|l| l.name.as_str()));
        let wanted: HashSet<&Name> = after_names.iter().collect();
        for n in &before_names {
            if !wanted.contains(n)
                && let Some(slot) = self.ids.layouts.remove(n)
            {
                self.remove_entry("layouts", &slot)?;
            }
        }
        let current: Vec<Option<f64>> = after_names.iter().map(|n| self.ids.layouts.get(n).map(|s| s.order)).collect();
        let orders = assign_orders(&current);
        let mut used: HashSet<String> = self.ids.layouts.values().map(|s| s.key.clone()).collect();
        for ((n, layout), order) in after_names.iter().zip(after).zip(orders) {
            match self.ids.layouts.get(n).cloned() {
                Some(mut slot) => {
                    let data = layout_data(layout)?;
                    if self.tx.get(&slot.object, "data")? != data {
                        self.tx.set(&slot.object, "data", data)?;
                        self.wrote = true;
                    }
                    if slot.order != order {
                        self.tx.set(&slot.object, "order", json!(order))?;
                        slot.order = order;
                        self.ids.layouts.insert(n.clone(), slot);
                        self.wrote = true;
                    }
                }
                None => {
                    let object = ObjectId::new();
                    let key = fold_key(&n.0, &mut used);
                    self.tx.create_with_id(
                        "Layout",
                        &object,
                        fields(vec![("order", json!(order)), ("data", layout_data(layout)?), ("name", json!(layout.name))]),
                    )?;
                    self.tx.map_set(&root, "layouts", &key, json!(object.as_str()))?;
                    self.fresh.insert(object.clone());
                    self.ids.layouts.insert(n.clone(), Slot { object, key, order });
                    self.wrote = true;
                }
            }
        }
        Ok(())
    }

    /// Header variables, one map key each, but for each person's own current properties
    /// ([`crate::CURRENT_VARS`]); and the parametric settings.
    fn write_header(&mut self, before: Option<&Drawing>, after: &Drawing) -> KdmResult<()> {
        let root = root_id();
        let empty = BTreeMap::new();
        let old = before.map_or(&empty, |b| &b.header.vars);
        if self.full || *old != after.header.vars {
            for (key, v) in &after.header.vars {
                if is_current_var(key) {
                    continue;
                }
                if old.get(key) != Some(v) {
                    self.tx.map_set(&root, "header", key, hval_out(v))?;
                    self.wrote = true;
                }
            }
            for key in old.keys() {
                if !is_current_var(key) && !after.header.vars.contains_key(key) {
                    self.tx.map_delete(&root, "header", key)?;
                    self.wrote = true;
                }
            }
        }
        if before.is_none_or(|b| b.parametric != after.parametric) {
            let v = parametric_out(&after.parametric, self.ids)?;
            self.tx.set(&root, "parametric", v)?;
            self.wrote = true;
        }
        Ok(())
    }

    /// Constraints by their local id: one object each, in the root's list.
    fn write_constraints(&mut self, before: Option<&[Constraint]>, after: &[Constraint]) -> KdmResult<()> {
        let before = before.unwrap_or(&[]);
        if !self.full && before == after {
            return Ok(());
        }
        let wanted: HashSet<u32> = after.iter().map(|c| c.id).collect();
        for c in before {
            if !wanted.contains(&c.id)
                && let Some(object) = self.ids.constraints.remove_local(c.id)
            {
                if self.tx.exists(&object) {
                    self.tx.delete(&object)?;
                }
                self.wrote = true;
            }
        }
        let old: HashMap<u32, &Constraint> = before.iter().map(|c| (c.id, c)).collect();
        let mut seen = HashSet::new();
        for c in after {
            if !seen.insert(c.id) {
                return Err(kdm_err(format!("constraint {} is listed twice; refusing to write it", c.id)));
            }
            match self.ids.constraints.object(c.id).cloned() {
                Some(object) => {
                    let new_fields = constraint_fields(c, self.ids)?;
                    let old_fields = old.get(&c.id).map(|o| constraint_fields(o, self.ids)).transpose()?;
                    for (i, (field, v)) in new_fields.into_iter().enumerate() {
                        if old_fields.as_ref().and_then(|f| f.get(i)).map(|(_, v)| v) != Some(&v) {
                            self.tx.set(&object, field, v)?;
                            self.wrote = true;
                        }
                    }
                }
                None => {
                    let object = ObjectId::new();
                    self.tx.create_with_id("Constraint", &object, fields(constraint_fields(c, self.ids)?))?;
                    self.ids.constraints.insert(object.clone(), c.id);
                    self.fresh.insert(object);
                    self.wrote = true;
                }
            }
        }
        let order: Vec<ObjectId> = after.iter().filter_map(|c| self.ids.constraints.object(c.id).cloned()).collect();
        self.wrote |= sync_list(self.tx, &root_id(), "constraints", &order)? > 0;
        Ok(())
    }
}

/// Every entity store of `before` and `after`, paired by owner: model space, layouts by name,
/// blocks by name.
fn store_pairs<'d>(before: Option<&'d Drawing>, after: &'d Drawing) -> Vec<Pair<'d>> {
    let mut out = vec![Pair { owner: Owner::Model, before: before.map(|b| &b.model), after: Some(&after.model) }];
    let after_layouts: Vec<(Name, &Layout)> = names(after.layouts.iter().map(|l| l.name.as_str())).into_iter().zip(&after.layouts).collect();
    let before_layouts: HashMap<Name, &Layout> =
        before.map(|b| names(b.layouts.iter().map(|l| l.name.as_str())).into_iter().zip(&b.layouts).collect()).unwrap_or_default();
    for (name, layout) in &after_layouts {
        out.push(Pair { owner: Owner::Layout(name.clone()), before: before_layouts.get(name).map(|l| &l.entities), after: Some(&layout.entities) });
    }
    let kept: HashSet<&Name> = after_layouts.iter().map(|(n, _)| n).collect();
    for (name, layout) in &before_layouts {
        if !kept.contains(name) {
            out.push(Pair { owner: Owner::Layout(name.clone()), before: Some(&layout.entities), after: None });
        }
    }
    for (name, block) in &after.blocks {
        out.push(Pair {
            owner: Owner::Block(name.clone()),
            before: before.and_then(|b| b.blocks.get(name)).map(|b| &b.entities),
            after: Some(&block.entities),
        });
    }
    if let Some(b) = before {
        for (name, block) in &b.blocks {
            if !after.blocks.contains_key(name) {
                out.push(Pair { owner: Owner::Block(name.clone()), before: Some(&block.entities), after: None });
            }
        }
    }
    out
}
