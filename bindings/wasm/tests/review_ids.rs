//! Named tool identities survive rename, replacement, and a native save/reopen.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{KitDocument, session::model::Exchange};
use serde_json::{Value, json};

fn load(d: &mut CadDocument, id: &str) -> Value {
    Exchange::new().batch(d, &json!({"ops":[],"load":[{"target":id}]}), false).unwrap()["loaded"][0]["fields"].clone()
}

#[test]
fn rename_away_and_recreate_does_not_reuse_the_old_identity() {
    let mut d = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    d.execute("layer.add", json!({"name":"Walls"})).unwrap();
    d.execute("layer.set", json!({"name":"Walls","newName":"Partitions"})).unwrap();
    d.execute("layer.add", json!({"name":"Walls"})).unwrap();
    for reopen in [false, true] {
        if reopen {
            d = CadDocument::open("Plan.dxf", &d.save(Some("Plan.dxf")).unwrap()).unwrap();
        }
        assert_eq!(load(&mut d, "layer:Walls")["name"], "Partitions", "{}", d.inspect().unwrap());
        let inspected = d.inspect().unwrap();
        let layers = inspected["layers"].as_array().unwrap();
        assert_eq!(layers.iter().find(|l| l["name"] == "Partitions").unwrap()["id"], "layer:Walls");
        let replacement = layers.iter().find(|l| l["name"] == "Walls").unwrap()["id"].clone();
        assert_ne!(replacement, "layer:Walls");
        d.execute("layer.set", json!({"name":"layer:Walls","color":"red"})).unwrap();
        assert_eq!(load(&mut d, "layer:Walls")["data"]["color"], json!({"kind":"index","value":1}));
        assert_ne!(load(&mut d, replacement.as_str().unwrap())["data"]["color"], json!({"kind":"index","value":1}));
    }
}

#[test]
fn style_block_and_layout_ids_survive_rename() {
    let mut d = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    d.execute("style", json!({"name":"Notes"})).unwrap();
    let h = d.execute("line", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    d.execute("block", json!({"name":"Door","base":[0,0],"handles":[h],"keep":"delete"})).unwrap();
    for (table, from, to) in [("style", "Notes", "NewNotes"), ("block", "Door", "NewDoor")] {
        d.execute("rename", json!({"table":table,"from":from,"to":to})).unwrap();
    }
    d.execute("layout.rename", json!({"from":"Layout1","to":"Sheet"})).unwrap();
    d = CadDocument::open("Plan.dxf", &d.save(Some("Plan.dxf")).unwrap()).unwrap();
    for (id, name) in [("textstyle:Notes", "NewNotes"), ("block:Door", "NewDoor"), ("layout:Layout1", "Sheet")] {
        assert_eq!(load(&mut d, id)["name"], name);
    }
}
