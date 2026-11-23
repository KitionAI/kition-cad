//! C8-cad-3: expectations are declared before execution; every cell rewrites a real DXF.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_doc::{Drawing, EntityKind, Handle};
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

const FILE: &str = "Matrix.dxf";
const CHANGES: &[&str] = &[
    "same",
    "other",
    "delete",
    "move",
    "rotate",
    "scale",
    "enter",
    "leave",
    "freeze",
    "lock",
    "off",
    "rename-layer",
    "redefine",
    "explode",
    "source",
    "ucs",
    "units",
    "dimstyle",
    "order",
    "unrelated",
];
// A: apply; C: changed; D: deleted; R: relocated. Expectations follow consumed facts,
// not the current implementation. Columns are CHANGES above, including unrelated controls.
// same-field decision: replacing an observed field holds its old value.
// C8-cad-pin API adaptation: public SDK guard.rs:2037-2042 distinguishes a
// deleted covered recipient (D) from one relocated out of the place (R).
// Owner lock policy is enforced by the engine transaction: handle-color, point,
// window, crossing and fence / lock must refuse; selection cannot bypass it.
const ADDRESSES: &[(&str, &str)] = &[
    ("handle-color", "CADAAAAAACAAADAAAAAA"),
    ("handle-move", "AADCCCAAACAAADAAAAAA"),
    ("point", "CADRRCRRRCRAARAAAARA"),
    ("window", "CADRRRRRRCRAARAAAAAA"),
    ("crossing", "CADRCCRRRCRAARAAAAAA"),
    ("fence", "CADRCCRRRCRAARAAAAAA"),
    ("layer-name", "CADAAAAAAAARAAAAAAAA"),
    ("style-name", "CADAAAAAAAAAAAAAAAAA"),
    ("block-name", "CCDAAAAAAAAACAAAAAAA"),
    ("table-index", "CADAAAAAAAAAAAAAAAAA"),
    ("snap", "AAAAAAAAAAAAAACAACAA"),
    ("dimension", "CADAAAAAAAAAAACAAAAA"),
    ("view-name", "RADAAAAAAAAAAAAAAAAA"),
    ("ucs-name", "RADAAAAAAAAAAAAAAAAA"),
];

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
    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!(FILE);
        tools::call_as(&self.kits, &format!("cad_{action}"), &params, CallOptions::default(), "matrix")
            .await
            .unwrap()
            .map(|r| r.output)
            .map_err(|e| e.structured())
    }
    fn colleague(&self, edit: impl FnOnce(&mut CadDocument)) {
        let before = self.disk();
        let mut d = CadDocument::open(FILE, &before).unwrap();
        edit(&mut d);
        let after = d.save(Some(FILE)).unwrap();
        assert!(before != after, "every cell must rewrite the file");
        std::fs::write(self.dir.path().join(FILE), after).unwrap();
    }
}
fn run(d: &mut CadDocument, command: &str, params: Value) -> Value {
    d.execute(command, params).unwrap_or_else(|e| panic!("{command}: {e}"))
}
fn raw(d: &mut CadDocument, edit: impl FnOnce(&mut Drawing)) {
    let mut drawing = kition_cad_io::read(&d.save(Some(FILE)).unwrap(), FILE).unwrap();
    edit(&mut drawing);
    *d = CadDocument::open(FILE, &kition_cad_io::write(&drawing, FILE).unwrap()).unwrap();
}
fn h(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}
fn shape(d: &mut CadDocument, y: i32, layer: &str) -> Value {
    run(d, "rectang", json!({"p1":[0,y],"p2":[10,y+10],"layer":layer}))["handle"].clone()
}
fn line(d: &mut CadDocument, a: [i32; 2], b: [i32; 2], layer: &str) -> Value {
    run(d, "line", json!({"points":[a,b],"layer":layer}))["handles"][0].clone()
}
struct DrawingIds {
    a: Value,
    b: Value,
    member: Value,
    insert: Value,
    table: Value,
    source: Value,
    dim: Value,
}
fn drawing(change: &str) -> (CadDocument, DrawingIds) {
    let mut d = CadDocument::create(FILE, 420, 297).unwrap();
    run(&mut d, "layer.new", json!({"name":"Walls"}));
    run(&mut d, "layer.new", json!({"name":"Far"}));
    let a = shape(&mut d, 0, "Walls");
    let b = shape(&mut d, if matches!(change, "leave" | "order") { 0 } else { 40 }, "Far");
    let original = line(&mut d, [0, 80], [10, 80], "0");
    run(&mut d, "block", json!({"name":"B","base":[0,80],"handles":[original],"keep":"delete"}));
    let saved = kition_cad_io::read(&d.save(Some(FILE)).unwrap(), FILE).unwrap();
    let member = json!(saved.block("B").unwrap().entities.iter().next().unwrap().handle.hex());
    let insert = run(&mut d, "insert", json!({"name":"B","at":[100,0]}))["handle"].clone();
    run(&mut d, "style", json!({"name":"S","height":2,"current":false}));
    let table = run(&mut d, "table", json!({"at":[150,0],"rows":2,"cols":2}))["handle"].clone();
    let source = line(&mut d, [30, 0], [40, 0], "0");
    let dim = run(&mut d, "dimlinear", json!({"object":source,"at":[30,5]}))["handle"].clone();
    raw(&mut d, |d| {
        d.views.push(serde_json::from_value(json!({"name":"N","center":{"x":0,"y":0},"height":10,"width":10})).unwrap());
        d.ucss.push(
            serde_json::from_value(json!({"name":"N","origin":{"x":0,"y":0,"z":0},"xAxis":{"x":1,"y":0,"z":0},"yAxis":{"x":0,"y":1,"z":0}})).unwrap(),
        );
    });
    (d, DrawingIds { a, b, member, insert, table, source, dim })
}
fn address_steps(row: &str, ids: &DrawingIds) -> Value {
    let p = match row {
        "handle-color" => json!({"handles":[ids.a],"color":"red"}),
        "handle-move" => return json!([{"command":"move","params":{"handles":[ids.a],"delta":[2,0]}}]),
        "point" => json!({"pick":[5,0],"aperture":0.1,"color":"red"}),
        "window" | "crossing" | "fence" => {
            return json!([
                {"command":"select","params": if row == "fence" {json!({"fence":[[5,-1],[5,11]]})}
                    else {json!({"window":[[-1,-1],[11,11]],"crossing":row == "crossing"})}},
                {"command":"properties.set","params":{"color":"red"}}
            ]);
        }
        "layer-name" => return json!([{"command":"layer.set","params":{"name":"Walls","color":"red"}}]),
        "style-name" => return json!([{"command":"style","params":{"name":"S","height":4,"current":false}}]),
        "block-name" => return json!([{"command":"insert","params":{"name":"B","at":[120,0]}}]),
        "table-index" => return json!([{"command":"table.set","params":{"handle":ids.table,"row":0,"col":0,"text":"Mine"}}]),
        "snap" => return json!([{"command":"dimlinear","params":{"object":ids.source,"at":[30,10]}}]),
        "dimension" => json!({"handles":[ids.dim],"color":"red"}),
        "view-name" | "ucs-name" => {
            return json!([{"command":"rename","params":{"table":if row == "view-name" {"view"} else {"ucs"},"from":"N","to":"Mine"}}]);
        }
        _ => panic!("unknown row {row}"),
    };
    json!([{"command":"properties.set","params":p}])
}
fn edit_address(d: &mut CadDocument, row: &str, change: &str, ids: &DrawingIds) {
    let target = if row == "table-index" {
        &ids.table
    } else if row == "dimension" {
        &ids.dim
    } else {
        &ids.a
    };
    match change {
        "same" | "other" | "delete" if matches!(row, "view-name" | "ucs-name") => {
            if change == "same" {
                run(d, "rename", json!({"table":if row == "view-name" {"view"} else {"ucs"},"from":"N","to":"Theirs"}));
            } else {
                raw(d, |d| {
                    if row == "view-name" {
                        if change == "delete" {
                            d.views.clear();
                        } else {
                            d.views.get_mut(0).unwrap().center.x += 1.0;
                        }
                    } else if change == "delete" {
                        d.ucss.clear();
                    } else {
                        d.ucss.get_mut(0).unwrap().origin.x += 1.0;
                    }
                });
            }
        }
        "same" | "other" | "delete" if row == "style-name" => {
            run(
                d,
                if change == "delete" { "style.delete" } else { "style" },
                match change {
                    "delete" => json!({"name":"S"}),
                    "same" => json!({"name":"S","height":3,"current":false}),
                    _ => json!({"name":"S","widthFactor":2,"current":false}),
                },
            );
        }
        "same" | "other" | "delete" if row == "layer-name" => {
            if change == "delete" {
                run(d, "laydel", json!({"names":["Walls"]}));
            } else {
                run(
                    d,
                    "layer.set",
                    if change == "same" { json!({"name":"Walls","color":"blue"}) } else { json!({"name":"Walls","description":"Colleague"}) },
                );
            }
        }
        "same" | "other" | "delete" if row == "block-name" => raw(d, |d| {
            if change == "delete" {
                d.remove_entity(h(&ids.insert));
                d.blocks.remove("B");
            } else if let Some(b) = d.blocks.get_mut("B") {
                let b = Arc::make_mut(b);
                if change == "same" {
                    b.base.x += 1.0;
                } else {
                    b.entities.modify(h(&ids.member), |e| e.common.visible = false);
                }
            }
        }),
        "same" if row == "table-index" => {
            run(d, "table.set", json!({"handle":ids.table,"row":0,"col":0,"text":"Theirs"}));
        }
        "other" if row == "table-index" => {
            run(d, "table.set", json!({"handle":ids.table,"row":1,"col":1,"text":"Theirs"}));
        }
        "same" => {
            run(d, "properties.set", json!({"handles":[target],"color":"blue"}));
        }
        "other" => {
            run(d, "properties.set", json!({"handles":[target],"lineweight":30}));
        }
        "delete" => {
            run(d, "erase", json!({"handles":[target]}));
        }
        "move" => {
            run(d, "move", json!({"handles":[ids.a],"delta":[0,20]}));
        }
        "rotate" => {
            run(d, "rotate", json!({"handles":[ids.a],"base":[0,0],"angle":45}));
        }
        "scale" => {
            run(d, "scale", json!({"handles":[ids.a],"base":[0,0],"factor":2}));
        }
        "enter" | "leave" => {
            run(d, "move", json!({"handles":[ids.b],"delta":[0,if change == "enter" {-40} else {40}]}));
        }
        "freeze" | "lock" | "off" => {
            let mut p = json!({"name":"Walls"});
            p[match change {
                "freeze" => "frozen",
                "lock" => "locked",
                _ => "on",
            }] = json!(change != "off");
            run(d, "layer.set", p);
        }
        "rename-layer" => {
            run(d, "layer.set", json!({"name":"Walls","newName":"Renamed"}));
        }
        "redefine" => raw(d, |d| {
            Arc::make_mut(d.blocks.get_mut("B").unwrap()).entities.modify(h(&ids.member), |e| {
                if let EntityKind::Line(l) = &mut e.kind {
                    l.b.x += 2.0;
                }
            });
        }),
        "explode" => {
            run(d, "explode", json!({"handles":[if row == "table-index" {&ids.insert} else {&ids.a}]}));
        }
        "source" => raw(d, |d| {
            d.modify_entity(h(&ids.source), |e| {
                if let EntityKind::Line(l) = &mut e.kind {
                    l.b.x += 2.0;
                }
            })
            .unwrap();
        }),
        "ucs" => raw(d, |d| {
            d.ucss.get_mut(0).unwrap().origin.x += 2.0;
        }),
        "units" => {
            run(d, "units", json!({"insunits":1}));
        }
        "dimstyle" => {
            run(d, "dimstyle", json!({"name":"ISO-25","textHeight":4}));
        }
        "order" => {
            run(d, "draworder.front", json!({"handles":[ids.a]}));
        }
        "unrelated" => {
            run(d, "layer.new", json!({"name":"Unrelated"}));
        }
        _ => panic!("unknown change {change}"),
    }
}

#[derive(Default, Debug)]
struct Counts {
    cells: usize,
    wrong: usize,
    false_refusal: usize,
    wrong_reason: usize,
    failures: usize,
}
impl Counts {
    async fn cell(&mut self, f: &Fixture, row: &str, change: &str, steps: Value, want: char, basis: Value) {
        self.cells += 1;
        let before = f.disk();
        let result = f.call("run", json!({"basis":basis,"steps":steps,"changes":"all"})).await;
        match (&result, want) {
            (Ok(_), 'A') => {
                // Independently run the same write on the colleague's file and compare saved geometry.
                let mut oracle = CadDocument::open(FILE, &before).unwrap();
                for step in steps.as_array().unwrap() {
                    run(&mut oracle, step["command"].as_str().unwrap(), step["params"].clone());
                }
                let expected = kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap();
                let actual = kition_cad_io::read(&f.disk(), FILE).unwrap();
                assert_eq!(content(&actual), content(&expected), "saved result {row}/{change}");
            }
            (Ok(_), _) => {
                self.wrong += 1;
                println!("WRONG {row}/{change} want={want} got=apply");
            }
            (Err(e), _) if e["error"]["code"] == "conflict" => {
                assert_eq!(f.disk(), before, "refusal wrote {row}/{change}");
                if want == 'A' {
                    self.false_refusal += 1;
                    println!("FALSE {row}/{change}: {}", e["conflicts"]);
                } else {
                    let reason = match want {
                        'C' => "changed",
                        'D' => "deleted",
                        'R' => "relocated",
                        _ => panic!("expectation"),
                    };
                    if !e["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == reason) {
                        self.wrong_reason += 1;
                        println!("REASON {row}/{change} want={reason}: {}", e["conflicts"]);
                    }
                }
            }
            (Err(e), _) => {
                self.failures += 1;
                println!("FAILURE {row}/{change} want={want}: {}", e["error"]);
            }
        }
    }
    fn finish(&self, name: &str) {
        println!("MATRIX {name}: {self:?}");
        assert_eq!((self.wrong, self.false_refusal, self.wrong_reason, self.failures), (0, 0, 0, 0), "{name}: {self:?}");
    }
}
fn content(d: &Drawing) -> Value {
    let mut entities: Vec<_> = d.model.iter().map(|e| json!([e.common, e.kind]).to_string()).collect();
    entities.sort();
    let header: std::collections::BTreeMap<_, _> =
        d.header.vars.iter().filter(|(k, _)| !matches!(k.as_str(), "EXTMIN" | "EXTMAX" | "HANDSEED")).collect();
    json!({"entities":entities,"layers":d.layers,"linetypes":d.linetypes,"styles":d.text_styles,"dimstyles":d.dim_styles,"header":header,"views":d.views,"ucss":d.ucss})
}

async fn address_cells(selection_consumers: bool) {
    let mut counts = Counts::default();
    for &(row, expected) in ADDRESSES {
        let expected: Vec<_> = expected.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(expected.len(), CHANGES.len(), "row {row}");
        for (index, &change) in CHANGES.iter().enumerate() {
            let sdk_consumer = matches!(row, "window" | "crossing" | "fence") && matches!(change, "other" | "rename-layer" | "order");
            if sdk_consumer != selection_consumers {
                continue;
            }
            let (mut d, ids) = drawing(change);
            let f = Fixture::new(&mut d);
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            f.colleague(|d| edit_address(d, row, change, &ids));
            let refusals = counts.false_refusal;
            counts.cell(&f, row, change, address_steps(row, &ids), expected[index], basis).await;
            if selection_consumers {
                assert_eq!(expected[index], 'A', "the desired outcome remains apply");
                assert_eq!(counts.false_refusal, refusals, "selection consumer must apply: {row}/{change}");
            }
            f.kits.shutdown().await;
        }
    }
    if selection_consumers {
        println!("MATRIX address selection consumers: {counts:?}");
        assert_eq!((counts.cells, counts.wrong, counts.false_refusal, counts.wrong_reason, counts.failures), (9, 0, 0, 0, 0));
    } else {
        counts.finish("address");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn address_matrix() {
    address_cells(false).await;
}

// Each consumer is planned after its selector; the frozen apply cells remain mandatory.
#[tokio::test(flavor = "multi_thread")]
async fn address_matrix_selection_consumers() {
    address_cells(true).await;
}

// Other source: move, delete, geometry, off, locked; then an unrelated entity move.
// review: JOIN/PEDIT joined-away source / locked follows the owner
// policy; the engine transaction enforces it.
const DERIVED: &[(&str, &str)] = &[
    ("trim", "CDCCAA"),
    ("extend", "CDCCAA"),
    ("fillet", "CDCACA"),
    ("chamfer", "CDCACA"),
    ("offset", "CDCAAA"),
    ("hatch-handles", "CDCAAA"),
    ("hatch-point", "CDCRAA"),
    ("dimension-object", "CDCAAA"),
    ("dimension-snap", "RDCAAA"),
    ("arraypath", "CDCAAA"),
    ("array-dispatch", "CDCAAA"),
    ("join", "CDCACA"),
    ("pedit-join", "CDCACA"),
];
fn derived(row: &str) -> (CadDocument, Value, Value, Value) {
    let mut d = CadDocument::create(FILE, 420, 297).unwrap();
    run(&mut d, "layer.new", json!({"name":"Source"}));
    let far = line(&mut d, [0, 100], [10, 100], "0");
    let a = line(&mut d, [0, 0], [10, 0], "0");
    let source = match row {
        "trim" => line(&mut d, [5, -10], [5, 10], "Source"),
        "extend" => line(&mut d, [15, -10], [15, 10], "Source"),
        "fillet" | "chamfer" => line(&mut d, [10, 0], [10, 10], "Source"),
        "join" | "pedit-join" => line(&mut d, [10, 0], [20, 0], "Source"),
        "hatch-handles" | "hatch-point" => shape(&mut d, 20, "Source"),
        _ => line(&mut d, [30, 0], [40, 0], "Source"),
    };
    let (command, params) = match row {
        "trim" | "extend" => (row, json!({"handle":a,"pick":[9,0],"edges":[source]})),
        "fillet" => (row, json!({"h1":a,"h2":source,"radius":1,"p1":[5,0],"p2":[10,5]})),
        "chamfer" => (row, json!({"h1":a,"h2":source,"d1":1,"d2":1,"p1":[5,0],"p2":[10,5]})),
        "offset" => (row, json!({"handle":source,"distance":2,"side":[35,5]})),
        "hatch-handles" => ("hatch", json!({"handles":[source],"pattern":"SOLID"})),
        "hatch-point" => ("hatch", json!({"points":[[5,25]],"pattern":"SOLID"})),
        "dimension-object" => ("dimlinear", json!({"object":source,"at":[35,5]})),
        "dimension-snap" => ("dimlinear", json!({"p1":[30,0],"p2":[40,0],"at":[35,5]})),
        "arraypath" => (row, json!({"handles":[a],"path":source,"count":3})),
        "array-dispatch" => ("array", json!({"type":"path","handles":[a],"path":source,"count":3})),
        "join" => (row, json!({"handles":[a,source]})),
        "pedit-join" => ("pedit", json!({"handle":a,"option":"join","handles2":[source]})),
        _ => panic!("unknown derived row"),
    };
    (d, json!([{"command":command,"params":params}]), source, far)
}
#[tokio::test(flavor = "multi_thread")]
async fn derived_geometry_matrix() {
    let mut counts = Counts::default();
    for &(row, expected) in DERIVED {
        for (index, change) in ["move", "delete", "geometry", "off", "locked", "unrelated"].into_iter().enumerate() {
            let (mut d, steps, source, far) = derived(row);
            let f = Fixture::new(&mut d);
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            f.colleague(|d| match change {
                "move" | "unrelated" => {
                    run(d, "move", json!({"handles":[if change == "move" {&source} else {&far}],"delta":[1,0]}));
                }
                "delete" => {
                    run(d, "erase", json!({"handles":[source]}));
                }
                "geometry" => raw(d, |d| {
                    d.modify_entity(h(&source), |e| match &mut e.kind {
                        EntityKind::Line(l) => l.b.x += 2.0,
                        EntityKind::LwPolyline(p) => {
                            p.vertices.get_mut(1).unwrap().p.x += 2.0;
                        }
                        _ => panic!("source kind"),
                    })
                    .unwrap();
                }),
                "off" | "locked" => {
                    run(d, "layer.set", if change == "off" { json!({"name":"Source","on":false}) } else { json!({"name":"Source","locked":true}) });
                }
                _ => panic!("unknown change"),
            });
            counts.cell(&f, row, change, steps, expected.chars().nth(index).unwrap(), basis).await;
            f.kits.shutdown().await;
        }
    }
    counts.finish("derived");
}

#[tokio::test(flavor = "multi_thread")]
async fn stored_settings_family_matrix() {
    // The last field: the setting is each person's own (the current colour, lineweight, text
    // style and linetype: kdm_model::CURRENT_VARS), so a colleague's is theirs and mine applies
    // (the saved file carries mine); the others are the drawing's, shared, and replacing one
    // holds the value I observed.
    let rows = [
        ("color", json!({"color":"red"}), json!({"color":"blue"}), true),
        ("lweight", json!({"lineweight":"ByLayer"}), json!({"lineweight":"ByBlock"}), true),
        ("ltscale", json!({"scale":2}), json!({"scale":3}), false),
        ("units", json!({"luprec":2}), json!({"luprec":3}), false),
        ("limits", json!({"min":[0,0],"max":[100,100]}), json!({"min":[0,0],"max":[200,200]}), false),
        ("ddptype", json!({"pdmode":2}), json!({"pdmode":3}), false),
        ("style.current", json!({"name":"S"}), json!({"name":"T"}), true),
        ("linetype", json!({"current":"ByBlock"}), json!({"current":"Continuous"}), true),
    ];
    let mut counts = Counts::default();
    for (command, mine, theirs, own) in rows {
        for relevant in [true, false] {
            let (mut d, ids) = drawing("unrelated");
            run(&mut d, "style", json!({"name":"T","current":false}));
            let f = Fixture::new(&mut d);
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            f.colleague(|d| {
                if relevant {
                    run(d, command, theirs.clone());
                } else {
                    run(d, "move", json!({"handles":[ids.b],"delta":[1,0]}));
                }
            });
            counts
                .cell(
                    &f,
                    command,
                    if relevant { "same" } else { "unrelated" },
                    json!([{"command":command,"params":mine}]),
                    if relevant && !own { 'C' } else { 'A' }, // same-field decision for shared settings
                    basis,
                )
                .await;
            f.kits.shutdown().await;
        }
    }
    counts.finish("settings");
}

#[tokio::test(flavor = "multi_thread")]
async fn current_style_family_matrix() {
    // The last field: the colleague changed their own current text style (TEXTSTYLE, each
    // person's own): my STYLE, which makes S my current style, applies. The current dimension
    // style and the DIM* variables measured against it stay the drawing's, shared.
    let rows = [
        ("style", json!({"name":"S"}), "style.current", json!({"name":"T"}), true),
        ("dimstyle", json!({"name":"ISO-25"}), "dimstyle.current", json!({"name":"Standard"}), false),
        ("dimstyle.dimension", json!({"name":"ISO-25"}), "dimstyle.current", json!({"name":"Standard"}), false),
        ("dimstyle.current", json!({"name":"ISO-25"}), "dimstyle.current", json!({"name":"Standard"}), false),
        ("dimstyle.current", json!({"name":"ISO-25"}), "setvar", json!({"name":"DIMASZ","value":3}), false),
    ];
    let mut counts = Counts::default();
    for (command, mine, other_command, theirs, own) in rows {
        for relevant in [true, false] {
            let (mut d, ids) = drawing("unrelated");
            run(&mut d, "style", json!({"name":"T","current":false}));
            let f = Fixture::new(&mut d);
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            f.colleague(|d| {
                if relevant {
                    run(d, other_command, theirs.clone());
                } else {
                    run(d, "move", json!({"handles":[ids.b],"delta":[1,0]}));
                }
            });
            counts
                .cell(
                    &f,
                    command,
                    if relevant { other_command } else { "unrelated" },
                    json!([{"command":command,"params":mine}]),
                    if relevant && !own { 'C' } else { 'A' }, // same-field decision for shared settings
                    // SETVAR introduces DIMASZ into the header; synchronization then stores
                    // a value that was absent at observation, without changing the style definition.
                    basis,
                )
                .await;
            f.kits.shutdown().await;
        }
    }
    counts.finish("current-styles");
}

#[tokio::test(flavor = "multi_thread")]
async fn library_and_base_family_matrix() {
    let mut counts = Counts::default();
    for row in ["base", "linetype-units", "linetype-loaded", "linetype-current"] {
        for relevant in [true, false] {
            let (mut d, ids) = drawing("unrelated");
            let f = Fixture::new(&mut d);
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            f.colleague(|d| {
                if !relevant {
                    if row == "linetype-current" {
                        raw(d, |d| {
                            d.linetypes.iter_mut().find(|t| t.name == "Continuous").unwrap().description = "Colleague".into();
                        });
                    } else {
                        run(d, "move", json!({"handles":[ids.b],"delta":[1,0]}));
                    }
                } else {
                    match row {
                        "base" => {
                            run(d, "base", json!({"at":[2,2]}));
                        }
                        "linetype-units" => {
                            run(d, "setvar", json!({"name":"MEASUREMENT","value":0}));
                        }
                        "linetype-loaded" => {
                            run(d, "linetype", json!({"load":"DASHED"}));
                        }
                        "linetype-current" => {
                            run(d, "linetype", json!({"current":"ByBlock"}));
                        }
                        _ => panic!("library row"),
                    }
                }
            });
            let (command, params) = match row {
                "base" => ("base", json!({"at":[1,1]})),
                "linetype-current" => ("linetype", json!({"current":"Continuous"})),
                _ => ("linetype", json!({"load":"DASHED"})),
            };
            counts
                .cell(
                    &f,
                    row,
                    if relevant { "same" } else { "unrelated" },
                    json!([{"command":command,"params":params}]),
                    // same-field decision: BASE replaces its observed setting. The current
                    // linetype is each person's own: a colleague's is theirs, mine applies.
                    if relevant && row != "linetype-current" { 'C' } else { 'A' },
                    basis,
                )
                .await;
            f.kits.shutdown().await;
        }
    }
    counts.finish("library-base");
}
