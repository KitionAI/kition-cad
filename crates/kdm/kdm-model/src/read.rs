//! Replica -> `Drawing`, in two passes: the first walks every entity list (skipping entries whose
//! object is gone, every object after its first listing, and every handle after its first holder)
//! so the second can translate every handle stored as data while it reads the registers.
//! Constraint ids are minted locally: the one [`Known`] remembers for an object, else a fresh one
//! above every id the view ever used.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_cad_doc as doc;
use kition_cad_doc::{Block, Common, Constraint, Drawing, Entity, EntityKind, EntityStore, Handle, Header, Layout, Parametric};
use kition_kit::kdm::{ActorId, KdmError, ObjectId, Replica};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ids::{Ids, Known, Slot, Table, names};
use crate::refs::{each_parametric_list, each_ref_handle, handle_in, handles_in, hval_in, kind_in};
use crate::source::Source;
use crate::{KdmResult, kdm_err, next_handle, root_id};

/// The entities of the last read, by object: a reload reuses those whose object did not change.
#[derive(Clone, Debug, Default)]
pub(crate) struct EntityCache(HashMap<ObjectId, Arc<Entity>>);

impl EntityCache {
    /// The entities of a view, by the objects `ids` maps their handles to.
    pub(crate) fn of(d: &Drawing, ids: &Ids) -> Self {
        Self(crate::all_entities(d).filter_map(|e| ids.entities.object(e.handle).map(|o| (o.clone(), e.clone()))).collect())
    }
}

/// Reads the whole drawing with fresh local ids.
pub fn read_document(replica: &Replica) -> KdmResult<(Drawing, Ids)> {
    let (d, ids, _) = read_with(replica, Some(replica.actor()), &Known::default(), None)?;
    Ok((d, ids))
}

/// Reads the whole drawing, giving every constraint `known` remembers the local id it had.
pub fn read_document_known(replica: &Replica, known: &Known) -> KdmResult<(Drawing, Ids)> {
    let (d, ids, _) = read_with(replica, Some(replica.actor()), known, None)?;
    Ok((d, ids))
}

/// One entity store's listed entities, in order.
type Listed = Vec<(ObjectId, Handle)>;

/// A record object's map entry: key, object, order.
pub(crate) struct Entry {
    pub key: String,
    pub object: ObjectId,
    pub order: f64,
}

/// `null` (a field never written) reads as `T`'s default.
fn from<T: DeserializeOwned + Default>(value: Value) -> KdmResult<T> {
    if value.is_null() { Ok(T::default()) } else { serde_json::from_value(value).map_err(kdm_err) }
}

/// The live objects of type `ty` a root map names, sorted by (`order`, key) (blocks: by key).
pub(crate) fn entries<S: Source>(src: &S, field: &str, ty: &str) -> KdmResult<Vec<Entry>> {
    let root = root_id();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (key, value) in src.entries(&root, field)? {
        let Value::String(object) = value else { continue };
        let object = ObjectId::from_string(object);
        if !src.exists(&object) || src.type_of(&object).as_deref() != Some(ty) || !seen.insert(object.clone()) {
            continue;
        }
        let order = if ty == "Block" { 0.0 } else { src.get(&object, "order")?.as_f64().filter(|o| o.is_finite()).unwrap_or(0.0) };
        out.push(Entry { key, object, order });
    }
    out.sort_by(|a, b| a.order.total_cmp(&b.order).then_with(|| a.key.cmp(&b.key)));
    Ok(out)
}

struct Reader<'s, S: Source> {
    src: &'s S,
    seen: HashSet<ObjectId>,
    handles: HashSet<Handle>,
    ids: Ids,
}

impl<S: Source> Reader<'_, S> {
    /// The list's entity objects that exist, were not listed before, and whose handle no earlier
    /// entity holds; each is mapped to its handle.
    fn listed(&mut self, owner: &ObjectId, field: &str) -> KdmResult<Listed> {
        let mut out = Vec::new();
        for object in self.src.list(owner, field)? {
            if !self.src.exists(&object) || self.seen.contains(&object) {
                continue;
            }
            let Some(handle) = self.src.get(&object, "handle")?.as_str().and_then(Handle::parse_hex) else { continue };
            if !self.handles.insert(handle) {
                continue;
            }
            self.seen.insert(object.clone());
            self.ids.entities.insert(object.clone(), handle);
            out.push((object, handle));
        }
        Ok(out)
    }

    fn entity(&self, object: &ObjectId, handle: Handle) -> KdmResult<Entity> {
        let common: Common = from(self.src.get(object, "common")?)?;
        let kind: EntityKind = kind_in(self.src.get(object, "kind")?, &self.ids)?;
        Ok(Entity { handle, common, kind })
    }

    fn table<T: DeserializeOwned>(&mut self, table: Table, name: fn(&T) -> &String) -> KdmResult<Vec<T>> {
        let mut records = Vec::new();
        let mut slots = Vec::new();
        for entry in entries(self.src, table.field(), table.ty())? {
            let mut data = if table == Table::Groups {
                Value::Object(self.src.entries(&entry.object, "data")?.into_iter().collect())
            } else {
                self.src.get(&entry.object, "data")?
            };
            if table == Table::Groups
                && let Some(members) = data.get_mut("members")
            {
                handles_in(members, &self.ids);
            }
            // A record that does not read (written by a newer kit) is left out of the view.
            let Ok(record) = serde_json::from_value::<T>(data) else { continue };
            records.push(record);
            slots.push(Slot { object: entry.object, key: entry.key, order: entry.order });
        }
        for (n, slot) in names(records.iter().map(|r| name(r).as_str())).into_iter().zip(slots) {
            self.ids.records.insert((table, n), slot);
        }
        Ok(records)
    }
}

/// Forgets the entities of a store the view leaves out.
fn forget(ids: &mut Ids, listed: &Listed) {
    for (object, _) in listed {
        ids.entities.remove(object);
    }
}

pub(crate) fn read_with<S: Source>(
    src: &S,
    actor: Option<ActorId>,
    known: &Known,
    reuse: Option<(&EntityCache, &BTreeSet<ObjectId>)>,
) -> KdmResult<(Drawing, Ids, EntityCache)> {
    let root = root_id();
    if !src.exists(&root) {
        return Err(KdmError::NotFound(root.to_string()));
    }
    let mut r = Reader { src, seen: HashSet::new(), handles: HashSet::new(), ids: Ids::default() };

    // Pass 1: the stores and their entities' handles.
    let model = r.listed(&root, "model")?;
    let layout_entries = entries(src, "layouts", "Layout")?;
    let mut layouts_listed = Vec::with_capacity(layout_entries.len());
    for entry in &layout_entries {
        layouts_listed.push(r.listed(&entry.object, "entities")?);
    }
    let block_entries = entries(src, "blocks", "Block")?;
    let mut blocks_listed = Vec::with_capacity(block_entries.len());
    for entry in &block_entries {
        blocks_listed.push(r.listed(&entry.object, "entities")?);
    }

    // Which cached entities still read the same: those whose object did not change, unless an
    // entity was deleted or got another handle (a cached dimension's associations would be stale).
    let mut cache = EntityCache::default();
    let (old, changed) = match reuse {
        Some((old, changed)) => (Some(old), Some(changed)),
        None => (None, None),
    };
    let stale_refs = match (old, changed) {
        (Some(old), Some(changed)) => changed.iter().any(|o| old.0.get(o).is_some_and(|e| r.ids.entities.local(o) != Some(e.handle))),
        _ => false,
    };

    // Pass 2: the entities.
    let mut build = |r: &mut Reader<'_, S>, listed: &Listed| -> KdmResult<EntityStore> {
        let mut store = EntityStore::new();
        for (object, handle) in listed {
            let cached = match (old, changed) {
                (Some(old), Some(changed)) if !changed.contains(object) => old
                    .0
                    .get(object)
                    .filter(|e| e.handle == *handle && !(stale_refs && matches!(&e.kind, EntityKind::Dimension(d) if !d.assoc.is_empty()))),
                _ => None,
            };
            let entity = match cached {
                Some(e) => e.clone(),
                None => match r.entity(object, *handle) {
                    Ok(e) => Arc::new(e),
                    // An entity that does not read (written by a newer kit) is left out of the view.
                    Err(_) => {
                        r.ids.entities.remove(object);
                        continue;
                    }
                },
            };
            cache.0.insert(object.clone(), entity.clone());
            store.push(Arc::unwrap_or_clone(entity));
        }
        Ok(store)
    };
    let model = build(&mut r, &model)?;
    let mut layouts = Vec::with_capacity(layout_entries.len());
    let mut layout_slots = Vec::with_capacity(layout_entries.len());
    for (entry, listed) in layout_entries.iter().zip(&layouts_listed) {
        let data = src.get(&entry.object, "data")?;
        let Ok(mut layout) = serde_json::from_value::<Layout>(data) else {
            forget(&mut r.ids, listed);
            continue;
        };
        layout.entities = build(&mut r, listed)?;
        layouts.push(layout);
        layout_slots.push(Slot { object: entry.object.clone(), key: entry.key.clone(), order: entry.order });
    }
    for (n, slot) in names(layouts.iter().map(|l| l.name.as_str())).into_iter().zip(layout_slots) {
        r.ids.layouts.insert(n, slot);
    }
    let mut blocks: BTreeMap<String, Arc<Block>> = BTreeMap::new();
    for (entry, listed) in block_entries.iter().zip(&blocks_listed) {
        let data = src.get(&entry.object, "data")?;
        let Ok(mut block) = serde_json::from_value::<Block>(data) else {
            forget(&mut r.ids, listed);
            continue;
        };
        if blocks.contains_key(&block.name) {
            forget(&mut r.ids, listed);
            continue;
        }
        block.entities = build(&mut r, listed)?;
        r.ids.blocks.insert(block.name.clone(), Slot { object: entry.object.clone(), key: entry.key.clone(), order: 0.0 });
        blocks.insert(block.name.clone(), Arc::new(block));
    }

    // The tables, the header, the parametric settings, the constraints.
    let layers = r.table::<doc::Layer>(Table::Layers, |x| &x.name)?;
    let linetypes = r.table::<doc::Linetype>(Table::Linetypes, |x| &x.name)?;
    let text_styles = r.table::<doc::TextStyle>(Table::TextStyles, |x| &x.name)?;
    let dim_styles = r.table::<doc::DimStyle>(Table::DimStyles, |x| &x.name)?;
    let mleader_styles = r.table::<doc::MLeaderStyle>(Table::MLeaderStyles, |x| &x.name)?;
    let table_styles = r.table::<doc::TableStyle>(Table::TableStyles, |x| &x.name)?;
    let views = r.table::<doc::NamedView>(Table::Views, |x| &x.name)?;
    let ucss = r.table::<doc::Ucs>(Table::Ucss, |x| &x.name)?;
    let layer_states = r.table::<doc::LayerState>(Table::LayerStates, |x| &x.name)?;
    let groups = r.table::<doc::Group>(Table::Groups, |x| &x.name)?;
    let plot_style_tables = r.table::<doc::PlotStyleTable>(Table::PlotStyleTables, |x| &x.name)?;

    let mut header = Header::default();
    for (key, value) in src.entries(&root, "header")? {
        if let Some(v) = hval_in(value) {
            header.vars.insert(key, v);
        }
    }
    let mut parametric = src.get(&root, "parametric")?;
    each_parametric_list(&mut parametric, &mut |list| handles_in(list, &r.ids));
    let parametric: Parametric = from(parametric).unwrap_or_default();

    let mut constraints = Vec::new();
    let mut used: HashSet<u32> = HashSet::new();
    let mut next = known.max;
    let mut seen_constraints = HashSet::new();
    for object in src.list(&root, "constraints")? {
        if !src.exists(&object) || !seen_constraints.insert(object.clone()) {
            continue;
        }
        let mut refs = src.get(&object, "refs")?;
        let mut resolved = true;
        each_ref_handle(&mut refs, &mut |slot| match handle_in(slot, &r.ids) {
            Some(h) => *slot = serde_json::json!(h.0),
            None => resolved = false,
        });
        // A constraint on an entity a merge deleted is not shown (the repair removes it).
        if !resolved {
            continue;
        }
        let (Ok(kind), Ok(refs)) = (serde_json::from_value(src.get(&object, "kind")?), serde_json::from_value(refs)) else { continue };
        let text = |field: &str| -> KdmResult<String> { Ok(src.get(&object, field)?.as_str().unwrap_or_default().to_owned()) };
        let stored = src.get(&object, "id")?.as_u64().and_then(|n| u32::try_from(n).ok());
        let id = match stored.or_else(|| known.constraints.get(&object).copied()) {
            Some(id) if id > 0 && used.insert(id) => id,
            _ => loop {
                next = next.saturating_add(1);
                if used.insert(next) || next == u32::MAX {
                    break next;
                }
            },
        };
        r.ids.constraints.insert(object.clone(), id);
        constraints.push(Constraint { id, kind, refs, name: text("name")?, expr: text("expr")? });
    }

    let mut d = Drawing {
        record_ids: BTreeMap::new(),
        record_ids_used: src.get(&root, "recordIdsUsed")?.as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default(),
        header,
        layers,
        linetypes,
        text_styles,
        dim_styles,
        mleader_styles,
        table_styles,
        blocks,
        model,
        layouts,
        views,
        ucss,
        layer_states,
        groups,
        plot_style_tables,
        constraints,
        parametric,
        handseed: 0,
    };
    for ((table, (name, nth)), slot) in &r.ids.records {
        if let Some(id) = src.get(&slot.object, "id")?.as_str() {
            d.record_ids.insert(Drawing::record_key(&table.ty().to_ascii_lowercase(), name, *nth), id.to_owned());
        }
    }
    for (name, slot) in &r.ids.blocks {
        if let Some(id) = src.get(&slot.object, "id")?.as_str() {
            d.record_ids.insert(Drawing::record_key("block", name, 0), id.to_owned());
        }
    }
    for ((name, nth), slot) in &r.ids.layouts {
        if let Some(id) = src.get(&slot.object, "id")?.as_str() {
            d.record_ids.insert(Drawing::record_key("layout", name, *nth), id.to_owned());
        }
    }
    d.handseed = match actor {
        Some(actor) => next_handle(actor, &d),
        None => crate::all_entities(&d).map(|e| e.handle.0).max().map_or(0x100, |h| h.saturating_add(1).max(0x100)),
    };
    Ok((d, r.ids, cache))
}
