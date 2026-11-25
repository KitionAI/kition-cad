//! C8-cad-read: one outside-writer test per class of write the independent reading found held
//! less than its handler reads (`bindings/wasm/tests/declarations_independent.rs`). Each writes the
//! file with a separate document between the agent's observation and its write: a change the write
//! rests on must refuse and leave the file's bytes as they were; an unrelated change must apply.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

const FILE: &str = "Plan.dxf";

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
    line: Value,
    other: Value,
    circle: Value,
    cross: Value,
    dim: Value,
}

impl Fixture {
    fn new() -> Self {
        let mut d = CadDocument::create(FILE, 420, 297).unwrap();
        let line = d.execute("line.add", json!({"points":[[0,0],[100,0]]})).unwrap()["handles"][0].clone();
        let other = d.execute("line.add", json!({"points":[[0,50],[100,50]]})).unwrap()["handles"][0].clone();
        let circle = d.execute("circle.add", json!({"center":[200,0],"radius":5})).unwrap()["handle"].clone();
        let cross = d.execute("line.add", json!({"points":[[50,-10],[50,10]]})).unwrap()["handles"][0].clone();
        let dim = d.execute("dimension.addLinear", json!({"object":line,"at":[50,-20]})).unwrap()["handle"].clone();
        d.execute("layer.add", json!({"name":"Walls"})).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(FILE), d.save(Some(FILE)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().into()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]);
        Self { kits, workspace, line, other, circle, cross, dim }
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

    async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }

    /// Another writer: opens the file, runs `steps`, writes it back (an older mtime, so content
    /// alone tells the change).
    fn colleague(&self, steps: &[(&str, Value)]) {
        let path = self.path();
        let bytes = std::fs::read(&path).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut d = CadDocument::open(FILE, &bytes).unwrap();
        for (command, p) in steps {
            d.execute(command, p.clone()).unwrap_or_else(|e| panic!("colleague {command}: {e}"));
        }
        let after = d.save(Some(FILE)).unwrap();
        assert_ne!(bytes, after, "the independent writer must really change the file");
        std::fs::write(&path, after).unwrap();
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
    }

    async fn write(&self, basis: Value, command: &str, params: Value) -> Result<Value, Value> {
        self.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await
    }

    async fn stop(self) {
        self.kits.shutdown().await;
    }
}

/// `write` after the colleague's `necessary` change refuses (bytes kept), after `unrelated` applies.
async fn check(
    write: impl Fn(&Fixture) -> (&'static str, Value),
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    check_selected(false, write, necessary, unrelated).await;
}

async fn check_selected(
    selected: bool,
    write: impl Fn(&Fixture) -> (&'static str, Value),
    necessary: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
    unrelated: impl Fn(&Fixture) -> Vec<(&'static str, Value)>,
) {
    for refuse in [true, false] {
        let f = Fixture::new();
        if selected {
            f.call("run", json!({"steps":[{"command":"select.set","params":{"handles":[f.circle]}}]})).await.unwrap();
        }
        let basis = f.basis().await;
        let steps = if refuse { necessary(&f) } else { unrelated(&f) };
        f.colleague(&steps);
        let before = f.disk();
        let (command, params) = write(&f);
        let answer = if selected {
            f.call(
                "run",
                json!({"basis":basis,"steps":[
                    {"command":"select.set","params":{"handles":[f.circle]}},
                    {"command":command,"params":params.clone()}
                ]}),
            )
            .await
        } else {
            f.write(basis, command, params.clone()).await
        };
        if refuse {
            assert!(answer.is_err(), "{command} {params} after {steps:?} must refuse");
            let error = answer.unwrap_err();
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(f.disk(), before, "a refused write changes nothing");
        } else {
            answer.unwrap_or_else(|e| panic!("{command} {params} after unrelated {steps:?} must apply: {e}"));
        }
        f.stop().await;
    }
}

/// The handles of the entities of `store`.
fn handles_of(store: &kition_cad_doc::EntityStore) -> Vec<kition_cad_doc::Handle> {
    store.iter().map(|e| e.handle).collect()
}

/// The viewports MVIEW makes take the current colour, which is each person's own: after a
/// colleague made red their current colour (or made an unrelated change), MVIEW applies and its
/// viewports take my current colour (ByLayer), which the saved file carries.
#[tokio::test(flavor = "multi_thread")]
async fn new_viewports_take_my_current_colour_not_a_colleagues() {
    for colleague_colour in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        // A move would change the model extents the viewport is fitted to.
        f.colleague(&[if colleague_colour { ("property.color", json!({"color":"red"})) } else { ("layer.add", json!({"name":"Other"})) }]);
        let layout = |d: &kition_cad_doc::Drawing| d.layouts.iter().find(|l| l.name == "Layout1").map(|l| handles_of(&l.entities)).unwrap();
        let old = layout(&kition_cad_io::read(&f.disk(), FILE).unwrap());
        f.write(basis, "viewport.add", json!({"layout":"Layout1"})).await.unwrap();
        let saved = kition_cad_io::read(&f.disk(), FILE).unwrap();
        let space = &saved.layouts.iter().find(|l| l.name == "Layout1").unwrap().entities;
        let made: Vec<_> = space.iter().filter(|e| !old.contains(&e.handle)).collect();
        assert!(made.iter().any(|e| matches!(e.kind, kition_cad_doc::EntityKind::Viewport(_))), "MVIEW made a viewport");
        assert!(made.iter().all(|e| e.common.color.to_aci() == 256), "the new viewports take my current colour, ByLayer");
        assert_eq!(saved.header.i64("CECOLOR", 256), 256, "the file carries the saving session's current colour");
        f.stop().await;
    }
}

/// OFFSET onto the current layer copies CLAYER, which is each person's own: after a colleague made
/// Walls their current layer (or made an unrelated change), the offset applies and lands on my
/// current layer (0), which the saved file carries.
#[tokio::test(flavor = "multi_thread")]
async fn an_offset_onto_the_current_layer_takes_mine_not_a_colleagues() {
    for colleague_current in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[if colleague_current {
            ("layer.setCurrent", json!({"name":"Walls"}))
        } else {
            ("object.move", json!({"handles":[f.other],"delta":[0,5]}))
        }]);
        let old = handles_of(&kition_cad_io::read(&f.disk(), FILE).unwrap().model);
        f.write(basis, "object.offset", json!({"handle":f.line,"distance":5,"side":[50,10],"layer":"current"})).await.unwrap();
        let saved = kition_cad_io::read(&f.disk(), FILE).unwrap();
        let made: Vec<_> = saved.model.iter().filter(|e| !old.contains(&e.handle)).collect();
        assert_eq!(made.len(), 1, "the offset made one entity");
        assert!(made.iter().all(|e| e.common.layer == "0"), "the offset lands on my current layer");
        assert_eq!(saved.header.str("CLAYER", "?"), "0", "the file carries the saving session's current layer");
        f.stop().await;
    }
}

/// A name the write creates: BLOCK redefines a block another writer made under that name.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_block_rests_on_its_name_being_free() {
    check(
        |f| ("block.addDialog", json!({"name":"B1","base":[0,0],"handles":[f.line],"keep":"retain"})),
        |f| vec![("block.addDialog", json!({"name":"B1","base":[0,50],"handles":[f.other],"keep":"retain"}))],
        |f| vec![("block.addDialog", json!({"name":"B2","base":[0,50],"handles":[f.other],"keep":"retain"}))],
    )
    .await;
}

/// Held more before: CENTERMARK draws on CENTER, never on the current linetype it held. Its
/// extension default (CENTEREXE) is each person's own like the current linetype
/// (kdm_model::CURRENT_VARS): a colleague's is theirs; the mark rests on its circle.
#[tokio::test(flavor = "multi_thread")]
async fn a_centre_mark_rests_on_its_circle_not_on_a_colleagues_extension_or_linetype() {
    check(
        |f| ("centerMark.add", json!({"handles":[f.circle]})),
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"radius":7}))],
        |_| vec![("variable.set", json!({"name":"CENTEREXE","value":7})), ("property.linetype", json!({"load":"DASHED","current":"DASHED"}))],
    )
    .await;
}

/// A write that replaces its target whole: TRIM discards the trimmed line, so a colleague's grouping
/// of it must refuse (the target was held for its geometry, common properties and lock alone).
#[tokio::test(flavor = "multi_thread")]
async fn a_trim_rests_on_all_of_the_line_it_replaces() {
    check(
        |f| ("object.trim", json!({"handle":f.line,"pick":[90,0],"edges":[f.cross],"mode":"quick"})),
        |f| vec![("arrange.group", json!({"name":"G","handles":[f.line]}))],
        |f| vec![("object.setProperties", json!({"handles":[f.other],"color":"blue"}))],
    )
    .await;
}

/// Auxiliary path/base parameters do not replace the handler's implicit target selection.
#[tokio::test(flavor = "multi_thread")]
async fn an_array_path_also_holds_its_implicit_source_selection() {
    check_selected(
        true,
        |f| ("object.arrayPath", json!({"path":f.line,"count":3})),
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"radius":7}))],
        |f| vec![("object.move", json!({"handles":[f.other],"delta":[0,5]}))],
    )
    .await;
}

/// PAGESETUP starts with the saved page, including fields its parameters leave alone.
#[tokio::test(flavor = "multi_thread")]
async fn page_setup_holds_the_page_it_copies() {
    check(
        |_| ("plot.pageSetup", json!({"layout":"Layout1","paper":"A3"})),
        |_| vec![("plot.pageSetup", json!({"layout":"Layout1","margins":[9,8,7,6]}))],
        |f| vec![("object.move", json!({"handles":[f.other],"delta":[0,5]}))],
    )
    .await;
}

/// The hosted engine disables every file-naming command, including STYLESMANAGER without a path.
#[tokio::test(flavor = "multi_thread")]
async fn hosted_plot_style_commands_are_disabled_without_writing() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("plot.styleManager", json!({"action":"set","name":"monochrome.ctb","style":{"name":"Color_2","screening":20}}))]);
    let before = f.disk();
    let e = f
        .write(basis, "plot.styleManager", json!({"action":"set","name":"monochrome.ctb","style":{"name":"Color_1","screening":50}}))
        .await
        .unwrap_err();
    // A correct premise may refuse this before the handler's disabled-command check.
    assert!(matches!(e["error"]["code"].as_str(), Some("conflict" | "command-failed")), "{e}");
    assert_eq!(f.disk(), before);
    f.stop().await;
}

/// The post-command dimension updater reads more than the saved source attachments.
#[tokio::test(flavor = "multi_thread")]
async fn a_geometry_edit_holds_the_dimensions_it_remeasures() {
    check(
        |f| ("object.move", json!({"handles":[f.line],"delta":[1,0]})),
        |f| vec![("text.edit", json!({"handle":f.dim,"text":"Custom"}))],
        |f| vec![("object.move", json!({"handles":[f.other],"delta":[0,5]}))],
    )
    .await;
}

/// Adding unrelated geometry never runs the constraint solver on existing components; a new line
/// takes my current colour, never a colleague's (the current colour is each person's own).
#[tokio::test(flavor = "multi_thread")]
async fn a_new_line_does_not_hold_unrelated_constraint_geometry() {
    for colleague_colour in [true, false] {
        let f = Fixture::new();
        f.colleague(&[("constraint.perpendicular", json!({"h1":f.line,"h2":f.cross}))]);
        let basis = f.basis().await;
        f.colleague(&if colleague_colour {
            vec![("property.color", json!({"color":"red"}))]
        } else {
            vec![("object.move", json!({"handles":[f.cross],"delta":[10,0]}))]
        });
        let answer = f.write(basis, "line.add", json!({"points":[[0,150],[10,150]]})).await.unwrap();
        let h = kition_cad_doc::Handle::parse_hex(answer["completed"][0]["result"]["handles"][0].as_str().unwrap()).unwrap();
        let saved = kition_cad_io::read(&f.disk(), FILE).unwrap();
        assert_eq!(saved.entity(h).unwrap().common.color.to_aci(), 256, "the new line takes my current colour, ByLayer");
        f.stop().await;
    }
}

/// The current colour (CECOLOR) is each person's own: setting mine after a colleague set theirs
/// (or made an unrelated change) applies, and the saved file carries mine.
#[tokio::test(flavor = "multi_thread")]
async fn setting_my_current_colour_applies_over_a_colleagues() {
    for colleague_colour in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[if colleague_colour {
            ("property.color", json!({"color":"blue"}))
        } else {
            ("object.move", json!({"handles":[f.circle],"delta":[1,0]}))
        }]);
        f.write(basis, "property.color", json!({"color":"red"})).await.unwrap();
        let saved = kition_cad_io::read(&f.disk(), FILE).unwrap();
        assert_eq!(saved.header.i64("CECOLOR", 256), 1);
        f.stop().await;
    }
}

/// review: these are not irrelevant reads. The colleague makes a
/// formerly valid deletion illegal, so its accepted result is no longer available. A colleague's
/// current text style, dimension style or layer is theirs, not the drawing's (each person's own):
/// it does not make my deletion or freeze illegal
/// (`eligibility_ignores_a_colleagues_current_style_or_layer`).
#[tokio::test(flavor = "multi_thread")]
async fn kit_defect_deletion_holds_current_style_and_remaining_layout_checks() {
    let mut wrong = Vec::new();
    for kind in ["usedtextstyle", "layout"] {
        let f = Fixture::new();
        let (command, params) = match kind {
            "usedtextstyle" => {
                f.colleague(&[("textStyle.open", json!({"name":"S","current":false}))]);
                ("textStyle.delete", json!({"name":"S"}))
            }
            _ => ("layout.delete", json!({"name":"Layout2"})),
        };
        let basis = f.basis().await;
        let colleague = match kind {
            "usedtextstyle" => vec![
                ("textStyle.setCurrent", json!({"name":"S"})),
                ("text.add", json!({"at":[300,300],"text":"uses S","height":3})),
                ("textStyle.setCurrent", json!({"name":"Standard"})),
            ],
            _ => vec![("layout.delete", json!({"name":"Layout1"}))],
        };
        f.colleague(&colleague);
        let before = f.disk();
        let answer = f.write(basis, command, params).await;
        assert_eq!(f.disk(), before, "{kind}: a refusal must preserve bytes");
        let error = answer.unwrap_err();
        println!("eligibility {kind}: {}", error["error"]["code"]);
        if error["error"]["code"] != "conflict" {
            wrong.push(kind);
        }
        f.stop().await;
    }
    assert!(wrong.is_empty(), "changed eligibility must be a premise conflict: {wrong:?}");
}

/// A colleague's current text style, dimension style or layer is theirs (each person's own):
/// deleting the style they made current, or freezing the layer they made current, applies, and
/// the saved file carries my current text style, dimension style and layer.
#[tokio::test(flavor = "multi_thread")]
async fn eligibility_ignores_a_colleagues_current_style_or_layer() {
    for kind in ["textstyle", "dimstyle", "freeze"] {
        let f = Fixture::new();
        let my_dimstyle = kition_cad_io::read(&f.disk(), FILE).unwrap().header.str("DIMSTYLE", "Standard");
        if kind == "textstyle" {
            f.colleague(&[("textStyle.open", json!({"name":"S","current":false}))]);
        } else if kind == "dimstyle" {
            f.colleague(&[("dimensionStyle.open", json!({"name":"D2","current":false}))]);
        }
        let basis = f.basis().await;
        if kind == "textstyle" {
            f.colleague(&[("textStyle.setCurrent", json!({"name":"S"}))]);
            f.write(basis, "textStyle.delete", json!({"name":"S"})).await.unwrap();
        } else if kind == "dimstyle" {
            f.colleague(&[("dimensionStyle.setCurrent", json!({"name":"D2"}))]);
            f.write(basis, "dimensionStyle.delete", json!({"name":"D2"})).await.unwrap();
        } else {
            f.colleague(&[("layer.setCurrent", json!({"name":"Walls"}))]);
            f.write(basis, "layer.set", json!({"name":"Walls","frozen":true})).await.unwrap();
        }
        let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
        if kind == "textstyle" {
            assert!(d.text_style("S").is_none(), "S is deleted");
        } else if kind == "dimstyle" {
            assert!(d.dim_style("D2").is_none(), "D2 is deleted");
        } else {
            assert!(d.layer("Walls").unwrap().frozen, "Walls is frozen");
        }
        assert_eq!(d.header.str("TEXTSTYLE", "Standard"), "Standard");
        assert_eq!(d.header.str("DIMSTYLE", "Standard"), my_dimstyle, "the file carries my current dimension style");
        assert_eq!(d.header.str("CLAYER", "0"), "0");
        f.stop().await;
    }
}

/// Eligibility is a boolean dependency: unrelated additions must merge with deletion.
#[tokio::test(flavor = "multi_thread")]
async fn deletion_eligibility_ignores_unrelated_records_and_geometry() {
    let f = Fixture::new();
    f.colleague(&[("textStyle.open", json!({"name":"S","current":false}))]);
    let basis = f.basis().await;
    f.colleague(&[("layer.add", json!({"name":"Colleague","color":3})), ("object.move", json!({"handles":[f.circle],"delta":[10,0]}))]);
    f.write(basis, "textStyle.delete", json!({"name":"S"})).await.unwrap();
    let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
    assert!(d.text_style("S").is_none());
    assert!(d.layer("Colleague").is_some());
    let h = kition_cad_doc::Handle::parse_hex(f.circle.as_str().unwrap()).unwrap();
    let kition_cad_doc::EntityKind::Circle(c) = &d.entity(h).unwrap().kind else { panic!("circle") };
    assert_eq!(c.center.x, 210.0);
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deletion_eligibility_does_not_hold_another_styles_eligibility() {
    let f = Fixture::new();
    f.colleague(&[("textStyle.open", json!({"name":"S","current":false})), ("textStyle.open", json!({"name":"Other","current":false}))]);
    let basis = f.basis().await;
    f.colleague(&[("textStyle.setCurrent", json!({"name":"Other"}))]);
    f.write(basis, "textStyle.delete", json!({"name":"S"})).await.unwrap();
    let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
    assert!(d.text_style("S").is_none());
    assert!(d.text_style("Other").is_some());
    // The current text style is each person's own: the file carries the saving session's.
    assert_eq!(d.header.str("TEXTSTYLE", "Standard"), "Standard");
    f.stop().await;
}

/// An SDK writer saves the file too, but its incremental receipt must name a
/// record whose derived eligibility changed through a header dependency.
#[tokio::test(flavor = "multi_thread")]
async fn incremental_style_eligibility_is_a_conflict() {
    let f = Fixture::new();
    f.colleague(&[("textStyle.open", json!({"name":"S","current":false}))]);
    let basis = f.basis().await;
    let before = f.disk();
    tools::call_as(
        &f.kits,
        "cad_run",
        &json!({"path":FILE,"steps":[{"command":"textStyle.setCurrent","params":{"name":"S"}}]}),
        CallOptions::default(),
        "colleague-kit",
    )
    .await
    .unwrap()
    .unwrap();
    assert_ne!(f.disk(), before, "the colleague must save an independent change");
    let saved = f.disk();
    let error = f.write(basis, "textStyle.delete", json!({"name":"S"})).await.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert_eq!(f.disk(), saved);
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn incremental_layer_lock_is_a_conflict() {
    let f = Fixture::new();
    let basis = f.basis().await;
    let before = f.disk();
    tools::call_as(
        &f.kits,
        "cad_run",
        &json!({"path":FILE,"steps":[{"command":"layer.set","params":{"name":"0","locked":true}}]}),
        CallOptions::default(),
        "colleague-kit",
    )
    .await
    .unwrap()
    .unwrap();
    assert_ne!(f.disk(), before, "the colleague must save an independent change");
    let saved = f.disk();
    let error = f.write(basis, "object.setProperties", json!({"handles":[f.line],"color":"red"})).await.unwrap_err();
    assert_eq!(f.disk(), saved);
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    f.stop().await;
}
