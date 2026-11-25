//! C8-cad-read: unrelated fields on the addressed object may change between observation and
//! an absolute field write. Each case uses an independent document to rewrite the saved DXF.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

const FILE: &str = "Premises.dxf";
struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
    line: Value,
    text: Value,
    dim: Value,
    insert: Value,
    circle: Value,
}
impl Fixture {
    fn new() -> Self {
        let mut d = CadDocument::create(FILE, 420, 297).unwrap();
        d.execute("layer.add", json!({"name":"Editable"})).unwrap();
        let line = d.execute("line", json!({"points":[[0,0],[100,0]],"layer":"Editable"})).unwrap()["handles"][0].clone();
        let circle = d.execute("circle", json!({"center":[100,100],"radius":5,"layer":"Editable"})).unwrap()["handle"].clone();
        let text = d.execute("text", json!({"at":[0,50],"text":"Before","height":3,"layer":"Editable"})).unwrap()["handle"].clone();
        let dim =
            d.execute("dimension.addLinear", json!({"p1":[200,100],"p2":[250,100],"at":[225,90],"layer":"Editable"})).unwrap()["handle"].clone();
        let att = d.execute("attribute.define", json!({"at":[200,200],"tag":"CODE","default":"A","height":3})).unwrap()["handle"].clone();
        d.execute("block", json!({"name":"Marker","base":[200,200],"handles":[att],"keep":"delete"})).unwrap();
        let insert = d.execute("insert", json!({"name":"Marker","at":[300,200],"layer":"Editable"})).unwrap()["handle"].clone();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), d.save(Some(FILE)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().into()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]);
        Self { kits, workspace, line, text, dim, insert, circle }
    }
    fn path(&self) -> PathBuf {
        self.workspace.path().join(FILE)
    }
    fn disk(&self) -> Vec<u8> {
        std::fs::read(self.path()).unwrap()
    }
    async fn call(&self, action: &str, mut p: Value) -> Result<Value, Value> {
        p["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("cad_{action}"), &p, CallOptions::default(), "agent-me").await.unwrap() {
            Ok(result) => Ok(result.output),
            Err(error) => Err(error.structured()),
        }
    }
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
    fn colleague(&self, steps: &[(&str, Value)]) {
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
fn content(d: &kition_cad_doc::Drawing) -> Value {
    let mut entities: Vec<_> = d.model.iter().map(|e| json!([e.common, e.kind]).to_string()).collect();
    entities.sort();
    let header: std::collections::BTreeMap<_, _> =
        d.header.vars.iter().filter(|(k, _)| !matches!(k.as_str(), "EXTMIN" | "EXTMAX" | "HANDSEED")).collect();
    json!({"object.query":entities,"layers":d.layers,"linetypes":d.linetypes,"styles":d.text_styles,"dimstyles":d.dim_styles,"groups":d.groups,"header":header})
}

async fn check(
    write: impl Fn(&Fixture) -> (&'static str, Value),
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    for refuse in [false, true] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let colleague = if refuse { necessary(&f) } else { unrelated(&f) };
        f.colleague(&colleague);
        let before = f.disk();
        let (command, params) = write(&f);
        let expected = if refuse {
            None
        } else {
            let mut oracle = CadDocument::open(FILE, &before).unwrap();
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
#[tokio::test(flavor = "multi_thread")]
async fn assigning_text_does_not_hold_its_placement() {
    check(
        |f| ("text.edit", json!({"handle":f.text,"text":"After"})),
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
        |f| vec![("move", json!({"handles":[f.text],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn assigning_plot_style_does_not_hold_geometry() {
    check(
        |f| ("plot.style", json!({"handles":[f.line],"name":"ByLayer"})),
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn grouping_does_not_hold_member_geometry() {
    check(
        |f| ("group", json!({"name":"G","handles":[f.line]})),
        |f| vec![("erase", json!({"handles":[f.line]}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn a_dimension_text_angle_does_not_hold_its_color() {
    check(
        |f| ("dimension.textAngle", json!({"handles":[f.dim],"angle":30})),
        |f| vec![("erase", json!({"handles":[f.dim]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.dim],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn a_dimension_text_override_does_not_hold_geometry() {
    check(
        |f| ("dimension.override", json!({"handles":[f.dim],"text":"After"})),
        |f| vec![("erase", json!({"handles":[f.dim]}))],
        |f| vec![("move", json!({"handles":[f.dim],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn reversing_a_line_does_not_hold_its_color() {
    check(
        |f| ("reverse", json!({"handles":[f.line]})),
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn lengthening_a_line_does_not_hold_its_color() {
    check(
        |f| ("object.lengthen", json!({"handle":f.line,"delta":5,"pick":[100,0]})),
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn flattening_a_line_does_not_hold_its_xy_placement() {
    check(
        |f| ("flatten", json!({"handles":[f.line]})),
        |_| vec![("layer.set", json!({"name":"Editable","locked":true}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn attribute_values_do_not_hold_insert_placement() {
    check(
        |f| ("attribute.edit", json!({"handle":f.insert,"values":{"CODE":"B"}})),
        |f| vec![("erase", json!({"handles":[f.insert]}))],
        |f| vec![("move", json!({"handles":[f.insert],"delta":[1,0]}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dimension_override_does_not_hold_other_override_keys() {
    check(
        |f| ("dimension.override", json!({"handles":[f.dim],"DIMASZ":4})),
        |f| vec![("erase", json!({"handles":[f.dim]}))],
        |f| vec![("dimension.override", json!({"handles":[f.dim],"DIMTXT":8}))],
    )
    .await;
}

/// same-field decision: the replaced override is an observed value.
#[tokio::test(flavor = "multi_thread")]
async fn a_dimension_override_holds_the_assigned_keys_old_value() {
    check(
        |f| ("dimension.override", json!({"handles":[f.dim],"DIMASZ":4})),
        |f| vec![("dimension.override", json!({"handles":[f.dim],"DIMASZ":8}))],
        |f| vec![("dimension.override", json!({"handles":[f.dim],"DIMTXT":8}))],
    )
    .await;
}

/// same-field decision: same-tag text is held; insertion placement is independent.
#[tokio::test(flavor = "multi_thread")]
async fn absolute_attribute_assignment_holds_written_text() {
    check(
        |f| ("attribute.edit", json!({"handle":f.insert,"values":{"CODE":"B"}})),
        |f| vec![("attribute.edit", json!({"handle":f.insert,"values":{"CODE":"Colleague"}}))],
        |f| vec![("move", json!({"handles":[f.insert],"delta":[1,0]}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_center_mark_does_not_copy_source_common_properties() {
    check(
        |f| ("centerMark.add", json!({"handles":[f.circle]})),
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"radius":7}))],
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn selecting_a_layer_through_an_object_does_not_hold_geometry() {
    check(
        |f| ("layer.off", json!({"handles":[f.line]})),
        |f| vec![("object.setProperties", json!({"handles":[f.line],"layer":"0"}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn copying_properties_does_not_hold_source_geometry() {
    check(
        |f| ("object.matchProperties", json!({"source":f.line,"targets":[f.text]})),
        |f| vec![("object.setProperties", json!({"handles":[f.line],"color":"blue"}))],
        |f| vec![("move", json!({"handles":[f.line],"delta":[1,0]}))],
    )
    .await;
}

/// An ignored tag must not silently start naming a newly added attribute; an unrelated tag
/// does not change the meaning of the write. Stored attribute content regions retain
/// tag membership and assigned values without holding unrelated tags or appearance.
#[tokio::test(flavor = "multi_thread")]
async fn an_absent_attribute_tag_is_held_without_holding_other_tags() {
    for refuse in [true, false] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        f.add_attribute(if refuse { "MISSING" } else { "OTHER" });
        let before = f.disk();
        let answer =
            f.call("run", json!({"basis":basis,"steps":[{"command":"attribute.edit","params":{"handle":f.insert,"values":{"MISSING":"B"}}}]})).await;
        if refuse {
            let error = answer.expect_err("a formerly ignored tag now names a new attribute");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(before, f.disk());
        } else {
            answer.unwrap_or_else(|e| panic!("a different new tag must apply: {e}"));
            let before = kition_cad_io::read(&before, FILE).unwrap();
            let after = kition_cad_io::read(&f.disk(), FILE).unwrap();
            assert_eq!(content(&before), content(&after));
        }
        f.kits.shutdown().await;
    }
}
