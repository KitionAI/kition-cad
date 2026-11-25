//! Per-call facts read by the audited command families. Other commands keep the SDK default.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use kition_cad_doc::{Common, DimKind, Drawing, Entity, EntityKind, Handle, Space};
use kition_cad_engine::{Session, select};
use kition_cad_geom::{Bounds2, Vec2};
use kition_kit::{Carried, Carries, Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, KitCommand, Premise, RelativeOrder, Under};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn handle(v: &Value) -> Option<Handle> {
    v.as_str().and_then(Handle::parse_hex).or_else(|| v.as_u64().map(Handle))
}

fn entity(d: &Drawing, h: Handle) -> Option<&Entity> {
    d.entity(h).map(AsRef::as_ref).or_else(|| d.blocks.values().find_map(|b| b.entities.get(h).map(AsRef::as_ref)))
}

fn all(d: &Drawing) -> impl Iterator<Item = &Entity> {
    d.model.iter().chain(d.layouts.iter().flat_map(|l| l.entities.iter())).chain(d.blocks.values().flat_map(|b| b.entities.iter())).map(AsRef::as_ref)
}

fn point(v: &Value) -> Option<Vec2> {
    let p = Vec2::new(v.get(0)?.as_f64()?, v.get(1)?.as_f64()?);
    p.is_finite().then_some(p)
}

fn tolerance(d: &Drawing, space: &Space) -> f64 {
    let ext = d.extents(space);
    let side = ext.width().max(ext.height());
    if !ext.is_empty() && side.is_finite() && side > 0.0 { side * crate::AGENT_TOLERANCE } else { 1.0 }
}

fn spatial(command: &str, p: &Value) -> bool {
    continuation(command, p)
        || implicit(command, p)
        || generic_pick(command) && p.get("pick").is_some()
        || command == "object.stretch" && p.get("window").is_some()
        || command == "select.set" && ["at", "window", "fence"].iter().any(|k| p.get(k).is_some())
        || matches!(command, "object.trim" | "object.extend") && ["fence", "crossing"].iter().any(|k| p.get(k).is_some())
}

fn continuation(command: &str, p: &Value) -> bool {
    command == "arc.addContinue" && (p.get("start").is_none() || p.get("direction").is_none())
}

fn implicit(command: &str, p: &Value) -> bool {
    matches!(command, "hatch.add" | "hatch.addGradient" | "boundary.add") && p.get("points").is_some()
        || matches!(
            command,
            "dimension.add"
                | "dimension.addLinear"
                | "dimension.addAligned"
                | "dimension.addRadius"
                | "dimension.addDiameter"
                | "dimension.addAngular"
                | "dimension.addArcLength"
                | "dimension.addOrdinate"
                | "dimension.addContinue"
                | "dimension.addBaseline"
                | "dimension.addQuick"
                | "dimension.reassociate"
        )
}

fn dependencies(s: &Session, d: &Drawing, command: &str, p: &Value) -> Vec<Handle> {
    if matches!(command, "hatch.add" | "hatch.addGradient" | "boundary.add") {
        return p
            .get("points")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(point)
            .flat_map(|p| kition_cad_engine::cmd::boundary_sources(d, &s.space(), p))
            .collect();
    }
    // Run the actual annotation command against an isolated drawing. Association selection
    // happens inside that command, before the post-command solver. No host/session is edited.
    let mut probe = Session::new();
    probe.open_drawing(d.clone(), "premise", None);
    probe.last_dim = s.last_dim;
    if let Ok(state) = probe.state_mut() {
        state.space = s.space();
        state.selection = s.selection();
    }
    let result = probe.execute(command, p);
    let mut hs = Vec::new();
    if matches!(command, "dimension.addContinue" | "dimension.addBaseline") {
        hs.extend(s.last_dim);
    }
    if result.is_ok()
        && let Ok(after) = probe.doc()
    {
        for e in after.model.iter().chain(after.layouts.iter().flat_map(|layout| layout.entities.iter())) {
            if let EntityKind::Dimension(dm) = &e.kind
                && d.entity(e.handle).is_none_or(|old| old.kind != e.kind)
            {
                hs.extend(dm.assoc.iter().map(|a| a.handle));
                hs.extend(dm.assoc.iter().filter_map(|a| match a.snap {
                    kition_cad_doc::AssocSnap::Intersection { other } => Some(other),
                    _ => None,
                }));
            }
        }
    }
    hs
}

/// Whether `command` deletes or replaces object `h` of its objects `hs` whole: TRIM's target
/// (pieces replace it), JOIN's and PEDIT join's joined objects, OFFSET's erased source.
fn destroys(command: &str, p: &Value, h: Handle, hs: &[Handle]) -> bool {
    let primary = p.get("handle").and_then(handle).or_else(|| hs.first().copied());
    match command {
        "object.join" => true,
        "polyline.edit" => {
            primary != Some(h) && p.get("option").and_then(Value::as_str).is_some_and(|o| matches!(o.to_ascii_lowercase().as_str(), "join" | "j"))
        }
        "object.offset" => p.get("erase") == Some(&Value::Bool(true)),
        // TRIM's targets (not its edges): picked, fenced or named, each is replaced by its pieces.
        "object.trim" => !crate::params::values(p, "edges").iter().filter_map(|v| v.as_array()).flatten().filter_map(handle).any(|e| e == h),
        // ARRAYPATH copies its sources whole; the path is read for its geometry.
        "object.arrayPath" => p.get("path").and_then(handle) != Some(h),
        _ => false,
    }
}

fn generic_pick(command: &str) -> bool {
    !matches!(command, "object.trim" | "object.extend" | "object.lengthen" | "object.copyNested")
}

fn picked(d: &Drawing, space: &Space, command: &str, p: &Value) -> Vec<Handle> {
    if continuation(command, p) {
        return kition_cad_engine::cmd::curves::last_curve_end(d, space).map(|(h, _, _)| h).into_iter().collect();
    }
    let at = p.get("pick").or_else(|| (command == "select.set").then(|| p.get("at")).flatten()).and_then(point);
    if let Some(at) = at.filter(|_| generic_pick(command)) {
        let ap = p.get("aperture").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).unwrap_or_else(|| tolerance(d, space));
        return select::pick(d, space, at, ap).into_iter().collect();
    }
    if matches!(command, "select.set" | "object.stretch" | "object.trim" | "object.extend") {
        if let Some(w) = p.get("window").or_else(|| p.get("crossing")).and_then(Value::as_array)
            && let Some((a, b)) = w.first().and_then(point).zip(w.get(1).and_then(point))
        {
            return select::select_window(
                d,
                space,
                Bounds2::new(a, b),
                matches!(command, "object.stretch" | "object.trim" | "object.extend") || p.get("crossing").and_then(Value::as_bool).unwrap_or(false),
            );
        }
        if let Some(f) = p.get("fence").and_then(Value::as_array) {
            return select::select_fence(d, space, &f.iter().filter_map(point).collect::<Vec<_>>());
        }
    }
    Vec::new()
}

pub fn declare(c: &mut KitCommand) {
    if c.id == "spline.add"
        && let Some(schema) = c.schema.as_mut().and_then(Value::as_object_mut)
    {
        schema.insert("object".into(), json!({"oneOf":[{"type":"string"},{"type":"boolean","enum":[true]}]}));
    }
    if c.id.starts_with("arc.add")
        && c.id != "arc.add"
        && let Some(schema) = c.schema.as_mut().and_then(Value::as_object_mut)
    {
        for key in ["start", "end", "center"] {
            if schema.contains_key(key) {
                schema.insert(key.into(), json!({"type":"array","items":{"type":"number"}}));
            }
        }
        if schema.contains_key("direction") {
            schema.insert("direction".into(), json!({"oneOf":[{"type":"number"},{"type":"array","items":{"type":"number"}}]}));
        }
    }
    // A creation/update by a record name is a typed place, including an absent
    // name. Replay the map rather than relying on case-sensitive SDK spelling.
    if matches!(c.id.as_str(), "textStyle.open" | "dimensionStyle.open" | "dimensionStyle.openManager" | "block.insert" | "layerState.save") {
        c.places = &["name"];
    }
    if matches!(c.id.as_str(), "layout.add" | "layout.duplicate" | "layout.manage") {
        c.places = &["name", "to"];
    }
    if c.id == "select.quick" {
        c.places = &["type", "layer", "color", "property", "operator", "value", "mode", "applyTo"];
    }
    if c.id == "edit.find" {
        c.places = &["find"];
    }
    if c.id == "arc.addContinue" {
        c.places = &["end"];
    }
    if matches!(
        c.id.as_str(),
        "hatch.add"
            | "hatch.addGradient"
            | "boundary.add"
            | "dimension.add"
            | "dimension.addLinear"
            | "dimension.addAligned"
            | "dimension.addRadius"
            | "dimension.addDiameter"
            | "dimension.addAngular"
            | "dimension.addArcLength"
            | "dimension.addOrdinate"
            | "dimension.addContinue"
            | "dimension.addBaseline"
            | "dimension.addQuick"
            | "dimension.reassociate"
    ) {
        c.places = &["points", "p1", "p2", "center", "point", "vertex", "feature"];
    }
    if c.targets.is_some() && generic_pick(&c.id) {
        if let Some(schema) = c.schema.as_mut().and_then(Value::as_object_mut) {
            schema.insert("pick".into(), json!({"type":"array","items":{"type":"number"},"description":"Instead of handles: a drawing-space point [x,y], picked by the engine in the call's layout. With basis, holds what was at that point."}));
            schema
                .insert("aperture".into(), json!({"type":"number","description":"Pick reach in drawing units; default 0.5% of the space extents."}));
        }
        c.places = if matches!(c.id.as_str(), "hatch.add" | "hatch.addGradient") {
            &["pick", "points"]
        } else if implicit(&c.id, &Value::Null) {
            &["pick", "points", "p1", "p2", "center", "point", "vertex", "feature"]
        } else {
            &["pick"]
        };
    }
    if c.id == "select.set" {
        c.places = &["pick", "at", "window", "fence"];
    }
    if c.id == "arrange.ungroup" {
        c.places = &["pick", "name"];
    }
    if c.id == "namedObject.rename" {
        c.places = &["table", "from"];
    }
    if c.id == "object.stretch" {
        c.places = &["window"];
    }
    if matches!(c.id.as_str(), "object.trim" | "object.extend") {
        c.places = &["fence", "crossing"];
    }
    if matches!(c.id.as_str(), "block.addDialog" | "block.add") {
        c.places = &["name", "pick"];
    }
}

pub fn resolve(s: &Session, command: &str, mut p: Value) -> Result<Value, String> {
    if p.get("pick").is_none() || !generic_pick(command) {
        return Ok(p);
    }
    let key = crate::acts::targets_any(command).ok_or("invalid-params: this command takes no pick")?;
    if p.get("handle").is_some() || p.get("handles").is_some() {
        return Err("invalid-params: pick and handles both name the target".into());
    }
    let d = s.doc().map_err(|e| e.to_string())?;
    if p.get("pick").and_then(point).is_none() {
        return Err("invalid-params: pick must be a finite [x,y]".into());
    }
    let hs = picked(d, &s.space(), command, &p);
    let h = hs.first().ok_or("not-found: no entity at pick")?;
    if let Some(map) = p.as_object_mut() {
        map.remove("pick");
        map.remove("aperture");
        map.insert(key.into(), if key == "handle" { json!(h.hex()) } else { json!([h.hex()]) });
    }
    Ok(p)
}

fn record_address<'a>(command: &str, p: &'a Value) -> Option<(&'static str, &'a str)> {
    let ty = match command {
        "layout.add" | "layout.duplicate" | "layout.set" => "layout",
        "block.addDialog" | "block.add" | "block.insert" => "block",
        "textStyle.open" => "textstyle",
        "layerState.save" => "layerstate",
        "dimensionStyle.open" | "dimensionStyle.openManager" => "dimstyle",
        _ => return None,
    };
    let name = p.get(if command == "layout.duplicate" { "to" } else { "name" }).and_then(Value::as_str)?;
    // Text/dimension style handlers trim names. BLOCK and INSERT preserve spaces.
    let name = if matches!(command, "textStyle.open" | "dimensionStyle.open" | "dimensionStyle.openManager" | "layout.add" | "layout.duplicate") {
        name.trim()
    } else {
        name
    };
    let name = if command == "block.insert" { name.strip_prefix('*').unwrap_or(name) } else { name };
    Some((ty, name))
}

fn need(id: impl Into<Value>, fields: Vec<String>) -> Premise {
    Premise::on(id).fields(fields).positions(Vec::<String>::new())
}

fn geometry_fields(kind: &EntityKind) -> Vec<String> {
    fn walk(v: &Value, path: String, out: &mut Vec<String>) {
        match v {
            // Vec3 is a fixed record (crates/support/geom/src/vec.rs): retain its parent
            // path rather than enumerating its three required scalar members.
            Value::Object(m) if m.len() == 3 && ["x", "y", "z"].iter().all(|key| m.get(*key).is_some_and(Value::is_number)) => out.push(path),
            Value::Object(m) => {
                for (k, v) in m {
                    if matches!(k.as_str(), "value" | "contents" | "text") && v.is_string() {
                        continue;
                    }
                    walk(v, format!("{path}.{k}"), out);
                }
            }
            _ => out.push(path),
        }
    }
    let mut out = Vec::new();
    if let Ok(v) = serde_json::to_value(kind) {
        walk(&v, "kind".into(), &mut out);
    }
    out
}

fn named_need(d: &Drawing, ty: &str, name: &str, fields: &[&str], out: &mut Vec<Premise>) {
    if let Some((id, _)) = named(d, ty, name) {
        out.push(need(id, fields.iter().map(|s| (*s).to_owned()).collect()));
    }
}

fn rename_type(p: &Value) -> &'static str {
    match p.get("table").and_then(Value::as_str).unwrap_or("layer").to_ascii_lowercase().as_str() {
        "layer" => "Layer",
        "linetype" | "ltype" => "Linetype",
        "style" | "textstyle" => "TextStyle",
        "dimstyle" => "DimStyle",
        "block" => "Block",
        "view" => "View",
        "ucs" => "Ucs",
        "mleaderstyle" | "multileaderstyle" => "MLeaderStyle",
        "tablestyle" => "TableStyle",
        _ => "",
    }
}

fn record_name_key(ty: &str, name: &str) -> String {
    format!("{ty}:{}", crate::scan::key(name))
}

fn record_names(d: &Drawing) -> Value {
    let mut names: BTreeMap<String, Value> = d
        .record_names()
        .into_iter()
        .filter_map(|(ty, name)| d.record_id(&ty, &name, 0).map(|id| (record_name_key(&ty, &name), json!(format!("{ty}:{id}")))))
        .collect();
    // Missing map paths are unknown to the SDK. Give implicitly consumed absent names an
    // explicit null so an unrelated new named record doesn't become a false refusal.
    for lt in kition_cad_doc::library::standard_linetypes() {
        names.entry(record_name_key("linetype", &lt.name)).or_insert(Value::Null);
    }
    for table in kition_cad_doc::BUILTIN_PLOT_STYLE_TABLES {
        names.entry(record_name_key("plotstyletable", table)).or_insert(Value::Null);
    }
    let layouts: BTreeSet<_> = d.layouts.iter().map(|l| l.name.to_ascii_lowercase()).collect();
    if let Some(name) = (1..=10_000).map(|i| format!("Layout{i}")).find(|n| !layouts.contains(&n.to_ascii_lowercase())) {
        names.entry(record_name_key("layout", &name)).or_insert(Value::Null);
    }
    for layout in d.layouts.iter().take(4096) {
        if let Some(name) = (2..=10_000).map(|i| format!("{} ({i})", layout.name)).find(|n| !layouts.contains(&n.to_ascii_lowercase())) {
            names.entry(record_name_key("layout", &name)).or_insert(Value::Null);
        }
    }
    json!(names)
}

fn definition(d: &Drawing, name: &str, out: &mut Vec<Premise>) {
    let kind = EntityKind::Insert(kition_cad_doc::Insert {
        block: name.into(),
        insert: Default::default(),
        scale: Default::default(),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    });
    for b in blocks(d, &kind).values() {
        named_need(d, "Block", &b.name, &["name", "data.base", "~memberOrder"], out);
        out.extend(b.entities.iter().map(|e| Premise::on(e.handle.hex()).fields(["common", "kind"]).positions(Vec::<String>::new())));
    }
}

/// Exact inputs copied by block insertion and attribute synchronization. Ordinary
/// drawing members never become attributes; their geometry and common fields are unused.
fn insert_attributes(block: &kition_cad_doc::Block) -> Value {
    let definitions: Vec<_> = block
        .entities
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::AttDef(a) if !a.constant => Some(json!({"id":e.handle.hex(),"tag":a.tag,"text":a.text,"invisible":a.invisible})),
            _ => None,
        })
        .collect();
    json!({"base":if definitions.is_empty() { Value::Null } else { json!(block.base) },"definitions":definitions})
}

/// The offset used by mleader_content::apply for a unit reference at the origin.
/// Native bounds include nested definitions and member-layer visibility, but not colour.
fn block_center(d: &Drawing, name: &str) -> Value {
    let probe = Entity {
        handle: Handle(0),
        common: Common::default(),
        kind: EntityKind::Insert(kition_cad_doc::Insert {
            block: name.into(),
            insert: Default::default(),
            scale: kition_cad_geom::Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    };
    let bounds = kition_cad_doc::entity_bounds(d, &probe, 0);
    let center = bounds.center();
    if bounds.is_empty() || !center.is_finite() { Value::Null } else { json!([center.x, center.y]) }
}

fn dim_style(d: &Drawing, name: &str, out: &mut Vec<Premise>) {
    named_need(d, "DimStyle", name, &["name", "data"], out);
    if let Some(st) = d.dim_style(name) {
        dim_dependencies(d, st, false, out);
    }
}

fn dim_dependencies(d: &Drawing, st: &kition_cad_doc::DimStyle, geometry: bool, out: &mut Vec<Premise>) {
    let fields = if geometry { &["name", "data.font", "data.height", "data.widthFactor", "data.oblique"][..] } else { &["name"][..] };
    named_need(d, "TextStyle", &st.text_style, fields, out);
    for block in [&st.arrow_block, &st.arrow_block1, &st.arrow_block2] {
        if !block.is_empty() {
            definition(d, block, out);
        }
    }
}

fn property_fields(e: &Entity, p: &Value) -> Vec<String> {
    let mut fields = Vec::new();
    for (param, field) in [
        ("color", "color"),
        ("layer", "layer"),
        ("linetype", "linetype"),
        ("lineweight", "lineweight"),
        ("ltscale", "ltscale"),
        ("transparency", "transparency"),
        ("visible", "visible"),
    ] {
        if p.get(param).is_some() {
            fields.push(format!("common.{field}"));
        }
    }
    let geometry = [
        "dimStyle",
        "start",
        "end",
        "length",
        "angle",
        "center",
        "radius",
        "diameter",
        "circumference",
        "area",
        "startAngle",
        "endAngle",
        "text",
        "textOverride",
        "height",
        "rotation",
        "widthFactor",
        "width",
        "position",
        "closed",
        "scale",
        "pattern",
    ];
    let mut kind = Vec::new();
    for key in geometry.into_iter().filter(|key| p.get(*key).is_some()) {
        let field = match (&e.kind, key) {
            (EntityKind::Line(_), "start") => "a",
            (EntityKind::Line(_), "end") => "b",
            (EntityKind::Circle(_) | EntityKind::Arc(_), "center") => "center",
            (EntityKind::Circle(_), "radius" | "diameter" | "circumference" | "area") | (EntityKind::Arc(_), "radius") => "radius",
            (EntityKind::Arc(_), "startAngle") => "start",
            (EntityKind::Arc(_), "endAngle") => "end",
            (EntityKind::Text(_), "text") => "value",
            (EntityKind::MText(_), "text") => "contents",
            (EntityKind::Text(_) | EntityKind::MText(_), "height") => "height",
            (EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::Insert(_), "rotation") => "rotation",
            (EntityKind::Text(_), "widthFactor") => "widthFactor",
            (EntityKind::LwPolyline(_), "closed") => "closed",
            (EntityKind::LwPolyline(_), "width") => "const_width",
            (EntityKind::MText(_), "width") => "width",
            (EntityKind::Insert(_), "scale") => "scale",
            _ => {
                return {
                    fields.push("kind".into());
                    fields
                };
            }
        };
        kind.push(format!("kind.{field}"));
    }
    if !kind.is_empty() {
        fields.push("kind.type".into());
        fields.extend(kind);
    }
    fields
}

/// An associative dimension rests on its saved attachments and the geometry they measure.
pub(crate) fn dimension_sources(e: &Entity, h: Handle, premises: &mut Vec<Premise>) {
    let EntityKind::Dimension(dm) = &e.kind else { return };
    premises.push(need(h.hex(), vec!["kind.assoc".into()]));
    for a in &dm.assoc {
        premises.push(need(a.handle.hex(), vec!["kind".into()]));
        if let kition_cad_doc::AssocSnap::Intersection { other } = a.snap {
            premises.push(need(other.hex(), vec!["kind".into()]));
        }
    }
}

fn merge_premises(premises: Vec<Premise>) -> BTreeMap<String, Premise> {
    let mut merged: BTreeMap<String, Premise> = BTreeMap::new();
    for premise in premises {
        let id = premise.id.to_string();
        if let Some(old) = merged.get_mut(&id) {
            for (old, new) in [(&mut old.fields, premise.fields), (&mut old.positions, premise.positions)] {
                match (old.as_mut(), new) {
                    (Some(fields), Some(extra)) => {
                        for field in extra {
                            if !fields.contains(&field) {
                                fields.push(field);
                            }
                        }
                    }
                    (_, None) => *old = None,
                    _ => {}
                }
            }
        } else {
            merged.insert(id, premise);
        }
    }
    merged
}

/// Read in props.rs, modify.rs, gripcmds.rs and table.rs. A missing family stays conservative.
// Explicit destination names are typed gaps, including when no record occupies them.
// Replay stored names individually so unrelated large registry edits need no receipt values.
fn destination_names(command: &str, p: &Value, d: &Drawing) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let target = match command {
        "layer.add" => Some(("Layer", "name")),
        "layer.set" => Some(("Layer", "newName")),
        "arrange.group" => Some(("Group", "name")),
        "group.edit" => Some(("Group", "rename")),
        "layout.rename" => Some(("Layout", "to")),
        "textStyle.rename" => Some(("TextStyle", "to")),
        "dimensionStyle.rename" => Some(("DimStyle", "to")),
        "layerState.rename" => Some(("LayerState", "to")),
        "namedObject.rename" => Some((rename_type(p), "to")),
        _ => None,
    };
    if let Some((ty, key)) = target
        && let Some(name) = p.get(key).and_then(Value::as_str).filter(|n| !n.trim().is_empty())
    {
        out.push((ty.into(), if command == "layer.add" || command == "layer.set" { name.into() } else { name.trim().into() }));
    }
    if command == "plot.styleManager" {
        let action = p.get("action").and_then(Value::as_str).unwrap_or("list").to_ascii_lowercase();
        if let Some(source) = p.get(if action == "new" { "from" } else { "name" }).and_then(Value::as_str) {
            out.push(("PlotStyleTable".into(), source.trim().into()));
        }
        if action == "new"
            && let Some(name) = p.get("name").and_then(Value::as_str)
        {
            let named = p
                .get("from")
                .and_then(Value::as_str)
                .and_then(|n| kition_cad_doc::plot_style_table(d, n))
                .map(|t| !t.is_color_dependent())
                .unwrap_or_else(|| {
                    p.get("kind")
                        .and_then(Value::as_str)
                        .map(|k| matches!(k.to_ascii_lowercase().as_str(), "named" | "stb"))
                        .unwrap_or_else(|| name.to_ascii_lowercase().ends_with(".stb"))
                });
            let ext = if named { ".stb" } else { ".ctb" };
            let mut name = name.trim().to_owned();
            if !name.to_ascii_lowercase().ends_with(ext) {
                name.push_str(ext);
            }
            out.push(("PlotStyleTable".into(), name));
        }
    }
    out
}

pub fn under(s: &Session, command: &str, p: &Value, commands: &[KitCommand]) -> Result<Option<Under>, String> {
    let Some(mut result) = under_native(s, command, p, commands)? else { return Ok(None) };
    let d = s.doc().map_err(|e| e.to_string())?;
    let names = destination_names(command, p, d);
    if !names.is_empty() {
        let covers = result.covers.get_or_insert_with(|| Covers { fields: vec![] });
        if !covers.fields.iter().any(|f| f == "name") {
            covers.fields.push("name".into());
        }
        for (ty, name) in names {
            if let Some((id, _)) = named(d, &ty, &name) {
                if !result.objects.contains(&json!(id)) {
                    result.objects.push(json!(id));
                }
                if !result.premises.iter().any(|p| p.id == id) {
                    result.premises.push(need(id, vec![]));
                }
            }
        }
    }
    Ok(Some(result))
}

fn under_native(s: &Session, command: &str, p: &Value, commands: &[KitCommand]) -> Result<Option<Under>, String> {
    if command == "edit.pasteAsBlock" && s.clipboard.is_empty() {
        return Ok(Some(Under::default()));
    }
    if command == "command.runLine" {
        let text = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
        if s.current_prompt().is_none() {
            if let Some((name, params)) = text.split_once(' ')
                && params.trim_start().starts_with('{')
                && let Ok(params) = serde_json::from_str::<Value>(params.trim())
            {
                let id = kition_cad_engine::cmd::resolve_alias(&name.to_ascii_lowercase());
                if id != "command.runLine" {
                    return under(s, &id, &params, commands);
                }
            }
            if let Some(spec) = text.split_whitespace().next().and_then(kition_cad_engine::cmd::resolve_typed)
                && spec.interactive.is_none()
            {
                // LTSCALE has no interactive machine: scalar text runs it with
                // missing parameters and errors before reading the drawing.
                if spec.id == "property.linetypeScale" {
                    return Ok(Some(Under::default()));
                }
                return under(s, spec.id, &json!({}), commands);
            }
        }
        // Other interactive text may consume the running prompt and drawing state.
        return Ok(Some(Under { premises: vec![need("drawing", vec!["~history".into()])], ..Default::default() }));
    }
    if command == "edit.delete" {
        return under(s, "object.delete", p, commands);
    }
    if command == "object.array" {
        let id = kition_cad_engine::cmd::array_operation(p).map_err(|e| e.to_string())?;
        return under(s, id, p, commands);
    }
    if command == "layout.manage" {
        let (command, p) = kition_cad_engine::cmd::layout_operation(p).map_err(|e| e.to_string())?;
        if command == "layout.set" {
            if p.get("name").and_then(Value::as_str).is_none_or(|name| name.eq_ignore_ascii_case("Model")) {
                return Ok(Some(Under::default()));
            }
            // Unlike direct LAYOUT.SET, the agent's LAYOUT dispatcher is bracketed
            // by execute(): its temporary paper view is restored. The address is
            // still guarded, while page/view values cannot change a saved result.
            let d = s.doc().map_err(|e| e.to_string())?;
            let p = crate::params::resolve(d, command, p)?;
            let mut premises = Vec::new();
            if let Some(name) = p.get("name").and_then(Value::as_str) {
                named_need(d, "Layout", name, &[], &mut premises);
            }
            let objects = p
                .get("name")
                .and_then(Value::as_str)
                .and_then(|name| record_names(d).get(record_name_key("layout", name)).cloned())
                .into_iter()
                .collect();
            return Ok(Some(Under { premises, objects, covers: Some(Covers { fields: vec!["name".into()] }), ..Default::default() }));
        }
        return under(s, command, &p, commands);
    }
    let d = s.doc().map_err(|e| e.to_string())?;
    if matches!(command, "layer.previous" | "layer.unisolate") {
        let key = if command == "layer.previous" { "KITION_CAD_LAYERP" } else { "KITION_CAD_LAYISO" };
        if serde_json::from_str::<Vec<kition_cad_doc::Layer>>(&d.header.str(key, "")).is_err() {
            return Ok(Some(Under { premises: vec![need("drawing", vec![format!("header.{key}")])], ..Default::default() }));
        }
    }
    if command == "constraint.parameters" && p.get("name").is_none() && p.get("delete").is_none() {
        return Ok(Some(Under::default()));
    }
    if command == "layout.set" && p.get("name").and_then(Value::as_str).is_some_and(|name| name.eq_ignore_ascii_case("Model")) {
        return Ok(Some(Under::default()));
    }
    let resolved = crate::params::resolve(d, command, p.clone())?;
    let p = &resolved;
    if command == "namedObject.rename"
        && let Some(from) = p.get("from").and_then(Value::as_str)
    {
        let ty = rename_type(p);
        let id = named(d, ty, from).map(|(id, _)| id);
        let premise = id.clone().unwrap_or_else(|| format!("{}:{from}", ty.to_ascii_lowercase()));
        let premise = need(premise, vec!["name".into()]);
        let premises = vec![premise];
        // Renaming these records repoints common/style references, without
        // changing solver geometry (props::run_rename and the delegated handlers).
        return Ok(Some(Under {
            objects: id.into_iter().map(Value::from).collect(),
            premises,
            covers: Some(Covers { fields: vec!["name".into()] }),
            ..Default::default()
        }));
    }
    if crate::query::selects(command) {
        return crate::query::under(s, command, p).map(Some);
    }
    if command == "arrange.ungroup" {
        return crate::scan::ungroup_under(s, p).map(Some);
    }
    let Some(c) = commands.iter().find(|c| c.id == command) else { return Ok(None) };
    if command == "edit.find" {
        let matching = matching_texts(&drawing_texts(d), &s.space(), p);
        let ids: Vec<_> = matching.as_array().into_iter().flatten().filter_map(|e| e.get(0).cloned()).collect();
        // A following consumer is checked on the SDK's isolated prefix state.
        // Search itself consumes membership, not matching word values or geometry.
        let mut premises: Vec<_> =
            ids.iter().map(|id| need(id.clone(), if p.get("replace").is_some() { vec!["~words".into()] } else { Vec::new() })).collect();
        if p.get("replace").is_some() {
            premises.extend(ids.iter().map(|id| need(id.clone(), vec!["~locked".into()])));
        }
        // A replaced dimension text is a write to a propagation target: it rests on its sources.
        for h in ids.iter().filter(|_| p.get("replace").is_some()).filter_map(handle) {
            if let Some(e) = entity(d, h) {
                dimension_sources(e, h, &mut premises);
            }
        }
        let orders = if ids.len() > 1 {
            let (parent, field) = match s.space() {
                Space::Model => (json!("drawing"), "model"),
                Space::Paper(name) => (json!(format!("layout:{}", d.record_id("layout", &name, 0).ok_or("unknown layout identity")?)), "entities"),
            };
            vec![RelativeOrder { parent, field: field.into(), objects: ids.clone() }]
        } else {
            vec![]
        };
        return Ok(Some(Under {
            orders,
            premises: merge_premises(premises).into_values().collect(),
            objects: ids,
            covers: Some(Covers { fields: vec!["kind".into(), "~space".into()] }),
            ..Default::default()
        }));
    }
    let by_place = spatial(command, p);
    // Target selection is independent of auxiliary object parameters (path, base, edges).
    let mut targets = if by_place { picked(d, &s.space(), command, p) } else { Vec::new() };
    if !by_place || command == "dimension.reassociate" {
        if let Some(key) = c.targets {
            for v in crate::params::values(p, key) {
                match v {
                    Value::Array(a) => targets.extend(a.iter().filter_map(handle)),
                    v => targets.extend(handle(v)),
                }
            }
        }
        // Several native handlers accept both singular and plural handles.
        if targets.is_empty() {
            for key in ["handle", "handles"] {
                for v in crate::params::values(p, key) {
                    match v {
                        Value::Array(a) => targets.extend(a.iter().filter_map(handle)),
                        v => targets.extend(handle(v)),
                    }
                }
            }
        }
        let explicit = ["handle", "handles"].iter().any(|key| p.get(*key).is_some_and(|v| !v.is_null()));
        let uses_selection = c.targets.is_some()
            || crate::acts::edits_a_table(command)
            || command == "constraint.delete"
            || crate::acts::constraint_bars(command)
            || crate::acts::dynamic_dimensions(command);
        let names_source = (crate::acts::constraint_bars(command) || crate::acts::dynamic_dimensions(command))
            && (crate::acts::shows_or_hides_all(command)
                || p.get("mode").and_then(Value::as_str).is_some_and(|mode| matches!(mode, "showall" | "hideall")))
            || matches!(command, "block.write" | "attribute.sync") && p.get("name").is_some()
            || command == "spline.add" && p.get("object") != Some(&Value::Bool(true))
            || command == "constraint.delete" && p.get("ids").is_some()
            || command == "attribute.editGlobal" && p.get("handle").is_none() && p.get("handles").is_none();
        if targets.is_empty() && !explicit && uses_selection && !names_source && c.reads.contains(&kition_kit::EditorState::Selection) {
            targets = s.selection();
        }
    }
    let mut hs = targets.clone();
    if by_place && implicit(command, p) {
        hs.extend(dependencies(s, d, command, p));
    }
    let place_objects = hs.clone();
    for key in c.objects.iter().copied() {
        if c.object_types.iter().any(|(k, tys)| *k == key && tys.contains(&"Entity")) {
            for v in crate::params::values(p, key) {
                match v {
                    Value::Array(a) if !key.ends_with("[]") => hs.extend(a.iter().filter_map(handle)),
                    v => hs.extend(handle(v)),
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    hs.retain(|h| seen.insert(*h));
    // A single circle supplies one curve and no points/lines to inference. With
    // no curve pair it cannot produce a constraint, for any radius or options.
    if command == "constraint.auto"
        && hs.len() == 1
        && hs.first().and_then(|h| entity(d, *h)).is_some_and(|e| matches!(e.kind, EntityKind::Circle(_)))
    {
        return Ok(Some(Under { premises: hs.iter().map(|h| need(h.hex(), vec!["kind.type".into()])).collect(), ..Default::default() }));
    }
    let mut premises = Vec::new();
    if matches!(command, "constraint.bars" | "constraint.dynamicDimensions") {
        let mode = p.get("mode").and_then(Value::as_str).unwrap_or("show").to_ascii_lowercase();
        let settings = &d.parametric.settings;
        let visible = if command == "constraint.dynamicDimensions" { settings.dims_visible } else { settings.bars_visible };
        if matches!(mode.as_str(), "show" | "hide") && (mode == "hide") == visible {
            // An added exception must survive drawing repair: missing recipients
            // are pruned. Removing a raw membership id never resolves its entity.
            premises.extend(hs.iter().map(|h| need(h.hex(), vec![])));
        }
    }
    // These explicit no-op targets cannot consume the current style or scan sources.
    // run_update still replaces the entity Arc, so its lock gate remains necessary.
    if matches!(command, "dimension.reassociate" | "dimension.disassociate" | "dimension.updateStyle")
        && ["handle", "handles"].iter().any(|key| p.get(*key).is_some_and(|v| !v.is_null()))
        && targets.iter().filter_map(|h| entity(d, *h)).all(|e| !matches!(e.kind, EntityKind::Dimension(_)))
    {
        premises.extend(hs.iter().map(|h| {
            need(h.hex(), if command == "dimension.updateStyle" { vec!["kind.type".into(), "~locked".into()] } else { vec!["kind.type".into()] })
        }));
        if command == "dimension.updateStyle" {
            for e in hs.iter().filter_map(|h| entity(d, *h)) {
                // The pinned same-server layer receipt does not refresh this entity's
                // effective-lock witness. Hold the layer scalar directly as well.
                named_need(d, "Layer", &e.common.layer, &["data.locked"], &mut premises);
            }
        }
        return Ok(Some(Under { premises, ..Default::default() }));
    }
    if command == "dimension.space" {
        let base = p.get("base").and_then(handle);
        let writes = targets
            .iter()
            .filter(|h| Some(**h) != base)
            .filter_map(|h| entity(d, *h))
            .any(|e| matches!(&e.kind, EntityKind::Dimension(dm) if matches!(dm.kind, DimKind::Linear { .. } | DimKind::Aligned)));
        if !writes {
            for h in &hs {
                let mut fields = vec!["kind.type".into()];
                if entity(d, *h).is_some_and(|e| matches!(e.kind, EntityKind::Dimension(_))) {
                    fields.push("~linearDimension".into());
                }
                premises.push(need(h.hex(), fields));
            }
            return Ok(Some(Under {
                premises,
                objects: if by_place { place_objects.iter().map(|h| json!(h.hex())).collect() } else { vec![] },
                covers: by_place.then_some(Covers { fields: vec!["kind".into(), "~pick".into()] }),
                ..Default::default()
            }));
        }
    }
    let eligibility = match command {
        "textStyle.delete" => Some(("TextStyle", "~canDelete")),
        "dimensionStyle.delete" => Some(("DimStyle", "~canDelete")),
        "layer.set" if p.get("frozen").and_then(Value::as_bool) == Some(true) => Some(("Layer", "~canFreeze")),
        _ => None,
    };
    if let Some((ty, fact)) = eligibility
        && let Some(name) = p.get("name").and_then(Value::as_str)
    {
        named_need(d, ty, name, &[fact], &mut premises);
    }
    if command == "layout.delete"
        || command == "layout.manage"
            && p.get("option").and_then(Value::as_str).is_some_and(|o| matches!(o.to_ascii_lowercase().as_str(), "delete" | "d"))
    {
        premises.push(need("drawing", vec!["~layoutCanDelete".into()]));
    }
    // same-field decision: hold every replaced setting.
    let setting_fields: &[(&str, &str)] = match command {
        "property.color" => &[("color", "CECOLOR"), ("color", "CECOLOR_RGB")],
        "attribute.display" => &[("mode", "ATTMODE")],
        "wipeout.add" => &[("frames", "WIPEOUTFRAME")],
        "property.lineweight" => &[("lineweight", "CELWEIGHT")],
        "property.linetypeScale" => &[("scale", "LTSCALE")],
        "property.linetype" => &[("current", "CELTYPE")],
        "file.setUnits" => &[("lunits", "LUNITS"), ("luprec", "LUPREC"), ("aunits", "AUNITS"), ("auprec", "AUPREC"), ("insunits", "INSUNITS")],
        "file.setLimits" => &[("min", "LIMMIN"), ("max", "LIMMAX")],
        "point.style" => &[("pdmode", "PDMODE"), ("pdsize", "PDSIZE")],
        "block.setBase" => &[("at", "INSBASE")],
        "plot.style" if hs.is_empty() => &[("name", "CPLOTSTYLE")],
        _ => &[],
    };
    let mut settings: Vec<_> = setting_fields.iter().filter(|(param, _)| p.get(*param).is_some()).map(|(_, var)| format!("header.{var}")).collect();
    if command == "textStyle.setCurrent" || command == "textStyle.open" && p.get("current") != Some(&Value::Bool(false)) {
        settings.push("header.TEXTSTYLE".into());
    }
    // same-field decision: these handlers also replace stored defaults
    // or layer snapshots even when their main result is a drawing/record change.
    if matches!(command, "layer.setCurrent" | "layer.makeObjectCurrent" | "layer.isolate")
        || command == "layer.add" && p.get("current") == Some(&Value::Bool(true))
    {
        settings.push("header.CLAYER".into());
    }
    if matches!(
        command,
        "layer.setCurrent" | "layer.isolate" | "layer.off" | "layer.freeze" | "layer.lock" | "layer.unlock" | "layer.allOn" | "layer.thawAll"
    ) {
        settings.push("header.KITION_CAD_LAYERP".into());
    }
    if command == "layer.isolate" {
        settings.push("header.KITION_CAD_LAYISO".into());
    }
    if matches!(
        command,
        "circle.addCenterDiameter" | "circle.addTwoPoints" | "circle.addThreePoints" | "circle.addTanTanRadius" | "circle.addTanTanTan"
    ) {
        settings.push("header.CIRCLERAD".into());
    }
    if p.get("name").is_some() && p.get("current") != Some(&Value::Bool(false)) {
        match command {
            "multileaderStyle.open" => settings.push("header.CMLEADERSTYLE".into()),
            "tableStyle.open" => settings.push("header.CTABLESTYLE".into()),
            _ => {}
        }
    }
    let dimension_style = matches!(command, "dimensionStyle.open" | "dimensionStyle.openManager" | "dimensionStyle.setCurrent");
    if dimension_style {
        if matches!(command, "dimensionStyle.open" | "dimensionStyle.openManager")
            && p.get("name").and_then(Value::as_str).is_some_and(|name| d.dim_style(name).is_none())
        {
            named_need(d, "DimStyle", &d.header.str("DIMSTYLE", "Standard"), &["name", "data"], &mut premises);
        }
        settings.push("header.DIMSTYLE".into());
        if command == "dimensionStyle.setCurrent"
            || p.get("current") != Some(&Value::Bool(false))
            || p.get("name").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(&d.header.str("DIMSTYLE", "Standard")))
        {
            settings.extend(kition_cad_doc::DIMVARS.iter().map(|(var, _)| format!("header.{var}")));
        }
    }
    if !settings.is_empty() {
        premises.push(need("drawing", settings));
    }
    if command == "property.linetype"
        && let Some(load) = p.get("load").and_then(Value::as_str)
    {
        let library = kition_cad_doc::library::standard_linetypes();
        let candidates: Vec<_> = library.iter().filter(|lt| load == "*" || lt.name.eq_ignore_ascii_case(load)).collect();
        let mut fields: Vec<_> = candidates.iter().map(|lt| format!("~recordNames.{}", record_name_key("linetype", &lt.name))).collect();
        if candidates.iter().any(|lt| d.linetype(&lt.name).is_none()) {
            fields.push("header.MEASUREMENT".into());
        }
        if !fields.is_empty() {
            premises.push(need("drawing", fields));
        }
    }
    if matches!(command, "edit.copy" | "edit.copyWithBasePoint" | "edit.cut") {
        let ents: Vec<_> = hs.iter().filter_map(|h| entity(d, *h).cloned()).collect();
        let defs = kition_cad_engine::cmd::clipboard::definitions(d, &ents);
        for (ty, name) in defs.record_names() {
            let ty = match ty.as_str() {
                "layer" => "Layer",
                "linetype" => "Linetype",
                "textstyle" => "TextStyle",
                "dimstyle" => "DimStyle",
                "mleaderstyle" => "MLeaderStyle",
                "tablestyle" => "TableStyle",
                "block" => "Block",
                _ => continue,
            };
            named_need(d, ty, &name, &["name", "data"], &mut premises);
            if ty == "Block" {
                definition(d, &name, &mut premises);
            }
        }
    }
    if matches!(command, "edit.undo" | "edit.redo" | "edit.undoStep" | "edit.undoMany" | "edit.redoStep") {
        premises.push(need("drawing", vec!["~history".into()]));
    }
    if matches!(command, "layer.merge" | "layer.deleteWithObjects" | "layer.delete") {
        premises.push(need("drawing", vec!["header.CLAYER".into()]));
        let members = crate::scan::layer_members(d);
        let keys = if command == "layer.merge" { &["from"][..] } else { &["name", "names"][..] };
        for key in keys {
            let values: Vec<_> = match p.get(*key) {
                Some(Value::Array(a)) => a.iter().collect(),
                Some(v) => vec![v],
                None => vec![],
            };
            for name in values.into_iter().filter_map(Value::as_str) {
                let key = crate::scan::key(name.trim());
                premises.push(need("drawing", vec![format!("~layerMembers.{key}")]));
                if let Some(m) = members.get(&key) {
                    for id in m.get("entities").and_then(Value::as_array).into_iter().flatten() {
                        premises.push(if command == "layer.deleteWithObjects" {
                            Premise::on(id.clone())
                        } else {
                            need(id.clone(), vec!["common.layer".into()])
                        });
                    }
                    for id in m.get("viewports").and_then(Value::as_array).into_iter().flatten() {
                        premises.push(need(id.clone(), vec!["kind.frozenLayers".into(), "kind.layerColors".into()]));
                    }
                }
            }
        }
    }
    if command == "file.purge" {
        let key = kition_cad_engine::cmd::purge_premise_key(p).map_err(|e| e.to_string())?;
        premises.push(need("drawing", vec![format!("~{key}")]));
    }
    if command == "file.audit" && p.get("fix") == Some(&Value::Bool(true)) {
        premises.push(need("drawing", vec!["~audit".into()]));
    }
    if command == "object.deleteDuplicates" && p.get("handles").is_none() {
        premises.push(need("drawing", vec!["~overkill".into()]));
    }
    if command == "layout.duplicate"
        && let Some(from) = p.get("from").and_then(Value::as_str)
    {
        if let Some((id, _)) = named(d, "Layout", from) {
            premises.push(need(id, vec!["name".into(), "data.page".into(), "data.view".into(), "~memberOrder".into()]));
        }
        if let Some(layout) = d.layout(from) {
            premises.extend(layout.entities.iter().map(|e| Premise::on(e.handle.hex()).fields(["common", "kind"]).positions(Vec::<String>::new())));
        }
        if p.get("to").is_none()
            && let Some(name) = (2..=10_000).map(|i| format!("{from} ({i})")).find(|n| d.layout(n).is_none())
        {
            premises.push(need("drawing", vec![format!("~recordNames.{}", record_name_key("layout", &name))]));
        }
        premises.push(need("drawing", vec!["~layoutSeed".into()]));
    }
    if command == "attribute.sync" {
        let block = kition_cad_engine::cmd::attsync_block(s, p).map_err(|e| e.to_string())?;
        // attribs.rs::sync copies nonconstant attribute definitions, not ordinary members.
        named_need(d, "Block", &block, &["name", "~insertAttributes"], &mut premises);
        premises.push(need("drawing", vec!["~insertRefs".into()]));
        let has_definitions =
            d.block(&block).is_some_and(|block| block.entities.iter().any(|e| matches!(&e.kind, EntityKind::AttDef(a) if !a.constant)));
        let fields: &[&str] = if has_definitions {
            &["kind.type", "kind.block", "kind.insert", "kind.scale", "kind.rotation", "kind.attribs", "~locked"]
        } else {
            &["kind.block"]
        };
        premises.extend(
            kition_cad_engine::cmd::attribute_references(d, &block).into_iter().map(|h| need(h.hex(), fields.iter().map(|f| (*f).into()).collect())),
        );
    }
    if command == "plot.pageSetup" {
        let layout = p.get("layout").and_then(Value::as_str).map(str::to_owned).or_else(|| match s.layout_space() {
            Space::Paper(name) => Some(name),
            _ => None,
        });
        if let Some(name) = layout {
            named_need(d, "Layout", &name, &["name", "data.page"], &mut premises);
        }
    }
    if matches!(command, "file.close" | "file.closeAll" | "file.save" | "file.saveAs") {
        premises.push(if matches!(command, "file.close" | "file.closeAll") {
            Premise::on("drawing").positions(Vec::<String>::new())
        } else {
            Premise::on("drawing")
        });
    }
    if command == "block.write" {
        premises.push(need("drawing", ["layers", "linetypes", "textStyles", "dimStyles", "blocks"].map(String::from).to_vec()));
        if let Some(name) = p.get("name").and_then(Value::as_str)
            && let Some((id, _)) = named(d, "Block", name)
        {
            premises.push(Premise::on(id));
        }
    }
    if command == "plot.styleManager" {
        let action = p.get("action").and_then(Value::as_str).unwrap_or("list");
        let source = match action {
            "set" | "delete" | "get" | "save" => p.get("name"),
            "new" => p.get("from"),
            _ => None,
        }
        .and_then(Value::as_str);
        if let Some(name) = source {
            // Source presence is replayed by its typed name, including builtins absent from the stored map.
            named_need(d, "PlotStyleTable", name, &["name", "data"], &mut premises);
        }
    }
    if command == "dimension.updateStyle" {
        premises.push(need("drawing", vec!["header.DIMSTYLE".into()]));
        named_need(d, "DimStyle", &d.header.str("DIMSTYLE", "Standard"), &["name"], &mut premises);
    }
    if matches!(
        command,
        "layer.allOn"
            | "layer.thawAll"
            | "layer.isolate"
            | "layer.setCurrent"
            | "layerState.save"
            | "layerState.restore"
            | "layer.previous"
            | "layer.unisolate"
            | "layer.off"
            | "layer.freeze"
            | "layer.lock"
            | "layer.unlock"
    ) {
        // These bodies capture or replace complete layer records, including undo snapshots.
        premises.push(need("drawing", vec!["layers".into()]));
        for layer in &d.layers {
            named_need(d, "Layer", &layer.name, &["name", "data"], &mut premises);
        }
        let header = match command {
            "layer.previous" => Some("header.KITION_CAD_LAYERP"),
            "layer.unisolate" => Some("header.KITION_CAD_LAYISO"),
            _ => None,
        };
        if let Some(header) = header {
            premises.push(need("drawing", vec![header.into()]));
        }
    }
    if command == "layer.changeToCurrent" {
        premises.push(need("drawing", vec!["header.CLAYER".into()]));
        if command == "layer.changeToCurrent" {
            named_need(d, "Layer", &d.header.str("CLAYER", "0"), &["name"], &mut premises);
        }
    }
    if matches!(command, "variable.set" | "variable.get")
        && let Some(var) = p.get("name").and_then(Value::as_str)
        && !kition_cad_engine::sysvars::is_session_var(var)
    {
        let var = var.trim().to_ascii_uppercase();
        let field = if var.contains(['.', '[', ']']) { "header".into() } else { format!("header.{var}") };
        // SETVAR's coercion depends on the existing HVal type (sysvars.rs:184-213).
        premises.push(need("drawing", vec![field]));
        if let Some(ty) = crate::params::sysvar_reference(&var).and_then(|types| types.first()) {
            let name = p.get("value").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| d.header.str(&var, ""));
            named_need(d, ty, &name, &["name"], &mut premises);
        }
        if command == "variable.get" && d.header.get(&var).is_none() && kition_cad_doc::DIMVARS.iter().any(|(name, _)| *name == var) {
            premises.push(need("drawing", vec!["header.DIMSTYLE".into()]));
            dim_style(d, &d.header.str("DIMSTYLE", "Standard"), &mut premises);
        }
    }
    if command == "polyline.edit"
        && p.get("option").and_then(Value::as_str).is_some_and(|o| matches!(o.to_ascii_lowercase().as_str(), "spline" | "s"))
    {
        premises.push(need("drawing", vec!["header.SPLINESEGS".into()]));
    }
    if matches!(command, "object.trim" | "object.extend") && p.get("edges").and_then(Value::as_array).is_none() {
        premises.push(need("drawing", vec!["~cutters".into()]));
        for h in kition_cad_engine::cmd::cutting_edge_handles(d, &s.space()) {
            premises.push(need(h.hex(), vec!["kind".into(), "~pick".into()]));
        }
    }
    if matches!(command, "object.trim" | "object.extend") {
        for h in p.get("edges").and_then(Value::as_array).into_iter().flatten().filter_map(handle) {
            premises.push(need(h.hex(), vec!["~visible".into(), "~space".into()]));
        }
    }
    if command == "object.fillet"
        && let Some(h) = p.get("h1").and_then(handle)
    {
        premises.push(need(h.hex(), vec!["~pick.space".into()]));
    }
    if matches!(command, "viewport.set" | "viewport.freezeLayer" | "vports.set") {
        for name in kition_cad_engine::cmd::viewport_layer_names(p) {
            named_need(d, "Layer", &name, &["name"], &mut premises);
        }
    }
    if command == "object.changeSpace" {
        let (target, vp) =
            kition_cad_engine::cmd::chspace_context(d, &s.space(), p.get("to").and_then(Value::as_str), p.get("viewport").and_then(handle))
                .map_err(|e| e.to_string())?;
        let layout = match target {
            Space::Paper(n) => Some(n),
            Space::Model => match s.space() {
                Space::Paper(n) => Some(n),
                _ => None,
            },
        };
        if let Some(layout) = layout {
            let fields = if p.get("viewport").is_none() { vec!["name", "~chspaceViewport"] } else { vec!["name"] };
            named_need(d, "Layout", &layout, &fields, &mut premises);
        }
        if p.get("to").is_none() && s.space() == Space::Model {
            premises.push(need("drawing", vec!["layouts".into()]));
            for l in &d.layouts {
                named_need(d, "Layout", &l.name, &["data.tabOrder"], &mut premises);
            }
        }
        if let Some(vp) = vp {
            premises.push(need(vp.hex(), vec!["kind.center".into(), "kind.height".into(), "kind.viewCenter".into(), "kind.viewHeight".into()]));
        }
    }
    // These branches preserve source properties or create no entity (engine handlers
    // blocks::make_block, spline_opts::run_object, layout::run_new, draw2::run_wipeout).
    let current_properties = crate::acts::takes_place(command)
        && !(command == "spline.add" && p.get("object").is_some()
            || matches!(command, "block.addDialog" | "block.add") && p.get("keep").and_then(Value::as_str).unwrap_or("convert") != "convert"
            || command == "layout.add" && p.get("viewport") == Some(&Value::Bool(false))
            || command == "wipeout.add" && p.get("frames").is_some()
            || command == "revisionCloud.add" && p.get("handle").is_some());
    if current_properties {
        // Session::current_common/add_entity copy these stored defaults, not editor state.
        let mut fields: Vec<_> =
            ["CECOLOR", "CECOLOR_RGB", "CELTYPE", "CELWEIGHT", "CELTSCALE", "CPLOTSTYLE"].into_iter().map(|key| format!("header.{key}")).collect();
        if p.get("layer").is_none() {
            fields.push("header.CLAYER".into());
            named_need(d, "Layer", &d.header.str("CLAYER", "0"), &["name"], &mut premises);
        }
        // The current linetype is copied by name; the record it names now decides what the new
        // objects draw with. CENTERMARK and CENTERLINE draw on CENTER instead, loading it when it
        // is missing (the drawing ends the same whether this write or another writer loaded it).
        if matches!(command, "centerMark.add" | "centerLine.add") {
            fields.push("~centerLinetype".into());
            fields.retain(|f| f != "header.CELTYPE");
        } else {
            named_need(d, "Linetype", &d.header.str("CELTYPE", "ByLayer"), &["name"], &mut premises);
        }
        if matches!(command, "layout.add" | "viewport.add" | "viewport.new" | "viewport.one" | "viewport.two" | "viewport.three" | "viewport.four") {
            fields.retain(|f| f != "header.CECOLOR_RGB");
        }
        if command == "spline.add" && p.get("knots").is_none() && (p.get("fit").is_some() || p.get("object").is_some()) {
            fields.push("header.SPLKNOTS".into());
        }
        if matches!(command, "text.add" | "multilineText.add" | "attribute.define") {
            if command != "multilineText.add" || p.get("style").is_none() {
                fields.push("header.TEXTSTYLE".into());
            }
            if p.get("height").is_none() {
                fields.push("header.TEXTSIZE".into());
            }
            let style = p
                .get("style")
                .and_then(Value::as_str)
                .filter(|_| command == "multilineText.add")
                .map(str::to_owned)
                .unwrap_or_else(|| d.header.str("TEXTSTYLE", "Standard"));
            named_need(d, "TextStyle", &style, &["name"], &mut premises);
        }
        if matches!(
            command,
            "dimension.add"
                | "dimension.addLinear"
                | "dimension.addAligned"
                | "dimension.addRadius"
                | "dimension.addDiameter"
                | "dimension.addAngular"
                | "dimension.addArcLength"
                | "dimension.addOrdinate"
                | "dimension.addContinue"
                | "dimension.addBaseline"
                | "dimension.addQuick"
        ) {
            fields.extend(["header.DIMSTYLE".into(), "header.DIMASSOC".into()]);
            fields.extend(kition_cad_doc::DIMVARS.iter().map(|(var, _)| format!("header.{var}")));
            dim_style(d, &d.header.str("DIMSTYLE", "Standard"), &mut premises);
            if let Some(style) = d.dim_style(&d.header.str("DIMSTYLE", "Standard")) {
                dim_dependencies(d, &style.with_overrides(&d.dim_overrides()), command != "leader.add", &mut premises);
            }
        }
        if command == "leader.add" {
            fields.push("header.DIMSTYLE".into());
            let fields = if p.get("text").and_then(Value::as_str).is_some() { vec!["name", "data.textHeight"] } else { vec!["name"] };
            named_need(d, "DimStyle", &d.header.str("DIMSTYLE", "Standard"), &fields, &mut premises);
            if p.get("text").and_then(Value::as_str).is_some() {
                named_need(d, "TextStyle", "Standard", &["name"], &mut premises);
            }
        }
        if command == "multileader.add" {
            fields.extend(["header.CMLEADERSTYLE".into(), "header.DIMSCALE".into()]);
            let cur = d.header.str("CMLEADERSTYLE", "Standard");
            if let Some(st) = d.mleader_styles.iter().find(|st| st.name.eq_ignore_ascii_case(&cur)).or(d.mleader_styles.first()) {
                let no_leader = p.get("leaderType").and_then(Value::as_str).is_some_and(|kind| kind.eq_ignore_ascii_case("none"));
                let text = p.get("block").is_none() && p.get("text").and_then(Value::as_str).is_some_and(|text| !text.is_empty());
                let mut fields = vec!["name", "data.arrowSize"];
                if !no_leader {
                    fields.push("data.dogleg");
                }
                if text {
                    fields.extend(["data.textHeight", "data.textStyle"]);
                    if !no_leader {
                        fields.push("data.landingGap");
                    }
                    named_need(d, "TextStyle", &st.text_style, &["name"], &mut premises);
                }
                named_need(d, "MLeaderStyle", &st.name, &fields, &mut premises);
            }
        }
        if matches!(command, "hatch.add" | "hatch.addGradient") {
            for (param, var) in [("pattern", "HPNAME"), ("scale", "HPSCALE"), ("angle", "HPANG")] {
                // HatchSettings::kind always stores SOLID for a gradient.
                if p.get(param).is_none() && !(command == "hatch.addGradient" && param == "pattern") {
                    fields.push(format!("header.{var}"));
                }
            }
        }
        if command == "table.add" {
            fields.push("header.CTABLESTYLE".into());
            if p.get("colWidth").is_none() {
                fields.push("header.MEASUREMENT".into());
            }
            let current = d.header.str("CTABLESTYLE", "Standard");
            if let Some(style) = d.table_styles.iter().find(|t| t.name.eq_ignore_ascii_case(&current)).or(d.table_styles.first())
                && let Some((id, _)) = named(d, "TableStyle", &style.name)
            {
                let mut fields = vec!["name".into(), "data.margin".into()];
                if p.get("textHeight").is_none() {
                    fields.push("data.textHeight".into());
                }
                premises.push(need(id, fields));
            }
        }
        premises.push(need("drawing", fields));
    }
    if command == "spline.add" && p.get("object").is_some() && p.get("knots").is_none() {
        premises.push(need("drawing", vec!["header.SPLKNOTS".into()]));
    }
    if matches!(command, "dimensionStyle.open" | "dimensionStyle.openManager")
        && p.get("name").and_then(Value::as_str).is_some_and(|name| d.dim_style(name).is_none())
    {
        premises.push(need("drawing", vec!["header.DIMSTYLE".into()]));
        dim_style(d, &d.header.str("DIMSTYLE", "Standard"), &mut premises);
    }
    if command == "object.offset" && p.get("layer").and_then(Value::as_str).is_some_and(|l| l.eq_ignore_ascii_case("current")) {
        premises.push(need("drawing", vec!["header.CLAYER".into()]));
        named_need(d, "Layer", &d.header.str("CLAYER", "0"), &["name"], &mut premises);
    }
    let transform = matches!(
        command,
        "object.move"
            | "object.rotate"
            | "object.scale"
            | "object.mirror"
            | "grip.move"
            | "grip.stretch"
            | "grip.rotate"
            | "grip.scale"
            | "grip.mirror"
    );
    let copies = if command == "object.mirror" {
        p.get("erase").and_then(Value::as_bool) != Some(true)
    } else {
        p.get("copy").and_then(Value::as_bool) == Some(true)
    };
    // A geometry edit re-measures the dimensions attached to what it changes: whether each is
    // still attached decides whether it follows (C8-cad-read).
    let reshapes = matches!(
        command,
        "object.move"
            | "object.rotate"
            | "object.scale"
            | "object.mirror"
            | "object.stretch"
            | "object.align"
            | "object.delete"
            | "object.trim"
            | "object.extend"
            | "object.fillet"
            | "object.chamfer"
            | "object.lengthen"
            | "polyline.edit"
            | "object.break"
            | "object.breakAtPoint"
            | "object.join"
            | "spline.edit"
            | "object.reverse"
            | "object.flatten"
            | "object.changeSpace"
    ) || command.starts_with("grip.")
        || command == "object.setProperties"
            && hs.iter().filter_map(|h| entity(d, *h)).any(|e| property_fields(e, p).iter().any(|f| f.starts_with("kind")));
    if reshapes && !(transform && copies) {
        for e in all(d) {
            if let EntityKind::Dimension(dm) = &e.kind
                && !hs.contains(&e.handle)
                && dm.assoc.iter().any(|a| hs.contains(&a.handle))
            {
                premises.push(need(e.handle.hex(), vec!["kind".into(), "common.layer".into()]));
                named_need(d, "Layer", &e.common.layer, &["name", "data.locked"], &mut premises);
                dim_style(d, &dm.style, &mut premises);
                let effective = d.dim_style(&dm.style).cloned().unwrap_or_default().with_overrides(&dm.overrides);
                dim_dependencies(d, &effective, true, &mut premises);
                if effective.scale <= 0.0 || !effective.scale.is_finite() {
                    premises.push(need("drawing", vec!["header.DIMSCALE".into()]));
                }
            }
        }
    }
    // TEXTEDIT refuses an object on a locked layer.
    if command == "text.edit" {
        for h in &hs {
            premises.push(need(h.hex(), vec!["~locked".into()]));
        }
    }
    // LAYFRZ leaves the current layer thawed and says it froze it.
    if command == "layer.freeze" {
        premises.push(need("drawing", vec!["header.CLAYER".into()]));
    }
    // INSERT "*NAME" inserts the definition exploded.
    if command == "block.insert"
        && let Some(name) = p.get("name").and_then(Value::as_str).and_then(|n| n.strip_prefix('*'))
        && d.block(name).is_some()
    {
        definition(d, name, &mut premises);
    }
    // A layer state saved over, or deleted, is discarded whole; so is a deleted layer.
    let discarded = match command {
        "layerState.save" | "layerState.delete" => Some("LayerState"),
        "layer.delete" => Some("Layer"),
        "layout.delete" => Some("Layout"),
        "textStyle.delete" => Some("TextStyle"),
        "dimensionStyle.delete" => Some("DimStyle"),
        _ => None,
    };
    if let Some(ty) = discarded
        && let Some(name) = p.get("name").and_then(Value::as_str)
        && let Some((id, _)) = named(d, ty, name)
    {
        premises.push(Premise::on(id).positions(Vec::<String>::new()));
    }
    // -ATTEDIT without handles edits the attributes of every matching reference it finds.
    if command == "attribute.editGlobal" && p.get("handle").is_none() && p.get("handles").is_none() {
        premises.push(need("drawing", vec!["~queries".into()]));
        for e in all(d).filter(|e| matches!(e.kind, EntityKind::Insert(_))) {
            premises.push(need(e.handle.hex(), vec!["kind.type".into(), "~attributes".into(), "kind.block".into(), "~locked".into()]));
        }
    }
    let defaults: &[(&str, &str)] = match command {
        "object.fillet" => &[("radius", "FILLETRAD")],
        "object.chamfer" if p.get("length").is_none() => &[("d1", "CHAMFERA")],
        "multiline.add" => &[("scale", "CMLSCALE"), ("justification", "CMLJUST")],
        "revisionCloud.add" | "revisionCloud.addPolygonal" | "revisionCloud.addFreehand" => &[("arcLength", "REVCLOUDARCLEN")],
        "centerMark.add" | "centerLine.add" => &[("extension", "CENTEREXE")],
        _ => &[],
    };
    let mut fields: Vec<_> = defaults.iter().filter(|(param, _)| p.get(*param).is_none()).map(|(_, var)| format!("header.{var}")).collect();
    if matches!(
        command,
        "multiline.add" | "revisionCloud.add" | "revisionCloud.addPolygonal" | "revisionCloud.addFreehand" | "centerMark.add" | "centerLine.add"
    ) && !fields.is_empty()
    {
        fields.push("header.MEASUREMENT".into());
    }
    if !fields.is_empty() {
        premises.push(need("drawing", fields));
    }
    let new_layout = command == "layout.add"
        || command == "layout.manage"
            && matches!(p.get("option").and_then(Value::as_str).unwrap_or("list").to_ascii_lowercase().as_str(), "new" | "n");
    let new_viewport = new_layout && p.get("viewport").and_then(Value::as_bool) != Some(false)
        || matches!(command, "viewport.add" | "viewport.new" | "viewport.one" | "viewport.two" | "viewport.three" | "viewport.four");
    if new_viewport {
        premises.push(need("drawing", vec!["~modelBounds".into(), "~newVpLayers".into(), "~viewportSeed".into()]));
        // layout.rs::fit_view falls back to limits only for an empty model.
        if d.extents(&Space::Model).is_empty() {
            premises.push(need("drawing", vec!["header.LIMMIN".into(), "header.LIMMAX".into()]));
        }
        // A new viewport takes the current properties (Session::current_common), CECOLOR_RGB aside.
        premises
            .push(need("drawing", ["CECOLOR", "CELTYPE", "CELWEIGHT", "CELTSCALE", "CPLOTSTYLE", "CLAYER"].map(|k| format!("header.{k}")).to_vec()));
        named_need(d, "Layer", &d.header.str("CLAYER", "0"), &["name"], &mut premises);
        named_need(d, "Linetype", &d.header.str("CELTYPE", "ByLayer"), &["name"], &mut premises);
        if let Some(layout) = p.get("layout").and_then(Value::as_str) {
            named_need(d, "Layout", layout, &["name"], &mut premises);
        }
        if p.get("p1").is_none() || p.get("p2").is_none() {
            if let Some(name) = p.get("layout").and_then(Value::as_str).map(str::to_owned).or_else(|| match s.space() {
                Space::Paper(n) => Some(n),
                _ => None,
            }) {
                named_need(d, "Layout", &name, &["name", "data.page"], &mut premises);
            }
            premises.push(need("drawing", vec!["header.MEASUREMENT".into()]));
        }
    }
    if command == "arrange.group" && p.get("name").and_then(Value::as_str).is_none_or(|n| n.trim().is_empty()) {
        premises.push(need("drawing", vec!["~nextGroupName".into()]));
    }
    if new_layout {
        if p.get("name").is_none() {
            premises.push(need("drawing", vec!["~nextLayoutName".into()]));
        }
        premises.push(need("drawing", vec!["header.MEASUREMENT".into(), "~layoutSeed".into()]));
    }
    for h in &hs {
        // DELCONSTRAINT compares these opaque handles only with constraint references.
        // Removed constraints and any remaining solver inputs are held separately below.
        if command == "constraint.delete" || crate::acts::constraint_bars(command) || crate::acts::dynamic_dimensions(command) {
            continue;
        }
        let Some(e) = entity(d, *h) else { continue };
        if matches!(command, "hatch.add" | "hatch.addGradient" | "boundary.add") && p.get("points").is_some() {
            premises.push(need(h.hex(), vec!["~boundaryOutline".into()]));
        }
        if crate::read_fields::writes_existing(command, p, *h)
            && !(command == "dimension.space" && p.get("base").and_then(handle) == Some(*h))
            && !(matches!(command, "dimension.reassociate" | "dimension.disassociate") && !matches!(e.kind, EntityKind::Dimension(_)))
        {
            premises.push(need(h.hex(), vec!["~locked".into()]));
        }
        if c.mutates && command != "dimension.disassociate" && command != "select.set" {
            dimension_sources(e, *h, &mut premises);
        }
        let fields = if command == "select.set" {
            // A spatial selector consumes the selected geometry even when a changed
            // outline still crosses the region. Common properties remain independent.
            Some(if by_place { vec!["kind".into()] } else { Vec::new() })
        } else if command == "object.changeSpace" {
            let mut fields = if targets.contains(h) { geometry_fields(&e.kind) } else { Vec::new() };
            fields.push("~space".into());
            if targets.contains(h) {
                fields.push("~locked".into());
            }
            Some(fields)
        } else if command == "dimension.space" {
            Some(if p.get("base").and_then(handle) == Some(*h) {
                vec!["kind.type".into(), "kind.kind".into(), "kind.p13".into(), "kind.p14".into(), "kind.defpt".into()]
            } else if matches!(&e.kind, EntityKind::Dimension(dm) if matches!(dm.kind, DimKind::Linear { .. } | DimKind::Aligned)) {
                vec!["kind".into(), "common.layer".into()]
            } else {
                vec!["kind.type".into(), "~linearDimension".into()]
            })
        } else if let Some(fields) = crate::read_fields::target_fields(command, p, e) {
            Some(fields)
        } else if implicit(command, p) {
            Some(if matches!(command, "hatch.add" | "hatch.addGradient" | "boundary.add") {
                vec!["kind".into(), "~pick".into()]
            } else {
                crate::read_fields::curve_fields(e)
            })
        } else if command == "object.setProperties" {
            let mut fields = property_fields(e, p);
            if by_place {
                fields.extend(["kind".into(), "~pick".into()]);
            }
            Some(fields)
        } else if transform && copies {
            Some(if targets.contains(h) { vec!["common".into(), "kind".into(), "~space".into()] } else { crate::read_fields::curve_fields(e) })
        } else if transform && !copies {
            let mut fields = geometry_fields(&e.kind);
            fields.push("~locked".into());
            Some(fields)
        } else if destroys(command, p, *h, &hs) {
            // JOIN and PEDIT's joined-away objects ignore their layers' visibility/lock.
            // Hold every stored value they discard and their groups, without ~pick/~locked.
            if matches!(command, "object.join" | "polyline.edit") { Some(vec!["common".into(), "kind".into(), "~groups".into()]) } else { None }
        } else if matches!(
            command,
            "object.trim"
                | "object.extend"
                | "object.fillet"
                | "object.chamfer"
                | "object.offset"
                | "hatch.add"
                | "hatch.addGradient"
                | "object.arrayPath"
                | "object.join"
                | "polyline.edit"
        ) {
            let mut fields = vec!["kind".into(), "common".into()];
            let primary = p.get("handle").and_then(handle).or_else(|| hs.first().copied());
            if matches!(command, "object.trim" | "object.extend") && primary != Some(*h) {
                fields = vec!["kind".into(), "~visible".into(), "~space".into()];
            }
            if matches!(command, "object.fillet" | "object.chamfer")
                || matches!(command, "object.trim" | "object.extend" | "polyline.edit") && primary == Some(*h)
                || command == "object.offset" && p.get("erase") == Some(&Value::Bool(true))
            {
                fields.push("~locked".into());
            }
            Some(fields)
        } else if crate::acts::edits_a_table(command) {
            if let EntityKind::Table(table) = &e.kind {
                let (fields, texts) = crate::read_fields::table_fields(command, p, table);
                for cell in texts {
                    premises.push(need(format!("cell:{}", kdm_model::table_cell_tool_id(*h, cell)), vec!["text".into()]));
                }
                Some(fields)
            } else {
                Some(vec!["kind.type".into()])
            }
        } else {
            None
        };
        if let Some(mut fields) = fields {
            if by_place && matches!(command, "object.trim" | "object.extend") && targets.contains(h) {
                fields.push("~pick".into());
            }
            let mut premise = need(h.hex(), fields);
            if matches!(command, "arrange.bringToFront" | "arrange.sendToBack") {
                premise.positions = None;
            }
            premises.push(premise);
        } else {
            // What the command acts on and reads no narrower: every field (the SDK's target
            // default), also when another premise (a constraint's reference, an attachment)
            // names the object for less.
            premises.push(Premise::on(h.hex()));
        }
        if command == "dimension.space"
            && p.get("base").and_then(handle) == Some(*h)
            && p.get("spacing").and_then(Value::as_f64).is_none_or(|v| !v.is_finite() || v < 0.0)
            && let EntityKind::Dimension(dm) = &e.kind
        {
            premises.push(need(h.hex(), vec!["kind.style".into(), "~dimOverride_baselineSpacing".into(), "~dimOverride_scale".into()]));
            named_need(d, "DimStyle", &dm.style, &["name", "data.baselineSpacing", "data.scale"], &mut premises);
            let effective = d.dim_style(&dm.style).cloned().unwrap_or_default().with_overrides(&dm.overrides);
            if effective.scale <= 0.0 || !effective.scale.is_finite() {
                premises.push(need("drawing", vec!["header.DIMSCALE".into()]));
            }
        }
        if (matches!(command, "dimension.addContinue" | "dimension.addBaseline" | "object.explode")
            || command == "dimension.space" && p.get("base").and_then(handle) != Some(*h))
            && let EntityKind::Dimension(dim) = &e.kind
        {
            dim_style(d, &dim.style, &mut premises);
            let effective = d.dim_style(&dim.style).cloned().unwrap_or_default().with_overrides(&dim.overrides);
            dim_dependencies(d, &effective, true, &mut premises);
            if matches!(command, "dimension.addContinue" | "dimension.addBaseline") || effective.scale <= 0.0 || !effective.scale.is_finite() {
                premises.push(need("drawing", vec!["header.DIMSCALE".into()]));
            }
        }
        // array.rs::first_center uses referenced definition extents for the first
        // nonrotated polar source; stored insert fields alone do not determine it.
        if command == "object.arrayPolar"
            && p.get("rotate").and_then(Value::as_bool) == Some(false)
            && targets.first() == Some(h)
            && let EntityKind::Insert(i) = &e.kind
        {
            definition(d, &i.block, &mut premises);
            // extents.rs::in_block_visible filters nonzero member layers.
            for block in blocks(d, &e.kind).values() {
                for member in block
                    .entities
                    .iter()
                    .filter(|member| member.common.visible && member.common.layer != "0" && !matches!(member.kind, EntityKind::AttDef(_)))
                {
                    named_need(d, "Layer", &member.common.layer, &["data.on", "data.frozen"], &mut premises);
                }
            }
        }
        if matches!(command, "object.copyNested" | "object.explode")
            && let EntityKind::Insert(i) = &e.kind
        {
            definition(d, &i.block, &mut premises);
        }
        // explode_kind reads the definition's base and copies every member's geometry and
        // properties. A changed definition must not silently alter the expansion.
        if command == "object.explode"
            && let EntityKind::Insert(i) = &e.kind
            && let Some(block) = d.block(&i.block)
        {
            if let Some((id, _)) = named(d, "Block", &block.name) {
                premises.push(need(id, vec!["name".into(), "data.base".into(), "~memberOrder".into()]));
            }
            premises.extend(block.entities.iter().map(|e| Premise::on(e.handle.hex()).fields(["common", "kind"]).positions(Vec::<String>::new())));
        }
    }
    // Values that reference named records hold the identity, even when no data is copied.
    for (key, _) in c.name_params {
        for value in crate::params::values(p, key) {
            if let Some(name) = value.as_str() {
                let ty = c.object_types.iter().find(|(k, _)| k == key).and_then(|(_, types)| types.first()).copied().unwrap_or("");
                if let Some((id, _)) = named(d, ty, name) {
                    let identity_only_block = command == "multileader.add"
                        && p.get("blockAttach").and_then(Value::as_str).is_some_and(|attach| attach.eq_ignore_ascii_case("insertion"));
                    let centered_block = command == "multileader.add" && !identity_only_block;
                    let attribute_block = matches!(command, "point.divide" | "point.measure" | "attribute.sync");
                    let fields = if ty == "Block" && attribute_block {
                        vec!["name".into(), "~insertAttributes".into()]
                    } else if ty == "Block" && centered_block {
                        vec!["name".into(), "~blockCenter".into()]
                    } else if ty == "Block" && identity_only_block {
                        vec!["name".into()]
                    } else if ty == "Block" && !matches!(command, "block.addDialog" | "block.add" | "block.write") {
                        let mut fields = vec!["name".into(), "data.base".into(), "~memberOrder".into()];
                        if command == "block.insert" {
                            fields.push("data.xrefPath".into());
                        }
                        fields
                    } else if command == "layer.merge" && *key == "to" {
                        let current = d.header.str("CLAYER", "0");
                        let becomes_current = match p.get("from") {
                            Some(Value::String(n)) => n.trim().eq_ignore_ascii_case(&current),
                            Some(Value::Array(ns)) => {
                                ns.iter().take(10_000).filter_map(Value::as_str).any(|n| n.trim().eq_ignore_ascii_case(&current))
                            }
                            _ => false,
                        };
                        if becomes_current { vec!["name".into(), "data.frozen".into()] } else { vec!["name".into()] }
                    } else if command == "layout.duplicate" && *key == "from" {
                        vec!["name".into(), "data.page".into(), "data.view".into()]
                    } else if command == "layer.set" && *key == "name" {
                        let mut f = vec!["name".into()];
                        for key in [
                            "on",
                            "frozen",
                            "locked",
                            "plot",
                            "plotStyle",
                            "color",
                            "linetype",
                            "lineweight",
                            "transparency",
                            "newVpFreeze",
                            "description",
                        ] {
                            if p.get(key).is_some() {
                                let stored = match key {
                                    "plotStyle" => "plotStyle",
                                    "newVpFreeze" => "vpFreezeNew",
                                    key => key,
                                };
                                f.push(format!("data.{stored}"));
                            }
                        }
                        f
                    } else if matches!(command, "multileaderStyle.open" | "tableStyle.open") && *key == "name" {
                        let keys: &[&str] = if command == "tableStyle.open" {
                            &["textHeight", "margin", "title", "header"]
                        } else {
                            &["arrowSize", "textHeight", "landingGap", "dogleg", "textStyle"]
                        };
                        let mut fields = vec!["name".into()];
                        fields.extend(keys.iter().filter(|key| p.get(**key).is_some()).map(|key| format!("data.{key}")));
                        fields
                    } else if command == "group.edit" && *key == "name" {
                        let mut fields = vec!["name".into()];
                        if p.get("add").is_some() || p.get("remove").is_some() {
                            fields.push("data.members".into());
                        }
                        fields
                    } else if command == "textStyle.open" && *key == "name" {
                        let mut fields = vec!["name".into()];
                        fields.extend(
                            ["font", "bigFont", "height", "widthFactor", "oblique", "backwards", "upsideDown", "vertical", "annotative"]
                                .into_iter()
                                .filter(|key| p.get(*key).is_some())
                                .map(|key| format!("data.{key}")),
                        );
                        fields
                    } else if matches!(
                        command,
                        "textStyle.rename" | "dimensionStyle.rename" | "layout.rename" | "layerState.rename" | "textStyle.setCurrent"
                    ) || command == "property.linetype" && *key == "current"
                        || matches!(command, "object.setProperties" | "layer.match" | "multilineText.add")
                        || command == "multileaderStyle.open" && *key == "textStyle"
                        || matches!(command, "viewport.set" | "viewport.freezeLayer") && matches!(*key, "freeze[]" | "thaw[]")
                        || command == "object.changeSpace" && *key == "to"
                        || matches!(*key, "layout" | "layer" | "linetype")
                    {
                        vec!["name".into()]
                    } else {
                        vec!["name".into(), "data".into()]
                    };
                    premises.push(need(id, fields));
                    if ty == "Block"
                        && !identity_only_block
                        && !centered_block
                        && !attribute_block
                        && let Some(block) = d.block(name)
                    {
                        definition(d, &block.name, &mut premises);
                        if let Some((id, _)) = named(d, "Block", &block.name) {
                            let data = if matches!(command, "block.addDialog" | "block.add" | "block.write") { "data" } else { "data.base" };
                            premises.push(need(id, vec!["name".into(), data.into(), "~memberOrder".into()]));
                        }
                        for e in block.entities.iter() {
                            premises.push(Premise::on(e.handle.hex()).fields(["common", "kind"]).positions(Vec::<String>::new()));
                        }
                    }
                }
            }
        }
    }
    if command == "constraint.delete" {
        // Membership in the deleted set is an input even when that set is empty.
        premises.push(need("drawing", vec!["constraints".into()]));
        for constraint in &d.constraints {
            let removed = if let Some(ids) = p.get("ids").and_then(Value::as_array) {
                ids.iter().any(|id| id.as_u64() == Some(u64::from(constraint.id)))
            } else {
                constraint.refs.iter().any(|r| targets.contains(&r.handle))
            };
            if removed {
                premises.push(Premise::on(format!("constraint:{}", constraint.id)).positions(Vec::<String>::new()));
            }
        }
    }
    if command == "constraint.delete"
        && let Some(ids) = p.get("ids").and_then(Value::as_array)
    {
        for id in ids.iter().filter_map(|v| v.as_u64().map(|n| format!("constraint:{n}")).or_else(|| v.as_str().map(str::to_owned))) {
            premises.push(Premise::on(id).positions(Vec::<String>::new()));
        }
    }
    // Only a changed constraint/parameter list causes a global solve (constraints.rs:958).
    // Geometry writes solve connected components; settings and unrelated creation never solve.
    let constraint_edit = crate::read_fields::constraint_command(command);
    if constraint_edit {
        let mut fields = Vec::new();
        if command == "constraint.settings" {
            fields.extend(
                p.as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(key, _)| {
                        matches!(
                            key.as_str(),
                            "infer" | "distanceTolerance" | "angleTolerance" | "barsVisible" | "dimsVisible" | "barTransparency" | "autoTypes"
                        )
                    })
                    .map(|(key, _)| format!("parametric.settings.{key}")),
            );
        }
        if crate::acts::constraint_bars(command) || crate::acts::dynamic_dimensions(command) {
            let (visible, exceptions) =
                if crate::acts::dynamic_dimensions(command) { ("dimsVisible", "dimExceptions") } else { ("barsVisible", "barExceptions") };
            fields.extend([format!("parametric.settings.{visible}"), format!("parametric.settings.{exceptions}")]);
        }
        if command == "constraint.auto" {
            for (param, key) in [("tolerance", "distanceTolerance"), ("angleTolerance", "angleTolerance"), ("types", "autoTypes")] {
                if p.get(param).is_none() {
                    fields.push(format!("parametric.settings.{key}"));
                }
            }
        }
        if command == "constraint.parameters" && (p.get("name").is_some() || p.get("delete").is_some())
            || p.get("expr").and_then(Value::as_str).is_some_and(|expr| expr.chars().any(char::is_alphabetic))
        {
            fields.push("parametric.parameters".into());
        }
        if !fields.is_empty() {
            premises.push(need("drawing", fields));
        }
    }
    let scoped = reshapes && !(transform && copies)
        || matches!(command, "edit.cut" | "object.explode" | "object.changeSpace" | "region.add")
        || matches!(command, "block.addDialog" | "block.add") && p.get("keep").and_then(Value::as_str) != Some("retain")
        || command == "object.offset" && p.get("erase") == Some(&Value::Bool(true));
    // No-op display/settings paths do not invoke the global solver. Their assigned
    // values remain held above, so a changed setting cannot turn a stale no-op into a write.
    let constraint_effects = if constraint_edit { crate::scan::effects(d, s.space(), s.selection(), command, p)? } else { Value::Null };
    let constraint_changes = constraint_edit && constraint_effects.as_array().is_none_or(|changes| !changes.is_empty());
    if c.mutates && (constraint_changes || scoped) {
        if !constraint_edit {
            premises.extend(hs.iter().map(|h| need(h.hex(), vec!["~solve".into()])));
        } else if !d.constraints.is_empty() {
            premises.push(need("drawing", vec!["constraints".into()]));
            let handles: Vec<_> = d.constraints.iter().flat_map(|c| c.refs.iter().map(|r| r.handle)).collect();
            let environment = kition_cad_engine::cmd::constraints::premise(d, &handles);
            if environment.get("providers").and_then(Value::as_object).is_some_and(|providers| !providers.is_empty()) {
                premises.push(need("drawing", vec!["~constraintProviders".into()]));
            }
        }
        let dependencies = if command == "constraint.delete" {
            d.constraints
                .iter()
                .filter(|constraint| {
                    if let Some(ids) = p.get("ids").and_then(Value::as_array) {
                        !ids.iter().any(|id| id.as_u64() == Some(u64::from(constraint.id)))
                    } else {
                        !constraint.refs.iter().any(|r| targets.contains(&r.handle))
                    }
                })
                .collect()
        } else if constraint_edit {
            d.constraints.iter().collect()
        } else {
            kition_cad_engine::cmd::constraints::dependencies(d, &hs)
        };
        for constraint in dependencies {
            premises.push(Premise::on(format!("constraint:{}", constraint.id)).positions(Vec::<String>::new()));
            for r in &constraint.refs {
                if let Some(e) = entity(d, r.handle) {
                    let mut fields = crate::read_fields::curve_fields(e);
                    // The solver writes back only coordinates that actually move
                    // (constraints/model.rs:202-216); source locks otherwise do not matter.
                    if !constraint_edit
                        || constraint_effects.as_array().is_none_or(|changes| {
                            changes.iter().any(|change| change.get("id").and_then(Value::as_str) == Some(r.handle.hex().as_str()))
                        })
                    {
                        fields.push("~locked".into());
                    }
                    premises.push(need(r.handle.hex(), fields));
                }
            }
        }
    }
    let mut merged = merge_premises(premises);
    // An entity named by handle is held wherever it is in its space's list: no write by handle
    // reads its draw order (C8-cad-read). A place keeps positions; `covers` replays them.
    if !by_place && command != "constraint.delete" && !crate::acts::constraint_bars(command) && !crate::acts::dynamic_dimensions(command) {
        for h in &hs {
            merged.entry(json!(h.hex()).to_string()).or_insert_with(|| Premise::on(h.hex()));
        }
        for premise in merged.values_mut() {
            if premise.positions.is_none()
                && !matches!(command, "arrange.bringToFront" | "arrange.sendToBack")
                && premise.id.as_str().and_then(Handle::parse_hex).is_some_and(|h| hs.contains(&h))
            {
                premise.positions = Some(Vec::new());
            }
        }
    }
    let mut objects: Vec<Value> = if by_place { place_objects.iter().map(|h| json!(h.hex())).collect() } else { vec![] };
    let mut covers = if by_place { vec!["kind".into(), "~pick".into()] } else { Vec::new() };
    if let Some((ty, name)) = record_address(command, p) {
        if let Some(id) = record_names(d).get(record_name_key(ty, name)).cloned() {
            objects.push(id);
        }
        covers.push("name".into());
        if ty != "layout" {
            covers.push("~recordNames".into());
        }
    }
    Ok(Some(Under {
        objects,
        premises: merged.into_values().collect(),
        covers: (!covers.is_empty()).then_some(Covers { fields: covers }),
        content: if command == "attribute.edit" {
            hs.iter()
                .filter(|h| entity(d, **h).is_some_and(|e| matches!(e.kind, EntityKind::Insert(_))))
                .map(|h| kition_kit::ContentPremise {
                    id: json!(h.hex()),
                    field: "kind.attribs".into(),
                    region: p.get("values").cloned().unwrap_or(json!({})),
                })
                .collect()
        } else {
            Vec::new()
        },
        ..Default::default()
    }))
}

fn named(d: &Drawing, ty: &str, name: &str) -> Option<(String, String)> {
    let names: Vec<(&str, &str)> = match ty {
        "Layer" => d.layers.iter().map(|x| ("layer", x.name.as_str())).collect(),
        "Linetype" => d.linetypes.iter().map(|x| ("linetype", x.name.as_str())).collect(),
        "TextStyle" => d.text_styles.iter().map(|x| ("textstyle", x.name.as_str())).collect(),
        "DimStyle" => d.dim_styles.iter().map(|x| ("dimstyle", x.name.as_str())).collect(),
        "Block" => d.blocks.keys().map(|x| ("block", x.as_str())).collect(),
        "Layout" => d.layouts.iter().map(|x| ("layout", x.name.as_str())).collect(),
        "TableStyle" => d.table_styles.iter().map(|x| ("tablestyle", x.name.as_str())).collect(),
        "MLeaderStyle" => d.mleader_styles.iter().map(|x| ("mleaderstyle", x.name.as_str())).collect(),
        "View" => d.views.iter().map(|x| ("view", x.name.as_str())).collect(),
        "Ucs" => d.ucss.iter().map(|x| ("ucs", x.name.as_str())).collect(),
        "LayerState" => d.layer_states.iter().map(|x| ("layerstate", x.name.as_str())).collect(),
        "PlotStyleTable" => d.plot_style_tables.iter().map(|x| ("plotstyletable", x.name.as_str())).collect(),
        "Group" => d.groups.iter().map(|x| ("group", x.name.as_str())).collect(),
        _ => vec![],
    };
    names
        .into_iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .and_then(|(prefix, n)| d.record_id(prefix, n, 0).map(|id| (format!("{prefix}:{id}"), n.to_owned())))
}

#[derive(Serialize, Deserialize)]
struct PickFact {
    visible: bool,
    space: Option<Space>,
    order: Option<usize>,
    blocks: BTreeMap<String, PickBlock>,
    layers: Vec<kition_cad_doc::Layer>,
}

#[derive(Serialize, Deserialize)]
struct PickBlock {
    data: kition_cad_doc::Block,
    // Block's own serde representation intentionally skips its entity store.
    entities: Vec<Entity>,
}

fn blocks(d: &Drawing, kind: &EntityKind) -> BTreeMap<String, Arc<kition_cad_doc::Block>> {
    let mut found = BTreeMap::new();
    let mut queue = match kind {
        EntityKind::Insert(i) => VecDeque::from([(i.block.clone(), 0usize)]),
        _ => VecDeque::new(),
    };
    while let Some((name, depth)) = queue.pop_front() {
        if found.contains_key(&name) || depth >= kition_cad_doc::MAX_BLOCK_DEPTH {
            continue;
        }
        if let Some(block) = d.block(&name) {
            queue.extend(
                block.entities.iter().filter_map(|e| if let EntityKind::Insert(i) = &e.kind { Some((i.block.clone(), depth + 1)) } else { None }),
            );
            found.insert(name, block.clone());
        }
    }
    found
}

pub fn derived(d: &Drawing, ids: &[Value], names: &[String]) -> Result<Vec<DerivedValues>, String> {
    let wanted = |n: &str| names.is_empty() || names.iter().any(|w| w == n);
    let chosen: BTreeSet<_> = ids.iter().filter_map(handle).collect();
    let ranks: BTreeMap<_, _> = d
        .model
        .iter()
        .enumerate()
        .map(|(i, e)| (e.handle, i))
        .chain(d.layouts.iter().flat_map(|l| l.entities.iter().enumerate().map(|(i, e)| (e.handle, i))))
        .collect();
    let mut out = Vec::new();
    if ids.is_empty() || ids.iter().any(|id| id == "drawing") {
        let mut values = serde_json::Map::new();
        if wanted("constraintProviders") {
            let handles: Vec<_> = d.constraints.iter().flat_map(|c| c.refs.iter().map(|r| r.handle)).collect();
            let environment = kition_cad_engine::cmd::constraints::premise(d, &handles);
            values.insert("constraintProviders".into(), environment.get("providers").cloned().unwrap_or(Value::Null));
        }
        if wanted("layoutCanDelete") {
            values.insert("layoutCanDelete".into(), json!(d.layouts.len() > 1));
        }
        if wanted("history") {
            values.insert("history".into(), kdm_model::canonical(d));
        }
        if wanted("centerLinetype") {
            values.insert("centerLinetype".into(), named(d, "Linetype", "CENTER").map_or(Value::Null, |(id, _)| json!(id)));
        }
        if wanted("recordNames") {
            values.insert("recordNames".into(), record_names(d));
        }
        if wanted("groupTargets") {
            values.insert("groupTargets".into(), crate::scan::groups(d));
        }
        if wanted("layerMembers") {
            values.insert("layerMembers".into(), crate::scan::layer_members(d));
        }
        for ty in [
            "all",
            "blocks",
            "dimstyles",
            "groups",
            "layers",
            "linetypes",
            "mleaderstyles",
            "tablestyles",
            "textstyles",
            "zeroLength",
            "emptyText",
            "other",
        ] {
            let key = format!("purge_{ty}");
            if wanted(&key) {
                values.insert(
                    key,
                    if ty == "other" { json!([]) } else { crate::scan::effects(d, Space::Model, vec![], "file.purge", &json!({"type":ty}))? },
                );
            }
        }
        if wanted("audit") {
            values.insert("audit".into(), crate::scan::effects(d, Space::Model, vec![], "file.audit", &json!({"fix":true}))?);
        }
        if wanted("object.deleteDuplicates") {
            let facts = std::iter::once(Space::Model)
                .chain(d.layouts.iter().map(|l| Space::Paper(l.name.clone())))
                .map(|space| crate::scan::effects(d, space, vec![], "object.deleteDuplicates", &json!({})))
                .collect::<Result<Vec<_>, _>>()?;
            values.insert("overkill".into(), json!(facts));
        }
        if wanted("texts") {
            values.insert("texts".into(), drawing_texts(d));
        }
        if wanted("queries") {
            values.insert("queries".into(), crate::query::fact(d)?);
        }
        if wanted("modelSelection") {
            values.insert("modelSelection".into(), crate::query::selectable(d, &d.model));
        }
        if wanted("cutters") {
            values.insert("cutters".into(), json!({
                "model":kition_cad_engine::cmd::cutting_edge_handles(d, &Space::Model),
                "layouts":d.layouts.iter().map(|l| (&l.name,kition_cad_engine::cmd::cutting_edge_handles(d, &Space::Paper(l.name.clone())))).collect::<BTreeMap<_,_>>()
            }));
        }
        if wanted("insertRefs") {
            values.insert(
                "insertRefs".into(),
                json!(
                    d.blocks
                        .values()
                        .map(|b| (b.name.to_ascii_lowercase(), kition_cad_engine::cmd::attribute_references(d, &b.name)))
                        .collect::<BTreeMap<_, _>>()
                ),
            );
        }
        if wanted("modelBounds") {
            let b = d.extents(&Space::Model);
            values.insert("modelBounds".into(), if b.is_empty() { Value::Null } else { json!([b.min.x, b.min.y, b.max.x, b.max.y]) });
        }
        if wanted("newVpLayers") {
            values.insert(
                "newVpLayers".into(),
                json!(d.layers.iter().filter(|l| l.vp_freeze_new).map(|l| (&l.name, d.record_id("layer", &l.name, 0))).collect::<Vec<_>>()),
            );
        }
        if wanted("viewportSeed") {
            values.insert(
                "viewportSeed".into(),
                json!(
                    d.layouts
                        .iter()
                        .flat_map(|l| l.entities.iter())
                        .filter_map(|e| if let EntityKind::Viewport(v) = &e.kind { Some(v.id) } else { None })
                        .max()
                        .unwrap_or(1)
                        .max(1)
                ),
            );
        }
        if wanted("nextLayoutName") {
            values.insert(
                "nextLayoutName".into(),
                json!((1..=10_000).map(|i| format!("Layout{i}")).find(|n| d.layout(n).is_none()).unwrap_or_else(|| "Layout".into())),
            );
        }
        if wanted("nextGroupName") {
            let limit = d.groups.len().saturating_add(1);
            values.insert(
                "nextGroupName".into(),
                json!(
                    (1..=limit)
                        .map(|i| format!("*A{i}"))
                        .find(|n| !d.groups.iter().any(|g| g.name.eq_ignore_ascii_case(n)))
                        .unwrap_or_else(|| format!("*A{limit}"))
                ),
            );
        }
        if wanted("layoutSeed") {
            values.insert("layoutSeed".into(), json!(d.layouts.iter().map(|l| l.tab_order).max().unwrap_or(0)));
        }
        out.push(DerivedValues { id: json!("drawing"), values });
    }
    if wanted("canDelete") || wanted("canFreeze") {
        let facts = kition_cad_engine::cmd::record_eligibility(d);
        for (ty, predicate, value) in [
            ("TextStyle", "textStyleCanDelete", "canDelete"),
            ("DimStyle", "dimStyleCanDelete", "canDelete"),
            ("Layer", "layerCanFreeze", "canFreeze"),
        ] {
            if !wanted(value) {
                continue;
            }
            for (name, fact) in facts.get(predicate).and_then(Value::as_object).into_iter().flatten() {
                if let Some((id, _)) = named(d, ty, name)
                    && (ids.is_empty() || ids.contains(&json!(id)))
                {
                    out.push(DerivedValues { id: json!(id), values: serde_json::Map::from_iter([(value.into(), fact.clone())]) });
                }
            }
        }
    }
    if wanted("memberOrder") || wanted("insertAttributes") || wanted("blockCenter") {
        for (ty, name, members) in d
            .blocks
            .values()
            .map(|b| ("Block", b.name.as_str(), &b.entities))
            .chain(d.layouts.iter().map(|l| ("Layout", l.name.as_str(), &l.entities)))
        {
            if let Some((id, _)) = named(d, ty, name)
                && (ids.is_empty() || ids.contains(&json!(id)))
            {
                let mut values = serde_json::Map::new();
                if wanted("memberOrder") {
                    values.insert("memberOrder".into(), json!(members.iter().map(|e| e.handle.hex()).collect::<Vec<_>>()));
                }
                if ty == "Block"
                    && wanted("insertAttributes")
                    && let Some(block) = d.block(name)
                {
                    values.insert("insertAttributes".into(), insert_attributes(block));
                }
                if ty == "Block" && wanted("blockCenter") {
                    values.insert("blockCenter".into(), block_center(d, name));
                }
                if !values.is_empty() {
                    out.push(DerivedValues { id: json!(id), values });
                }
            }
        }
    }
    if wanted("chspaceViewport") {
        for l in &d.layouts {
            if let Some((id, _)) = named(d, "Layout", &l.name)
                && (ids.is_empty() || ids.contains(&json!(id)))
            {
                let viewport =
                    kition_cad_engine::cmd::chspace_context(d, &Space::Model, Some(&l.name), None).ok().and_then(|(_, vp)| vp).map(|h| h.hex());
                out.push(DerivedValues { id: json!(id), values: serde_json::Map::from_iter([("chspaceViewport".into(), json!(viewport))]) });
            }
        }
    }
    if wanted("selection") {
        for l in &d.layouts {
            if let Some(id) = d.record_id("layout", &l.name, 0).map(|id| json!(format!("layout:{id}")))
                && (ids.is_empty() || ids.contains(&id))
            {
                out.push(DerivedValues { id, values: serde_json::Map::from_iter([("selection".into(), crate::query::selectable(d, &l.entities))]) });
            }
        }
    }
    out.extend(all(d).filter(|e| ids.is_empty() || chosen.contains(&e.handle)).map(|e| {
        let mut values = serde_json::Map::new();
        if wanted("groups") {
            values.insert("groups".into(), crate::scan::groups_for(d, e.handle));
        }
        if wanted("attributes") {
            values.insert("attributes".into(), crate::read_fields::attribute_values(e));
        }
        // Independent scalars avoid coarse parent-map diffs on first override insertion.
        // Null retains absence as a fact; each scalar is exactly its canonical stored field.
        for (_, field) in kition_cad_doc::DIMVARS {
            let key = format!("dimOverride_{field}");
            if wanted(&key) {
                let value = if let EntityKind::Dimension(dm) = &e.kind { dm.overrides.get(*field).cloned().unwrap_or(Value::Null) } else { Value::Null };
                values.insert(key, value);
            }
        }
        if wanted("polylineCanClose") {
            values.insert("polylineCanClose".into(), json!(matches!(&e.kind, EntityKind::LwPolyline(pl) if pl.vertices.len() > 1)));
        }
        if wanted("linearDimension") {
            values.insert("linearDimension".into(), json!(matches!(&e.kind, EntityKind::Dimension(dm) if matches!(dm.kind, DimKind::Linear { .. } | DimKind::Aligned))));
        }
        if wanted("words") {
            values.insert("words".into(), words(e));
        }
        if wanted("solve") {
            values.insert("solve".into(),kition_cad_engine::cmd::constraints::premise(d,&[e.handle]));
        }
        if wanted("locked") {
            values.insert("locked".into(), json!(d.layer(&e.common.layer).is_some_and(|l| l.locked)));
        }
        if wanted("visible") {
            values.insert("visible".into(), json!(d.is_visible(e)));
        }
        if wanted("space") {
            values.insert("space".into(), json!(d.space_of(e.handle)));
        }
        if wanted("pick") {
            let space = d.space_of(e.handle);
            let order = ranks.get(&e.handle).copied();
            let definitions = blocks(d,&e.kind);
            let layer_names: BTreeSet<_> = definitions.values().flat_map(|b| b.entities.iter().map(|e| e.common.layer.as_str())).collect();
            let layers = d.layers.iter().filter(|l| layer_names.contains(l.name.as_str())).cloned().collect();
            let blocks = definitions.into_iter().map(|(name, b)| (name,PickBlock {data:b.as_ref().clone(),entities:b.entities.iter().map(|e| e.as_ref().clone()).collect()})).collect();
            let fact = PickFact { visible:d.is_visible(e), space, order, blocks, layers };
            values.insert("pick".into(),serde_json::to_value(fact).map_err(|e| e.to_string())?);
        }
        if wanted("boundaryOutline") {
            // Finite lines are completely determined by their held endpoints and
            // pick state. Duplicating those coordinates here would turn numeric
            // moves into array edits and prevent the SDK's footprint compaction.
            // Curved sampling and infinite-line clipping still need this projection.
            let outline = if matches!(e.kind, EntityKind::Line(_)) {
                None
            } else {
                d.space_of(e.handle).map(|space| kition_cad_engine::cmd::boundary_outline(d, &space, e))
            };
            values.insert("boundaryOutline".into(), json!(outline));
        }
        if wanted("grid") {
            let grid = if let EntityKind::Table(t) = &e.kind { json!({"rows":t.row_heights.len(),"cols":t.col_widths.len(),"merges":t.cells.iter().map(|r| r.iter().map(|c| c.merged).collect::<Vec<_>>()).collect::<Vec<_>>()}) } else {Value::Null};
            values.insert("grid".into(),grid);
        }
        Ok(DerivedValues { id:json!(e.handle.hex()),values })
    }).collect::<Result<Vec<_>,String>>()?);
    Ok(out)
}

fn texts(store: &kition_cad_doc::EntityStore) -> Value {
    Value::Array(
        store
            .iter()
            .filter_map(|e| {
                let words = words(e);
                if words.is_null() {
                    return None;
                }
                Some(json!([e.handle.hex(), e.kind.type_name(), words]))
            })
            .collect(),
    )
}

fn words(e: &Entity) -> Value {
    match &e.kind {
        EntityKind::Text(t) => json!(t.value),
        EntityKind::MText(t) => json!(t.contents),
        EntityKind::Insert(i) => json!(i.attribs.iter().map(|a| &a.text.value).collect::<Vec<_>>()),
        EntityKind::Dimension(d) => json!(d.text),
        EntityKind::MLeader(m) => json!(m.text.as_ref().map(|t| &t.contents)),
        EntityKind::Table(t) => json!(t.cells.iter().map(|r| r.iter().map(|c| &c.text).collect::<Vec<_>>()).collect::<Vec<_>>()),
        _ => Value::Null,
    }
}

fn drawing_texts(d: &Drawing) -> Value {
    json!({"model":texts(&d.model),"layouts":d.layouts.iter().map(|l|(&l.name,texts(&l.entities))).collect::<BTreeMap<_,_>>()})
}

fn matching_texts(texts: &Value, space: &Space, p: &Value) -> Value {
    fn matches(v: &Value, p: &Value) -> bool {
        match v {
            Value::String(t) => kition_cad_engine::cmd::find_matches(t, p),
            Value::Array(a) => a.iter().any(|v| matches(v, p)),
            _ => false,
        }
    }
    let entries = match space {
        Space::Model => texts.get("model"),
        Space::Paper(n) => texts.get("layouts").and_then(|l| l.get(n)),
    };
    json!(entries.and_then(Value::as_array).into_iter().flatten().filter(|e| e.get(2).is_some_and(|v| matches(v, p))).collect::<Vec<_>>())
}

fn name_history(d: &Drawing, ty: &str, name: &str, asked: &[CoveredAsk], aggregate: bool) -> Result<CoveredAt, String> {
    let ty = ty.to_ascii_lowercase();
    let prefix = format!("{ty}:");
    let mut names: BTreeMap<String, String> = d
        .record_names()
        .into_iter()
        .filter(|(table, _)| table.eq_ignore_ascii_case(&ty))
        .filter_map(|(table, n)| d.record_id(&table, &n, 0).map(|id| (format!("{table}:{id}"), n)))
        .collect();
    let query = names.get(name).cloned().unwrap_or_else(|| name.into());
    let current: BTreeSet<_> = names.iter().filter(|(_, n)| n.eq_ignore_ascii_case(&query)).map(|(id, _)| id.clone()).collect();
    if aggregate
        && let Some(old) = asked.iter().find(|a| a.id == "drawing").and_then(|a| {
            a.before
                .get("~recordNames")
                .filter(|v| v.is_object())
                .filter(|_| !a.unknown.iter().any(|f| f == "~recordNames" || f.starts_with("~recordNames.")))
        })
    {
        return Ok(CoveredAt {
            under: old.get(record_name_key(&ty, &query)).cloned().into_iter().collect(),
            covered: asked.iter().map(|a| Covered { id: a.id.clone(), covered: Some(a.id == "drawing"), ..Default::default() }).collect(),
        });
    }
    for a in asked {
        let Some(id) = a.id.as_str().filter(|id| id.starts_with(&prefix)) else { continue };
        if a.created {
            names.remove(id);
        } else if a.unknown.iter().any(|f| f == "name" || f.starts_with("name.")) {
            return Err("the record name at the observation is unavailable".into());
        } else if let Some(name) = a.before.get("name").and_then(Value::as_str) {
            names.insert(id.into(), name.into());
        } else if a.deleted {
            return Err("the deleted record's observed name is unavailable".into());
        }
    }
    let old: BTreeSet<_> = names.into_iter().filter(|(_, n)| n.eq_ignore_ascii_case(&query)).map(|(id, _)| id).collect();
    Ok(CoveredAt {
        under: old.iter().map(|id| json!(id)).collect(),
        covered: asked
            .iter()
            .map(|a| Covered {
                id: a.id.clone(),
                covered: Some(a.id.as_str().is_some_and(|id| old.contains(id) || current.contains(id))),
                ..Default::default()
            })
            .collect(),
    })
}

pub fn covered(s: &Session, command: &str, p: &Value, asked: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
    let base = covered_native(s, command, p, asked)?;
    let d = s.doc().map_err(|e| e.to_string())?;
    let names = destination_names(command, p, d);
    if names.is_empty() {
        return Ok(base);
    }
    let mut result = base.unwrap_or_else(|| CoveredAt {
        under: vec![],
        covered: asked.iter().map(|a| Covered { id: a.id.clone(), covered: Some(false), ..Default::default() }).collect(),
    });
    for (ty, name) in names {
        let at = name_history(d, &ty, &name, asked, false)?;
        for id in at.under {
            if !result.under.contains(&id) {
                result.under.push(id);
            }
        }
        for fact in &mut result.covered {
            if at.covered.iter().any(|n| n.id == fact.id && n.covered == Some(true)) {
                fact.covered = Some(true);
            }
        }
    }
    Ok(Some(result))
}

fn covered_native(s: &Session, command: &str, p: &Value, asked: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
    if command == "command.runLine" && s.current_prompt().is_none() {
        let text = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
        if let Some((name, params)) = text.split_once(' ')
            && params.trim_start().starts_with('{')
            && let Ok(params) = serde_json::from_str::<Value>(params.trim())
        {
            let id = kition_cad_engine::cmd::resolve_alias(&name.to_ascii_lowercase());
            if id != "command.runLine" {
                return covered(s, &id, &params, asked);
            }
        }
        if let Some(spec) = text.split_whitespace().next().and_then(kition_cad_engine::cmd::resolve_typed)
            && spec.interactive.is_none()
            && spec.id != "property.linetypeScale"
        {
            return covered(s, spec.id, &json!({}), asked);
        }
    }
    if command == "layout.manage" {
        let (command, p) = kition_cad_engine::cmd::layout_operation(p).map_err(|e| e.to_string())?;
        return covered(s, command, &p, asked);
    }
    if matches!(command, "layout.add" | "layout.duplicate" | "layout.set")
        && let Some((ty, name)) = record_address(command, p)
    {
        return name_history(s.doc().map_err(|e| e.to_string())?, ty, name, asked, false).map(Some);
    }
    if command == "namedObject.rename" {
        return name_history(s.doc().map_err(|e| e.to_string())?, rename_type(p), p.get("from").and_then(Value::as_str).unwrap_or(""), asked, false)
            .map(Some);
    }
    if let Some((ty, name)) = record_address(command, p) {
        let d = s.doc().map_err(|e| e.to_string())?;
        let names = name_history(d, ty, name, asked, true)?;
        let mut result = if spatial(command, p) {
            let mut spatial_params = p.clone();
            if let Some(fields) = spatial_params.as_object_mut() {
                fields.remove("name");
            }
            covered(s, command, &spatial_params, asked)?.ok_or("the spatial address at the observation is unavailable")?
        } else {
            CoveredAt {
                under: vec![],
                covered: asked.iter().map(|a| Covered { id: a.id.clone(), covered: Some(false), ..Default::default() }).collect(),
            }
        };
        result.under.extend(names.under);
        for fact in &mut result.covered {
            if fact.covered.is_some() && names.covered.iter().any(|n| n.id == fact.id && n.covered == Some(true)) {
                fact.covered = Some(true);
            }
        }
        return Ok(Some(result));
    }
    if crate::query::selects(command) {
        return crate::query::covered(s, command, p, asked);
    }
    if command == "arrange.ungroup" {
        return crate::scan::ungroup_covered(s, p, asked);
    }
    if command == "edit.find" {
        // Replay stored predicate inputs; a drawing-wide eager text fact can exceed
        // the SDK receipt-value bound even for a small drawing.
        let mut old = s.doc().map_err(|e| e.to_string())?.clone();
        let mut unknown = BTreeSet::new();
        for a in asked {
            let Some(h) = handle(&a.id) else { continue };
            if a.created {
                old.remove_entity(h);
                continue;
            }
            if a.unknown.iter().any(|field| field == "kind" || field.starts_with("kind.") || field == "~space") {
                unknown.insert(h);
                continue;
            }
            let kind = if let Some(value) = a.before.get("kind") {
                let empty = kdm_model::Ids::default();
                let ids = s.kdm().map(|k| k.model().ids()).unwrap_or(&empty);
                kdm_model::read_entity_kind(value.clone(), ids).map_err(|e| e.to_string())?
            } else if let Some(e) = entity(&old, h) {
                e.kind.clone()
            } else {
                unknown.insert(h);
                continue;
            };
            let space = if let Some(value) = a.before.get("~space") {
                serde_json::from_value::<Option<Space>>(value.clone()).map_err(|e| e.to_string())?
            } else {
                old.space_of(h)
            };
            old.remove_entity(h);
            if let Some(store) = space.as_ref().and_then(|space| old.space_mut(space)) {
                store.push(Entity { handle: h, kind, common: Common::default() });
            }
        }
        let matching = matching_texts(&drawing_texts(&old), &s.space(), p);
        let under: Vec<_> = matching.as_array().into_iter().flatten().filter_map(|e| e.get(0).cloned()).collect();
        return Ok(Some(CoveredAt {
            covered: asked
                .iter()
                .map(|a| Covered {
                    id: a.id.clone(),
                    covered: if handle(&a.id).is_some_and(|h| unknown.contains(&h)) { None } else { Some(under.contains(&a.id)) },
                    gone: a.deleted,
                    ..Default::default()
                })
                .collect(),
            under,
        }));
    }
    if !spatial(command, p) {
        return Ok(None);
    }
    let mut d = s.doc().map_err(|e| e.to_string())?.clone();
    let mut orders: Vec<(Space, BTreeMap<Handle, usize>)> = Vec::new();
    let mut unavailable = BTreeSet::new();
    for a in asked {
        let Some(h) = handle(&a.id) else { continue };
        if a.created {
            d.remove_entity(h);
            continue;
        }
        if !a.unknown.is_empty() {
            continue;
        }
        let now = derived(&d, std::slice::from_ref(&a.id), &["pick".into()])?.into_iter().next();
        let fact = a.before.get("~pick").or_else(|| now.as_ref().and_then(|v| v.values.get("pick")));
        if let Some(fact) = fact {
            let fact: PickFact = serde_json::from_value(fact.clone()).map_err(|e| e.to_string())?;
            let kind = if let Some(kind) = a.before.get("kind") {
                let empty = kdm_model::Ids::default();
                let ids = s.kdm().map(|k| k.model().ids()).unwrap_or(&empty);
                kdm_model::read_entity_kind(kind.clone(), ids).map_err(|e| e.to_string())?
            } else if let Some(e) = entity(&d, h) {
                e.kind.clone()
            } else {
                unavailable.insert(h);
                continue;
            };
            if let Some((space, rank)) = fact.space.as_ref().zip(fact.order) {
                if let Some((_, ranks)) = orders.iter_mut().find(|(s, _)| s == space) {
                    ranks.insert(h, rank);
                } else {
                    orders.push((space.clone(), BTreeMap::from([(h, rank)])));
                }
            }
            for layer in fact.layers {
                if let Some(current) = d.layers.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&layer.name)) {
                    *current = layer;
                } else {
                    d.layers.push(layer);
                }
            }
            for (name, mut block) in fact.blocks {
                block.data.entities = kition_cad_doc::EntityStore::new();
                for e in block.entities {
                    block.data.entities.push(e);
                }
                d.blocks.insert(name, Arc::new(block.data));
            }
            let restored =
                Entity { handle: h, kind, common: Common { visible: fact.visible, layer: "premise:visible".into(), ..Default::default() } };
            let existing_space = d.space_of(h);
            if existing_space == fact.space {
                let _ = d.modify_entity(h, |e| *e = restored.clone());
            } else {
                d.remove_entity(h);
                if let Some(space) = fact.space
                    && let Some(store) = d.space_mut(&space)
                {
                    store.push(restored);
                }
            }
        } else {
            unavailable.insert(h);
        }
    }
    // Put changed entities back at their historical ranks and preserve the relative order
    // of unchanged entities. Created entities were removed above.
    for (space, ranks) in orders {
        if let Some(store) = d.space_mut(&space) {
            let mut fixed = BTreeMap::new();
            let mut rest = Vec::new();
            for e in store.iter() {
                if let Some(rank) = ranks.get(&e.handle) {
                    fixed.insert(*rank, e.clone());
                } else {
                    rest.push(e.clone());
                }
            }
            let mut rest = rest.into_iter();
            let mut restored = kition_cad_doc::EntityStore::new();
            for rank in 0..store.len() {
                if let Some(e) = fixed.remove(&rank).or_else(|| rest.next()) {
                    restored.push(e.as_ref().clone());
                }
            }
            for e in fixed.into_values().chain(rest) {
                restored.push(e.as_ref().clone());
            }
            *store = restored;
        }
    }
    let mut hs = picked(&d, &s.space(), command, p);
    if implicit(command, p) {
        hs.extend(dependencies(s, &d, command, p));
    }
    Ok(Some(CoveredAt {
        under: hs.iter().map(|h| json!(h.hex())).collect(),
        covered: asked
            .iter()
            .map(|a| Covered {
                id: a.id.clone(),
                covered: if a.unknown.is_empty() && !handle(&a.id).is_some_and(|h| unavailable.contains(&h)) {
                    Some(handle(&a.id).is_some_and(|h| hs.contains(&h)))
                } else {
                    None
                },
                ..Default::default()
            })
            .collect(),
    }))
}

pub fn carried(d: &Drawing, ids: &[Value], changes: &[Value]) -> Vec<Carried> {
    ids.iter()
        .filter_map(|id| {
            let h = handle(id)?;
            let e = entity(d, h)?;
            let EntityKind::Insert(i) = &e.kind else { return None };
            let moved = changes
                .iter()
                .filter(|c| c.get("id") == Some(id))
                .flat_map(|c| c.get("edits").and_then(Value::as_array).into_iter().flatten())
                .filter_map(|e| e["field"].as_str())
                .any(|f| ["kind.insert", "kind.rotation", "kind.scale"].iter().any(|p| f == *p || f.starts_with(&format!("{p}."))));
            if !moved {
                return None;
            }
            let count = d.block(&i.block)?.entities.len();
            Some(Carried {
                id: id.clone(),
                carries: Carries { count: count as u64, what: "block definition entities in this instance's drawing space".into() },
            })
        })
        .collect()
}
