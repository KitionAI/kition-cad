//! Object parameters follow the engine's resolvers, not integer-looking schema words.
use std::collections::HashMap;
use std::sync::OnceLock;

use kition_kit::KitCommand;
use serde_json::Value;

#[derive(Default)]
struct Declaration {
    objects: &'static [&'static str],
    types: &'static [(&'static str, &'static [&'static str])],
    names: &'static [(&'static str, &'static str)],
}

const HANDLES: &[&str] = &["handle", "handles", "h1", "h2", "h3", "baseHandle", "handles2"];

fn extra_handles(id: &str) -> &'static [&'static str] {
    match id {
        "object.matchProperties" => &["source", "targets"],
        "array" | "object.arrayPath" => &["path"],
        "object.changeSpace" => &["viewport"],
        "trim" | "extend" => &["edges"],
        // Removal compares an opaque handle with membership; it never resolves the entity.
        "group.edit" => &["add"],
        "dim" | "dimension.addLinear" | "spline" => &["object"],
        "dimension.addAngular" => &["lines", "arc"],
        "dimension.space" => &["base"],
        "object.measure" => &["bounds", "intersections", "distance[]"],
        _ => &[],
    }
}

fn names(id: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match id {
        "layer.set" => &[("name", &["Layer"]), ("linetype", &["Linetype"])],
        "layer.setCurrent" | "layer.delete" => &[("name", &["Layer"])],
        "layer.match" => &[("layer", &["Layer"])],
        "layer.merge" => &[("from", &["Layer"]), ("to", &["Layer"])],
        "layer.deleteWithObjects" => &[("name", &["Layer"]), ("names", &["Layer"])],
        "object.setProperties" => &[("dimStyle", &["DimStyle"]), ("layer", &["Layer"]), ("linetype", &["Linetype"])],
        "layer.add" => &[("linetype", &["Linetype"])],
        "style" | "textStyle.setCurrent" | "textStyle.delete" => &[("name", &["TextStyle"])],
        "textStyle.rename" => &[("from", &["TextStyle"])],
        "dimensionStyle.open" | "dimensionStyle.openManager" | "dimensionStyle.setCurrent" | "dimensionStyle.delete" | "dimensionStyle.list" => {
            &[("name", &["DimStyle"])]
        }
        "dimensionStyle.rename" => &[("from", &["DimStyle"])],
        "tableStyle.open" => &[("name", &["TableStyle"])],
        "multileaderStyle.open" => &[("name", &["MLeaderStyle"]), ("textStyle", &["TextStyle"])],
        "insert" | "attribute.manage" | "attribute.sync" | "block.write" => &[("name", &["Block"])],
        "block" | "block.add" => &[("name", &["Block"])],
        "divide" | "measure" => &[("block", &["Block"])],
        "group.edit" | "ungroup" => &[("name", &["Group"])],
        "layout.set" | "layout.delete" => &[("name", &["Layout"])],
        "layout.rename" | "layout.duplicate" => &[("from", &["Layout"])],
        "layerState.save" | "layerState.restore" | "layerState.delete" => &[("name", &["LayerState"])],
        "layerState.rename" => &[("from", &["LayerState"])],
        "linetype" => &[("current", &["Linetype"])],
        "multilineText.add" => &[("style", &["TextStyle"])],
        "multileader.add" => &[("block", &["Block"])],
        "plot.pageSetup" | "plot" | "file.exportPdf" => &[("layout", &["Layout"])],
        "viewport.set" | "viewport.freezeLayer" | "vports.set" => &[("freeze[]", &["Layer"]), ("thaw[]", &["Layer"])],
        "object.changeSpace" => &[("to", &["Layout"])],
        _ => &[],
    }
}

pub fn declare(commands: &mut [KitCommand]) {
    for command in commands
        .iter_mut()
        .filter(|c| c.id == "constraint.delete" || c.id.starts_with("constraint.bars") || c.id.starts_with("constraint.dynamicDimensions"))
    {
        if let Some(schema) = command.schema.as_mut().and_then(Value::as_object_mut)
            && schema.contains_key("handles")
        {
            schema.insert("handles".into(), serde_json::json!({"type":"array","items":{"oneOf":[{"type":"string"},{"type":"integer"}]}}));
        }
    }
    // draw2::run_xline2 supports reference-line offset forms which the textual
    // union parser stops reading after the earlier bisector form.
    for command in commands.iter_mut().filter(|c| c.id == "constructionLine.add") {
        if let Some(schema) = command.schema.as_mut().and_then(Value::as_object_mut) {
            schema.insert("handle".into(), serde_json::json!({"type":"string"}));
            schema.insert("distance".into(), serde_json::json!({"type":"number"}));
            schema.insert("side".into(), serde_json::json!({"type":"array","items":{"type":"number"}}));
        }
    }
    static DECLARATIONS: OnceLock<HashMap<String, Declaration>> = OnceLock::new();
    let declarations = DECLARATIONS.get_or_init(|| {
        commands
            .iter()
            .map(|c| {
                let has = |key: &str| c.schema.as_ref().and_then(|s| s.get(key.split('.').next().unwrap_or(key).trim_end_matches("[]"))).is_some();
                let mut types: Vec<(&'static str, &'static [&'static str])> = HANDLES
                    .iter()
                    .chain(extra_handles(&c.id))
                    .filter(|key| {
                        has(key)
                            && c.id != "constraint.delete"
                            && !c.id.starts_with("constraint.bars")
                            && !c.id.starts_with("constraint.dynamicDimensions")
                    })
                    .map(|key| (*key, &["Entity"][..]))
                    .collect();
                if c.id == "constraint.delete" {
                    types.push(("ids", &["Constraint"]));
                }
                let objects: Vec<_> = types.iter().filter_map(|(key, _)| (c.targets != Some(*key)).then_some(*key)).collect();
                let mut named: Vec<_> = names(&c.id).iter().filter(|(key, _)| has(key)).copied().collect();
                named.extend(dimension_names(&c.id));
                if crate::acts::takes_place(&c.id) {
                    // Explicit layout is resolved before drawing. Layer is an assignment and may
                    // create a new layer, so its narrower premise is supplied by `under`.
                    named.push(("layout", &["Layout"]));
                    named.push(("layer", &["Layer"]));
                }
                let name_params = named.iter().map(|(key, _)| (*key, "name")).collect::<Vec<_>>();
                types.extend(named);
                (
                    c.id.clone(),
                    Declaration {
                        objects: Box::leak(objects.into_boxed_slice()),
                        types: Box::leak(types.into_boxed_slice()),
                        names: Box::leak(name_params.into_boxed_slice()),
                    },
                )
            })
            .collect()
    });
    for command in commands {
        if let Some(d) = declarations.get(&command.id) {
            command.objects = d.objects;
            command.object_types = d.types;
            command.name_params = d.names;
        }
    }
}

/// A constraint has a prefixed id in the shared tool id space. Only its typed parameter is
/// decoded; integer coordinates and indexes are never ids.
pub fn resolve(d: &kition_cad_doc::Drawing, command: &str, mut params: Value) -> Result<Value, String> {
    if command == "layout" {
        let (command, p) = kition_cad_engine::cmd::layout_operation(&params).map_err(|e| e.to_string())?;
        let mut p = resolve(d, command, p)?;
        if matches!(command, "layout.duplicate" | "layout.rename")
            && let Some(map) = p.as_object_mut()
            && let Some(from) = map.remove("from")
        {
            map.insert("name".into(), from);
        }
        return Ok(p);
    }
    if matches!(command, "style" | "dimensionStyle.open" | "dimensionStyle.openManager" | "tableStyle.open" | "multileaderStyle.open")
        && let Some(name) = params.get_mut("name")
        && let Some(text) = name.as_str()
    {
        *name = Value::String(text.trim().into());
    }
    let mut named = names(command).to_vec();
    if command == "variable.set"
        && let Some(ty) = params.get("name").and_then(Value::as_str).and_then(sysvar_reference)
    {
        named.push(("value", ty));
    }
    if command == "rename" {
        named.push((
            "from",
            match params.get("table").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase().as_str() {
                "layer" => &["Layer"],
                "linetype" | "ltype" => &["Linetype"],
                "style" | "textstyle" => &["TextStyle"],
                "dimensionStyle.open" => &["DimStyle"],
                "block" => &["Block"],
                "view" => &["View"],
                "ucs" => &["Ucs"],
                "multileaderStyle.open" | "multileaderstyle" => &["MLeaderStyle"],
                "tableStyle.open" => &["TableStyle"],
                _ => &[],
            },
        ));
    }
    named.extend(dimension_names(command));
    if crate::acts::takes_place(command) {
        named.extend([("layout", &["Layout"][..]), ("layer", &["Layer"][..])]);
    }
    for (key, types) in named {
        if let Some(v) = params.get_mut(key.trim_end_matches("[]")) {
            for v in values_mut(v) {
                let Some(s) = v.as_str() else { continue };
                for ty in types {
                    let ty = ty.to_ascii_lowercase();
                    if let Some(id) = s.strip_prefix(&format!("{ty}:")) {
                        let name = d
                            .record_names()
                            .into_iter()
                            .find(|(t, n)| t == &ty && d.record_id(t, n, 0) == Some(id))
                            .map(|(_, n)| n)
                            .ok_or_else(|| format!("not-found: {s}"))?;
                        *v = Value::String(name);
                        break;
                    }
                }
            }
        }
    }
    if command == "constraint.delete"
        && let Some(items) = params.get_mut("ids").and_then(Value::as_array_mut)
    {
        for item in items {
            if let Some(id) = item.as_str().and_then(|s| s.strip_prefix("constraint:")).and_then(|s| s.parse::<u32>().ok()) {
                *item = Value::from(id);
            }
        }
    }
    if matches!(command, "viewport.set" | "viewport.freezeLayer" | "vports.set")
        && let Some(colors) = params.get_mut("colors").and_then(Value::as_object_mut)
    {
        let mut resolved = serde_json::Map::new();
        for (name, color) in std::mem::take(colors) {
            let name = if let Some(id) = name.strip_prefix("layer:") {
                d.layers
                    .iter()
                    .find(|l| d.record_id("layer", &l.name, 0) == Some(id))
                    .map(|l| l.name.clone())
                    .ok_or_else(|| format!("not-found: {name}"))?
            } else {
                name
            };
            resolved.insert(name, color);
        }
        *colors = resolved;
    }
    Ok(params)
}

pub fn sysvar_reference(var: &str) -> Option<&'static [&'static str]> {
    match var.trim().to_ascii_uppercase().as_str() {
        "CLAYER" => Some(&["Layer"]),
        "CELTYPE" => Some(&["Linetype"]),
        "TEXTSTYLE" | "DIMTXSTY" => Some(&["TextStyle"]),
        "DIMSTYLE" => Some(&["DimStyle"]),
        "DIMBLK" | "DIMBLK1" | "DIMBLK2" | "INSNAME" => Some(&["Block"]),
        "CMLEADERSTYLE" => Some(&["MLeaderStyle"]),
        "CTABLESTYLE" => Some(&["TableStyle"]),
        _ => None,
    }
}

fn dimension_names(command: &str) -> Vec<(&'static str, &'static [&'static str])> {
    if !matches!(command, "dimensionStyle.open" | "dimensionStyle.openManager" | "dimensionStyle.override" | "dimension.override") {
        return vec![];
    }
    vec![
        ("textStyle", &["TextStyle"]),
        ("DIMTXSTY", &["TextStyle"]),
        ("arrowBlock", &["Block"]),
        ("arrowBlock1", &["Block"]),
        ("arrowBlock2", &["Block"]),
        ("DIMBLK", &["Block"]),
        ("DIMBLK1", &["Block"]),
        ("DIMBLK2", &["Block"]),
    ]
}

/// The same path vocabulary as KitCommand's typed declarations. A union member which is
/// an array of coordinates stays one value, and is never flattened into numeric ids.
pub fn values<'a>(p: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut found = vec![p];
    for segment in path.split('.') {
        let array = segment.ends_with("[]");
        let key = segment.trim_end_matches("[]");
        found = found
            .into_iter()
            .filter_map(|v| v.get(key))
            .flat_map(|v| if array { v.as_array().map(|a| a.iter().collect::<Vec<_>>()).unwrap_or_default() } else { vec![v] })
            .collect();
    }
    found
}

fn values_mut(v: &mut Value) -> Vec<&mut Value> {
    match v {
        Value::Array(a) => a.iter_mut().collect(),
        v => vec![v],
    }
}
