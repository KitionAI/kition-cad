//! C8-cad-read-2: independent file writers and necessity controls for the reviewed classes.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

pub const FILE: &str = "Premises.dxf";
pub struct Fixture {
    pub kits: KitManager,
    pub workspace: tempfile::TempDir,
    pub line: Value,
    pub text: Value,
    pub dim: Value,
    pub insert: Value,
    pub circle: Value,
    pub table: Value,
    pub joined: Value,
}
impl Fixture {
    pub fn new() -> Self {
        let mut d = CadDocument::create(FILE, 420, 297).unwrap();
        d.execute("layer.add", json!({"name":"Editable"})).unwrap();
        let line = d.execute("line.add", json!({"points":[[0,0],[100,0]],"layer":"Editable"})).unwrap()["handles"][0].clone();
        let circle = d.execute("circle.add", json!({"center":[100,100],"radius":5,"layer":"Editable"})).unwrap()["handle"].clone();
        let text = d.execute("text.add", json!({"at":[0,50],"text":"Before","height":3,"layer":"Editable"})).unwrap()["handle"].clone();
        let dim =
            d.execute("dimension.addLinear", json!({"p1":[200,100],"p2":[250,100],"at":[225,90],"layer":"Editable"})).unwrap()["handle"].clone();
        let att = d.execute("attribute.define", json!({"at":[200,200],"tag":"CODE","default":"A","height":3})).unwrap()["handle"].clone();
        d.execute("block.addDialog", json!({"name":"Marker","base":[200,200],"handles":[att],"keep":"delete"})).unwrap();
        let insert = d.execute("block.insert", json!({"name":"Marker","at":[300,200],"layer":"Editable"})).unwrap()["handle"].clone();
        let table = d.execute("table.add", json!({"at":[400,0],"rows":3,"cols":3})).unwrap()["handle"].clone();
        d.execute("layer.add", json!({"name":"Secondary"})).unwrap();
        let joined = d.execute("line.add", json!({"points":[[100,0],[100,50]],"layer":"Secondary"})).unwrap()["handles"][0].clone();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), d.save(Some(FILE)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().into()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]);
        Self { kits, workspace, line, text, dim, insert, circle, table, joined }
    }
    pub fn path(&self) -> PathBuf {
        self.workspace.path().join(FILE)
    }
    pub fn disk(&self) -> Vec<u8> {
        std::fs::read(self.path()).unwrap()
    }
    pub async fn call(&self, action: &str, mut p: Value) -> Result<Value, Value> {
        p["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("cad_{action}"), &p, CallOptions::default(), "agent-me").await.unwrap() {
            Ok(result) => Ok(result.output),
            Err(error) => Err(error.structured()),
        }
    }
    #[allow(dead_code)]
    fn add_attribute(&self, tag: &str) {
        let before = self.disk();
        let modified = std::fs::metadata(self.path()).unwrap().modified().unwrap();
        let mut drawing = kition_cad_io::read(&before, FILE).unwrap();
        let handle = kition_cad_doc::Handle::parse_hex(self.insert.as_str().unwrap()).unwrap();
        drawing
            .modify_entity(handle, |entity| {
                if let kition_cad_doc::EntityKind::Insert(insert) = &mut entity.kind {
                    let mut attr = insert.attribs.first().unwrap().clone();
                    attr.tag = tag.to_owned();
                    attr.text.value = "New".to_owned();
                    insert.attribs.push(attr);
                }
            })
            .unwrap();
        let after = kition_cad_io::write(&drawing, FILE).unwrap();
        assert_ne!(before, after);
        std::fs::write(self.path(), after).unwrap();
        std::fs::File::options().write(true).open(self.path()).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
    }
    pub fn rewrite(&self, edit: impl FnOnce(&mut kition_cad_doc::Drawing)) {
        let before = self.disk();
        let mut drawing = kition_cad_io::read(&before, FILE).unwrap();
        edit(&mut drawing);
        let after = kition_cad_io::write(&drawing, FILE).unwrap();
        assert_ne!(before, after);
        std::fs::write(self.path(), after).unwrap();
    }
    pub fn colleague(&self, steps: &[(&str, Value)]) {
        let bytes = self.disk();
        let modified = std::fs::metadata(self.path()).unwrap().modified().unwrap();
        let mut d = CadDocument::open(FILE, &bytes).unwrap();
        for (command, p) in steps {
            d.execute(command, p.clone()).unwrap_or_else(|e| panic!("colleague {command}: {e}"));
        }
        let after = d.save(Some(FILE)).unwrap();
        assert_ne!(bytes, after);
        std::fs::write(self.path(), after).unwrap();
        std::fs::File::options().write(true).open(self.path()).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
    }
}
pub fn content(d: &kition_cad_doc::Drawing) -> Value {
    let entities: Vec<_> = d.model.iter().map(|e| json!([e.common, e.kind]).to_string()).collect();
    let header: std::collections::BTreeMap<_, _> =
        d.header.vars.iter().filter(|(k, _)| !matches!(k.as_str(), "EXTMIN" | "EXTMAX" | "HANDSEED")).collect();
    let members = |store: &kition_cad_doc::EntityStore| {
        let values: Vec<_> = store.iter().map(|e| json!([e.common, e.kind]).to_string()).collect();
        values
    };
    let blocks: std::collections::BTreeMap<_, _> =
        d.blocks.iter().map(|(name, block)| (name, json!({"data":block,"members":members(&block.entities)}))).collect();
    let layouts: Vec<_> = d
        .layouts
        .iter()
        .map(|layout| json!({"name":layout.name,"page":layout.page,"order":layout.tab_order,"view":layout.view,"members":members(&layout.entities)}))
        .collect();
    json!({"entities":entities,"layouts":layouts,"blocks":blocks,"layers":d.layers,"linetypes":d.linetypes,"styles":d.text_styles,"dimstyles":d.dim_styles,"groups":d.groups,"header":header,"constraints":d.constraints,"parametric":d.parametric})
}

pub async fn check(
    write: impl Fn(&Fixture) -> (&'static str, Value),
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    for refuse in [false, true] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let observed = f.disk();
        let colleague = if refuse { necessary(&f) } else { unrelated(&f) };
        f.colleague(&colleague);
        let before = f.disk();
        let (command, params) = write(&f);
        let expected = if refuse {
            None
        } else {
            // The current layer, styles and settings (kdm_model::is_current_var) are each
            // person's own: the agent keeps those of the file it observed, whatever the colleague
            // saved as theirs.
            let mine = kition_cad_io::read(&observed, FILE).unwrap();
            let mut file = kition_cad_io::read(&before, FILE).unwrap();
            let oracle_bytes = if kdm_model::current_vars(&mine) != kdm_model::current_vars(&file) {
                kdm_model::carry_current(&mine, &mut file);
                kition_cad_io::write(&file, FILE).unwrap()
            } else {
                before.clone()
            };
            let mut oracle = CadDocument::open(FILE, &oracle_bytes).unwrap();
            oracle.execute(command, params.clone()).unwrap_or_else(|e| panic!("oracle {command}: {e}"));
            Some(kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap())
        };
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await;
        if refuse {
            let error = answer.expect_err("a needed premise changed");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(before, f.disk());
        } else {
            answer.unwrap_or_else(|e| panic!("{command} after unrelated {colleague:?} must apply: {e}"));
            let actual = kition_cad_io::read(&f.disk(), FILE).unwrap();
            assert_eq!(content(&actual), content(expected.as_ref().unwrap()), "{command}: saved result");
        }
        f.kits.shutdown().await;
    }
}
