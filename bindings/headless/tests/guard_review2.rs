//! C8-cad review 2: independent writers rewrite the real file between observe and write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

const FILE: &str = "Review2.dxf";
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
    fn disk(&self) -> Vec<u8> {
        std::fs::read(self.dir.path().join(FILE)).unwrap()
    }
    async fn call(&self, action: &str, mut p: Value) -> Result<Value, Value> {
        p["path"] = json!(FILE);
        tools::call_as(&self.kits, &format!("cad_{action}"), &p, CallOptions::default(), "review2")
            .await
            .unwrap()
            .map(|r| r.output)
            .map_err(|e| e.structured())
    }
    async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }
    fn colleague(&self, steps: &[(&str, Value)]) {
        let before = self.disk();
        let mut d = CadDocument::open(FILE, &before).unwrap();
        for (id, p) in steps {
            d.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        let after = d.save(Some(FILE)).unwrap();
        assert_ne!(before, after, "the independent writer must actually rewrite the file");
        std::fs::write(self.dir.path().join(FILE), after).unwrap();
    }
    fn colleague_drawing(&self, edit: impl FnOnce(&mut kition_cad_doc::Drawing)) {
        let before = self.disk();
        let mut d = kition_cad_io::read(&before, FILE).unwrap();
        edit(&mut d);
        let after = kition_cad_io::write(&d, FILE).unwrap();
        assert_ne!(before, after);
        std::fs::write(self.dir.path().join(FILE), after).unwrap();
    }
    async fn steps(&self, basis: Value, steps: Value, conflict: bool) -> Option<Value> {
        let before = self.disk();
        let r = self.call("run", json!({"basis":basis,"steps":steps,"changes":"all"})).await;
        if conflict {
            let e = r.expect_err("changed observed membership must refuse");
            assert_eq!(e["error"]["code"], "conflict", "{e}");
            assert_eq!(self.disk(), before, "refusal must not write");
            None
        } else {
            Some(r.unwrap())
        }
    }
}
fn drawing() -> CadDocument {
    CadDocument::create(FILE, 420, 297).unwrap()
}
fn line(d: &mut CadDocument, layer: &str, y: f64) -> Value {
    d.execute("line", json!({"points":[[0,y],[10,y]],"layer":layer})).unwrap()["handles"][0].clone()
}

fn handleless_layers(swapped: bool) -> Vec<u8> {
    let mut tags = vec![
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1015"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (70, "3"),
    ];
    for name in if swapped { ["0", "B", "A"] } else { ["0", "A", "B"] } {
        tags.extend([(0, "LAYER"), (2, name), (70, "0"), (62, "3"), (6, "Continuous")]);
    }
    tags.extend([(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES"), (0, "ENDSEC"), (0, "EOF")]);
    tags.into_iter().map(|(code, value)| format!("{code}\n{value}\n")).collect::<String>().into_bytes()
}

// Neither a name-derived ID nor equal values prove survival across a handleless rewrite.
#[tokio::test(flavor = "multi_thread")]
async fn handleless_name_swaps_refuse_unknown() {
    let mut d = drawing();
    let f = Fixture::new(&mut d);
    let before = handleless_layers(false);
    std::fs::write(f.dir.path().join(FILE), &before).unwrap();
    let basis = f.basis().await;
    let colleague = handleless_layers(true);
    assert_ne!(before, colleague, "independent writer swaps records without handles or identity metadata");
    std::fs::write(f.dir.path().join(FILE), &colleague).unwrap();
    let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"layer.set","params":{"name":"A","color":"red"}}]})).await;
    let error = answer.unwrap_err();
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == "unknown"), "{error}");
    assert_eq!(f.disk(), colleague, "unprovable identities must preserve the colleague's file");
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn propagation_dimension_target_holds_its_sources() {
    for relevant in [true, false] {
        let mut d = drawing();
        let source = line(&mut d, "0", 0.0);
        let far = line(&mut d, "0", 40.0);
        let dim = d.execute("dimlinear", json!({"object":source,"at":[0,5]})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("move", json!({"handles":[if relevant {source} else {far}],"delta":[1,0]}))]);
        f.steps(basis, json!([{"command":"properties.set","params":{"handles":[dim],"color":"red"}}]), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn propagation_find_dimension_target_holds_its_sources() {
    for relevant in [true, false] {
        let mut d = drawing();
        let source = line(&mut d, "0", 0.0);
        let far = line(&mut d, "0", 40.0);
        d.execute("dimlinear", json!({"object":source,"at":[0,5],"text":"Old"})).unwrap();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("move", json!({"handles":[if relevant {source} else {far}],"delta":[1,0]}))]);
        f.steps(basis, json!([{"command":"find","params":{"find":"Old","replace":"New"}}]), relevant).await;
        f.kits.shutdown().await;
    }
}

// An external writer that moves the source without re-measuring the dimension leaves the
// dimension's stored values equal: only its held sources can refuse.
#[tokio::test(flavor = "multi_thread")]
async fn propagation_unpropagated_source_edit_refuses_every_dimension_write_form() {
    for form in ["handles", "qselect", "select", "find"] {
        for relevant in [true, false] {
            let mut d = drawing();
            let source = line(&mut d, "0", 0.0);
            let far = line(&mut d, "0", 40.0);
            let dim = d.execute("dimlinear", json!({"object":source,"at":[0,5],"text":"Old"})).unwrap()["handle"].clone();
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            let moved = if relevant { source.clone() } else { far.clone() };
            f.colleague_drawing(|d| {
                d.modify_entity(kition_cad_doc::Handle::parse_hex(moved.as_str().unwrap()).unwrap(), |e| {
                    if let kition_cad_doc::EntityKind::Line(l) = &mut e.kind {
                        l.b.x += 3.0;
                    }
                })
                .unwrap();
            });
            let steps = match form {
                "handles" => json!([{"command":"properties.set","params":{"handles":[dim],"color":"red"}}]),
                "qselect" => json!([
                    {"command":"qselect","params":{"type":"Dimension"}},
                    {"command":"properties.set","params":{"color":"red"}}
                ]),
                "select" => json!([
                    {"command":"select","params":{"handles":[dim]}},
                    {"command":"properties.set","params":{"color":"red"}}
                ]),
                _ => json!([{"command":"find","params":{"find":"Old","replace":"New"}}]),
            };
            f.steps(basis, steps, relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn propagation_source_receipt_names_dimension_and_constraint_consequences() {
    for constrained in [false, true] {
        let mut d = drawing();
        let a = line(&mut d, "0", 0.0);
        let b = d.execute("line", json!({"points":[[10,0],[20,0]]})).unwrap()["handles"][0].clone();
        let dim = d.execute("dimlinear", json!({"object":a,"at":[0,5]})).unwrap()["handle"].clone();
        if constrained {
            d.execute("gccoincident", json!({"h1":a,"p1":"end","h2":b,"p2":"start"})).unwrap();
        }
        let far = line(&mut d, "0", 40.0);
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("move", json!({"handles":[far],"delta":[1,0]}))]);
        let result = f.steps(basis, json!([{"command":"move","params":{"handles":[a],"delta":[2,0]}}]), false).await.unwrap();
        let changed = result["changes"]["changed"].as_array().unwrap();
        assert!(changed.iter().any(|c| c["id"] == dim), "propagated dimension must be named: {result}");
        if constrained {
            assert!(changed.iter().any(|c| c["id"] == b), "solved geometry must be named: {result}");
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn curves_continuation_holds_the_engine_last_curve() {
    for relevant in [true, false] {
        let mut d = drawing();
        line(&mut d, "0", 0.0);
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if relevant { ("line", json!({"points":[[0,20],[10,20]]})) } else { ("circle", json!({"center":[40,40],"radius":2})) }]);
        f.steps(basis, json!([{"command":"arc.continue","params":{"end":[20,10]}}]), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn curves_continuation_holds_source_identity_in_only_its_space() {
    for same_space in [true, false] {
        let mut d = drawing();
        line(&mut d, "0", 0.0);
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[("line", json!({"points":[[0,0],[10,0]],"layout":if same_space {"Model"} else {"Layout1"}}))]);
        f.steps(basis, json!([{"command":"arc.continue","params":{"end":[20,10]}}]), same_space).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn curves_nested_copy_holds_recursive_members_and_tangent_tools_hold_sources() {
    for family in ["ncopy", "divide", "circle.ttr"] {
        for relevant in [true, false] {
            let mut d = drawing();
            let a = line(&mut d, "0", 0.0);
            let b = d.execute("line", json!({"points":[[0,0],[0,10]]})).unwrap()["handles"][0].clone();
            let replacement = line(&mut d, "0", 20.0);
            let far = line(&mut d, "0", 40.0);
            let (command, params) = if family == "ncopy" {
                d.execute("block", json!({"name":"Inner","base":[0,0],"handles":[a],"keep":"delete"})).unwrap();
                let inner = d.execute("insert", json!({"name":"Inner","at":[0,0]})).unwrap()["handle"].clone();
                d.execute("block", json!({"name":"Outer","base":[0,0],"handles":[inner],"keep":"delete"})).unwrap();
                let outer = d.execute("insert", json!({"name":"Outer","at":[0,0]})).unwrap()["handle"].clone();
                (family, json!({"handle":outer,"pick":[5,0],"delta":[20,0]}))
            } else if family == "divide" {
                (family, json!({"handle":a,"segments":4}))
            } else {
                (family, json!({"h1":a,"p1":[5,0],"h2":b,"p2":[0,5],"radius":2}))
            };
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[if !relevant {
                ("move", json!({"handles":[far],"delta":[1,0]}))
            } else if family == "ncopy" {
                ("block", json!({"name":"Inner","base":[0,0],"handles":[replacement],"keep":"delete"}))
            } else {
                ("move", json!({"handles":[a],"delta":[1,0]}))
            }]);
            f.steps(basis, json!([{"command":command,"params":params}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn curves_trim_extend_fence_and_crossing_hold_membership() {
    for command in ["trim", "extend"] {
        for form in ["fence", "crossing"] {
            for relevant in [true, false] {
                let mut d = drawing();
                d.execute("line", json!({"points":[[0,0],[if command=="trim" {10}else{3},0]]})).unwrap();
                let edge = d.execute("line", json!({"points":[[5,-10],[5,10]]})).unwrap()["handles"][0].clone();
                let far = line(&mut d, "0", 40.0);
                let f = Fixture::new(&mut d);
                let basis = f.basis().await;
                f.colleague(&[if relevant {
                    ("line", json!({"points":[[0,0.5],[if command=="trim" {10}else{3},0.5]]}))
                } else {
                    ("move", json!({"handles":[far],"delta":[1,0]}))
                }]);
                let mut p = json!({"edges":[edge]});
                p[form] = if form == "fence" { json!([[2.8, -1], [2.8, 1]]) } else { json!([[2.7, -1], [2.9, 1]]) };
                f.steps(basis, json!([{"command":command,"params":p}]), relevant).await;
                f.kits.shutdown().await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn query_find_holds_only_matching_words_and_membership() {
    for mutation in ["add", "enter", "change", "unrelated"] {
        let mut d = drawing();
        let a = d.execute("text", json!({"at":[0,0],"text":"Door"})).unwrap()["handle"].clone();
        let b = d.execute("text", json!({"at":[0,20],"text":"Roof"})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[match mutation {
            "add" => ("text", json!({"at":[0,40],"text":"Door"})),
            "enter" => ("textedit", json!({"handle":b,"text":"Door"})),
            "change" => ("textedit", json!({"handle":a,"text":"Door colleague"})),
            _ => ("textedit", json!({"handle":b,"text":"Ceiling"})),
        }]);
        f.steps(basis, json!([{"command":"find","params":{"find":"Door","replace":"Window"}}]), mutation != "unrelated").await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn clipboard_capture_holds_nested_definitions() {
    for copy in ["copyclip", "copybase", "cutclip"] {
        for relevant in [true, false] {
            let mut d = drawing();
            let a = line(&mut d, "0", 0.0);
            d.execute("block", json!({"name":"Door","base":[0,0],"handles":[a],"keep":"delete"})).unwrap();
            let insert = d.execute("insert", json!({"name":"Door","at":[0,0]})).unwrap()["handle"].clone();
            let replacement = line(&mut d, "0", 20.0);
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            if relevant {
                f.colleague(&[("block", json!({"name":"Door","base":[5,0],"handles":[replacement],"keep":"delete"}))]);
            } else {
                f.colleague(&[("layer.new", json!({"name":"Other"}))]);
            }
            let params = if copy == "copybase" { json!({"handles":[insert],"base":[0,0]}) } else { json!({"handles":[insert]}) };
            f.steps(
                basis,
                json!([
                    {"command":copy,"params":params},
                    {"command":"pasteclip","params":{"at":[40,0]}}
                ]),
                relevant,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn clipboard_capture_holds_layer_and_text_style_values() {
    for family in ["layer.set", "style"] {
        for relevant in [true, false] {
            let mut d = drawing();
            let a = d.execute("text", json!({"at":[0,0],"text":"Door"})).unwrap()["handle"].clone();
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[if relevant {
                (family, if family == "style" { json!({"name":"Standard","height":8}) } else { json!({"name":"0","color":"red"}) })
            } else {
                ("layer.new", json!({"name":"Other"}))
            }]);
            f.steps(basis, json!([{"command":"copyclip","params":{"handles":[a]}},{"command":"pasteclip","params":{"at":[40,0]}}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn history_reload_never_reverts_the_independent_writer() {
    for command in ["edit.undo", "edit.redo", "u", "undo", "redo"] {
        let mut d = drawing();
        let a = line(&mut d, "0", 0.0);
        let b = line(&mut d, "0", 20.0);
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        let own = f.steps(basis, json!([{"command":"move","params":{"handles":[a],"delta":[1,0]}}]), false).await.unwrap();
        if command.ends_with("redo") {
            f.call("run", json!({"steps":[{"command":"edit.undo","params":{}}]})).await.unwrap();
        }
        let basis = if command.ends_with("redo") { f.basis().await } else { own["revision"].clone() };
        f.colleague(&[("move", json!({"handles":[b],"delta":[3,0]}))]);
        let before = f.disk();
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":{}}]})).await;
        assert_eq!(f.disk(), before, "reload discarded history; undo must never replace the colleague's drawing");
        assert_eq!(answer.unwrap_err()["error"]["code"], "conflict");
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_layer_merge_delete_and_purge_hold_membership() {
    for command in ["laymrg", "laydel", "purge", "overkill"] {
        for relevant in [true, false] {
            let mut d = drawing();
            d.execute("layer.new", json!({"name":"Walls"})).unwrap();
            if command != "purge" {
                line(&mut d, "Walls", 0.0);
            }
            let far = line(&mut d, "0", 20.0);
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            if relevant {
                f.colleague(&[if command == "purge" {
                    ("layer.new", json!({"name":"NewUnused"}))
                } else {
                    ("line", json!({"points":[[0,0],[10,0]],"layer":"Walls"}))
                }]);
            } else {
                f.colleague(&[("move", json!({"handles":[far],"delta":[1,0]}))]);
            }
            let params = match command {
                "laymrg" => json!({"from":["Walls"],"to":"0"}),
                "laydel" => json!({"names":["Walls"]}),
                "purge" => json!({"type":"layers"}),
                _ => json!({}),
            };
            f.steps(basis, json!([{"command":command,"params":params}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_purge_holds_victim_content_and_degenerate_membership() {
    for kind in ["layers", "emptytext", "zerolength"] {
        for relevant in [true, false] {
            let mut d = drawing();
            d.execute("layer.new", json!({"name":"Unused"})).unwrap();
            let a = d.execute("text", json!({"at":[0,0],"text":"Door"})).unwrap()["handle"].clone();
            let b = line(&mut d, "0", 20.0);
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            if relevant && kind == "zerolength" {
                f.colleague_drawing(|d| {
                    d.modify_entity(kition_cad_doc::Handle::parse_hex(b.as_str().unwrap()).unwrap(), |e| {
                        if let kition_cad_doc::EntityKind::Line(l) = &mut e.kind {
                            l.b = l.a;
                        }
                    })
                    .unwrap();
                });
            } else {
                f.colleague(&[if !relevant {
                    ("move", json!({"handles":[b],"delta":[1,0]}))
                } else {
                    match kind {
                        "layers" => ("layer.set", json!({"name":"Unused","color":"red"})),
                        "emptytext" => ("textedit", json!({"handle":a,"text":""})),
                        _ => ("line", json!({"points":[[0,40],[0,40]]})),
                    }
                }]);
            }
            f.steps(basis, json!([{"command":"purge","params":{"type":kind}}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_purge_type_does_not_hold_other_types() {
    for ty in ["layers", "emptytext"] {
        let mut d = drawing();
        d.execute("layer.new", json!({"name":"Unused"})).unwrap();
        let text = d.execute("text", json!({"at":[0,0],"text":"Door"})).unwrap()["handle"].clone();
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[if ty == "layers" { ("textedit", json!({"handle":text,"text":""})) } else { ("layer.new", json!({"name":"AnotherUnused"})) }]);
        f.steps(basis, json!([{"command":"purge","params":{"type":ty}}]), false).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_ungroup_holds_named_and_discovered_groups() {
    for form in 0..3 {
        for relevant in [true, false] {
            let mut d = drawing();
            let a = line(&mut d, "0", 0.0);
            let b = line(&mut d, "0", 20.0);
            let c = line(&mut d, "0", 40.0);
            let extra = line(&mut d, "0", 60.0);
            for (name, h) in [("G1", a.clone()), ("G2", b), ("Other", c.clone())] {
                d.execute("group", json!({"name":name,"handles":[h]})).unwrap();
            }
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[("groupedit", json!({"name":if relevant {"G1"} else {"Other"},"add":[extra]}))]);
            let params = match form {
                0 => json!({"name":"G1,G2"}),
                1 => json!({"name":["G1","G2"]}),
                _ => json!({"handles":[a]}),
            };
            f.steps(basis, json!([{"command":"ungroup","params":params}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_ungroup_holds_newly_discovered_groups_and_replaced_names() {
    for by_handle in [true, false] {
        for relevant in [true, false] {
            let mut d = drawing();
            let a = line(&mut d, "0", 0.0);
            let b = line(&mut d, "0", 20.0);
            let c = line(&mut d, "0", 40.0);
            d.execute("group", json!({"name":"G1","handles":[a]})).unwrap();
            d.execute("group", json!({"name":"G2","handles":[b]})).unwrap();
            d.execute("group", json!({"name":"Other","handles":[c]})).unwrap();
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            let params = if by_handle {
                f.colleague(&[("group", json!({"name":"New","handles":[if relevant {a.clone()} else {c.clone()}]}))]);
                json!({"handles":[a]})
            } else {
                let name = if relevant { "G1" } else { "Other" };
                f.colleague(&[("ungroup", json!({"name":name})), ("group", json!({"name":name,"handles":[b]}))]);
                json!({"name":"G1,G2"})
            };
            f.steps(basis, json!([{"command":"ungroup","params":params}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_audit_holds_new_invalid_geometry_but_allows_healthy_edits() {
    for relevant in [true, false] {
        let mut d = drawing();
        let a = line(&mut d, "0", 0.0);
        let b = line(&mut d, "0", 20.0);
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        if relevant {
            f.colleague_drawing(|d| {
                d.modify_entity(kition_cad_doc::Handle::parse_hex(a.as_str().unwrap()).unwrap(), |e| {
                    if let kition_cad_doc::EntityKind::Line(l) = &mut e.kind {
                        l.b = l.a;
                    }
                })
                .unwrap();
            });
        } else {
            f.colleague(&[("move", json!({"handles":[b],"delta":[1,0]}))]);
        }
        f.steps(basis, json!([{"command":"audit","params":{"fix":true}}]), relevant).await;
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn query_matching_set_is_held_across_independent_rewrites() {
    for mutation in ["add", "remove", "enter", "unrelated"] {
        let mut d = drawing();
        d.execute("layer.new", json!({"name":"Walls"})).unwrap();
        let a = line(&mut d, "Walls", 0.0);
        let b = line(&mut d, "0", 20.0);
        let f = Fixture::new(&mut d);
        let basis = f.basis().await;
        f.colleague(&[match mutation {
            "add" => ("line", json!({"points":[[0,40],[10,40]],"layer":"Walls"})),
            "remove" => ("erase", json!({"handles":[a]})),
            "enter" => ("properties.set", json!({"handles":[b],"layer":"Walls"})),
            _ => ("move", json!({"handles":[b],"delta":[1,0]})),
        }]);
        let result = f
            .steps(
                basis,
                json!([
                    {"command":"qselect","params":{"layer":"Walls"}},
                    {"command":"properties.set","params":{"color":"red"}}
                ]),
                mutation != "unrelated",
            )
            .await;
        if result.is_some() {
            let mut saved = CadDocument::open(FILE, &f.disk()).unwrap();
            assert_eq!(saved.execute("properties", json!({"handles":[a]})).unwrap()["objects"][0]["color"], "Red");
        }
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn query_chained_selection_holds_values_consumed_by_later_filters() {
    for by_window in [false, true] {
        for relevant in [true, false] {
            let mut d = drawing();
            let a = line(&mut d, "0", 0.0);
            let b = line(&mut d, "0", 20.0);
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[("properties.set", json!({"handles":[if relevant {a.clone()} else {b}],"color":"red"}))]);
            f.steps(
                basis,
                json!([
                    {"command":"select","params":if by_window { json!({"window":[[-1,-1],[11,1]]}) } else { json!({"handles":[a]}) }},
                    {"command":"qselect","params":{"color":"red","applyTo":"selection","mode":"append"}},
                    {"command":"properties.set","params":{"color":"yellow"}}
                ]),
                relevant,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn query_type_color_numeric_wildcard_and_exclusion_use_engine_membership() {
    for (query, change) in [
        (json!({"type":"Circle"}), json!({"radius":4})),
        (json!({"color":"red"}), json!({"color":"red"})),
        (json!({"type":"Circle","property":"radius","operator":">","value":3}), json!({"radius":4})),
        (json!({"property":"layer","operator":"*","value":"W*"}), json!({"layer":"Walls"})),
        (json!({"color":"red","mode":"exclude"}), json!({"color":"red"})),
    ] {
        for relevant in [false, true] {
            let mut d = drawing();
            d.execute("layer.new", json!({"name":"Walls"})).unwrap();
            let b = d.execute("circle", json!({"center":[20,20],"radius":2})).unwrap()["handle"].clone();
            let far = line(&mut d, "0", 40.0);
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            if relevant {
                if query.get("property").is_none() && query.get("type").is_some() {
                    f.colleague(&[("circle", json!({"center":[40,20],"radius":4}))]);
                } else {
                    let mut p = change.clone();
                    p["handles"] = json!([b]);
                    f.colleague(&[("properties.set", p)]);
                }
            } else {
                f.colleague(&[("layer.new", json!({"name":"Other"}))]);
            }
            f.steps(
                basis,
                json!([
                    {"command":"qselect","params":query},
                    {"command":"properties.set","params":{"handles":[far],"color":"green"}}
                ]),
                relevant,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn query_selectall_replays_layer_flags_in_only_its_space() {
    for (space, used, other) in [("Model", "Used", "Other"), ("Layout1", "Other", "Used")] {
        for relevant in [true, false] {
            let mut d = drawing();
            for name in ["Used", "Other"] {
                d.execute("layer.new", json!({"name":name})).unwrap();
            }
            line(&mut d, "Used", 0.0);
            d.execute("line", json!({"points":[[0,0],[10,0]],"layer":"Other","layout":"Layout1"})).unwrap();
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[("layer.set", json!({"name":if relevant {used} else {other},"locked":true}))]);
            f.steps(
                basis,
                json!([
                    {"command":"selectall","at":{"part":{"layout":space}}},
                    {"command":"properties.set","at":{"part":{"layout":space}},"params":{"color":"yellow"}}
                ]),
                relevant,
            )
            .await;
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn query_unfiltered_selection_holds_added_members() {
    for command in ["selectall", "qselect"] {
        for relevant in [false, true] {
            let mut d = drawing();
            line(&mut d, "0", 0.0);
            let f = Fixture::new(&mut d);
            let basis = f.basis().await;
            f.colleague(&[if relevant { ("line", json!({"points":[[0,20],[10,20]]})) } else { ("layer.new", json!({"name":"Other"})) }]);
            f.steps(basis, json!([{"command":command,"params":{}},{"command":"scale","params":{"base":[0,0],"factor":2}}]), relevant).await;
            f.kits.shutdown().await;
        }
    }
}
