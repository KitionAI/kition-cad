//! The Kition CAD drawing database.
//!
//! A [`Drawing`] holds the header variables, symbol tables (layers, linetypes, styles), block
//! definitions, model space and paper-space layouts. Entity collections are copy-on-write
//! ([`EntityStore`]), so a whole-drawing clone is cheap and serves as an undo snapshot.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod constraint;
mod entity;
mod extents;
mod header;
pub mod library;
mod plotstyle;
mod store;
mod tables;

use std::collections::BTreeMap;

pub use constraint::*;
pub use entity::*;
pub use extents::{MAX_BLOCK_DEPTH, entity_bounds};
pub use header::{HVal, Header};
pub use kition_cad_color as color;
pub use kition_cad_geom as geom;
pub use plotstyle::*;
pub use store::EntityStore;
pub use tables::*;

/// The header variable holding the dimension scale one person draws new dimensions at, once
/// making a dimension style current or editing the current one set it ([`Drawing::sync_dim_vars`]).
/// It is each person's own (never replicated, never written to a file); the drawing's `DIMSCALE`
/// stays shared because existing dimensions whose style's scale is 0 are drawn at it.
pub const OWN_DIMSCALE: &str = "KITION_CAD_DIMSCALE";

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("no such layer `{0}`")]
    NoLayer(String),
    #[error("no such block `{0}`")]
    NoBlock(String),
    #[error("no such entity {0:?}")]
    NoEntity(Handle),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, DocError>;

/// Which entity collection: model space or a named layout's paper space.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Space {
    #[default]
    Model,
    Paper(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    /// Persistent identities of named records, independent of their current spelling.
    pub record_ids: BTreeMap<String, String>,
    /// Previously issued identities are never reused after deletion or rename.
    pub record_ids_used: std::collections::BTreeSet<String>,
    pub header: Header,
    pub layers: Vec<Layer>,
    pub linetypes: Vec<Linetype>,
    pub text_styles: Vec<TextStyle>,
    pub dim_styles: Vec<DimStyle>,
    pub mleader_styles: Vec<MLeaderStyle>,
    pub table_styles: Vec<TableStyle>,
    pub blocks: BTreeMap<String, std::sync::Arc<Block>>,
    pub model: EntityStore,
    pub layouts: Vec<Layout>,
    pub views: Vec<NamedView>,
    pub ucss: Vec<Ucs>,
    pub layer_states: Vec<LayerState>,
    pub groups: Vec<Group>,
    /// Plot style tables kept with the drawing (STYLESMANAGER); they shadow built-in tables of
    /// the same name.
    pub plot_style_tables: Vec<PlotStyleTable>,
    /// Parametric constraints (GEOMCONSTRAINT / DIMCONSTRAINT).
    pub constraints: Vec<Constraint>,
    /// User parameters and parametric settings.
    pub parametric: Parametric,
    /// Next free handle.
    pub handseed: u64,
}

impl Default for Drawing {
    fn default() -> Self {
        Drawing::new_imperial()
    }
}

impl Drawing {
    /// An empty drawing with the standard tables (inch-based, like acad.dwt-style defaults).
    pub fn new_imperial() -> Self {
        let mut d = Drawing::bare();
        d.header = Header::imperial();
        d
    }
    /// An empty metric drawing (millimetres, ISO-25 dimensions, millimetre-sized Standard
    /// multileader and table styles).
    pub fn new_metric() -> Self {
        let mut d = Drawing::bare();
        d.header = Header::metric();
        d.mleader_styles = vec![MLeaderStyle::metric()];
        d.table_styles = vec![TableStyle::metric()];
        d.dim_styles.push(DimStyle::iso25());
        d.header.set_str("DIMSTYLE", "ISO-25");
        d
    }
    fn bare() -> Self {
        Drawing {
            record_ids: BTreeMap::new(),
            record_ids_used: std::collections::BTreeSet::new(),
            header: Header::default(),
            layers: vec![Layer::default(), Layer { name: "Defpoints".into(), plot: false, ..Layer::default() }],
            linetypes: vec![
                Linetype { name: "ByBlock".into(), description: String::new(), pattern: Vec::new() },
                Linetype { name: "ByLayer".into(), description: String::new(), pattern: Vec::new() },
                Linetype::continuous(),
            ],
            text_styles: vec![TextStyle::default()],
            dim_styles: vec![DimStyle::default()],
            mleader_styles: vec![MLeaderStyle::default()],
            table_styles: vec![TableStyle::default()],
            blocks: BTreeMap::new(),
            model: EntityStore::new(),
            layouts: vec![Layout::new("Layout1", 1), Layout::new("Layout2", 2)],
            views: Vec::new(),
            ucss: Vec::new(),
            layer_states: Vec::new(),
            groups: Vec::new(),
            plot_style_tables: Vec::new(),
            constraints: Vec::new(),
            parametric: Parametric::default(),
            handseed: 0x100,
        }
    }

    /// Allocate a fresh handle.
    pub fn new_handle(&mut self) -> Handle {
        let h = Handle(self.handseed);
        self.handseed = self.handseed.saturating_add(1);
        h
    }

    /// Live named records in table order. The occurrence distinguishes malformed duplicate names.
    pub fn record_names(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        macro_rules! table {
            ($ty:literal, $records:expr) => {
                out.extend($records.iter().map(|r| ($ty.to_owned(), r.name.clone())));
            };
        }
        table!("layer", self.layers);
        table!("linetype", self.linetypes);
        table!("textstyle", self.text_styles);
        table!("dimensionStyle.open", self.dim_styles);
        table!("multileaderStyle.open", self.mleader_styles);
        table!("tableStyle.open", self.table_styles);
        table!("view", self.views);
        table!("ucs", self.ucss);
        table!("layerstate", self.layer_states);
        table!("group", self.groups);
        table!("plotstyletable", self.plot_style_tables);
        table!("layout", self.layouts);
        out.extend(self.blocks.keys().map(|name| ("block".to_owned(), name.clone())));
        out
    }

    pub fn record_key(ty: &str, name: &str, nth: usize) -> String {
        // JSON framing avoids collisions with names containing punctuation.
        serde_json::json!([ty, name, nth]).to_string()
    }

    pub fn record_id(&self, ty: &str, name: &str, nth: usize) -> Option<&str> {
        self.record_ids.get(&Self::record_key(ty, name, nth)).map(String::as_str)
    }

    /// Called after an edit and on import. Initial ids may be readable names; once issued,
    /// an id stays with that record, and a new record cannot claim an old id.
    pub fn ensure_record_ids(&mut self) {
        let mut seen = BTreeMap::<(String, String), usize>::new();
        let mut live = std::collections::BTreeSet::new();
        let mut identities = std::collections::BTreeSet::new();
        for (ty, name) in self.record_names() {
            let nth = seen.entry((ty.clone(), name.clone())).or_default();
            let key = Self::record_key(&ty, &name, *nth);
            *nth += 1;
            live.insert(key.clone());
            if let Some(id) = self.record_ids.get(&key)
                && identities.insert(format!("{ty}:{id}"))
            {
                self.record_ids_used.insert(format!("{ty}:{id}"));
                continue;
            }
            let mut id = name.clone();
            // At most one candidate per issued id, plus a fresh candidate.
            for suffix in 0..=self.record_ids_used.len() {
                if !self.record_ids_used.contains(&format!("{ty}:{id}")) {
                    break;
                }
                id = format!("{name}~{suffix}");
            }
            self.record_ids_used.insert(format!("{ty}:{id}"));
            identities.insert(format!("{ty}:{id}"));
            self.record_ids.insert(key, id);
        }
        self.record_ids.retain(|key, _| live.contains(key));
    }

    /// Rename implementations call this with the names they actually resolved.
    pub fn rename_record_id(&mut self, ty: &str, from: &str, to: &str) {
        if let Some(id) = self.record_ids.remove(&Self::record_key(ty, from, 0)) {
            self.record_ids.insert(Self::record_key(ty, to, 0), id);
        }
    }
    /// Make sure future handles don't collide with `h` (after loading a file).
    pub fn bump_handseed(&mut self, h: Handle) {
        if h.0 >= self.handseed {
            self.handseed = h.0.saturating_add(1);
        }
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn layer_mut(&mut self, name: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn linetype(&self, name: &str) -> Option<&Linetype> {
        self.linetypes.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn text_style(&self, name: &str) -> Option<&TextStyle> {
        self.text_styles.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn dim_style(&self, name: &str) -> Option<&DimStyle> {
        self.dim_styles.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    /// The value of a DIM* header variable this person draws with: `DIMSCALE` is their own
    /// ([`OWN_DIMSCALE`]) once making a style current or editing it set it, else the drawing's;
    /// every other variable is the header's.
    pub fn my_dim_var(&self, var: &str) -> Option<&HVal> {
        if var.eq_ignore_ascii_case("DIMSCALE") {
            self.header.get(OWN_DIMSCALE).or_else(|| self.header.get("DIMSCALE"))
        } else {
            self.header.get(var)
        }
    }

    /// The DIMSCALE new objects take ([`Drawing::my_dim_var`]), 1 when there is none.
    pub fn my_dimscale(&self) -> f64 {
        self.my_dim_var("DIMSCALE").and_then(HVal::as_f64).filter(|v| v.is_finite()).unwrap_or(1.0)
    }

    /// The DIM* variables in the header that differ from the current dimension style, as
    /// [`DimStyle`] fields: AutoCAD's style overrides (set with SETVAR), which new dimensions
    /// take on.
    pub fn dim_overrides(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut out = serde_json::Map::new();
        let Some(st) = self.dim_style(&self.header.str("DIMSTYLE", "Standard")) else { return out };
        let base = serde_json::to_value(st).unwrap_or_default();
        for (var, field) in DIMVARS {
            let Some(v) = self.my_dim_var(var).and_then(|h| serde_json::to_value(h).ok()) else { continue };
            let mut cand = st.clone();
            if !cand.apply_fields(&serde_json::Map::from_iter([((*field).to_string(), v)])).is_empty() {
                continue;
            }
            let Some(cv) = serde_json::to_value(&cand).ok().and_then(|c| c.get(*field).cloned()) else { continue };
            let same = match (base.get(*field), &cv) {
                // Text style and arrow block names compare without case.
                (Some(serde_json::Value::String(a)), serde_json::Value::String(b))
                    if matches!(*field, "textStyle" | "arrowBlock" | "arrowBlock1" | "arrowBlock2") =>
                {
                    a.eq_ignore_ascii_case(b)
                }
                (b, cv) => b == Some(cv),
            };
            if !same {
                out.insert((*field).to_string(), cv);
            }
        }
        out
    }

    /// Set the header's DIM* variables to the current dimension style's values, clearing the
    /// overrides (as making a style current or editing it does). Variables the header doesn't
    /// carry stay absent; ones that can't hold the style's value are removed. These are the
    /// person's own settings: `DIMSCALE` goes to [`OWN_DIMSCALE`], never to the drawing's
    /// `DIMSCALE`, which existing dimensions whose style's scale is 0 are drawn at.
    pub fn sync_dim_vars(&mut self) {
        let Some(st) = self.dim_style(&self.header.str("DIMSTYLE", "Standard")) else { return };
        let v = serde_json::to_value(st).unwrap_or_default();
        for (name, field) in DIMVARS {
            let var = if *name == "DIMSCALE" { OWN_DIMSCALE } else { *name };
            let Some(old) = self.my_dim_var(name) else { continue };
            let new = match (old, v.get(*field)) {
                (HVal::Int(_), Some(serde_json::Value::Bool(b))) => Some(HVal::Int(i64::from(*b))),
                (HVal::Int(_), Some(serde_json::Value::Number(n))) => {
                    n.as_i64().or_else(|| n.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 1e15).map(|f| f as i64)).map(HVal::Int)
                }
                (HVal::Real(_), Some(serde_json::Value::Number(n))) => n.as_f64().map(HVal::Real),
                (HVal::Str(_), Some(serde_json::Value::String(s))) => Some(HVal::Str(s.clone())),
                _ => None,
            };
            match new {
                Some(h) => self.header.set(var, h),
                None => {
                    self.header.vars.remove(var);
                }
            }
        }
    }
    pub fn block(&self, name: &str) -> Option<&std::sync::Arc<Block>> {
        self.blocks.get(name).or_else(|| self.blocks.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v))
    }
    pub fn layout(&self, name: &str) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    /// Add a layer if missing; returns whether it was created.
    pub fn ensure_layer(&mut self, name: &str) -> bool {
        if self.layer(name).is_some() {
            return false;
        }
        self.layers.push(Layer::new(name));
        true
    }

    pub fn space(&self, space: &Space) -> Option<&EntityStore> {
        match space {
            Space::Model => Some(&self.model),
            Space::Paper(n) => self.layouts.iter().find(|l| &l.name == n).map(|l| &l.entities),
        }
    }
    pub fn space_mut(&mut self, space: &Space) -> Option<&mut EntityStore> {
        match space {
            Space::Model => Some(&mut self.model),
            Space::Paper(n) => self.layouts.iter_mut().find(|l| &l.name == n).map(|l| &mut l.entities),
        }
    }

    /// Find an entity in model space or any layout.
    pub fn entity(&self, h: Handle) -> Option<&std::sync::Arc<Entity>> {
        self.model.get(h).or_else(|| self.layouts.iter().find_map(|l| l.entities.get(h)))
    }
    pub fn space_of(&self, h: Handle) -> Option<Space> {
        if self.model.contains(h) {
            return Some(Space::Model);
        }
        self.layouts.iter().find(|l| l.entities.contains(h)).map(|l| Space::Paper(l.name.clone()))
    }
    pub fn modify_entity<F: FnOnce(&mut Entity)>(&mut self, h: Handle, f: F) -> Result<()> {
        let sp = self.space_of(h).ok_or(DocError::NoEntity(h))?;
        let store = self.space_mut(&sp).ok_or(DocError::NoEntity(h))?;
        if store.modify(h, f) { Ok(()) } else { Err(DocError::NoEntity(h)) }
    }
    pub fn remove_entity(&mut self, h: Handle) -> Option<std::sync::Arc<Entity>> {
        let sp = self.space_of(h)?;
        self.space_mut(&sp)?.remove(h)
    }

    /// Add an entity to a space with a new handle, using the given common properties.
    pub fn add(&mut self, space: &Space, common: Common, kind: EntityKind) -> Result<Handle> {
        let h = self.new_handle();
        self.ensure_layer(&common.layer);
        let store = self.space_mut(space).ok_or_else(|| DocError::Invalid(format!("no such space {space:?}")))?;
        store.push(Entity { handle: h, common, kind });
        Ok(h)
    }

    /// Effective visibility of an entity (its own flag, its layer on/thawed).
    pub fn is_visible(&self, e: &Entity) -> bool {
        e.common.visible && self.layer(&e.common.layer).is_none_or(Layer::visible)
    }

    /// Bounds of all visible entities in a space.
    pub fn extents(&self, space: &Space) -> kition_cad_geom::Bounds2 {
        let mut b = kition_cad_geom::Bounds2::EMPTY;
        if let Some(store) = self.space(space) {
            for e in store.iter() {
                if self.is_visible(e) {
                    b = b.union(&entity_bounds(self, e, 0));
                }
            }
        }
        b
    }

    /// Total entity count across model and paper space.
    pub fn entity_count(&self) -> usize {
        self.model.len() + self.layouts.iter().map(|l| l.entities.len()).sum::<usize>()
    }

    /// Next unused anonymous block name with the given prefix (e.g. `*D`).
    pub fn anonymous_block_name(&self, prefix: &str) -> String {
        let mut i = 1usize;
        loop {
            let n = format!("{prefix}{i}");
            if self.block(&n).is_none() || i > 1_000_000 {
                return n;
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests;
