//! C8-land-11-cad: independent disk writer, fixed rules and fixed-seed tables.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const FILE: &str = "Plan.dxf";
struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
    a: Value,
    b: Value,
    table: Value,
}
impl Fixture {
    fn new() -> Self {
        let mut d = CadDocument::create(FILE, 420, 297).unwrap();
        let a = d.execute("line.add", json!({"points":[[0,0],[100,0]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[0,50],[100,50]]})).unwrap()["handles"][0].clone();
        let table =
            d.execute("table.add", json!({"at":[200,0],"rows":3,"cols":2,"cells":[["A","B"],["C","D"],["E","F"]]})).unwrap()["handle"].clone();
        d.execute("layer.add", json!({"name":"Walls","color":"white"})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), d.save(Some(FILE)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().into()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]);
        Self { kits, workspace, a, b, table }
    }
    fn path(&self) -> PathBuf {
        self.workspace.path().join(FILE)
    }
    fn disk(&self) -> Vec<u8> {
        std::fs::read(self.path()).unwrap()
    }
    async fn call(&self, action: &str, mut p: Value) -> Result<Value, Value> {
        p["path"] = json!(FILE);
        let guard = p.get("guard").and_then(Value::as_str).and_then(tools::Guard::parse).unwrap_or_default();
        if let Some(map) = p.as_object_mut() {
            map.remove("guard");
        }
        match tools::call_as(&self.kits, &format!("cad_{action}"), &p, CallOptions { guard, ..Default::default() }, "agent-me").await.unwrap() {
            Ok(result) => Ok(result.output),
            Err(error) => Err(error.structured()),
        }
    }
    async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }
    fn colleague(&self, steps: &[(&str, Value)]) {
        colleague(&self.path(), steps);
    }
    async fn write(&self, basis: Value, guard: &str, command: &str, params: Value) -> Result<Value, Value> {
        self.call("run", json!({"basis":basis,"guard":guard,"steps":[{"command":command,"params":params}]})).await
    }
    async fn stop(self) {
        self.kits.shutdown().await;
    }
}

fn colleague(path: &Path, steps: &[(&str, Value)]) {
    let bytes = std::fs::read(path).unwrap();
    let modified = std::fs::metadata(path).unwrap().modified().unwrap();
    let mut d = CadDocument::open(FILE, &bytes).unwrap();
    for (command, p) in steps {
        d.execute(command, p.clone()).unwrap_or_else(|e| panic!("colleague {command}: {e}"));
    }
    let after = d.save(Some(FILE)).unwrap();
    assert_ne!(bytes, after, "the independent writer must really change the file");
    std::fs::write(path, after).unwrap();
    // Do not let increasing mtime stand in for content detection.
    std::fs::File::options().write(true).open(path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
}

fn reason(error: &Value, reason: &str) {
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == reason), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn unrelated_change_applies_and_others_names_only_the_changed_entity() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("object.setProperties", json!({"handles":[f.b],"color":"blue"}))]);
    let answer = f.write(basis, "premise", "object.setProperties", json!({"handles":[f.a],"color":"red"})).await.unwrap();
    let ids: Vec<_> = answer["others"]["changed"].as_array().unwrap().iter().map(|e| e["id"].clone()).collect();
    assert_eq!(ids, vec![f.b.clone()], "{answer}");
    let entity = answer["others"]["changed"].as_array().unwrap().iter().find(|e| e["id"] == f.b).unwrap();
    assert_eq!(entity["fields"], json!(["common.color.kind", "common.color.value"]), "public receipts contain only the proven stored edits");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn same_field_is_changed_and_a_deleted_target_is_deleted() {
    for deleted in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[(
            if deleted { "object.delete" } else { "object.setProperties" },
            if deleted { json!({"handles":[f.a]}) } else { json!({"handles":[f.a],"color":"blue"}) },
        )]);
        let before = f.disk();
        let refused = f.write(basis, "premise", "object.setProperties", json!({"handles":[f.a],"color":"red"})).await.unwrap_err();
        reason(&refused, if deleted { "deleted" } else { "changed" });
        assert_eq!(before, f.disk());
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn geometry_changes_do_not_refuse_a_color_by_id_but_do_relocate_a_pick() {
    for pick in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[f.a],"delta":[0,20]}))]);
        let params = if pick { json!({"pick":[50,0],"aperture":0.1,"color":"red"}) } else { json!({"handles":[f.a],"color":"red"}) };
        let answer = f.write(basis, "premise", "object.setProperties", params).await;
        if pick {
            reason(&answer.unwrap_err(), "relocated");
        } else {
            answer.unwrap();
        }
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unrelated_fields_on_the_same_object_apply_and_moves_hold_the_lock() {
    for lock in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        if lock {
            f.colleague(&[("layer.set", json!({"name":"0","locked":true}))]);
        } else {
            f.colleague(&[("object.setProperties", json!({"handles":[f.a],"color":"blue"}))]);
        }
        let answer = f.write(basis, "premise", "object.move", json!({"handles":[f.a],"delta":[2,0]})).await;
        if lock {
            reason(&answer.unwrap_err(), "changed");
        } else {
            answer.unwrap();
        }
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn another_object_moving_into_the_pick_is_refused() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("object.move", json!({"handles":[f.b],"delta":[0,-50]}))]);
    reason(&f.write(basis, "premise", "object.setProperties", json!({"pick":[50,0],"aperture":0.1,"color":"red"})).await.unwrap_err(), "relocated");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn overlapping_draw_order_changes_relocate_a_pick() {
    let f = Fixture::new();
    f.colleague(&[("object.move", json!({"handles":[f.b],"delta":[0,-50]}))]);
    let basis = f.basis().await;
    f.colleague(&[("arrange.bringToFront", json!({"handles":[f.a]}))]);
    reason(&f.write(basis, "premise", "object.setProperties", json!({"pick":[50,0],"aperture":0.1,"color":"red"})).await.unwrap_err(), "relocated");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_move_relocates_a_pick_of_its_children() {
    let f = Fixture::new();
    let mut d = CadDocument::open(FILE, &f.disk()).unwrap();
    d.execute("block.addDialog", json!({"name":"Parent","base":[0,0],"handles":[f.a],"keep":"delete"})).unwrap();
    let parent = d.execute("block.insert", json!({"name":"Parent","at":[0,0]})).unwrap()["handle"].clone();
    std::fs::write(f.path(), d.save(Some(FILE)).unwrap()).unwrap();
    let basis = f.basis().await;
    f.colleague(&[("object.move", json!({"handles":[parent],"delta":[0,20]}))]);
    reason(&f.write(basis, "premise", "object.setProperties", json!({"pick":[50,0],"aperture":0.1,"color":"red"})).await.unwrap_err(), "relocated");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn swapping_named_layers_with_different_values_is_refused() {
    let f = Fixture::new();
    f.colleague(&[("layer.add", json!({"name":"Other","color":"blue"}))]);
    let basis = f.basis().await;
    f.colleague(&[
        ("layer.set", json!({"name":"Walls","newName":"Temporary"})),
        ("layer.set", json!({"name":"Other","newName":"Walls"})),
        ("layer.set", json!({"name":"Temporary","newName":"Other"})),
    ]);
    reason(&f.write(basis, "premise", "layer.set", json!({"name":"Walls","color":"red"})).await.unwrap_err(), "relocated");
    f.stop().await;
}

// An observed empty insertion gap keeps its ordinal after changes before it.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_insertion_gap_holds_its_ordinal() {
    let f = Fixture::new();
    let read = f.call("query", json!({"objects":[f.a],"fields":["common","kind"]})).await.unwrap();
    let fields = &read["objects"][0]["fields"];
    let basis = f.basis().await;
    f.colleague(&[("line.add", json!({"points":[[0,100],[100,100]]})), ("arrange.sendToBack", json!({"handles":[f.b]}))]);
    let saved = f.disk();
    let answer = f.call("batch", json!({"basis":basis,"ops":[{"op":"create","type":"Entity","in":"root.model","at":1,"fields":fields}]})).await;
    reason(&answer.unwrap_err(), "relocated");
    assert_eq!(f.disk(), saved);
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rename_away_is_refused_before_the_name_resolver_runs() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("layer.set", json!({"name":"Walls","newName":"Away"}))]);
    let error = f.write(basis, "premise", "layer.set", json!({"name":"Walls","color":"red"})).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
/// The current layer is each person's own: a colleague who made Walls their current layer changes
/// nothing of mine, so a new line without a layer applies and lands on my current layer (0), one
/// with an explicit layer lands there, and the saved file carries my current layer.
async fn creation_holds_my_current_layer_not_a_colleagues_and_respects_explicit_layer() {
    for explicit in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("layer.setCurrent", json!({"name":"Walls"}))]);
        let mut p = json!({"points":[[0,100],[100,100]]});
        if explicit {
            p["layer"] = json!("Walls");
        }
        let result = f.write(basis, "premise", "line.add", p).await.unwrap();
        let h = result["completed"][0]["result"]["handles"][0].as_str().unwrap().to_owned();
        let saved = kition_cad_io::read(&f.disk(), FILE).unwrap();
        let line = saved.entity(kition_cad_doc::Handle::parse_hex(&h).unwrap()).unwrap();
        assert_eq!(line.common.layer, if explicit { "Walls" } else { "0" });
        assert_eq!(saved.header.str("CLAYER", "?"), "0", "the file carries the saving session's current layer");
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn inserting_before_a_table_cell_refuses_but_appending_applies() {
    for row in [0, 3] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("table.insertRow", json!({"handle":f.table,"row":row}))]);
        let result = f.write(basis, "premise", "table.set", json!({"handle":f.table,"row":1,"col":0,"text":"mine"})).await;
        if row == 0 {
            reason(&result.unwrap_err(), "changed");
        } else {
            result.unwrap();
        }
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn epoch_premise_and_off_have_distinct_behavior() {
    for guard in ["epoch", "premise", "off"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("object.setProperties", json!({"handles":[f.a],"color":"blue"}))]);
        let result = f.write(basis, guard, "object.setProperties", json!({"handles":[f.a],"color":"red"})).await;
        if guard == "off" {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
        }
        f.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_unrelated_rounds() {
    for guard in ["premise", "epoch"] {
        let f = Fixture::new();
        let mut refused = 0;
        for round in 0..50 {
            let basis = f.basis().await;
            f.colleague(&[("object.move", json!({"handles":[f.b],"delta":[1,0]}))]);
            let r = f.write(basis, guard, "object.setProperties", json!({"handles":[f.a],"color":if round%2==0 {"red"}else{"green"}})).await;
            refused += usize::from(r.is_err());
        }
        println!("UNRELATED {guard}: rounds=50 applied={} refused={refused}", 50 - refused);
        assert_eq!(refused, if guard == "premise" { 0 } else { 50 });
        f.stop().await;
    }
}

// Necessary rules, fixed BEFORE running:
// id color: A deleted or its color changed. Its move/visibility and other objects are unrelated.
// name color: Walls renamed away or its color changed; other properties/objects are unrelated.
// point color: the engine's picked identity at [50,0] changed, or the picked A's color changed.
// erase: any stored change of A or deletion of A, since this write discards all its content.
// Each round tries all four write forms on independent copies. No outcome changes these rules.
const KINDS: [&str; 10] =
    ["B-color", "A-color", "A-move", "B-into-pick", "A-delete", "Walls-rename", "Walls-color", "new-layer", "A-hidden", "B-move"];
#[tokio::test(flavor = "multi_thread")]
async fn mixed_fifty_rounds_fixed_seed() {
    let mut seed = 0x9e3779b97f4a7c15u64;
    // Query rule: qselect holds its observed matching IDs and their whole values because a
    // following step consumes the selection. Both lines match; unused layer edits do not.
    let mut table = [[[0u32; 5]; 5]; 10];
    for _ in 0..50 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let kind = (seed % 10) as usize;
        for (form, counts) in table[kind].iter_mut().enumerate() {
            let f = Fixture::new();
            let basis = f.basis().await;
            let change = match kind {
                0 => ("object.setProperties", json!({"handles":[f.b],"color":"blue"})),
                1 => ("object.setProperties", json!({"handles":[f.a],"color":"blue"})),
                2 => ("object.move", json!({"handles":[f.a],"delta":[0,20]})),
                3 => ("object.move", json!({"handles":[f.b],"delta":[0,-50]})),
                4 => ("object.delete", json!({"handles":[f.a]})),
                5 => ("layer.set", json!({"name":"Walls","newName":"Away"})),
                6 => ("layer.set", json!({"name":"Walls","color":"blue"})),
                7 => ("layer.add", json!({"name":"Other"})),
                8 => ("object.setProperties", json!({"handles":[f.a],"visible":false})),
                _ => ("object.move", json!({"handles":[f.b],"delta":[1,0]})),
            };
            f.colleague(&[change]);
            let necessary = match form {
                0 => matches!(kind, 1 | 4),
                1 => matches!(kind, 5 | 6),
                2 => matches!(kind, 1 | 2 | 3 | 4 | 8),
                3 => matches!(kind, 1 | 2 | 4 | 8),
                _ => matches!(kind, 0 | 1 | 2 | 3 | 4 | 8 | 9),
            };
            let (command, params) = match form {
                0 => ("object.setProperties", json!({"handles":[f.a],"color":"red"})),
                1 => ("layer.set", json!({"name":"Walls","color":"red"})),
                2 => ("object.setProperties", json!({"pick":[50,0],"aperture":0.1,"color":"red"})),
                _ => ("object.delete", json!({"handles":[f.a]})),
            };
            let before = f.disk();
            let result = if form == 4 {
                f.call(
                    "run",
                    json!({"basis":basis,"steps":[
                        {"command":"select.quick","params":{"type":"Line"}},
                        {"command":"object.setProperties","params":{"color":"red"}}
                    ]}),
                )
                .await
            } else {
                f.write(basis, "premise", command, params).await
            };
            let class = match &result {
                Ok(_) => {
                    if necessary {
                        3
                    } else {
                        0
                    }
                }
                Err(e) => {
                    if e["error"]["code"] == "conflict" {
                        if necessary { 1 } else { 2 }
                    } else {
                        4
                    }
                }
            };
            counts[class] += 1;
            if result.is_err() {
                assert_eq!(f.disk(), before, "a refused write changes nothing");
            }
            if class >= 2 {
                println!("MIXED bad {} form {form} necessary={necessary}: {result:?}", KINDS[kind]);
            }
            // Inspect the file, not merely the response's lack of an error.
            if class == 0 {
                let mut d = CadDocument::open(FILE, &f.disk()).unwrap();
                if form == 0 || form == 2 {
                    let p = d.execute("object.properties", json!({"handles":[f.a]})).unwrap();
                    assert_eq!(p["objects"][0]["color"], "Red", "{p}");
                }
                if form == 1 {
                    let drawing = kition_cad_io::read(&f.disk(), FILE).unwrap();
                    assert_eq!(drawing.layer("Walls").unwrap().color.to_aci(), 1, "the named write must be saved");
                }
                if form == 3 {
                    assert!(
                        kition_cad_io::read(&f.disk(), FILE)
                            .unwrap()
                            .model
                            .get(kition_cad_doc::Handle::parse_hex(f.a.as_str().unwrap()).unwrap())
                            .is_none()
                    );
                }
                if form == 4 {
                    let p = d.execute("object.properties", json!({"handles":[f.a,f.b]})).unwrap();
                    assert_eq!(p["objects"].as_array().unwrap().len(), 2);
                    assert!(p["objects"].as_array().unwrap().iter().all(|o| o["color"] == "Red"), "query result must be saved: {p}");
                }
            }
            f.stop().await;
        }
    }
    for (kind, row) in KINDS.iter().zip(table) {
        println!("MIXED {kind}: id={:?} name={:?} point={:?} erase={:?} query={:?}", row[0], row[1], row[2], row[3], row[4]);
    }
    let totals = (0..5).map(|c| table.iter().flat_map(|r| r.iter()).map(|v| v[c]).sum::<u32>()).collect::<Vec<_>>();
    println!("MIXED TOTAL applied/necessary/false/wrong/kit={totals:?}");
    assert_eq!(&totals[2..], &[0, 0, 0]);
}
