//! Target fields consumed by audited field edits. The handlers work against the current drawing:
//! assigning a text string does not consume its placement, and assigning a plot style does not
//! consume geometry. Branch checks (entity kind and effective layer lock) remain premises.
use std::collections::BTreeMap;

use kition_cad_doc::{DimKind, DimStyle, Entity, EntityKind, Handle};
use kition_kit::KitCommand;
use serde_json::{Value, json};

fn fields(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| (*p).to_owned()).collect()
}

/// `None` leaves the caller's existing policy in force. An empty list holds existence alone.
/// The command families below were read in the engine, independently of declaration tables.
pub(crate) fn target_fields(command: &str, p: &Value, e: &Entity) -> Option<Vec<String>> {
    Some(match command {
        id if constraint_command(id) => {
            if matches!(id, "autoconstrain")
                || id.starts_with("gc")
                || id.starts_with("dc") && !id.starts_with("dcdisplay")
                || matches!(id, "geomconstraint" | "dimconstraint")
            {
                let mut f = curve_fields(e);
                f.push("~solve".into());
                f
            } else {
                Vec::new()
            }
        }
        "draworder.front" | "draworder.back" => fields(&["~pick.order", "~space"]),
        "hatchedit" => {
            let mut f = fields(&["kind.type"]);
            if matches!(e.kind, EntityKind::Hatch(_)) {
                f.push("~locked".into());
                for key in ["pattern", "scale", "angle"] {
                    if p.get(key).is_some() {
                        f.push(format!("kind.{key}"));
                    }
                }
                if p.get("pattern").is_some() {
                    f.extend(fields(&["kind.solid", "kind.gradient"]));
                }
            }
            f
        }
        "viewport.set" | "vplayer" => {
            let mut f = fields(&["kind.type"]);
            if matches!(e.kind, EntityKind::Viewport(_)) {
                f.extend(fields(&["kind.id", "~locked"]));
                if p.get("scale").is_some() {
                    f.extend(fields(&["kind.height", "kind.viewHeight"]));
                }
                if p.get("viewHeight").is_some() {
                    f.push("kind.viewHeight".into());
                }
                if p.get("scale").is_some() || p.get("viewHeight").is_some() || p.get("center").is_some() || p.get("locked").is_some() {
                    f.push("kind.locked".into());
                }
                if p.get("center").is_some() {
                    f.push("kind.viewCenter".into());
                }
                if p.get("freeze").is_some() || p.get("thaw").is_some() {
                    f.push("kind.frozenLayers".into());
                }
                if p.get("colors").is_some() {
                    f.push("kind.layerColors".into());
                }
            }
            f
        }
        "ncopy" => fields(if matches!(e.kind, EntityKind::Insert(_)) {
            &["kind.type", "kind.block", "kind.insert", "kind.scale", "kind.rotation"]
        } else {
            &["kind.type"]
        }),
        "wipeout" if p.get("erase").and_then(Value::as_bool) != Some(true) => curve_fields(e),
        "offset" if p.get("erase").and_then(Value::as_bool) != Some(true) => {
            let mut f = curve_fields(e);
            if p.get("layer").and_then(Value::as_str).is_some_and(|l| l.eq_ignore_ascii_case("current")) {
                f.extend(fields(&[
                    "common.color",
                    "common.linetype",
                    "common.lineweight",
                    "common.ltscale",
                    "common.transparency",
                    "common.visible",
                    "common.thickness",
                    "common.extrusion",
                    "common.plotStyle",
                ]));
            } else {
                f.push("common".into());
            }
            f
        }
        "copy" | "arrayrect" | "arraypolar" => fields(&["common", "kind", "~space"]),
        "xline" => curve_fields(e),
        "spline" if p.get("object").is_some() => {
            let mut f = curve_fields(e);
            f.push("~locked".into());
            f
        }
        "hatch" | "gradient" if p.get("points").is_none() => curve_fields(e),
        // modify2.rs:259-351 clones preserved fields; these flags assign only one
        // field (CLOSE additionally reads whether there are at least two vertices).
        "pedit" if !p.get("option").and_then(Value::as_str).is_some_and(|o| matches!(o.to_ascii_lowercase().as_str(), "join" | "j")) => {
            let option = p.get("option").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
            match (&e.kind, option.as_str()) {
                (EntityKind::LwPolyline(_), "open" | "o") => fields(&["kind.type", "kind.closed"]),
                (EntityKind::LwPolyline(_), "close" | "c") => fields(&["kind.type", "kind.closed", "~polylineCanClose"]),
                (EntityKind::LwPolyline(_) | EntityKind::Line(_) | EntityKind::Arc(_), _) => fields(&["kind"]),
                _ => fields(&["kind.type"]),
            }
        }
        // groups.rs::make_group/add_members only resolve membership by handle.
        "group" | "groupedit" => Vec::new(),
        // layout/plotstyle.rs::run_plotstyle: a constant assignment after is_locked.
        "plotstyle" => fields(&["common.plotStyle", "~locked"]),
        // blocks/attedit.rs::run and run_global: the insert transform is not consulted.
        // Tag/value membership is a content region over stored attribs, including absence.
        "attedit" => fields(&["kind.type"]),
        "attsync" => fields(if matches!(e.kind, EntityKind::Insert(_)) { &["kind.type", "kind.block"] } else { &["kind.type"] }),
        "-attedit" => fields(if matches!(e.kind, EntityKind::Insert(_)) { &["kind.type", "kind.block", "~attributes"] } else { &["kind.type"] }),
        // annotate.rs::run_update and run_disassociate inspect the kind, then assign fields.
        "dimstyle.update" => fields(if matches!(e.kind, EntityKind::Dimension(_)) { &["kind.type", "kind.style"] } else { &["kind.type"] }),
        "dimdisassociate" => fields(if matches!(e.kind, EntityKind::Dimension(_)) { &["kind.type", "kind.assoc"] } else { &["kind.type"] }),
        // props.rs::dim_override inserts/removes requested keys without reading other map
        // values. annotate.rs::run_dimtedit does the same for textJust.
        "dimoverride" | "dimstyle.override" => {
            let mut f = fields(&["kind.type"]);
            if matches!(e.kind, EntityKind::Dimension(_)) {
                if command == "dimoverride" && p.get("text").is_some() {
                    f.push("kind.text".into());
                }
                if p.get("clear").and_then(Value::as_bool) == Some(true) {
                    f.push("kind.overrides".into());
                } else {
                    f.extend(
                        p.as_object()
                            .into_iter()
                            .flatten()
                            .filter_map(|(key, _)| DimStyle::field_name(key).map(|field| format!("~dimOverride_{field}"))),
                    );
                }
            }
            f
        }
        id if id == "dimtedit" || id.starts_with("dimtedit.") => {
            let mut f = fields(&["kind.type"]);
            if matches!(e.kind, EntityKind::Dimension(_)) {
                if p.get("at").is_some() {
                    f.extend(fields(&["kind.textMid", "kind.userTextPos"]));
                }
                let mode = id.strip_prefix("dimtedit.").or_else(|| p.get("mode").and_then(Value::as_str)).unwrap_or("").to_ascii_lowercase();
                match mode.as_str() {
                    "home" => f.extend(fields(&["kind.userTextPos", "kind.textRotation", "~dimOverride_textJust"])),
                    "angle" => f.push("kind.textRotation".into()),
                    "left" | "center" | "right" => f.extend(fields(&["kind.userTextPos", "~dimOverride_textJust"])),
                    _ => {}
                }
            }
            f
        }
        // assoc.rs::reassociate scans at the stored definition points appropriate to the kind.
        "dimreassociate" => {
            if !matches!(e.kind, EntityKind::Dimension(_)) {
                // The caller's `hs` may include association scan sources; their geometry is
                // still required. Apply this policy only to the dimension target.
                return None;
            }
            let mut f = fields(&["kind.type", "kind.assoc", "~space"]);
            if let EntityKind::Dimension(dm) = &e.kind {
                f.push("kind.kind".into());
                f.extend(fields(match dm.kind {
                    DimKind::Linear { .. } | DimKind::Aligned => &["kind.p13", "kind.p14"],
                    DimKind::Radius | DimKind::Diameter => &["kind.defpt", "kind.p15"],
                    DimKind::Angular3P | DimKind::ArcLength => &["kind.p13", "kind.p14", "kind.p15"],
                    DimKind::Angular => &["kind.p13", "kind.p14", "kind.defpt", "kind.p15"],
                    DimKind::Ordinate { .. } => &["kind.p13"],
                }));
            }
            f
        }
        // modify2.rs::reverse_kind rebuilds supported kinds and skips all other kinds.
        "reverse" => match e.kind {
            EntityKind::Line(_) => fields(&["kind.a", "kind.b", "~locked"]),
            EntityKind::LwPolyline(_) | EntityKind::Polyline3d(_) | EntityKind::Spline(_) => fields(&["kind", "~locked"]),
            _ => fields(&["kind.type", "~locked"]),
        },
        // modify2.rs::lengthen_kind computes new endpoints from these geometry fields.
        "lengthen" => match e.kind {
            EntityKind::Line(_) => fields(&["kind.a", "kind.b", "~locked"]),
            EntityKind::Arc(_) => fields(&["kind.center", "kind.radius", "kind.start", "kind.end", "~locked"]),
            EntityKind::LwPolyline(_) => fields(&["kind", "~locked"]),
            _ => fields(&["kind.type", "~locked"]),
        },
        // FLATTEN assigns z/elevation/thickness. Ray directions are renormalized; polyline-3d
        // vertices become new 2d vertices, so those two paths also consume stored coordinates.
        "flatten" => {
            let mut f = fields(&["kind.type", "common.thickness", "~locked"]);
            fn coordinates(value: &Value, path: &str, out: &mut Vec<String>) {
                match value {
                    Value::Object(m) => {
                        for (key, value) in m {
                            let path = format!("{path}.{key}");
                            if matches!(key.as_str(), "z" | "elevation") {
                                out.push(path);
                            } else {
                                coordinates(value, &path, out);
                            }
                        }
                    }
                    Value::Array(a) => {
                        for (i, value) in a.iter().enumerate() {
                            coordinates(value, &format!("{path}.{i}"), out);
                        }
                    }
                    _ => {}
                }
            }
            if let Ok(value) = serde_json::to_value(&e.kind) {
                coordinates(&value, "kind", &mut f);
            }
            match e.kind {
                EntityKind::Ray(_) | EntityKind::XLine(_) => f.push("kind.dir".into()),
                EntityKind::Polyline3d(_) => f.extend(fields(&["kind.points", "kind.closed"])),
                _ => {}
            }
            f
        }
        // modify2.rs::set_text replaces one string and clears the cached dimension block.
        "textedit" => fields(match e.kind {
            EntityKind::Text(_) => &["kind.type", "kind.value", "~locked"],
            EntityKind::MText(_) => &["kind.type", "kind.contents", "~locked"],
            EntityKind::Dimension(_) => &["kind.type", "kind.text", "~locked"],
            _ => &["kind.type", "~locked"],
        }),
        // draw2.rs::center_mark/center_line use only the source geometry; output properties
        // come from current_common, not from the addressed circle/line.
        "centermark" => fields(&["kind.type", "kind.center", "kind.radius"]),
        "centerline" => fields(&["kind.type", "kind.a", "kind.b"]),
        // draw2::divide/measure sample geometry; continuation takes the saved endpoint/tangent.
        "divide" | "measure" | "arc.continue" => match e.kind {
            EntityKind::Line(_) => fields(&["kind.a", "kind.b"]),
            EntityKind::Circle(_) => fields(&["kind.center", "kind.radius"]),
            EntityKind::Arc(_) if command == "arc.continue" => fields(&["kind.center", "kind.radius", "kind.end"]),
            EntityKind::Arc(_) => fields(&["kind.center", "kind.radius", "kind.start", "kind.end"]),
            EntityKind::LwPolyline(_) => fields(&["kind.vertices", "kind.closed"]),
            _ => fields(&["kind"]),
        },
        "arraypath" => {
            if p.get("path").and_then(Value::as_str).and_then(Handle::parse_hex) == Some(e.handle) {
                curve_fields(e)
            } else {
                fields(&["common", "kind", "~space"])
            }
        }
        // Tangency and blend likewise take source shapes, with no copied common properties.
        "circle.ttr" | "circle.ttt" | "blend" => curve_fields(e),
        // layer.rs::obj_layers selects layers from the target's layer name alone.
        "laymcur" | "layiso" | "layoff" | "layfrz" | "laylck" | "layulk" => fields(&["common.layer"]),
        "laymch" | "laycur" => fields(&["common.layer"]),
        // props.rs::run_matchprop copies six common fields, never source geometry.
        "matchprop" => fields(&["common.layer", "common.color", "common.linetype", "common.lineweight", "common.ltscale", "common.transparency"]),
        // hatch.rs::closed_loop converts only these source geometry fields.
        "region" => match e.kind {
            EntityKind::Circle(_) => fields(&["kind.type", "kind.center", "kind.radius"]),
            EntityKind::LwPolyline(_) => fields(&["kind.type", "kind.vertices", "kind.closed"]),
            EntityKind::Ellipse(_) | EntityKind::Spline(_) => fields(&["kind"]),
            _ => fields(&["kind.type"]),
        },
        // SPLINEDIT replaces the supported spline kind, without reading common properties.
        "splinedit" => fields(if matches!(e.kind, EntityKind::Spline(_)) { &["kind", "~locked"] } else { &["kind.type", "~locked"] }),
        // ALIGN applies one transform to the kind; common properties remain untouched.
        "align" => fields(&["kind", "~locked"]),
        _ => return None,
    })
}

/// Publish exactly the DIM* and camelCase fields `DimStyle::apply_fields` accepts. Its coercions
/// are part of the handler contract: booleans/numbers accept numeric text, colours accept ACI
/// numbers or names, and a string field also accepts a number as text. Unknown keys remain absent.
pub(crate) fn declare(c: &mut KitCommand) {
    if !matches!(c.id.as_str(), "dimstyle" | "dimstyle.dimension" | "dimstyle.override" | "dimoverride") {
        return;
    }
    let Ok(default) = serde_json::to_value(DimStyle::default()) else { return };
    let (Some(default), Some(schema)) = (default.as_object(), c.schema.as_mut().and_then(Value::as_object_mut)) else { return };
    let schema_for = |value: &Value| match value {
        Value::Bool(_) | Value::Number(_) => json!({"oneOf":[{"type":"boolean"},{"type":"number"},{"type":"string"}]}),
        Value::String(_) => json!({"oneOf":[{"type":"string"},{"type":"number"}]}),
        Value::Object(_) => json!({"oneOf":[
            {"type":"object","properties":{"kind":{"type":"string","enum":["byLayer","byBlock","aci","rgb"]},
                "value":{"oneOf":[{"type":"integer"},
                    {"type":"array","items":{"type":"integer"}}]}},
                "additionalProperties":false},
            {"type":"number"},{"type":"string"}]}),
        _ => json!({}),
    };
    for (field, value) in default {
        if field != "name" && DimStyle::field_name(field).is_some() {
            schema.entry(field.clone()).or_insert_with(|| schema_for(value));
        }
    }
    for (var, field) in kition_cad_doc::DIMVARS {
        if let Some(value) = default.get(*field) {
            schema.entry((*var).to_owned()).or_insert_with(|| schema_for(value));
        }
    }
}

/// Attribute matching facts, omitting placement and appearance. Fold tags as the handler does;
/// retain a sorted multiset because every duplicate matching attribute is edited. Explicit
/// assignments hold membership, constants and the written values; global find/replace
/// also consumes matching values.
pub(crate) fn attribute_values(e: &Entity) -> Value {
    let mut tags: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    if let EntityKind::Insert(insert) = &e.kind {
        for attr in &insert.attribs {
            let value = if attr.constant { json!({"constant":true}) } else { json!({"constant":false,"value":attr.text.value}) };
            tags.entry(attr.tag.to_ascii_lowercase()).or_default().push(value);
        }
    }
    json!(
        tags.into_iter()
            .map(|(tag, mut values)| {
                values.sort_by_key(Value::to_string);
                let mut constants: Vec<bool> = values.iter().filter_map(|value| value.get("constant").and_then(Value::as_bool)).collect();
                constants.sort_unstable();
                json!({"tag":tag,"constants":constants,"values":values})
            })
            .collect::<Vec<_>>()
    )
}

/// Values and branch decisions consumed by ATTEDIT, without attribute appearance or order.
/// Duplicate tags stay a multiset; exact-case request keys follow the native resolver.
pub(crate) fn attribute_projection(attributes: &Value, values: &Value) -> Option<Value> {
    let values = values.as_object()?;
    let mut projected = Vec::new();
    for attribute in attributes.as_array()? {
        let tag = attribute.get("tag")?.as_str()?;
        let supplied = values.get(tag).or_else(|| values.iter().find(|(key, _)| key.eq_ignore_ascii_case(tag)).map(|(_, value)| value));
        let replacement = match supplied {
            Some(Value::String(value)) => value.clone(),
            Some(value) if value.is_number() || value.is_boolean() => value.to_string(),
            _ => continue,
        };
        let constant = attribute.get("constant")?.as_bool()?;
        projected.push(if constant {
            json!([tag.to_ascii_lowercase(), true])
        } else {
            json!([tag.to_ascii_lowercase(), false, attribute.get("text")?.get("value")?.as_str()?, replacement])
        });
    }
    projected.sort_by_key(Value::to_string);
    Some(json!(projected))
}

/// Explicit source sampling/copying is allowed on locked layers. These paths mutate targets.
pub(crate) fn writes_existing(command: &str, p: &Value, h: Handle) -> bool {
    match command {
        "matchprop" => p.get("source").and_then(Value::as_str).and_then(Handle::parse_hex) != Some(h),
        "wipeout" => p.get("erase").and_then(Value::as_bool) == Some(true),
        "block" | "-block" => p.get("keep").and_then(Value::as_str) != Some("retain"),
        id => {
            id.starts_with("table.")
                || id.starts_with("dimtedit.")
                || matches!(
                    id,
                    "properties.set"
                        | "attedit"
                        | "-attedit"
                        | "attsync"
                        | "dimoverride"
                        | "dimstyle.override"
                        | "dimstyle.update"
                        | "dimdisassociate"
                        | "dimreassociate"
                        | "dimtedit"
                        | "dimspace"
                        | "join"
                        | "pedit"
                        | "region"
                        | "revcloud"
                        | "revcloud.polygonal"
                        | "revcloud.freehand"
                        | "laycur"
                        | "laymch"
                        | "breakatpoint"
                )
        }
    }
}

/// table.rs::edit normalizes the grid; positional edits consume identity/topology,
/// defaults and merge text. Other cell values survive on the current drawing.
pub(crate) fn table_fields(command: &str, p: &Value, t: &kition_cad_doc::Table) -> Vec<String> {
    let mut f = vec!["kind.type".into()];
    let index = |key: &str| p.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
    let (rows, cols) = (t.row_heights.len(), t.col_widths.len());
    let (r, c) = (index("row").unwrap_or(rows), index("col").unwrap_or(cols));
    if matches!(command, "table.set" | "table.unmerge") {
        f.push(format!("kind.cells.{r}.{c}.id"));
        if command == "table.set" {
            let (ar, ac) = t
                .cells
                .iter()
                .enumerate()
                .find_map(|(ar, cells)| {
                    cells.iter().enumerate().find_map(|(ac, cell)| {
                        let (rs, cs) = cell.merged?;
                        (r >= ar && r < ar.saturating_add(rs as usize) && c >= ac && c < ac.saturating_add(cs as usize)).then_some((ar, ac))
                    })
                })
                .unwrap_or((r, c));
            f.push(format!("kind.cells.{ar}.{ac}.text"));
        }
        for ar in 0..rows.min(r.saturating_add(1)) {
            for ac in 0..cols.min(c.saturating_add(1)) {
                f.push(format!("kind.cells.{ar}.{ac}.merged"));
            }
        }
        return f;
    }
    f.extend(["~grid.rows".into(), "~grid.cols".into()]);
    for (row, cells) in t.cells.iter().enumerate() {
        for col in 0..cells.len() {
            f.push(format!("kind.cells.{row}.{col}.id"));
            f.push(format!("kind.cells.{row}.{col}.merged"));
        }
    }
    if command == "table.insertrow" && !p.get("height").and_then(Value::as_f64).is_some_and(|h| h.is_finite() && h > 0.0) {
        if rows > 0 {
            f.push(format!("kind.rowHeights.{}", rows - 1));
        } else {
            f.push("kind.textHeight".into());
        }
    }
    if command == "table.insertcol" && !p.get("width").and_then(Value::as_f64).is_some_and(|w| w.is_finite() && w > 0.0) && cols > 0 {
        f.push(format!("kind.colWidths.{}", cols - 1));
    }
    if command == "table.merge" {
        let rs = index("rows").unwrap_or(1).max(1).min(rows.saturating_sub(r));
        let cs = index("cols").unwrap_or(1).max(1).min(cols.saturating_sub(c));
        for row in r..r.saturating_add(rs) {
            for col in c..c.saturating_add(cs) {
                f.push(format!("kind.cells.{row}.{col}.text"));
            }
        }
    }
    f
}

/// Geometric inputs of curve sampling; explicit source display properties are preserved.
pub(crate) fn curve_fields(e: &Entity) -> Vec<String> {
    fields(match e.kind {
        EntityKind::Line(_) => &["kind.type", "kind.a", "kind.b"],
        EntityKind::Circle(_) => &["kind.type", "kind.center", "kind.radius"],
        EntityKind::Arc(_) => &["kind.type", "kind.center", "kind.radius", "kind.start", "kind.end"],
        EntityKind::LwPolyline(_) => &["kind.type", "kind.vertices", "kind.closed"],
        _ => &["kind"],
    })
}

/// The commands whose handlers edit constraint or parametric state.
pub(crate) fn constraint_command(id: &str) -> bool {
    id.starts_with("gc")
        || id.starts_with("dc")
        || matches!(
            id,
            "geomconstraint"
                | "dimconstraint"
                | "parameters"
                | "autoconstrain"
                | "delconstraint"
                | "constraintsettings"
                | "constraintbar"
                | "constraintbar.hideall"
                | "constraintbar.showall"
        )
}
