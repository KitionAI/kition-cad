//! Scan witnesses describe the actual victims, rather than every object inspected by a scan.
use kition_cad_doc::{Drawing, Entity, EntityKind, Handle, Space};
use kition_cad_engine::Session;
use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, Premise, Under};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn groups(d: &Drawing) -> Value {
    json!(d.groups.iter().map(|g| (format!("group:{}", d.record_id("group", &g.name, 0).unwrap_or(&g.name)), g)).collect::<Vec<_>>())
}

pub fn groups_for(d: &Drawing, h: Handle) -> Value {
    json!(
        d.groups
            .iter()
            .filter(|g| g.members.contains(&h))
            .map(|g| (format!("group:{}", d.record_id("group", &g.name, 0).unwrap_or(&g.name)), g))
            .collect::<Vec<_>>()
    )
}

fn ungroup_ids(s: &Session, p: &Value, facts: &Value) -> Result<Vec<Value>, String> {
    let records: Vec<(String, kition_cad_doc::Group)> = serde_json::from_value(facts.clone()).map_err(|e| e.to_string())?;
    let mut d = s.doc().map_err(|e| e.to_string())?.clone();
    d.groups = records.iter().map(|(_, g)| g.clone()).collect();
    let mut probe = Session::new();
    probe.open_drawing(d, "ungroup premise", None);
    probe.set_selection(s.selection());
    let names = kition_cad_engine::cmd::ungroup_targets(&probe, p).map_err(|e| e.to_string())?;
    Ok(records.into_iter().filter(|(_, g)| names.iter().any(|n| g.name.eq_ignore_ascii_case(n))).map(|(id, _)| json!(id)).collect())
}

pub fn ungroup_under(s: &Session, p: &Value) -> Result<Under, String> {
    let ids = ungroup_ids(s, p, &groups(s.doc().map_err(|e| e.to_string())?))?;
    let mut premises: Vec<_> = ids.iter().map(|id| Premise::on(id.clone()).fields(["name", "data"]).positions(Vec::<String>::new())).collect();
    if p.get("name").is_none_or(Value::is_null) {
        let read = |v: &Value| v.as_str().and_then(Handle::parse_hex).or_else(|| v.as_u64().map(Handle));
        let hs = if let Some(a) = p.get("handles").and_then(Value::as_array) {
            a.iter().filter_map(read).collect()
        } else if let Some(h) = p.get("handle").and_then(read) {
            vec![h]
        } else {
            s.selection()
        };
        premises.extend(hs.iter().map(|h| Premise::on(h.hex()).fields(["~groups"]).positions(Vec::<String>::new())));
    }
    Ok(Under { premises, objects: ids, covers: Some(Covers { fields: vec!["~groupTargets".into()] }), ..Default::default() })
}

pub fn ungroup_covered(s: &Session, p: &Value, asked: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
    let now = groups(s.doc().map_err(|e| e.to_string())?);
    let old = asked.iter().find(|a| a.id == "drawing").and_then(|a| a.before.get("~groupTargets")).unwrap_or(&now);
    Ok(Some(CoveredAt {
        under: ungroup_ids(s, p, old)?,
        covered: asked
            .iter()
            .map(|a| Covered {
                id: a.id.clone(),
                covered: if a.id == "drawing" { a.before.get("~groupTargets").filter(|_| a.unknown.is_empty()).map(|_| true) } else { Some(false) },
                ..Default::default()
            })
            .collect(),
    }))
}

fn entities(d: &Drawing) -> impl Iterator<Item = &Entity> {
    d.model.iter().chain(d.layouts.iter().flat_map(|l| l.entities.iter())).chain(d.blocks.values().flat_map(|b| b.entities.iter())).map(AsRef::as_ref)
}

fn objects(d: &Drawing) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let canonical = kdm_model::canonical(d);
    for (field, ty) in [
        ("layers", "layer"),
        ("linetypes", "linetype"),
        ("textStyles", "textstyle"),
        ("dimStyles", "dimstyle"),
        ("mleaderStyles", "mleaderstyle"),
        ("tableStyles", "tablestyle"),
        ("views", "view"),
        ("ucss", "ucs"),
        ("layerStates", "layerstate"),
        ("groups", "group"),
        ("plotStyleTables", "plotstyletable"),
    ] {
        for v in canonical.get(field).and_then(Value::as_array).into_iter().flatten() {
            if let Some(name) = v.get("name").and_then(Value::as_str) {
                out.insert(format!("{ty}:{}", d.record_id(ty, name, 0).unwrap_or(name)), v.clone());
            }
        }
    }
    for b in d.blocks.values() {
        out.insert(format!("block:{}", d.record_id("block", &b.name, 0).unwrap_or(&b.name)), json!({"data":b,"members":b.entities.handles()}));
    }
    for l in &d.layouts {
        out.insert(format!("layout:{}", d.record_id("layout", &l.name, 0).unwrap_or(&l.name)), json!({"data":l,"members":l.entities.handles()}));
    }
    for e in entities(d) {
        out.insert(e.handle.hex(), json!(e));
    }
    out.insert("header".into(), canonical.get("header").cloned().unwrap_or(Value::Null));
    out.insert("constraints".into(), json!(d.constraints));
    out.insert("parametric".into(), json!(d.parametric));
    out
}

pub fn effects(d: &Drawing, space: Space, selection: Vec<Handle>, command: &str, p: &Value) -> Result<Value, String> {
    let mut probe = Session::new();
    probe.open_drawing(d.clone(), "scan premise", None);
    probe.state_mut().map_err(|e| e.to_string())?.space = space;
    probe.set_selection(selection);
    // A formerly eligible scan can become forbidden when a victim is locked. That
    // refusal is itself a changed premise; observing the drawing must still work.
    if let Err(error) = probe.execute(command, p) {
        return Ok(json!({"refused":error.to_string()}));
    }
    let before = objects(d);
    let after = objects(probe.doc().map_err(|e| e.to_string())?);
    let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    Ok(json!(
        keys.into_iter()
            .filter(|k| before.get(*k) != after.get(*k))
            .map(|k| json!({"id":k,"before":before.get(k),"after":after.get(k)}))
            .collect::<Vec<_>>()
    ))
}

pub fn key(name: &str) -> String {
    name.to_ascii_lowercase().bytes().map(|b| format!("{b:02x}")).collect()
}

pub fn layer_members(d: &Drawing) -> Value {
    json!(
        d.layers
            .iter()
            .map(|l| {
                let hs: Vec<_> = entities(d).filter(|e| e.common.layer.eq_ignore_ascii_case(&l.name)).map(|e| e.handle.hex()).collect();
                let vps: Vec<_> = d
                    .layouts
                    .iter()
                    .flat_map(|l| l.entities.iter())
                    .filter(|e| match &e.kind {
                        EntityKind::Viewport(v) => {
                            v.frozen_layers.iter().any(|n| n.eq_ignore_ascii_case(&l.name))
                                || v.layer_colors.iter().any(|(n, _)| n.eq_ignore_ascii_case(&l.name))
                        }
                        _ => false,
                    })
                    .map(|e| e.handle.hex())
                    .collect();
                (key(&l.name), json!({"entities":hs,"viewports":vps}))
            })
            .collect::<BTreeMap<_, _>>()
    )
}
