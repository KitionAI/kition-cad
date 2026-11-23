//! C8-land-11-cad: per-call premises and spatial replay.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{CoveredAsk, KitDocument};
use serde_json::{Value, json};

fn drawing() -> (CadDocument, Value) {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let h = doc.execute("line", json!({"points":[[0,0],[100,0]]})).unwrap()["handles"][0].clone();
    (doc, h)
}

#[test]
fn a_common_property_holds_only_the_field_it_sets() {
    let (doc, h) = drawing();
    let under = doc.under("properties.set", &json!({"handles":[h],"color":"red"})).unwrap().unwrap();
    let premise = under.premises.iter().find(|p| p.id == h).unwrap();
    assert_eq!(premise.fields, Some(vec!["~locked".into(), "common.color".into()]));
    assert_eq!(premise.positions, Some(vec![]));
}

#[test]
fn drawing_holds_stored_defaults_but_an_explicit_layer_drops_clayer() {
    let (doc, _) = drawing();
    for explicit in [false, true] {
        let mut p = json!({"points":[[0,0],[10,0]]});
        if explicit {
            p["layer"] = json!("0");
        }
        let under = doc.under("line", &p).unwrap().unwrap();
        let root = under.premises.iter().find(|p| p.id == "drawing").unwrap();
        let fields = root.fields.as_ref().unwrap();
        assert_eq!(fields.contains(&"header.CLAYER".into()), !explicit);
        assert!(fields.contains(&"header.CECOLOR".into()));
    }
}

#[test]
fn a_move_holds_geometry_and_the_layer_lock() {
    let (mut doc, h) = drawing();
    let under = doc.under("move", &json!({"handles":[h],"delta":[5,0]})).unwrap().unwrap();
    let p = under.premises.iter().find(|p| p.id == h).unwrap();
    assert_eq!(p.fields, Some(["kind.a", "kind.b", "kind.type", "~locked", "~solve"].into_iter().map(str::to_owned).collect()));
    let before = doc.derived(std::slice::from_ref(&h), &["locked".into()]).unwrap();
    assert_eq!(before[0].values["locked"], false);
    doc.execute("layer.set", json!({"name":"0","locked":true})).unwrap();
    let after = doc.derived(std::slice::from_ref(&h), &["locked".into()]).unwrap();
    assert_eq!(after[0].values["locked"], true);
}

#[test]
fn a_pick_and_replay_use_the_same_engine_hit_test() {
    let (mut doc, h) = drawing();
    let params = json!({"pick":[50,0],"aperture":0.1,"color":"red"});
    let under = doc.under("properties.set", &params).unwrap().unwrap();
    assert_eq!(under.objects, vec![h.clone()]);
    assert!(under.covers.is_some());
    let asked = before_pick(&mut doc, h.clone());
    doc.execute("move", json!({"handles":[h],"delta":[0,20]})).unwrap();
    assert!(doc.under("properties.set", &params).unwrap().unwrap().objects.is_empty());
    let replay = doc.covered("properties.set", &params, &[asked]).unwrap().unwrap();
    assert_eq!(replay.under, vec![h]);
    assert_eq!(replay.covered[0].covered, Some(true));
}

#[test]
fn referenced_names_hold_the_name_and_not_unrelated_style_fields() {
    let (doc, h) = drawing();
    let under = doc.under("properties.set", &json!({"handles":[h],"dimStyle":"Standard"})).unwrap().unwrap();
    assert!(under.premises.iter().any(|p| p.id == "dimstyle:Standard" && p.fields == Some(vec!["name".into()])));
}

#[test]
fn table_cells_hold_the_cell_and_merge_topology_not_every_other_cell() {
    let (mut doc, _) = drawing();
    let h = doc.execute("table", json!({"at":[0,20],"rows":3,"cols":2})).unwrap()["handle"].clone();
    let under = doc.under("table.set", &json!({"handle":h,"row":1,"col":0,"text":"new"})).unwrap().unwrap();
    let p = under.premises.iter().find(|p| p.id == h).unwrap();
    assert_eq!(
        p.fields,
        Some(
            ["~locked", "kind.type", "kind.cells.1.0.id", "kind.cells.0.0.merged", "kind.cells.1.0.merged"].into_iter().map(str::to_owned).collect()
        )
    );
    // same-field decision: the addressed text is replaced. A cell's text is its own object's
    // (`cell:<table>-<cell id>`; the table numbers its cells 1, 2, … row by row).
    let cell = json!(format!("cell:{}-3", h.as_str().unwrap()));
    let text = under.premises.iter().find(|p| p.id == cell).unwrap_or_else(|| panic!("no premise on {cell}: {:?}", under.premises));
    assert_eq!(text.fields, Some(vec!["text".to_owned()]));
    assert_eq!(
        under.premises.iter().filter(|p| p.id.as_str().is_some_and(|id| id.starts_with("cell:"))).count(),
        1,
        "only the addressed cell's text"
    );
}

fn before_pick(doc: &mut CadDocument, id: Value) -> CoveredAsk {
    let fact = doc.derived(std::slice::from_ref(&id), &["pick".into()]).unwrap();
    let loaded = kition_kit::session::model::Exchange::new().batch(doc, &json!({"ops":[],"load":[{"target":id,"fields":["kind"]}]}), false).unwrap();
    let mut before: serde_json::Map<_, _> = fact[0].values.iter().map(|(k, v)| (format!("~{k}"), v.clone())).collect();
    before.insert("kind".into(), loaded["loaded"][0]["fields"]["kind"].clone());
    CoveredAsk { id, before, ..Default::default() }
}

#[test]
fn spatial_replay_restores_the_observed_draw_order() {
    let (mut doc, a) = drawing();
    let b = doc.execute("line", json!({"points":[[0,0],[100,0]]})).unwrap()["handles"][0].clone();
    let params = json!({"pick":[50,0],"aperture":0.1,"color":"red"});
    assert_eq!(doc.under("properties.set", &params).unwrap().unwrap().objects, vec![b.clone()]);
    let asked = before_pick(&mut doc, a.clone());
    doc.execute("draworder.front", json!({"handles":[a.clone()]})).unwrap();
    assert_eq!(doc.under("properties.set", &params).unwrap().unwrap().objects, vec![a]);
    assert_eq!(doc.covered("properties.set", &params, &[asked]).unwrap().unwrap().under, vec![b]);
}

#[test]
fn spatial_replay_restores_block_child_layer_visibility() {
    let (mut doc, child) = drawing();
    doc.execute("layer.new", json!({"name":"Children"})).unwrap();
    doc.execute("properties.set", json!({"handles":[child],"layer":"Children"})).unwrap();
    doc.execute("block", json!({"name":"Parent","base":[0,0],"handles":[child],"keep":"delete"})).unwrap();
    let parent = doc.execute("insert", json!({"name":"Parent","at":[0,0]})).unwrap()["handle"].clone();
    let params = json!({"pick":[50,0],"aperture":0.1,"color":"red"});
    let asked = before_pick(&mut doc, parent.clone());
    doc.execute("layer.set", json!({"name":"Children","on":false})).unwrap();
    assert!(doc.under("properties.set", &params).unwrap().unwrap().objects.is_empty());
    assert_eq!(doc.covered("properties.set", &params, &[asked]).unwrap().unwrap().under, vec![parent]);
}

#[test]
fn exploding_an_insert_holds_the_block_definition_it_copies() {
    let (mut doc, child) = drawing();
    doc.execute("block", json!({"name":"Parent","base":[0,0],"handles":[child],"keep":"delete"})).unwrap();
    let parent = doc.execute("insert", json!({"name":"Parent","at":[0,0]})).unwrap()["handle"].clone();
    let under = doc.under("explode", &json!({"handles":[parent]})).unwrap().unwrap();
    assert!(under.premises.iter().any(|p| p.id == "block:Parent"));
    let block = doc.derived(&[], &["pick".into()]).unwrap().into_iter().find(|v| v.id == parent).unwrap();
    let copied: kition_cad_doc::Handle = serde_json::from_value(block.values["pick"]["blocks"]["Parent"]["entities"][0]["handle"].clone()).unwrap();
    let copied = copied.hex();
    assert!(under.premises.iter().any(|p| p.id == copied));
}

#[test]
fn deleted_geometry_without_its_stored_kind_is_unknown() {
    let (mut doc, h) = drawing();
    let mut asked = before_pick(&mut doc, h.clone());
    asked.before.remove("kind");
    doc.execute("erase", json!({"handles":[h]})).unwrap();
    let result = doc.covered("properties.set", &json!({"pick":[50,0],"aperture":0.1,"color":"red"}), &[asked]).unwrap().unwrap();
    assert_eq!(result.covered[0].covered, None, "Missing historical geometry cannot prove that a deleted entity was unrelated");
}

#[test]
fn plot_table_creation_holds_its_typed_destination_gap() {
    for copied in [false, true] {
        let (mut doc, _) = drawing();
        let params =
            if copied { json!({"action":"new","name":"Taken.ctb","from":"monochrome.ctb"}) } else { json!({"action":"new","name":"Taken.ctb"}) };
        let before = doc.under("stylesmanager", &params).unwrap().unwrap();
        assert!(before.covers.as_ref().is_some_and(|c| c.fields.contains(&"name".into())));
        doc.execute("stylesmanager", json!({"action":"new","name":"Taken.ctb"})).unwrap();
        let after = doc.under("stylesmanager", &params).unwrap().unwrap();
        assert!(after.objects.contains(&json!("plotstyletable:Taken.ctb")));
        let asked = CoveredAsk { id: json!("plotstyletable:Taken.ctb"), created: true, ..Default::default() };
        let replay = doc.covered("stylesmanager", &params, &[asked]).unwrap().unwrap();
        assert!(replay.under.is_empty());
        assert_eq!(replay.covered[0].covered, Some(true));
        assert_ne!(before.objects, after.objects);
    }
}
