//! Public SDK contracts, exercised with the CAD model and an independent DXF writer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_headless::{Headless, KitInfo};
use kition_kit::{KitCommand, KitDocument, Premise, Under, session::model::ReplicaPort};
use kition_kit_server::backend::KitBackend;
use kition_kits::{KitDefinition, KitManager, tools};
use serde_json::{Value, json};
use std::sync::Arc;

const MANIFEST: &str = include_str!("../../../kit.json");
const FILE: &str = "Waiting.dxf";

// This executable is both the test and its synthetic kit. Only the probe's schema and
// premises differ from CAD; the DXF reads, writes and colleague's edit use the real kit.
fn main() -> std::process::ExitCode {
    if std::env::args().any(|arg| arg == "--kition-kit") {
        return kition_headless::run_manifest_model::<Probe>(MANIFEST, &[], kition_cad_kit::EDITOR_STATE);
    }
    tokio::runtime::Runtime::new().unwrap().block_on(position_parent_premises_hold_changed_leaves());
    nested_union_schema_roles_are_not_available();
    std::process::ExitCode::SUCCESS
}

struct Probe(CadDocument);

impl KitDocument for Probe {
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        CadDocument::open(name, bytes).map(Self)
    }
    fn create(name: &str, width: u32, height: u32) -> Result<Self, String> {
        CadDocument::create(name, width, height).map(Self)
    }
    fn width(&self) -> u32 {
        self.0.width()
    }
    fn height(&self) -> u32 {
        self.0.height()
    }
    fn revision(&self) -> u64 {
        self.0.revision()
    }
    fn dirty(&self) -> bool {
        self.0.dirty()
    }
    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.0.render(x, y, scale, width, height)
    }
    fn render_area(&self, x: i32, y: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.0.render_area(x, y, x1, y1, width, height)
    }
    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        let (command, params) = mapped(command, params);
        self.0.execute(command, params)
    }
    fn inspect(&self) -> Result<Value, String> {
        self.0.inspect()
    }
    fn commands(&self) -> Vec<KitCommand> {
        vec![
            KitCommand {
                id: "review.position".into(),
                title: "Position probe".into(),
                enabled: true,
                mutates: true,
                targets: Some("handle"),
                object_types: &[("handle", &["Entity"])],
                schema: Some(json!({"handle":{"type":"string"},"delta":{"type":"array","items":{"type":"number"}}})),
                ..Default::default()
            },
            KitCommand {
                id: "review.union".into(),
                title: "Union probe".into(),
                enabled: true,
                schema: Some(json!({"reference":{"oneOf":[
                    {"type":"array","items":{"type":"number"}},
                    {"type":"object","properties":{"source":{"type":"string"}},"additionalProperties":false}
                ]}})),
                object_types: &[("reference.source", &["Layer"])],
                name_params: &[("reference.source", "name")],
                ..Default::default()
            },
        ]
    }
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        self.0.save(name)
    }
    fn mark_saved(&mut self) {
        self.0.mark_saved();
    }
    fn hosted(&mut self, hosting: &kition_kit::host::Hosting) {
        self.0.hosted(hosting);
    }
    fn editor_state(&self) -> Value {
        self.0.editor_state()
    }
    fn restore_editor_state(&mut self, saved: &Value) -> Result<(), String> {
        self.0.restore_editor_state(saved)
    }
    fn fork_for_premises(&self) -> Result<Self, String> {
        self.0.fork_for_premises().map(Self)
    }
    fn carry_replica_objects(&mut self, previous: &Self) -> Result<(), String> {
        self.0.carry_replica_objects(&previous.0)
    }
    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        self.0.reconcile_reread(&fresh.0)
    }
    fn under(&self, command: &str, params: &Value) -> Result<Option<Under>, String> {
        if command != "review.position" {
            return Ok(None);
        }
        Ok(Some(Under { premises: vec![Premise::on(params["handle"].clone()).fields([] as [&str; 0]).positions(["kind.a"])], ..Default::default() }))
    }
}

fn mapped(command: &str, params: Value) -> (&str, Value) {
    if command == "review.position" { ("object.move", json!({"handles":[params["handle"]],"delta":params["delta"]})) } else { (command, params) }
}

impl ReplicaPort for Probe {
    fn model_schema(&self) -> Arc<kition_kit::kdm::Schema> {
        let mut schema: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
        schema["types"]["Entity"]["fields"]["kind"]["position"] = json!(true);
        Arc::new(kition_kit::kdm::Schema::parse(&schema.to_string()).unwrap())
    }
    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.0.model_export(since)
    }
    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.0.model_import(bytes)
    }
    fn model_command(&mut self, command: &str, params: Value, origin: &kition_kit::kdm::Origin) -> Result<Value, String> {
        let (command, params) = mapped(command, params);
        self.0.model_command(command, params, origin)
    }
    fn model_commands(&self) -> Vec<KitCommand> {
        self.commands()
    }
}

fn nested_union_schema_roles_are_not_available() {
    let kit = KitInfo::from_manifest(MANIFEST, &[], kition_cad_kit::EDITOR_STATE).unwrap();
    let commands = Headless::<Probe>::new(kit).commands(None).unwrap();
    let schema = &commands.iter().find(|c| c.id == "review.union").unwrap().params;
    let source = &schema["properties"]["reference"]["oneOf"][1]["properties"]["source"];
    // Public SDK headless::nested_property walks properties/items, never oneOf.
    // Keep this limitation explicit; production CAD uses declared top-level roles.
    assert!(source.get("x-khp-name").is_none(), "{schema}");
    assert!(source.get("x-khp-types").is_none(), "{schema}");
    println!("LIMITATION: public SDK does not annotate named members nested in oneOf");
}

async fn position_parent_premises_hold_changed_leaves() {
    let mut drawing = CadDocument::create(FILE, 420, 297).unwrap();
    let handle = drawing.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    std::fs::write(&path, drawing.save(Some(FILE)).unwrap()).unwrap();
    let kit = KitDefinition::from_manifest_text(MANIFEST).unwrap();
    let kits = KitManager::new(Some(dir.path().into()), vec![(kit, Some(std::env::current_exe().unwrap()))]);
    let before = tools::call(&kits, "cad_inspect", &json!({"path":FILE})).await.unwrap().unwrap().output;
    let bytes = std::fs::read(&path).unwrap();
    let mut colleague = CadDocument::open(FILE, &bytes).unwrap();
    colleague.execute("object.move", json!({"handles":[handle],"delta":[1,0]})).unwrap();
    let after = colleague.save(Some(FILE)).unwrap();
    assert_ne!(bytes, after);
    std::fs::write(&path, &after).unwrap();
    let answer = tools::call(
        &kits,
        "cad_run",
        &json!({
            "path":FILE,"basis":before["revision"],
            "steps":[{"command":"review.position","params":{"handle":handle,"delta":[1,0]}}]
        }),
    )
    .await
    .unwrap();
    kits.shutdown().await;
    let answer = answer.unwrap_err().structured();
    assert_eq!(answer["error"]["code"], "conflict", "{answer}");
    assert!(answer["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == "relocated"), "{answer}");
    assert_eq!(std::fs::read(&path).unwrap(), after);
    assert!(answer.get("guardUnavailable").is_none(), "{answer}");
    assert!(
        answer["others"]["changed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == handle && e["edits"].as_array().unwrap().iter().any(|edit| edit["field"] == "kind.a.x")),
        "the guard must have seen the colleague's held positional leaf: {answer}"
    );
    let mut reread = CadDocument::open(FILE, &std::fs::read(&path).unwrap()).unwrap();
    let measured = reread.execute("object.measure", json!({"bounds":[handle]})).unwrap();
    assert_eq!(measured["bounds"][0]["min"][0].as_f64(), Some(1.0), "the refused move must leave the colleague's geometry: {measured}");
}
