//! Local id bookkeeping. [`Ids`] maps what a view holds to its objects: entities by handle
//! (handles are replicated, so this is the only map that names an object by replicated data),
//! named records, blocks and layouts by name, constraints by their local id. [`Known`] remembers
//! the constraint ids a view ever gave an object, so a reload after an undo, redo or merge gives
//! a surviving or restored constraint the id it had.

use std::collections::{HashMap, HashSet};

use kition_cad_doc::{Drawing, Handle};
use kition_kit::kdm::ObjectId;
use kition_kit::session::IdMap;

/// A named table of the drawing: an object type, and the root map that names its records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Table {
    Layers,
    Linetypes,
    TextStyles,
    DimStyles,
    MLeaderStyles,
    TableStyles,
    Views,
    Ucss,
    LayerStates,
    Groups,
    PlotStyleTables,
}

impl Table {
    pub const ALL: [Table; 11] = [
        Table::Layers,
        Table::Linetypes,
        Table::TextStyles,
        Table::DimStyles,
        Table::MLeaderStyles,
        Table::TableStyles,
        Table::Views,
        Table::Ucss,
        Table::LayerStates,
        Table::Groups,
        Table::PlotStyleTables,
    ];

    /// The root's map field.
    pub fn field(self) -> &'static str {
        match self {
            Table::Layers => "layers",
            Table::Linetypes => "linetypes",
            Table::TextStyles => "textStyles",
            Table::DimStyles => "dimStyles",
            Table::MLeaderStyles => "mleaderStyles",
            Table::TableStyles => "tableStyles",
            Table::Views => "views",
            Table::Ucss => "ucss",
            Table::LayerStates => "layerStates",
            Table::Groups => "groups",
            Table::PlotStyleTables => "plotStyleTables",
        }
    }

    /// Whether the records carry their name as their tool id (`name`, KitObjectModel OM9):
    /// every named table has its own prefix in the shared tool id space.
    pub fn named(self) -> bool {
        true
    }

    /// The records' object type.
    pub fn ty(self) -> &'static str {
        match self {
            Table::Layers => "Layer",
            Table::Linetypes => "Linetype",
            Table::TextStyles => "TextStyle",
            Table::DimStyles => "DimStyle",
            Table::MLeaderStyles => "MLeaderStyle",
            Table::TableStyles => "TableStyle",
            Table::Views => "View",
            Table::Ucss => "Ucs",
            Table::LayerStates => "LayerState",
            Table::Groups => "Group",
            Table::PlotStyleTables => "PlotStyleTable",
        }
    }
}

/// Where a named record (or block, or layout) lives: its object, its key in the root's map, and
/// its `order` (records and layouts; blocks are sorted by name).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Slot {
    pub object: ObjectId,
    pub key: String,
    pub order: f64,
}

/// A record's identity in a view: its exact name, and which occurrence of that name it is (a
/// file may hold one name twice; each keeps its own object).
pub(crate) type Name = (String, usize);

/// The objects behind a view's entities, named records, blocks, layouts and constraints.
#[derive(Clone, Debug, Default)]
pub struct Ids {
    /// Entities by handle.
    pub entities: IdMap<Handle>,
    /// Constraints by their local id.
    pub constraints: IdMap<u32>,
    pub(crate) records: HashMap<(Table, Name), Slot>,
    pub(crate) blocks: HashMap<String, Slot>,
    pub(crate) layouts: HashMap<Name, Slot>,
}

impl Ids {
    /// The object of the `nth` record named `name` (exactly) in `table`.
    pub fn record(&self, table: Table, name: &str, nth: usize) -> Option<&ObjectId> {
        self.records.get(&(table, (name.to_owned(), nth))).map(|s| &s.object)
    }

    pub fn block(&self, name: &str) -> Option<&ObjectId> {
        self.blocks.get(name).map(|s| &s.object)
    }

    pub fn layout(&self, name: &str) -> Option<&ObjectId> {
        self.layouts.get(&(name.to_owned(), 0)).map(|s| &s.object)
    }

    /// The object of the entity with handle `h`.
    pub fn entity(&self, h: Handle) -> Option<&ObjectId> {
        self.entities.object(h)
    }

    /// The keys a table's records hold in the root's map.
    pub(crate) fn keys(&self, table: Table) -> HashSet<String> {
        self.records.iter().filter(|((t, _), _)| *t == table).map(|(_, s)| s.key.clone()).collect()
    }
}

/// Object ids to the local constraint ids a view has ever given them. See [`Known::remember`].
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub(crate) constraints: HashMap<ObjectId, u32>,
    /// The highest constraint id ever handed out, so fresh ids never reuse one.
    pub(crate) max: u32,
}

impl Known {
    /// Remembers the object each of `d`'s constraints maps to.
    pub fn remember(&mut self, d: &Drawing, ids: &Ids) {
        for c in &d.constraints {
            self.max = self.max.max(c.id);
            if let Some(object) = ids.constraints.object(c.id) {
                self.constraints.insert(object.clone(), c.id);
            }
        }
    }
}

/// Each record's identity, in order: (exact name, occurrence of that name so far).
pub(crate) fn names<'a>(names: impl Iterator<Item = &'a str>) -> Vec<Name> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    names
        .map(|n| {
            let nth = seen.entry(n).or_insert(0);
            let out = (n.to_owned(), *nth);
            *nth += 1;
            out
        })
        .collect()
}

/// The map key a new record named `name` takes: the name folded to lower case (the drawing
/// compares names that way, so two people adding one name meet on one key), with `#2`, `#3`…
/// when the view already uses it (a file holding names that differ only in case). An unnamed
/// group (`*A1`…: the engine numbers them, so two people grouping at once would pick the same
/// name) is keyed by its object instead, so both groups survive.
pub(crate) fn new_key(table: Table, name: &str, object: &ObjectId, used: &mut HashSet<String>) -> String {
    if table == Table::Groups && name.starts_with('*') {
        let key = format!("*{}", object.as_str());
        used.insert(key.clone());
        return key;
    }
    fold_key(name, used)
}

/// `name` folded to lower case, with `#2`, `#3`… when `used` already holds it.
pub(crate) fn fold_key(name: &str, used: &mut HashSet<String>) -> String {
    let base = name.to_ascii_lowercase();
    let mut key = base.clone();
    let mut n = 2usize;
    while used.contains(&key) {
        key = format!("{base}#{n}");
        n += 1;
    }
    used.insert(key.clone());
    key
}
