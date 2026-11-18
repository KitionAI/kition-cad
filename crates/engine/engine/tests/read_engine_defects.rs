//! C8-cad-read: local resolver and reference-preservation defects found by blind handler reads.
use kition_cad_doc::{EntityKind, Handle};
use kition_cad_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, command: &str, p: Value) -> Value {
    s.execute(command, &p).unwrap()
}

fn handle(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}

fn stored(s: &Session) -> kition_cad_doc::Drawing {
    s.doc().unwrap().clone()
}

#[test]
fn a_malformed_table_handle_never_edits_the_selected_table() {
    let mut s = Session::new();
    let h = run(&mut s, "table", json!({"at":[0,0],"rows":2,"cols":2}))["handle"].clone();
    run(&mut s, "select", json!({"handles":[h]}));
    for bad in [json!("zz"), json!(true), json!(-1)] {
        let before = stored(&s);
        assert!(s.execute("table.set", &json!({"handle":bad,"row":0,"col":0,"text":"wrong"})).is_err());
        assert_eq!(stored(&s), before);
    }
    run(&mut s, "table.set", json!({"row":0,"col":0,"text":"right"}));
}

#[test]
fn a_malformed_grip_handle_never_falls_back_to_another_target() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    for p in [json!({"handle":"zz","handles":[h],"index":0,"to":[1,2]}), json!({"handle":true,"handles":[h],"index":0,"to":[1,2]})] {
        let before = stored(&s);
        assert!(s.execute("grip.stretch", &p).is_err(), "{p}");
        assert_eq!(stored(&s), before);
    }
}

#[test]
fn a_malformed_base_handle_never_rotates_about_the_first_target() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    let before = stored(&s);
    assert!(s.execute("grip.rotate", &json!({"handles":[h],"baseHandle":"zz","index":0,"angle":90})).is_err());
    assert_eq!(stored(&s), before);
}

#[test]
fn malformed_trim_edges_never_erase_the_target_in_quick_mode() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    for edges in [json!(["zz"]), json!([true]), json!("zz")] {
        let before = stored(&s);
        assert!(s.execute("trim", &json!({"handle":h,"pick":[5,0],"edges":edges,"mode":"quick"})).is_err(), "{edges}");
        assert_eq!(stored(&s), before);
    }
}

fn attributed_insert(s: &mut Session) -> Value {
    let a = run(s, "attdef", json!({"tag":"A","default":"kept","at":[0,0],"height":1}))["handle"].clone();
    run(s, "block", json!({"name":"B","base":[0,0],"handles":[a],"keep":"delete"}));
    run(s, "insert", json!({"name":"B","at":[0,0]}))["handle"].clone()
}

#[test]
fn attsync_refuses_a_definition_without_attributes_without_losing_instance_values() {
    let mut s = Session::new();
    let h = attributed_insert(&mut s);
    let line = run(&mut s, "line", json!({"points":[[0,0],[1,0]]}))["handles"][0].clone();
    run(&mut s, "block", json!({"name":"B","handles":[line],"base":[0,0],"keep":"delete"}));
    let before = stored(&s);
    let e = s.execute("attsync", &json!({"handle":h})).expect_err("a block without attribute definitions must be refused");
    assert!(e.to_string().contains("attribute definitions"), "{e}");
    assert_eq!(stored(&s), before);
}

#[test]
fn attedit_preserves_constant_attributes() {
    let mut s = Session::new();
    let h = attributed_insert(&mut s);
    s.doc_mut()
        .unwrap()
        .modify_entity(handle(&h), |e| {
            let EntityKind::Insert(i) = &mut e.kind else { panic!("not an insert") };
            i.attribs[0].constant = true;
        })
        .unwrap();
    let before = stored(&s);
    assert_eq!(run(&mut s, "attedit", json!({"handle":h,"values":{"A":"wrong"}}))["changed"], 0);
    assert_eq!(stored(&s), before);
}

#[test]
fn renamed_text_styles_follow_text_inside_block_definitions() {
    let mut s = Session::new();
    run(&mut s, "style", json!({"name":"S"}));
    let h = run(&mut s, "text", json!({"at":[0,0],"text":"inside","height":1}))["handle"].clone();
    run(&mut s, "block", json!({"name":"B","handles":[h],"base":[0,0],"keep":"delete"}));
    run(&mut s, "style.rename", json!({"from":"S","to":"S2"}));
    let b = s.doc().unwrap().block("B").unwrap();
    let EntityKind::Text(t) = &b.entities.iter().next().unwrap().kind else { panic!("not text") };
    assert_eq!(t.style, "S2");
}

fn block_dimension(s: &mut Session) {
    run(s, "dimstyle", json!({"name":"D"}));
    let h = run(s, "dimlinear", json!({"p1":[0,0],"p2":[10,0],"at":[5,2]}))["handle"].clone();
    run(s, "block", json!({"name":"B","handles":[h],"base":[0,0],"keep":"delete"}));
    run(s, "dimstyle.current", json!({"name":"Standard"}));
}

#[test]
fn renamed_dimension_styles_follow_dimensions_inside_blocks() {
    let mut s = Session::new();
    block_dimension(&mut s);
    run(&mut s, "dimstyle.rename", json!({"from":"D","to":"D2"}));
    let b = s.doc().unwrap().block("B").unwrap();
    let EntityKind::Dimension(dm) = &b.entities.iter().next().unwrap().kind else { panic!("not a dimension") };
    assert_eq!(dm.style, "D2");
}

#[test]
fn dimension_styles_used_inside_blocks_cannot_be_deleted() {
    let mut s = Session::new();
    block_dimension(&mut s);
    let before = stored(&s);
    assert!(s.execute("dimstyle.delete", &json!({"name":"D"})).is_err());
    assert_eq!(stored(&s), before);
}

#[test]
fn renamed_text_styles_follow_multileader_text() {
    let mut s = Session::new();
    run(&mut s, "style", json!({"name":"S"}));
    run(&mut s, "mleaderstyle", json!({"name":"M","textStyle":"S"}));
    let h = run(&mut s, "mleader", json!({"points":[[0,0],[10,5]],"text":"callout"}))["handle"].clone();
    run(&mut s, "style.rename", json!({"from":"S","to":"S2"}));
    let EntityKind::MLeader(m) = &s.doc().unwrap().entity(handle(&h)).unwrap().kind else { panic!("not a multileader") };
    assert_eq!(m.text.as_ref().unwrap().style, "S2");
}

#[test]
fn tangent_three_circle_updates_the_radius_default_like_other_circle_commands() {
    let mut s = Session::new();
    let h1 = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    let h2 = run(&mut s, "line", json!({"points":[[0,0],[0,10]]}))["handles"][0].clone();
    let h3 = run(&mut s, "line", json!({"points":[[10,0],[0,10]]}))["handles"][0].clone();
    let h = run(&mut s, "circle.ttt", json!({"h1":h1,"p1":[3,0],"h2":h2,"p2":[0,3],"h3":h3,"p3":[5,5]}))["handle"].clone();
    let EntityKind::Circle(c) = &s.doc().unwrap().entity(handle(&h)).unwrap().kind else { panic!("not a circle") };
    assert_eq!(s.doc().unwrap().header.f64("CIRCLERAD", 0.0), c.radius);
}

#[test]
fn lengthen_preserves_an_elevated_lines_plane() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    s.doc_mut()
        .unwrap()
        .modify_entity(handle(&h), |e| {
            let EntityKind::Line(l) = &mut e.kind else { panic!("not a line") };
            l.a.z = 5.0;
            l.b.z = 5.0;
        })
        .unwrap();
    run(&mut s, "lengthen", json!({"handle":h,"delta":5,"pick":[10,0]}));
    let EntityKind::Line(l) = &s.doc().unwrap().entity(handle(&h)).unwrap().kind else { panic!("not a line") };
    assert_eq!((l.a.x, l.b.x), (0.0, 15.0));
    assert_eq!((l.a.z, l.b.z), (5.0, 5.0));
}

#[test]
fn an_unrelated_preexisting_expression_error_does_not_block_deleting_an_unused_parameter() {
    let mut s = Session::new();
    run(&mut s, "parameters", json!({"name":"unused","value":10}));
    s.doc_mut().unwrap().parametric.parameters.push(kition_cad_doc::Parameter {
        name: "broken".into(),
        expr: "missing + 1".into(),
        description: String::new(),
    });
    run(&mut s, "parameters", json!({"delete":"unused"}));
    let p = &s.doc().unwrap().parametric.parameters;
    assert_eq!(p.len(), 1);
    assert_eq!((p[0].name.as_str(), p[0].expr.as_str()), ("broken", "missing + 1"));
}

#[test]
fn text_dimoverride_counts_only_dimensions_it_changes() {
    let mut s = Session::new();
    let line = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    let dim = run(&mut s, "dimlinear", json!({"p1":[0,0],"p2":[10,0],"at":[5,2]}))["handle"].clone();
    assert_eq!(run(&mut s, "dimoverride", json!({"handles":[line,dim],"text":"overridden"}))["changed"], 1);
}

#[test]
fn text_dimoverride_propagates_missing_target_errors_without_partial_changes() {
    let mut s = Session::new();
    let dim = run(&mut s, "dimlinear", json!({"p1":[0,0],"p2":[10,0],"at":[5,2]}))["handle"].clone();
    let before = stored(&s);
    assert!(s.execute("dimoverride", &json!({"handles":[dim,"FFFFFF"],"text":"overridden"})).is_err());
    assert_eq!(stored(&s), before);
}

#[test]
fn arc_continue_never_discards_a_partial_explicit_start_or_direction() {
    let mut s = Session::new();
    run(&mut s, "line", json!({"points":[[0,0],[10,0]]}));
    for p in [json!({"start":[100,100],"end":[15,5]}), json!({"direction":90,"end":[15,5]})] {
        let before = stored(&s);
        assert!(s.execute("arc.continue", &p).is_err(), "{p}");
        assert_eq!(stored(&s), before);
    }
}

#[test]
fn group_never_silently_drops_a_nonexistent_named_member() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    let before = stored(&s);
    assert!(s.execute("group", &json!({"name":"G","handles":[h,"FFFFFF"]})).is_err());
    assert_eq!(stored(&s), before);
}

#[test]
fn layer_delete_reports_a_missing_named_layer() {
    let mut s = Session::new();
    let before = stored(&s);
    assert!(s.execute("layer.delete", &json!({"name":"Nope"})).is_err());
    assert_eq!(stored(&s), before);
}

#[test]
fn properties_reject_an_unloaded_linetype_without_changing_the_target() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[10,0]]}))["handles"][0].clone();
    let before = stored(&s);
    assert!(s.execute("properties.set", &json!({"handles":[h],"linetype":"NOPE"})).is_err());
    assert_eq!(stored(&s), before);
    run(&mut s, "properties.set", json!({"handles":[h],"linetype":"ByBlock"}));
}

#[test]
fn multileader_styles_reject_a_missing_text_style_without_creating_the_record() {
    let mut s = Session::new();
    let before = stored(&s);
    assert!(s.execute("mleaderstyle", &json!({"name":"M","textStyle":"Nope"})).is_err());
    assert_eq!(stored(&s), before);
}

#[test]
fn a_third_constraint_slot_never_silently_reuses_the_first_object() {
    let mut s = Session::new();
    let h1 = run(&mut s, "line", json!({"points":[[0,-10],[0,10]]}))["handles"][0].clone();
    let h2 = run(&mut s, "line", json!({"points":[[2,0],[0,-10]]}))["handles"][0].clone();
    for bad_h3 in [None, Some(json!("zz"))] {
        let mut p = json!({"h1":h1,"p1":"start","h2":h2,"p2":"end","p3":"object"});
        if let Some(v) = bad_h3 {
            p["h3"] = v;
        }
        let before = stored(&s);
        assert!(s.execute("gcsymmetric", &p).is_err(), "{p}");
        assert_eq!(stored(&s), before);
    }
}

#[test]
fn a_malformed_constraint_handle_never_reuses_the_first_object() {
    let mut s = Session::new();
    let h = run(&mut s, "line", json!({"points":[[0,0],[0,10]]}))["handles"][0].clone();
    let before = stored(&s);
    assert!(s.execute("gchorizontal", &json!({"h1":h,"h2":"zz","p2":"end"})).is_err());
    assert_eq!(stored(&s), before);
}
