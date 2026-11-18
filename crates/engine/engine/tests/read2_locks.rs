//! C8-cad-read-2: the decided lock policy applies to every entity mutation, including hooks.
use kition_cad_doc::{EntityKind, Handle};
use kition_cad_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, c: &str, p: Value) -> Value {
    s.execute(c, &p).unwrap()
}
fn h(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}

#[test]
fn locked_entity_writes_are_atomic_across_command_families() {
    let mut wrong = Vec::new();
    for case in [
        "breakatpoint",
        "join",
        "pedit",
        "pedit-secondary",
        "region",
        "revcloud",
        "wipeout",
        "block",
        "-block",
        "find",
        "purge-zero",
        "purge-text",
        "properties.set",
        "matchprop",
        "dimoverride",
        "dimtedit",
        "dimstyle.update",
        "dimdisassociate",
        "dimreassociate",
        "laycur",
        "laymch",
        "table.set",
        "table.insertrow",
        "table.deleterow",
        "table.insertcol",
        "table.deletecol",
        "table.merge",
        "table.unmerge",
        "attedit",
        "-attedit",
        "attsync",
        "constraint",
    ] {
        let mut s = Session::new();
        run(&mut s, "layer.new", json!({"name":"Locked"}));
        run(&mut s, "layer.current", json!({"name":"Locked"}));
        let a = run(&mut s, "line", json!({"points":[[0,0],[10,0]],"layer":"Locked"}))["handles"][0].clone();
        let b = run(&mut s, "line", json!({"points":[[10,0],[10,10]],"layer":"Locked"}))["handles"][0].clone();
        let circle = run(&mut s, "circle", json!({"center":[30,0],"radius":5,"layer":"Locked"}))["handle"].clone();
        let rect = run(&mut s, "rectang", json!({"p1":[40,0],"p2":[50,10],"layer":"Locked"}))["handle"].clone();
        let text = run(&mut s, "text", json!({"at":[0,30],"text":"Before","height":2,"layer":"Locked"}))["handle"].clone();
        let dim = run(&mut s, "dimlinear", json!({"object":a,"at":[5,-10],"layer":"Locked"}))["handle"].clone();
        let table = run(&mut s, "table", json!({"at":[0,50],"rows":3,"cols":3,"layer":"Locked"}))["handle"].clone();
        let att = run(&mut s, "attdef", json!({"at":[80,0],"tag":"CODE","default":"Before","height":2}))["handle"].clone();
        run(&mut s, "block", json!({"name":"Attribute","base":[80,0],"handles":[att],"keep":"delete"}));
        let insert = run(&mut s, "insert", json!({"name":"Attribute","at":[80,0],"layer":"Locked"}))["handle"].clone();
        run(&mut s, "layer.current", json!({"name":"0"}));
        let source = run(&mut s, "line", json!({"points":[[90,0],[100,0]]}))["handles"][0].clone();
        if case == "pedit-secondary" {
            s.doc_mut().unwrap().modify_entity(h(&a), |e| e.common.layer = "0".into()).unwrap();
        }
        if case == "dimreassociate" {
            s.doc_mut()
                .unwrap()
                .modify_entity(h(&dim), |e| {
                    if let EntityKind::Dimension(dm) = &mut e.kind {
                        dm.assoc.clear();
                    }
                })
                .unwrap();
        }
        if case == "attsync" {
            run(&mut s, "attedit", json!({"handle":insert,"values":{"CODE":"Kept"}}));
            s.doc_mut()
                .unwrap()
                .modify_entity(h(&insert), |e| {
                    if let EntityKind::Insert(i) = &mut e.kind {
                        i.attribs.clear();
                    }
                })
                .unwrap();
        }
        if case == "purge-zero" {
            s.doc_mut()
                .unwrap()
                .modify_entity(h(&a), |e| {
                    if let EntityKind::Line(l) = &mut e.kind {
                        l.b = l.a;
                    }
                })
                .unwrap();
        }
        if case == "purge-text" {
            run(&mut s, "textedit", json!({"handle":text,"text":""}));
        }
        if case == "table.unmerge" {
            run(&mut s, "table.merge", json!({"handle":table,"row":0,"col":0,"rows":2,"cols":2}));
        }
        if case == "dimstyle.update" {
            run(&mut s, "dimstyle", json!({"name":"Updated","textHeight":6}));
        }
        run(&mut s, "layer.set", json!({"name":"Locked","locked":true}));
        let (command, p) = match case {
            "breakatpoint" => (case, json!({"handle":a,"at":[5,0]})),
            "join" => (case, json!({"handles":[a,b]})),
            "pedit" | "pedit-secondary" => ("pedit", json!({"handle":a,"option":"join","handles2":[b]})),
            "region" => (case, json!({"handles":[circle]})),
            "revcloud" => (case, json!({"handle":rect,"erase":true})),
            "wipeout" => (case, json!({"handle":rect,"erase":true})),
            "block" | "-block" => (case, json!({"name":"NewBlock","base":[0,0],"handles":[a],"keep":"delete"})),
            "find" => (case, json!({"find":"Before","replace":"After"})),
            "purge-zero" => ("purge", json!({"type":"zerolength"})),
            "purge-text" => ("purge", json!({"type":"emptytext"})),
            "properties.set" => (case, json!({"handles":[a],"color":"red"})),
            "matchprop" => (case, json!({"source":source,"targets":[a]})),
            "dimoverride" => (case, json!({"handles":[dim],"DIMASZ":7})),
            "dimtedit" => (case, json!({"handles":[dim],"mode":"left"})),
            "dimstyle.update" | "dimdisassociate" | "dimreassociate" => (case, json!({"handles":[dim]})),
            "laycur" => (case, json!({"handles":[a]})),
            "laymch" => (case, json!({"handles":[a],"layer":"0"})),
            "table.set" => (case, json!({"handle":table,"row":0,"col":0,"text":"After"})),
            "table.insertrow" | "table.deleterow" => (case, json!({"handle":table,"row":1})),
            "table.insertcol" | "table.deletecol" => (case, json!({"handle":table,"col":1})),
            "table.merge" => (case, json!({"handle":table,"row":0,"col":0,"rows":2,"cols":2})),
            "table.unmerge" => (case, json!({"handle":table,"row":0,"col":0})),
            "attedit" => (case, json!({"handle":insert,"values":{"CODE":"After"}})),
            "-attedit" => (case, json!({"find":"Before","replace":"After"})),
            "attsync" => (case, json!({"name":"Attribute"})),
            "constraint" => ("gcvertical", json!({"h1":a})),
            _ => unreachable!(),
        };
        let before = s.doc().unwrap().clone();
        let answer = s.execute(command, &p);
        if answer.is_ok() {
            wrong.push(case);
        } else {
            assert_eq!(s.doc().unwrap(), &before, "{case}: partial mutation");
        }
    }
    assert!(wrong.is_empty(), "locked writes applied: {wrong:?}");
}

#[test]
fn a_solver_cannot_move_a_locked_neighbour_of_an_unlocked_target() {
    let mut s = Session::new();
    run(&mut s, "layer.new", json!({"name":"Locked"}));
    let a = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    run(&mut s, "layer.current", json!({"name":"Locked"}));
    let b = run(&mut s, "line", json!({"points":[[10,0],[20,0]]}))["handles"][0].clone();
    run(&mut s, "gccoincident", json!({"h1":a,"p1":"end","h2":b,"p2":"start"}));
    run(&mut s, "layer.current", json!({"name":"0"}));
    run(&mut s, "layer.set", json!({"name":"Locked","locked":true}));
    let before = s.doc().unwrap().clone();
    assert!(s.execute("move", &json!({"handles":[a],"delta":[2,3]})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn an_idempotent_field_assignment_still_refuses_a_locked_target() {
    let mut s = Session::new();
    run(&mut s, "layer.new", json!({"name":"Locked"}));
    run(&mut s, "layer.current", json!({"name":"Locked"}));
    let a = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    run(&mut s, "layer.current", json!({"name":"0"}));
    run(&mut s, "layer.set", json!({"name":"Locked","locked":true}));
    let before = s.doc().unwrap().clone();
    assert!(s.execute("properties.set", &json!({"handles":[a],"color":"ByLayer"})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}
