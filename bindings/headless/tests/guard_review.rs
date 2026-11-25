//! C8-cad review: a second document rewrites the actual DXF between observation and write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

const FILE: &str = "Review.dxf";

#[tokio::test(flavor = "multi_thread")]
async fn named_style_writes_hold_the_record_the_engine_resolves() {
    for family in ["multileaderStyle.open", "textStyle.open"] {
        for relevant in [false, true] {
            let mut d = drawing();
            d.execute(family, json!({"name":"A"})).unwrap();
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[if relevant {
                (family, if family == "textStyle.open" { json!({"name":"A","height":8}) } else { json!({"name":"A","textHeight":8}) })
            } else {
                ("layer.add", json!({"name":"Unrelated"}))
            }]);
            f.write(
                basis,
                family,
                if family == "textStyle.open" { json!({"name":" A ","height":4}) } else { json!({"name":"A","textHeight":4}) },
                relevant,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn selected_layer_tools_hold_the_snapshot_they_capture() {
    for relevant in [false, true] {
        let mut d = drawing();
        d.execute("layer.add", json!({"name":"Walls"})).unwrap();
        let h = d.execute("line.add", json!({"points":[[0,0],[10,0]],"layer":"Walls"})).unwrap()["handles"][0].clone();
        let other = d.execute("line.add", json!({"points":[[0,20],[10,20]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if relevant {
            ("layer.set", json!({"name":"Walls","color":"red"}))
        } else {
            ("object.move", json!({"handles":[other],"delta":[1,0]}))
        }]);
        f.write(basis, "layer.off", json!({"handles":[h]}), relevant).await;
        f.kits.shutdown().await;
    }
}

/// LAYCUR moves the objects to my current layer. The current layer is each person's own: a
/// colleague who made another layer theirs changes nothing of mine, so the write applies, the
/// line lands on my current layer, and the saved file carries my current layer.
#[tokio::test(flavor = "multi_thread")]
async fn laycur_assigns_my_current_layer_not_a_colleagues() {
    for colleague_current in [false, true] {
        let mut d = drawing();
        d.execute("layer.add", json!({"name":"Walls"})).unwrap();
        d.execute("layer.add", json!({"name":"Doors"})).unwrap();
        let h = d.execute("line.add", json!({"points":[[0,0],[10,0]],"layer":"Walls"})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if colleague_current { ("layer.setCurrent", json!({"name":"Doors"})) } else { ("layer.add", json!({"name":"Unrelated"})) }]);
        f.write(basis, "layer.changeToCurrent", json!({"handles":[h]}), false).await;
        let saved = kition_cad_io::read(&std::fs::read(f.dir.path().join(FILE)).unwrap(), FILE).unwrap();
        let line = saved.entity(kition_cad_doc::Handle::parse_hex(h.as_str().unwrap()).unwrap()).unwrap();
        assert_eq!(line.common.layer, "0", "the line is on my current layer");
        assert_eq!(saved.header.str("CLAYER", "?"), "0", "the file carries the saving session's current layer");
        f.kits.shutdown().await;
    }
}

/// DIMSCALE is the drawing's (a dimension whose style's scale is zero is drawn at it), shared:
/// replacing it holds the value I observed.
#[tokio::test(flavor = "multi_thread")]
async fn stored_variable_writes_hold_the_value_they_replace() {
    for relevant in [false, true] {
        let mut d = drawing();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if relevant { ("variable.set", json!({"name":"DIMSCALE","value":3})) } else { ("layer.add", json!({"name":"Unrelated"})) }]);
        f.write(basis, "variable.set", json!({"name":"DIMSCALE","value":2}), relevant).await;
        f.kits.shutdown().await;
    }
}

/// The other DIM* variables are each person's own (kdm_model::is_current_var): a colleague's DIMDEC
/// is theirs, my SETVAR DIMDEC applies over it, and the saved file carries mine.
#[tokio::test(flavor = "multi_thread")]
async fn my_dimension_variable_applies_over_a_colleagues() {
    for colleague_set in [false, true] {
        let mut d = drawing();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if colleague_set { ("variable.set", json!({"name":"DIMDEC","value":3})) } else { ("layer.add", json!({"name":"Unrelated"})) }]);
        f.write(basis, "variable.set", json!({"name":"DIMDEC","value":1}), false).await;
        assert_eq!(f.saved().header.i64("DIMDEC", -1), 1, "the file carries my DIMDEC");
        f.kits.shutdown().await;
    }
}

/// The spline segment count PEDIT Spline uses is each person's own (kdm_model::CURRENT_VARS): a
/// colleague's SPLINESEGS is theirs, so my PEDIT applies with mine and the file carries mine (the
/// drawing had none: 8).
#[tokio::test(flavor = "multi_thread")]
async fn pedit_uses_my_spline_segment_default_not_a_colleagues() {
    for colleague_set in [false, true] {
        let mut d = drawing();
        let h = d.execute("polyline.add", json!({"points":[[0,0],[10,10],[20,0],[30,10]]})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if colleague_set {
            ("variable.set", json!({"name":"SPLINESEGS","value":12}))
        } else {
            ("layer.add", json!({"name":"Unrelated"}))
        }]);
        f.write(basis, "polyline.edit", json!({"handles":[h],"option":"spline"}), false).await;
        assert_eq!(f.saved().header.i64("SPLINESEGS", 8), 8, "the file carries my SPLINESEGS");
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn layout_dispatch_copy_holds_the_source() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("text.add", json!({"at":[0,0],"text":"Door","layout":"Layout1"})).unwrap()["handle"].clone();
        let b = d.execute("text.add", json!({"at":[0,20],"text":"Window"})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("text.edit", json!({"handle":if relevant {a}else{b},"text":"Colleague"}))]);
        f.write(basis, "layout.manage", json!({"option":"copy","name":"Layout1","to":"Copy"}), relevant).await;
        f.kits.shutdown().await;
    }
}

struct Fixture {
    kits: KitManager,
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new(d: &mut CadDocument) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), d.save(Some(FILE)).unwrap()).unwrap();
        let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        Self { kits: KitManager::new(Some(dir.path().into()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-cad"))))]), dir }
    }
    async fn call(&self, action: &str, mut p: Value) -> Result<Value, Value> {
        p["path"] = json!(FILE);
        tools::call_as(&self.kits, &format!("cad_{action}"), &p, CallOptions::default(), "reviewer")
            .await
            .unwrap()
            .map(|r| r.output)
            .map_err(|e| e.structured())
    }
    async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }
    fn colleague(&self, steps: &[(&str, Value)]) {
        let path = self.dir.path().join(FILE);
        let before = std::fs::read(&path).unwrap();
        let mut d = CadDocument::open(FILE, &before).unwrap();
        for (id, p) in steps {
            d.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        let after = d.save(Some(FILE)).unwrap();
        assert_ne!(before, after, "independent writer must change the file");
        std::fs::write(path, after).unwrap();
    }
    /// The file as saved now.
    fn saved(&self) -> kition_cad_doc::Drawing {
        kition_cad_io::read(&std::fs::read(self.dir.path().join(FILE)).unwrap(), FILE).unwrap()
    }
    fn colleague_drawing(&self, edit: impl FnOnce(&mut kition_cad_doc::Drawing)) {
        let path = self.dir.path().join(FILE);
        let before = std::fs::read(&path).unwrap();
        let mut d = kition_cad_io::read(&before, FILE).unwrap();
        edit(&mut d);
        let after = kition_cad_io::write(&d, FILE).unwrap();
        assert_ne!(before, after, "independent writer must change the file");
        std::fs::write(path, after).unwrap();
    }
    async fn write(&self, basis: Value, id: &str, p: Value, conflict: bool) {
        self.steps(basis, json!([{"command":id,"params":p}]), conflict).await;
    }
    async fn steps(&self, basis: Value, steps: Value, conflict: bool) {
        let path = self.dir.path().join(FILE);
        let before = std::fs::read(&path).unwrap();
        let result = self.call("run", json!({"basis":basis,"steps":steps})).await;
        if conflict {
            let error = result.expect_err("a changed premise must refuse");
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(std::fs::read(path).unwrap(), before);
        } else {
            result.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn styles_and_blocks_keep_identity_through_identical_swaps() {
    for block in [false, true] {
        for swap in [false, true] {
            let mut d = drawing();
            for name in ["A", "B"] {
                if block {
                    let h = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
                    d.execute("block.addDialog", json!({"name":name,"base":[0,0],"handles":[h],"keep":"delete"})).unwrap();
                } else {
                    d.execute("textStyle.open", json!({"name":name})).unwrap();
                }
            }
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            let table = if block { "block" } else { "style" };
            if swap {
                f.colleague(&[
                    ("namedObject.rename", json!({"table":table,"from":"A","to":"Tmp"})),
                    ("namedObject.rename", json!({"table":table,"from":"B","to":"A"})),
                    ("namedObject.rename", json!({"table":table,"from":"Tmp","to":"B"})),
                ]);
            } else {
                f.colleague(&[("layer.add", json!({"name":"Unrelated"}))]);
            }
            f.write(
                basis,
                if block { "block.insert" } else { "textStyle.open" },
                if block { json!({"name":"A","at":[20,0]}) } else { json!({"name":"A","height":7}) },
                swap,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn moving_text_holds_geometry_but_not_its_words() {
    for geometry in [false, true] {
        let mut d = drawing();
        let h = d.execute("text.add", json!({"at":[0,0],"text":"before","height":2})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if geometry {
            ("object.move", json!({"handles":[h],"delta":[1,0]}))
        } else {
            ("text.edit", json!({"handle":h,"text":"colleague"}))
        }]);
        f.write(basis, "object.move", json!({"handles":[h],"delta":[0,5]}), geometry).await;
        if !geometry {
            let mut reread = CadDocument::open(FILE, &std::fs::read(f.dir.path().join(FILE)).unwrap()).unwrap();
            assert!(reread.execute("object.query", json!({})).unwrap().to_string().contains("colleague"));
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn mirror_default_copy_holds_the_words_it_copies() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("text.add", json!({"at":[10,0],"text":"Door"})).unwrap()["handle"].clone();
        let b = d.execute("text.add", json!({"at":[10,20],"text":"Window"})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("text.edit", json!({"handle":if relevant {&a}else{&b},"text":"Colleague"}))]);
        f.write(basis, "object.mirror", json!({"handles":[a],"p1":[0,0],"p2":[0,10]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_constraint_conflicts_only_in_the_edited_component() {
    for connected in [false, true] {
        let mut d = drawing();
        let mut hs = Vec::new();
        for y in [0, 20] {
            let h = d.execute("line.add", json!({"points":[[0,y],[10,y]]})).unwrap()["handles"][0].clone();
            d.execute("constraint.horizontal", json!({"h1":h})).unwrap();
            hs.push(h);
        }
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[hs[usize::from(!connected)]],"delta":[1,0]}))]);
        f.write(basis, "object.move", json!({"handles":[hs[0]],"delta":[0,5]}), connected).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_constraint_holds_only_its_connected_component() {
    for relevant in [false, true] {
        let mut d = drawing();
        let mut hs = Vec::new();
        for y in [0, 20] {
            hs.push(d.execute("line.add", json!({"points":[[0,y],[10,y]]})).unwrap()["handles"][0].clone());
        }
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("constraint.horizontal", json!({"h1":hs[usize::from(!relevant)]}))]);
        f.write(basis, "object.move", json!({"handles":[hs[0]],"delta":[0,5]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hosted_constraint_membership_is_refreshed_without_entity_motion() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[0,20],[10,20]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        let path = f.dir.path().join(FILE);
        let before = std::fs::read(&path).unwrap();
        tools::call_as(
            &f.kits,
            "cad_run",
            &json!({"path":FILE,"steps":[{"command":"constraint.horizontal","params":{"h1":if relevant {&a}else{&b}}}]}),
            CallOptions::default(),
            "colleague",
        )
        .await
        .unwrap()
        .unwrap();
        assert_ne!(std::fs::read(path).unwrap(), before);
        f.write(basis, "object.move", json!({"handles":[a],"delta":[0,5]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn point_or_handle_measurements_hold_handles_without_treating_coordinates_as_ids() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[0,20],[10,20]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[if relevant {&a}else{&b}],"delta":[1,0]}))]);
        f.steps(basis,json!([{"command":"object.measure","params":{"distance":[a,[0,5]]}},{"command":"object.setProperties","params":{"handles":[b],"color":"red"}}]),relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_hatch_boundaries_are_premises() {
    for relevant in [false, true] {
        let mut d = drawing();
        let boundary = d.execute("rectangle.add", json!({"p1":[0,0],"p2":[10,10]})).unwrap()["handle"].clone();
        let far = d.execute("line.add", json!({"points":[[50,50],[60,50]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[if relevant {boundary}else{far}],"delta":[1,0]}))]);
        f.write(basis, "hatch.add", json!({"points":[[5,5]],"pattern":"SOLID"}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn stretch_holds_its_crossing_window() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[50,50],[60,50]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[if relevant {a}else{b}],"delta":[0,10]}))]);
        f.write(basis, "object.stretch", json!({"window":[[-1,-1],[1,1]],"delta":[0,5]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_dimension_associations_are_premises() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[50,50],[60,50]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[if relevant {a}else{b}],"delta":[1,0]}))]);
        f.write(basis, "dimension.addLinear", json!({"p1":[0,0],"p2":[10,0],"at":[0,5]}), relevant).await;
        f.kits.shutdown().await;
    }
}
fn drawing() -> CadDocument {
    CadDocument::create(FILE, 420, 297).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn chspace_holds_the_destination_projection() {
    for relevant in [false, true] {
        let mut d = drawing();
        let h = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let mut vps = Vec::new();
        for name in ["Layout1", "Other"] {
            if name == "Other" {
                d.execute("layout.add", json!({"name":name,"viewport":false})).unwrap();
            }
            let r = d.execute("viewport.add", json!({"layout":name,"p1":[0,0],"p2":[100,100]})).unwrap();
            vps.push(r["viewports"][0].clone());
        }
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("viewport.set", json!({"handle":vps[usize::from(!relevant)],"viewHeight":500}))]);
        f.write(basis, "object.changeSpace", json!({"handles":[h],"to":"Layout1"}), relevant).await;
        f.kits.shutdown().await;
    }
}

async fn definition_copying(family: &str) {
    for relevant in [false, true] {
        let mut d = drawing();
        let dm = d.execute("dimension.addLinear", json!({"p1":[0,0],"p2":[10,0],"at":[0,5]})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        let change = if !relevant {
            ("layer.add", json!({"name":"Other"}))
        } else if family == "object.explode" {
            ("dimensionStyle.open", json!({"name":"ISO-25","extExtend":9,"current":false}))
        } else {
            ("dimensionStyle.open", json!({"name":"ISO-25","textHeight":9,"current":false}))
        };
        f.colleague(&[change]);
        let params = match family {
            "dimensionStyle.open" | "dimensionStyle.openManager" => json!({"name":"Copy","current":false}),
            _ => json!({"handles":[dm]}),
        };
        f.write(basis, family, params, relevant).await;
        f.kits.shutdown().await;
    }
}

/// The text height new text takes (TEXTSIZE) is each person's own (kdm_model::CURRENT_VARS): a
/// colleague's is theirs, so my ATTDEF without a height applies with mine (the drawing's 2.5), and
/// the file carries mine.
#[tokio::test(flavor = "multi_thread")]
async fn attribute_definition_takes_my_text_height_not_a_colleagues() {
    for colleague_set in [false, true] {
        let mut d = drawing();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if colleague_set { ("variable.set", json!({"name":"TEXTSIZE","value":9})) } else { ("layer.add", json!({"name":"Other"})) }]);
        f.write(basis, "attribute.define", json!({"tag":"ID","at":[20,20],"default":"Door"}), false).await;
        let saved = f.saved();
        let heights: Vec<f64> =
            saved.model.iter().filter_map(|e| if let kition_cad_doc::EntityKind::AttDef(a) = &e.kind { Some(a.text.height) } else { None }).collect();
        assert_eq!(heights, vec![2.5], "my attribute definition takes my text height");
        assert_eq!(saved.header.f64("TEXTSIZE", 0.0), 2.5, "the file carries my TEXTSIZE");
        f.kits.shutdown().await;
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_style_copying_holds_the_source() {
    definition_copying("dimensionStyle.open").await;
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_menu_style_copying_holds_the_source() {
    definition_copying("dimensionStyle.openManager").await;
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_explosion_holds_the_style() {
    definition_copying("object.explode").await;
}

/// The defaults these commands take (FILLETRAD, CHAMFERA, CMLSCALE, REVCLOUDARCLEN, CENTEREXE)
/// are each person's own (kdm_model::CURRENT_VARS): a colleague's is theirs, so my command applies
/// with mine, and the file never carries the colleague's.
async fn stored_default(family: &str) {
    for colleague_set in [false, true] {
        let mut d = drawing();
        let a = d.execute("line.add", json!({"points":[[0,0],[20,0]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[0,0],[0,20]]})).unwrap()["handles"][0].clone();
        let circle = d.execute("circle.add", json!({"center":[50,50],"radius":10})).unwrap()["handle"].clone();
        let (var, value, params) = match family {
            "object.fillet" => ("FILLETRAD", json!(3), json!({"h1":a,"h2":b,"p1":[10,0],"p2":[0,10]})),
            "object.chamfer" => ("CHAMFERA", json!(3), json!({"h1":a,"h2":b,"p1":[10,0],"p2":[0,10]})),
            "multiline.add" => ("CMLSCALE", json!(9), json!({"points":[[20,20],[40,20]]})),
            "revisionCloud.add" => ("REVCLOUDARCLEN", json!(3), json!({"p1":[20,20],"p2":[40,40]})),
            _ => ("CENTEREXE", json!(9), json!({"handles":[circle]})),
        };
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if colleague_set {
            ("variable.set", json!({"name":var,"value":value.clone()}))
        } else {
            ("layer.add", json!({"name":"Other"}))
        }]);
        f.write(basis, family, params, false).await;
        let saved = f.saved().header.get(var).and_then(kition_cad_doc::HVal::as_f64);
        assert_ne!(saved, value.as_f64(), "{family}: the file carries my {var}, not the colleague's");
        f.kits.shutdown().await;
    }
}

macro_rules! default_test {
    ($test:ident,$command:literal) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $test() {
            stored_default($command).await;
        }
    };
}
default_test!(fillet_takes_my_radius_not_a_colleagues, "object.fillet");
default_test!(chamfer_takes_my_distance_not_a_colleagues, "object.chamfer");
default_test!(mline_takes_my_scale_not_a_colleagues, "multiline.add");
default_test!(revcloud_takes_my_arc_length_not_a_colleagues, "revisionCloud.add");
default_test!(center_mark_takes_my_extension_not_a_colleagues, "centerMark.add");

#[tokio::test(flavor = "multi_thread")]
async fn viewport_creation_holds_model_extents() {
    for relevant in [false, true] {
        let mut d = drawing();
        let h = d.execute("line.add", json!({"points":[[0,0],[20,20]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if relevant { ("object.move", json!({"handles":[h],"delta":[10,0]})) } else { ("layer.add", json!({"name":"Other"})) }]);
        f.write(basis, "viewport.add", json!({"layout":"Layout1","p1":[0,0],"p2":[100,100]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn find_replacement_holds_the_words_it_scans() {
    for relevant in [false, true] {
        let mut d = drawing();
        let h = d.execute("text.add", json!({"at":[0,0],"text":"Door"})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if relevant {
            ("text.edit", json!({"handle":h,"text":"Door colleague"}))
        } else {
            ("object.move", json!({"handles":[h],"delta":[10,0]}))
        }]);
        f.write(basis, "edit.find", json!({"find":"Door","replace":"Window"}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn identical_named_layer_swap_refuses_but_unrelated_layer_applies() {
    for swap in [true, false] {
        let mut d = drawing();
        for name in ["A", "B"] {
            d.execute("layer.add", json!({"name":name})).unwrap();
        }
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if swap {
            f.colleague(&[
                ("layer.set", json!({"name":"A","newName":"Tmp"})),
                ("layer.set", json!({"name":"B","newName":"A"})),
                ("layer.set", json!({"name":"Tmp","newName":"B"})),
            ]);
        } else {
            f.colleague(&[("layer.set", json!({"name":"B","color":"blue"}))]);
        }
        f.write(basis, "layer.set", json!({"name":"A","color":"red"}), swap).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn current_dimension_style_is_a_premise() {
    for relevant in [true, false] {
        let mut d = drawing();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if relevant {
            ("dimensionStyle.open", json!({"name":"ISO-25","DIMTXT":7}))
        } else {
            ("layer.add", json!({"name":"Unrelated"}))
        }]);
        f.write(basis, "dimension.addLinear", json!({"p1":[0,0],"p2":[100,0],"at":[0,10]}), relevant).await;
        f.kits.shutdown().await;
    }
}

async fn dimension_override_swap(block: bool) {
    for relevant in [false, true] {
        let mut d = drawing();
        for name in ["A", "B"] {
            if block {
                let h = d.execute("line.add", json!({"points":[[0,0],[1,0]]})).unwrap()["handles"][0].clone();
                d.execute("block.addDialog", json!({"name":name,"base":[0,0],"handles":[h],"keep":"delete"})).unwrap();
            } else {
                d.execute("textStyle.open", json!({"name":name,"current":false})).unwrap();
            }
        }
        d.execute("variable.set", json!({"name":if block {"DIMBLK"}else{"DIMTXSTY"},"value":"A"})).unwrap();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            let table = if block { "block" } else { "style" };
            f.colleague(&[
                ("namedObject.rename", json!({"table":table,"from":"A","to":"Tmp"})),
                ("namedObject.rename", json!({"table":table,"from":"B","to":"A"})),
                ("namedObject.rename", json!({"table":table,"from":"Tmp","to":"B"})),
            ]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "dimension.addLinear", json!({"p1":[20,0],"p2":[30,0],"at":[20,5]}), relevant).await;
        f.kits.shutdown().await;
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_default_text_style_override_holds_identity() {
    dimension_override_swap(false).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn dimension_default_arrow_override_holds_identity() {
    dimension_override_swap(true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn table_append_is_unrelated_but_inserting_before_the_cell_refuses() {
    for row in [0, 3] {
        let mut d = drawing();
        let table = d.execute("table.add", json!({"at":[0,0],"rows":3,"cols":2,"cells":[["a","b"],["c","d"],["e","f"]]})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("table.insertRow", json!({"handle":table,"row":row}))]);
        f.write(basis, "table.set", json!({"handle":table,"row":1,"col":0,"text":"mine"}), row == 0).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn viewport_color_keys_hold_layer_identity() {
    for relevant in [false, true] {
        let mut d = drawing();
        for name in ["A", "B"] {
            d.execute("layer.add", json!({"name":name})).unwrap();
        }
        let vp = d.execute("viewport.add", json!({"layout":"Layout1","p1":[0,0],"p2":[100,100]})).unwrap()["viewports"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[
                ("layer.set", json!({"name":"A","newName":"Tmp"})),
                ("layer.set", json!({"name":"B","newName":"A"})),
                ("layer.set", json!({"name":"Tmp","newName":"B"})),
            ]);
        } else {
            f.colleague(&[("layer.set", json!({"name":"B","color":"blue"}))]);
        }
        f.write(basis, "viewport.set", json!({"handle":vp,"colors":{"A":"red"}}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn nonrotating_polar_array_holds_definition_extents() {
    for relevant in [false, true] {
        let mut d = drawing();
        let source = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        d.execute("block.addDialog", json!({"name":"Door","base":[0,0],"handles":[source],"keep":"delete"})).unwrap();
        let insert = d.execute("block.insert", json!({"name":"Door","at":[10,0]})).unwrap()["handle"].clone();
        let replacement = d.execute("line.add", json!({"points":[[0,0],[20,0]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[("block.addDialog", json!({"name":"Door","base":[5,0],"handles":[replacement],"keep":"delete"}))]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "object.arrayPolar", json!({"handles":[insert],"center":[0,0],"count":3,"rotate":false}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_trim_edges_are_premises() {
    for relevant in [false, true] {
        let mut d = drawing();
        let target = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let edge = d.execute("line.add", json!({"points":[[5,-5],[5,5]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[("object.move", json!({"handles":[edge],"delta":[2,0]}))]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "object.trim", json!({"handle":target,"pick":[2,0]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_constraint_write_holds_its_referenced_geometry() {
    for relevant in [false, true] {
        let mut d = drawing();
        let a = d.execute("line.add", json!({"points":[[0,0],[10,1]]})).unwrap()["handles"][0].clone();
        let b = d.execute("line.add", json!({"points":[[0,20],[10,21]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[if relevant {&a}else{&b}],"delta":[2,0]}))]);
        f.write(basis, "constraint.horizontal", json!({"h1":a}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn setting_a_layer_linetype_holds_the_resolved_record() {
    for relevant in [false, true] {
        let mut d = drawing();
        d.execute("property.linetype", json!({"load":"*"})).unwrap();
        d.execute("layer.add", json!({"name":"Walls"})).unwrap();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[
                ("namedObject.rename", json!({"table":"linetype","from":"DASHED","to":"Tmp"})),
                ("namedObject.rename", json!({"table":"linetype","from":"CENTER","to":"DASHED"})),
                ("namedObject.rename", json!({"table":"linetype","from":"Tmp","to":"CENTER"})),
            ]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "layer.set", json!({"name":"Walls","linetype":"DASHED"}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn layer_bulk_write_holds_the_layer_values_it_captures() {
    for relevant in [false, true] {
        let mut d = drawing();
        d.execute("layer.add", json!({"name":"Walls"})).unwrap();
        let line = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[("layer.set", json!({"name":"Walls","color":"red"}))]);
        } else {
            f.colleague(&[("object.move", json!({"handles":[line],"delta":[1,0]}))]);
        }
        f.write(basis, "layer.allOn", json!({}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_defaults_hold_the_dimension_style() {
    for relevant in [false, true] {
        let mut d = drawing();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[("dimensionStyle.open", json!({"name":"ISO-25","textHeight":9,"current":false}))]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "leader.add", json!({"points":[[0,0],[10,10]],"text":"Door"}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn multileader_defaults_hold_the_current_style() {
    for relevant in [false, true] {
        let mut d = drawing();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague(&[("multileaderStyle.open", json!({"name":"Standard","textHeight":9,"current":false}))]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "multileader.add", json!({"points":[[0,0],[10,10]],"text":"Door"}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn default_named_records_hold_identity_when_header_names_do_not_change() {
    for ty in ["layer", "textstyle", "linetype"] {
        for relevant in [false, true] {
            let mut d = drawing();
            for name in ["A", "B"] {
                d.execute(if ty == "layer.open" { "layer.add" } else { "textStyle.open" }, json!({"name":name})).unwrap();
            }
            if ty == "linetype" {
                d.execute("property.linetype", json!({"load":"*"})).unwrap();
            }
            let (a, b, var) = match ty {
                "layer" => ("A", "B", "CLAYER"),
                "textstyle" => ("A", "B", "TEXTSTYLE"),
                _ => ("DASHED", "CENTER", "CELTYPE"),
            };
            d.execute("variable.set", json!({"name":var,"value":a})).unwrap();
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            if relevant {
                f.colleague_drawing(|d| {
                    d.rename_record_id(ty, a, "Tmp");
                    d.rename_record_id(ty, b, a);
                    d.rename_record_id(ty, "Tmp", b);
                    let rename = |name: &mut String| {
                        if name == a {
                            *name = b.into();
                        } else if name == b {
                            *name = a.into();
                        }
                    };
                    match ty {
                        "layer" => {
                            for record in &mut d.layers {
                                rename(&mut record.name);
                            }
                        }
                        "textstyle" => {
                            for record in &mut d.text_styles {
                                rename(&mut record.name);
                            }
                        }
                        _ => {
                            for record in &mut d.linetypes {
                                rename(&mut record.name);
                            }
                        }
                    }
                    assert_eq!(d.header.str(var, ""), a);
                });
            } else {
                f.colleague(&[("layer.add", json!({"name":"Other"}))]);
            }
            f.write(
                basis,
                if ty == "textstyle" { "text.add" } else { "line.add" },
                if ty == "textstyle" { json!({"at":[0,0],"text":"Door"}) } else { json!({"points":[[0,0],[10,0]]}) },
                relevant,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn xref_insert_holds_the_definition_path() {
    for relevant in [false, true] {
        let mut d = drawing();
        let line = d.execute("line.add", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
        d.execute("block.addDialog", json!({"name":"Site","base":[0,0],"handles":[line],"keep":"delete"})).unwrap();
        let f = Fixture::new(&mut d);
        f.colleague_drawing(|d| {
            std::sync::Arc::make_mut(d.blocks.get_mut("Site").unwrap()).xref_path = Some("site.dxf".into());
        });
        let basis = f.basis().await;
        if relevant {
            f.colleague_drawing(|d| {
                std::sync::Arc::make_mut(d.blocks.get_mut("Site").unwrap()).xref_path = Some("other.dxf".into());
            });
        } else {
            f.colleague(&[("layer.add", json!({"name":"Other"}))]);
        }
        f.write(basis, "block.insert", json!({"name":"Site","at":[0,0]}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn layout_copy_holds_the_entities_it_copies() {
    for relevant in [false, true] {
        let mut d = drawing();
        let paper = d.execute("text.add", json!({"layout":"Layout1","at":[0,0],"text":"Door"})).unwrap()["handle"].clone();
        let model = d.execute("text.add", json!({"at":[0,0],"text":"Window"})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("text.edit", json!({"handle":if relevant {&paper}else{&model},"text":"Colleague"}))]);
        f.write(basis, "layout.duplicate", json!({"from":"Layout1","to":"Copy"}), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn attribute_sync_holds_every_reference_it_updates() {
    for relevant in [false, true] {
        let mut d = drawing();
        let ad = d.execute("attribute.define", json!({"tag":"ID","at":[0,0],"default":"Door"})).unwrap()["handle"].clone();
        d.execute("block.addDialog", json!({"name":"Door","base":[0,0],"handles":[ad],"keep":"delete"})).unwrap();
        let ins = d.execute("block.insert", json!({"name":"Door","at":[10,0]})).unwrap()["handle"].clone();
        let far = d.execute("line.add", json!({"points":[[0,20],[10,20]]})).unwrap()["handles"][0].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("object.move", json!({"handles":[if relevant {&ins}else{&far}],"delta":[2,0]}))]);
        f.write(basis, "attribute.sync", json!({"name":"Door"}), relevant).await;
        f.kits.shutdown().await;
    }
}

/// DIMSTYLE Update gives the dimensions my current dimension style. The current dimension style is
/// each person's own: a colleague who made Other theirs changes nothing of mine, so the write
/// applies, the dimension takes my current style (ISO-25), and the saved file carries mine.
#[tokio::test(flavor = "multi_thread")]
async fn dimension_style_update_assigns_my_current_style_not_a_colleagues() {
    for colleague_current in [false, true] {
        let mut d = drawing();
        d.execute("dimensionStyle.open", json!({"name":"Other"})).unwrap();
        let dim = d.execute("dimension.addLinear", json!({"p1":[0,0],"p2":[10,0],"at":[0,5]})).unwrap()["handle"].clone();
        d.execute("dimensionStyle.setCurrent", json!({"name":"ISO-25"})).unwrap();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if colleague_current {
            f.colleague(&[("dimensionStyle.setCurrent", json!({"name":"Other"}))]);
        } else {
            f.colleague(&[("layer.add", json!({"name":"Far"}))]);
        }
        f.write(basis, "dimension.updateStyle", json!({"handles":[dim]}), false).await;
        let saved = kition_cad_io::read(&std::fs::read(f.dir.path().join(FILE)).unwrap(), FILE).unwrap();
        let style = match &saved.entity(kition_cad_doc::Handle::parse_hex(dim.as_str().unwrap()).unwrap()).unwrap().kind {
            kition_cad_doc::EntityKind::Dimension(d) => d.style.clone(),
            other => panic!("not a dimension: {other:?}"),
        };
        assert_eq!(style, "ISO-25", "the dimension takes my current dimension style");
        assert_eq!(saved.header.str("DIMSTYLE", "?"), "ISO-25", "the file carries the saving session's current dimension style");
        assert!(saved.dim_style("Other").is_some(), "the colleague's style stays in the drawing");
        f.kits.shutdown().await;
    }
}
