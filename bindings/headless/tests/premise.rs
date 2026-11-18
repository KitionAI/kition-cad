//! The CAD kit's call context, receipts, ids and the guard end to end (task card C4/C5/C6-cad):
//! the headless `kition-cad` executable under the kits' MCP server's tools (`kition-kits`, the
//! library `kition-kits-mcp` serves, at the pinned kition-sdk revision), on a `.dxf` in a
//! workspace, called by two agents.
//!
//! A run names where each step acts (`target`, `at.part`: a space by `{"layout": name}`) and what
//! on (`on`: handles); receipts (`changes`), `others` and `conflicts` name entities by their
//! handles and layers and layouts by `layer:<name>` and `layout:<name>`, the ids inspect lists and
//! commands take; nothing an agent's run navigates to is saved into the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use serde_json::{Value, json};

const ME: &str = "agent-me";
const OTHER: &str = "agent-other";
const FILE: &str = "Plan.dxf";

fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()
}

fn handle(answer: &Value) -> String {
    answer["handle"].as_str().or_else(|| answer["handles"][0].as_str()).unwrap_or_else(|| panic!("no handle in {answer}")).to_owned()
}

/// Model space: rectangle `R` (a closed polyline, 0,0 to 100,50), lines `A` and `B`, an insert of
/// block `Door` (a line), layer `Walls`. Layout1: line `P`. Saved with the person in Model space.
struct Drawing {
    bytes: Vec<u8>,
    r: String,
    a: String,
    b: String,
    door_line: String,
    insert: String,
    p: String,
}

fn drawing() -> Drawing {
    let bytes = CadDocument::create(FILE, 420, 297).unwrap().save(Some(FILE)).unwrap();
    let mut doc = CadDocument::open(FILE, &bytes).unwrap();
    let r = handle(&doc.execute("rectang", json!({ "p1": [0, 0], "p2": [100, 50] })).unwrap());
    let a = handle(&doc.execute("line", json!({ "points": [[0, 80], [100, 80]] })).unwrap());
    let b = handle(&doc.execute("line", json!({ "points": [[0, 90], [100, 90]] })).unwrap());
    let door_line = handle(&doc.execute("line", json!({ "points": [[300, 0], [300, 20]] })).unwrap());
    doc.execute("block", json!({ "name": "Door", "base": [300, 0], "handles": [door_line] })).unwrap();
    let insert = handle(&doc.execute("insert", json!({ "name": "Door", "at": [150, 0] })).unwrap());
    doc.execute("layer.new", json!({ "name": "Walls" })).unwrap();
    let p = handle(&doc.execute("line", json!({ "points": [[10, 10], [60, 10]], "layout": "Layout1" })).unwrap());
    let bytes = doc.save(Some(FILE)).unwrap();
    // The block's line: its handle inside the definition, as the file has it.
    let reopened = kition_cad_io::read(&bytes, FILE).unwrap();
    let door_line = reopened.blocks.get("Door").and_then(|b| b.entities.iter().next().map(|e| e.handle.hex())).unwrap_or(door_line);
    Drawing { bytes, r, a, b, door_line, insert, p }
}

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
    d: Drawing,
}

impl Fixture {
    fn new() -> Self {
        let d = drawing();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), &d.bytes).unwrap();
        Self { kits: Self::server(workspace.path()), workspace, d }
    }

    /// A new server on `workspace` (a process of `kition-kits-mcp` is one of these).
    fn server(workspace: &Path) -> KitManager {
        KitManager::new(Some(workspace.to_path_buf()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))])
    }

    fn file(&self) -> Vec<u8> {
        std::fs::read(self.workspace.path().join(FILE)).unwrap()
    }

    async fn call_as(&self, agent: &str, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("cad_{action}"), &input, CallOptions::default(), agent).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("cad_{action} is no tool"),
        }
    }

    async fn ok(&self, action: &str, input: Value) -> Value {
        self.by(ME, action, input).await
    }

    async fn by(&self, agent: &str, action: &str, input: Value) -> Value {
        self.call_as(agent, action, input).await.unwrap_or_else(|error| panic!("{agent} cad_{action}: {error}"))
    }

    async fn refused(&self, action: &str, input: Value) -> Value {
        match self.call_as(ME, action, input).await {
            Err(error) => error,
            Ok(answer) => panic!("cad_{action} was expected to be refused: {answer}"),
        }
    }

    /// The handles of a space's entities, as `entities {layout}` lists them.
    async fn space(&self, layout: &str) -> Vec<String> {
        let ran = self.ok("run", json!({ "save": false, "steps": [{ "command": "entities", "params": { "layout": layout } }] })).await;
        ran["completed"][0]["result"]["entities"].as_array().unwrap().iter().map(|e| e["handle"].as_str().unwrap().to_owned()).collect()
    }

    async fn revision(&self) -> String {
        revision(&self.ok("inspect", json!({})).await)
    }

    async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

fn revision(answer: &Value) -> String {
    answer["revision"].as_str().unwrap_or_else(|| panic!("a revision: {answer}")).to_owned()
}

/// The receipt entry for `id` in `list` (`changes.changed`, `deleted`, …).
fn entry(list: &Value, id: &str) -> Value {
    list.as_array().and_then(|items| items.iter().find(|item| item["id"] == id)).cloned().unwrap_or_else(|| panic!("no {id} in {list}"))
}

fn edit(changed: &Value, field: &str) -> Value {
    changed["edits"]
        .as_array()
        .and_then(|edits| edits.iter().find(|edit| edit["field"] == field))
        .cloned()
        .unwrap_or_else(|| panic!("no edit of {field}: {changed}"))
}

fn ids(list: &Value) -> Vec<String> {
    let mut out: Vec<String> =
        list.as_array().map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    out.sort();
    out
}

fn assert_conflict(refused: &Value, id: &str, reason: &str) {
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    let found = refused["conflicts"].as_array().and_then(|conflicts| conflicts.iter().find(|conflict| conflict["id"] == id));
    let found = found.unwrap_or_else(|| panic!("no conflict on {id}: {refused}"));
    assert_eq!(found["reason"], reason, "{refused}");
}

// ---- Receipts name what a call changed by handle ----

#[tokio::test(flavor = "multi_thread")]
async fn receipts_name_entities_by_handle_with_before_and_after() {
    let f = Fixture::new();
    let d = &f.d;
    // A move: the entity's geometry (`kind`), before and after.
    let ran = f.ok("run", json!({ "steps": [{ "command": "move", "on": [d.a], "params": { "delta": [0, 5] } }] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), vec![d.a.clone()], "{ran}");
    assert_eq!(entry(&ran["changes"]["changed"], &d.a)["fields"], json!(["kind.a.y", "kind.b.y"]));
    let moved = entry(&ran["changes"]["changed"], &d.a);
    assert_eq!(edit(&moved, "kind.a.y"), json!({"field":"kind.a.y","before":80,"after":85}));
    assert_eq!(edit(&moved, "kind.b.y"), json!({"field":"kind.b.y","before":80,"after":85}));
    // A property change: its `common`.
    let ran = f.ok("run", json!({ "steps": [{ "command": "properties.set", "params": { "handles": [d.b], "color": "red" } }] })).await;
    let common = entry(&ran["changes"]["changed"], &d.b);
    assert_eq!(edit(&common, "common.color.kind"), json!({"field":"common.color.kind","before":"byLayer","after":"index"}));
    assert_eq!(edit(&common, "common.color.value"), json!({"field":"common.color.value","before":null,"after":1}));
    // A vertex edit: the polyline itself, its `kind` holding the vertices.
    let ran = f.ok("run", json!({ "steps": [{ "command": "grip.stretch", "params": { "handle": d.r, "index": 2, "to": [120, 60] } }] })).await;
    let vertex = entry(&ran["changes"]["changed"], &d.r);
    assert_eq!(edit(&vertex, "kind.vertices.1.p.x"), json!({"field":"kind.vertices.1.p.x","before":100,"after":120}));
    assert_eq!(edit(&vertex, "kind.vertices.1.p.y"), json!({"field":"kind.vertices.1.p.y","before":0,"after":60}));
    // The raw common replacement leaves this child unreadable to the native view.
    // The SDK refreshes indirect guard facts for references and their block privately;
    // public changes contain only stored edits and proven public bounds.
    let ran = f
        .ok(
            "batch",
            json!({ "ops": [{ "op": "set", "target": d.door_line, "field": "common", "value": { "layer": "0", "color": { "index": 3 } } }] }),
        )
        .await;
    let mut changed = vec![d.door_line.clone(), "drawing".into()];
    changed.sort();
    assert_eq!(ids(&ran["changes"]["changed"]), changed, "{ran}");
    assert_eq!(entry(&ran["changes"]["changed"], "drawing")["fields"], json!(["~modelBounds"]), "{ran}");
    // Private outline/pick facts still protect guards, but have no public receipt entry.
    assert!(!ids(&ran["changes"]["changed"]).contains(&d.insert));
    assert!(!ids(&ran["changes"]["changed"]).contains(&"block:Door".to_owned()));
    // An erase: deleted, by handle.
    let ran = f.ok("run", json!({ "steps": [{ "command": "erase", "on": [d.b] }] })).await;
    assert_eq!(ids(&ran["changes"]["deleted"]), vec![d.b.clone()], "{ran}");
    f.shutdown().await;
}

// ---- Nothing carries over between calls ----

#[tokio::test(flavor = "multi_thread")]
async fn a_run_acts_in_two_layouts_and_the_file_keeps_no_navigation() {
    let f = Fixture::new();
    let ran = f
        .ok(
            "run",
            json!({ "target": { "layout": "Layout1" }, "steps": [
                // On the run's layout.
                { "command": "circle", "params": { "center": [50, 50], "radius": 5 } },
                // Another layout as the step's part.
                { "command": "circle", "params": { "center": [20, 20], "radius": 5 }, "at": { "part": { "layout": "Layout2" } } },
                // `on` an entity of Model space, whose handle names its space.
                { "command": "move", "on": [f.d.a], "params": { "delta": [1, 0] } },
                // An explicit layer and layout on a drawing command.
                { "command": "line", "params": { "points": [[0, 0], [5, 5]], "layer": "Walls", "layout": "Model" } },
            ] }),
        )
        .await;
    assert_eq!(ran["changes"]["created"].as_array().map(Vec::len), Some(3), "{ran}");
    assert_eq!((f.space("Layout1").await.len(), f.space("Layout2").await.len(), f.space("Model").await.len()), (2, 1, 6));
    let opened = CadDocument::open(FILE, &f.file()).unwrap().inspect().unwrap();
    assert_eq!((opened["space"].clone(), opened["currentLayer"].clone()), (json!("Model"), json!("0")), "the file keeps the person's");
    // A run that only navigates changes nothing in the file.
    let before = f.file();
    f.ok(
        "run",
        json!({ "steps": [
            { "command": "layout.set", "params": { "name": "Layout1" } },
            { "command": "zoom", "params": { "mode": "extents" } },
            { "command": "select", "params": { "window": [[0, 0], [100, 100]] } },
        ] }),
    )
    .await;
    assert_eq!(f.file(), before, "the saved drawing is the person's");
    // The next call starts where the person is: a circle without a part goes in Model space.
    f.ok("run", json!({ "steps": [{ "command": "circle", "params": { "center": [0, 0], "radius": 1 } }] })).await;
    assert_eq!(f.space("Model").await.len(), 7);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_later_step_changes_nothing() {
    let f = Fixture::new();
    let before = f.file();
    let refused = f
        .refused(
            "run",
            json!({ "target": { "layout": "Layout1" }, "steps": [
                { "command": "move", "on": [f.d.p], "params": { "delta": [5, 5] } },
                { "command": "properties.set", "params": { "handles": [f.d.p], "layer": "Fresh", "dimStyle": "Nope" } },
            ] }),
        )
        .await;
    assert_eq!(refused["error"]["index"], 1, "{refused}");
    assert_eq!(f.file(), before, "nothing changed in the file");
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn inspect_lists_every_space_and_previews_a_layout_by_name() {
    let f = Fixture::new();
    let seen = f.ok("inspect", json!({})).await;
    let layouts = seen["details"]["layouts"].clone();
    assert_eq!(layouts[0]["part"], json!({ "layout": "Model" }), "{seen}");
    assert_eq!((layouts[1]["id"].clone(), layouts[1]["entities"].clone()), (json!("layout:Layout1"), json!(1)), "{seen}");
    assert!(seen["details"].get("space").is_none() && seen["details"].get("selection").is_none(), "editor state is not what an agent reads");
    // A layout's preview by name, through the render command, without going there.
    let seen = f.ok("inspect", json!({ "target": { "layout": "Layout1" }, "preview": true })).await;
    assert_eq!(seen["preview"]["layout"], "Layout1", "{}", seen["preview"]);
    // A window of Model space.
    let seen = f.ok("inspect", json!({ "target": { "window": [-5, -5, 105, 55] }, "preview": true })).await;
    assert_eq!(seen["preview"]["window"], json!([-5.0, -5.0, 105.0, 55.0]), "{}", seen["preview"]);
    // The inspection did not move the person: a circle without a part goes in Model space.
    f.ok("run", json!({ "steps": [{ "command": "circle", "params": { "center": [0, 0], "radius": 1 } }] })).await;
    assert_eq!(f.space("Model").await.len(), 6);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn handles_from_an_earlier_call_name_the_same_entities_after_a_new_server_opens_the_saved_file() {
    let f = Fixture::new();
    let made = handle(
        &f.ok("run", json!({ "steps": [{ "command": "circle", "params": { "center": [5, 5], "radius": 2 } }] })).await["completed"][0]["result"],
    );
    f.kits.shutdown().await;
    let again = Fixture { kits: Fixture::server(f.workspace.path()), workspace: f.workspace, d: f.d };
    assert!(again.space("Model").await.contains(&made), "the circle made in the earlier call keeps its handle");
    let ran = again.ok("run", json!({ "steps": [{ "command": "move", "on": [made], "params": { "delta": [1, 1] } }] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), vec![made.clone()]);
    // The object model takes and answers the same ids.
    let read = again.ok("query", json!({ "objects": [made], "fields": ["handle", "kind"] })).await;
    assert_eq!(read["objects"][0]["fields"]["kind"]["center"], json!({"x":6,"y":6,"z":0}));
    assert_eq!((read["objects"][0]["id"].clone(), read["objects"][0]["fields"]["handle"].clone()), (json!(made), json!(made)), "{read}");
    let read = again.ok("query", json!({ "objects": ["layer:Walls", "layout:Layout1"], "fields": ["name"] })).await;
    assert_eq!(ids(&read["objects"]), ["layer:Walls", "layout:Layout1"], "{read}");
    again.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn entities_measure_through_the_run_tool() {
    let f = Fixture::new();
    let ran = f
        .ok(
            "run",
            json!({ "save": false, "steps": [
                { "command": "entities.measure", "params": { "bounds": [f.d.r] } },
                { "command": "entities.measure", "params": { "distance": [f.d.a, f.d.b] } },
                { "command": "entities.measure", "params": { "overlaps": {} } },
            ] }),
        )
        .await;
    assert_eq!(ran["completed"][0]["result"]["bounds"][0]["max"], json!([100.0, 50.0]), "{ran}");
    assert_eq!(ran["completed"][1]["result"]["distance"], json!(10.0), "{ran}");
    assert_eq!(ran["completed"][2]["result"]["pairs"], json!([]), "{ran}");
    assert!(ran.get("changes").is_none_or(|changes| ids(&changes["changed"]).is_empty()), "measuring changed nothing: {ran}");
    f.shutdown().await;
}

// ---- The guard between two agents on one server ----

#[tokio::test(flavor = "multi_thread")]
async fn another_agents_edit_of_an_unrelated_entity_is_reported_and_my_write_applies() {
    let f = Fixture::new();
    let basis = f.revision().await;
    f.by(OTHER, "run", json!({ "steps": [{ "command": "move", "on": [f.d.b], "params": { "delta": [0, 1] } }] })).await;
    let ran = f.ok("run", json!({ "basis": basis, "steps": [{ "command": "move", "on": [f.d.a], "params": { "delta": [0, 2] } }] })).await;
    assert_eq!(ids(&ran["others"]["changed"]), vec![f.d.b.clone(), "drawing".into()], "{ran}");
    assert_eq!(entry(&ran["others"]["changed"], "drawing")["fields"], json!(["~modelBounds"]), "{ran}");
    assert_eq!(ran["others"]["by"], json!([OTHER]), "{ran}");
    assert_eq!(ids(&ran["changes"]["changed"]), vec![f.d.a.clone()], "{ran}");
    assert_eq!(entry(&ran["changes"]["changed"], &f.d.a)["fields"], json!(["kind.a.y", "kind.b.y"]));
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn another_agents_edit_or_erase_of_my_target_refuses_my_write() {
    let f = Fixture::new();
    let mine = |h: &str| json!([{ "command": "move", "on": [h], "params": { "delta": [3, 0] } }]);
    // Geometry edited by the other agent: changed. A color edit alone permits the move
    // (outside.rs covers that unrelated-field case).
    let basis = f.revision().await;
    f.by(OTHER, "run", json!({ "steps": [{ "command": "move", "params": { "handles": [f.d.a], "delta": [0, 5] } }] })).await;
    let before = f.file();
    let refused = f.refused("run", json!({ "basis": basis, "steps": mine(&f.d.a) })).await;
    assert_conflict(&refused, &f.d.a, "changed");
    assert_eq!(f.file(), before, "nothing changed in the file");
    // Erased by the other agent: deleted.
    let basis = revision(&refused);
    f.by(OTHER, "run", json!({ "steps": [{ "command": "erase", "on": [f.d.b] }] })).await;
    let gone = f.file();
    let refused = f.refused("run", json!({ "basis": basis, "steps": mine(&f.d.b) })).await;
    assert_conflict(&refused, &f.d.b, "deleted");
    assert_eq!(f.file(), gone, "nothing changed in the file");
    f.shutdown().await;
}
