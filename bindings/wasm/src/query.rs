//! Query selections replay the engine's selector against saved entity facts.
use kition_cad_doc::{Drawing, Entity, EntityStore, Handle, Layer, Layout, Space};
use kition_cad_engine::Session;
use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, Premise, Under};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn selects(command: &str) -> bool {
    matches!(command, "qselect" | "selectall")
}

#[derive(Serialize, Deserialize)]
struct Fact {
    model: Vec<Entity>,
    layouts: Vec<(Layout, Vec<Entity>)>,
    layers: Vec<Layer>,
}

pub fn fact(d: &Drawing) -> Result<Value, String> {
    serde_json::to_value(Fact {
        model: d.model.iter().map(|e| e.as_ref().clone()).collect(),
        layouts: d.layouts.iter().map(|l| (l.clone(), l.entities.iter().map(|e| e.as_ref().clone()).collect())).collect(),
        layers: d.layers.clone(),
    })
    .map_err(|e| e.to_string())
}

pub fn selectable(d: &Drawing, store: &EntityStore) -> Value {
    json!(store.iter().map(|e| (e.handle.hex(), d.is_visible(e), d.layer(&e.common.layer).is_some_and(|l| l.locked))).collect::<Vec<_>>())
}

fn selection(s: &Session, d: &Drawing, command: &str, p: &Value) -> Result<Vec<Handle>, String> {
    let mut probe = Session::new();
    probe.open_drawing(d.clone(), "query premise", None);
    let state = probe.state_mut().map_err(|e| e.to_string())?;
    state.space = s.space();
    state.selection = s.selection();
    probe.execute(command, p).map_err(|e| e.to_string())?;
    Ok(probe.selection())
}

pub fn under(s: &Session, command: &str, p: &Value) -> Result<Under, String> {
    let d = s.doc().map_err(|e| e.to_string())?;
    let hs = selection(s, d, command, p)?;
    // Queries retain whole observed matches: their frozen consumer contract also
    // holds values a later command can discard or inspect. FIND is separate.
    let mut premises: Vec<_> = hs.iter().map(|h| Premise::on(h.hex())).collect();
    for h in &hs {
        if let Some(e) = d.entity(*h) {
            crate::place::dimension_sources(e, *h, &mut premises);
        }
    }
    // Parameterless steps have no place for the pinned SDK to replay.
    if command == "selectall" || p.as_object().is_none_or(|o| o.is_empty()) {
        let (id, field) = match s.space() {
            Space::Model => ("drawing".into(), "~modelSelection"),
            Space::Paper(name) => (format!("layout:{}", d.record_id("layout", &name, 0).ok_or("unknown layout identity")?), "~selection"),
        };
        premises.push(Premise::on(id).fields([field]).positions(Vec::<String>::new()));
    }
    Ok(Under {
        objects: hs.iter().map(|h| json!(h.hex())).collect(),
        premises,
        covers: Some(Covers { fields: vec!["~queries".into()] }),
        ..Default::default()
    })
}

pub fn covered(s: &Session, command: &str, p: &Value, asked: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
    let mut d = s.doc().map_err(|e| e.to_string())?.clone();
    if let Some(v) = asked.iter().find(|a| a.id == "drawing").and_then(|a| a.before.get("~queries")) {
        let fact: Fact = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        d.model = EntityStore::new();
        for e in fact.model {
            d.model.push(e);
        }
        d.layouts = fact
            .layouts
            .into_iter()
            .map(|(mut l, entities)| {
                l.entities = EntityStore::new();
                for e in entities {
                    l.entities.push(e);
                }
                l
            })
            .collect();
        d.layers = fact.layers;
    }
    let hs = selection(s, &d, command, p)?;
    Ok(Some(CoveredAt {
        under: hs.iter().map(|h| json!(h.hex())).collect(),
        covered: asked
            .iter()
            .map(|a| Covered {
                id: a.id.clone(),
                covered: if a.id == "drawing" { a.before.get("~queries").filter(|_| a.unknown.is_empty()).map(|_| true) } else { Some(false) },
                ..Default::default()
            })
            .collect(),
    }))
}
