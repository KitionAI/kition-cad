//! C8-cad-pin: behavioral witnesses for corrections found in the remaining blind rows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read4/common.rs"]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn new_layers_hold_the_observed_name_gap() {
    check(
        |_| ("layer.new", json!({"name":"Taken"})),
        |_| vec![("layer.new", json!({"name":"Taken"}))],
        |_| vec![("layer.new", json!({"name":"Other"}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn group_creation_and_rename_hold_the_destination_gap() {
    for rename in [false, true] {
        check(
            |f| if rename { ("groupedit", json!({"name":"Group","rename":"Taken"})) } else { ("group", json!({"name":"Taken","handles":[f.line]})) },
            |f| vec![("group", json!({"name":"Taken","handles":[f.circle]}))],
            |f| vec![("group", json!({"name":"Other","handles":[f.circle]}))],
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn record_renames_hold_destination_names_within_their_type() {
    for command in ["layer.set", "layout.rename", "style.rename", "dimstyle.rename", "layerstate.rename"] {
        check_with_setup(
            |_| match command {
                "layer.set" => (command, json!({"name":"Editable","newName":"Taken"})),
                "layout.rename" => (command, json!({"from":"Layout1","to":"Taken"})),
                _ => (command, json!({"from":"Source","to":"Taken"})),
            },
            |_| {
                vec![match command {
                    "layer.set" => ("layer.new", json!({"name":"Taken"})),
                    "layout.rename" => ("layout.new", json!({"name":"Taken","viewport":false})),
                    "style.rename" => ("style", json!({"name":"Taken","current":false})),
                    "dimstyle.rename" => ("dimstyle", json!({"name":"Taken","current":false})),
                    _ => ("layerstate.save", json!({"name":"Taken"})),
                }]
            },
            |_| vec![("layer.new", json!({"name":"Other"}))],
            |_| match command {
                "style.rename" => vec![("style", json!({"name":"Source","current":false}))],
                "dimstyle.rename" => vec![("dimstyle", json!({"name":"Source","current":false}))],
                "layerstate.rename" => vec![("layerstate.save", json!({"name":"Source"}))],
                _ => vec![],
            },
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_names_hold_the_first_free_candidate() {
    check(
        |_| ("layout.new", json!({})),
        |_| vec![("layout.rename", json!({"from":"Layout1","to":"Renamed"}))],
        |_| vec![("groupedit", json!({"name":"Group","rename":"Other"}))],
    )
    .await;
    check(
        |f| ("group", json!({"handles":[f.line]})),
        |f| vec![("group", json!({"handles":[f.circle]}))],
        |f| vec![("group", json!({"name":"Other","handles":[f.circle]}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn command_line_assignments_hold_the_same_field() {
    for text in ["ltscale {\"scale\":2}", "LTS {\"scale\":2}"] {
        check(|_| ("commandLine", json!({"text":text})), |_| vec![("ltscale", json!({"scale":3}))], |_| vec![("units", json!({"insunits":1}))]).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_clipboard_does_not_consume_drawing_defaults() {
    let f = Fixture::new();
    let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
    f.colleague(&[("color", json!({"color":"red"}))]);
    let saved = f.disk();
    let error = f.call("run", json!({"basis":basis,"steps":[{"command":"pasteblock","params":{"at":[0,0]}}]})).await.unwrap_err();
    assert_eq!(error["error"]["code"], "command-failed", "{error}");
    assert!(error["error"]["message"].as_str().unwrap().contains("clipboard is empty"), "{error}");
    assert_eq!(saved, f.disk());
    f.kits.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn radius_is_assigned_even_by_a_transform_with_unit_scale() {
    for command in ["move", "grip.move", "grip.rotate", "grip.mirror"] {
        check(
            |f| {
                (
                    command,
                    match command {
                        "move" => json!({"handles":[f.circle],"delta":[1,0]}),
                        "grip.move" => json!({"handles":[f.circle],"base":[100,100],"to":[101,100],"mode":"move"}),
                        "grip.rotate" => json!({"handles":[f.circle],"base":[0,0],"angle":30}),
                        _ => json!({"handles":[f.circle],"base":[0,0],"to":[0,10]}),
                    },
                )
            },
            |f| vec![("properties.set", json!({"handles":[f.circle],"radius":8}))],
            |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_layer_histories_do_not_consume_layer_values() {
    for command in ["layerp", "layuniso"] {
        let f = Fixture::new();
        let mut native = kition_cad_kit::CadDocument::open(FILE, &f.disk()).unwrap();
        use kition_kit::KitDocument;
        assert!(native.execute(command, json!({})).is_err(), "fixture must lack the saved layer history");
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        f.colleague(&[("layer.set", json!({"name":"Editable","color":"red"}))]);
        let before = f.disk();
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command}]})).await.unwrap_err();
        assert_eq!(answer["error"]["code"], "command-failed", "{answer}");
        assert_eq!(before, f.disk());
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_circle_cannot_infer_a_constraint() {
    check(
        |f| ("autoconstrain", json!({"handles":[f.circle]})),
        |f| vec![("erase", json!({"handles":[f.circle]}))],
        |f| vec![("properties.set", json!({"handles":[f.circle],"radius":8}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn display_exception_handles_are_opaque_membership_ids() {
    for command in ["constraintbar", "dcdisplay"] {
        check(
            |f| (command, json!({"mode":"show","handles":[f.circle]})),
            |f| vec![(command, json!({"mode":"hide","handles":[f.circle]}))],
            |f| vec![("erase", json!({"handles":[f.circle]}))],
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_line_without_parameters_holds_its_native_snapshot() {
    check(
        |_| ("commandLine", json!({"text":"layon"})),
        |_| vec![("layer.set", json!({"name":"Editable","color":"blue"}))],
        |f| vec![("move", json!({"handles":[f.circle],"delta":[1,0]}))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn find_then_first_recipient_edit_holds_match_order() {
    for outside in [false, true] {
        for relevant in [false, true] {
            let f = Fixture::new();
            f.colleague(&[("text", json!({"at":[200,30],"text":"Before second"}))]);
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            let params = json!({"handles":[if relevant { &f.text } else { &f.circle }]});
            if outside {
                f.colleague(&[("draworder.front", params)]);
            } else {
                kition_kits::tools::call_as(
                    &f.kits,
                    "cad_run",
                    &json!({"path":FILE,"steps":[{"command":"draworder.front","params":params}]}),
                    kition_kits::tools::CallOptions::default(),
                    "colleague",
                )
                .await
                .unwrap()
                .unwrap();
            }
            let before = f.disk();
            let answer = f
                .call(
                    "run",
                    json!({"basis":basis,"steps":[{"command":"find","params":{"find":"Before"}},{"command":"textedit","params":{"text":"Mine"}}]}),
                )
                .await;
            if relevant {
                let e = answer.expect_err("reordering matching texts changes the first recipient");
                assert_eq!(e["error"]["code"], "conflict", "{e}");
                assert!(e["conflicts"].as_array().unwrap().iter().any(|c| c["reason"] == "relocated"), "{e}");
                assert_eq!(before, f.disk());
            } else {
                answer.unwrap();
                use kition_kit::KitDocument;
                let mut actual = kition_cad_kit::CadDocument::open(FILE, &f.disk()).unwrap();
                let p = actual.execute("properties", json!({"handles":[f.text]})).unwrap();
                assert_eq!(p["objects"][0]["geometry"]["value"], "Mine", "{p}");
            }
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_close_has_no_document_or_save_and_stops_the_run() {
    for command in ["close", "closeall"] {
        for guard in ["premise", "off"] {
            let f = Fixture::new();
            let before = f.disk();
            let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
            // Keep the single-step guarded close and the unguarded suffix controls.
            // The guarded suffix is covered separately below.
            let mut steps = vec![json!({"command":command,"params":{}})];
            if guard == "off" {
                steps.push(json!({"command":"color","params":{"color":"red"}}));
            }
            let result = kition_kits::tools::call_as(
                &f.kits,
                "cad_run",
                &json!({"path":FILE,"basis":basis,"steps":steps}),
                kition_kits::tools::CallOptions { guard: kition_kits::tools::Guard::parse(guard).unwrap(), ..Default::default() },
                "agent-me",
            )
            .await
            .unwrap()
            .map(|r| r.output)
            .unwrap_or_else(|e| panic!("{command}, guard={guard}: {e:?}"));
            assert_eq!(result["closed"], true, "{result}");
            assert_eq!(result["saved"], false, "{result}");
            assert!(result.get("document").is_none(), "{result}");
            assert_eq!(result["completed"].as_array().unwrap().len(), 1, "{result}");
            if guard == "off" {
                assert_eq!(result["notRun"], 1, "{result}");
            }
            assert_eq!(before, f.disk());
            f.kits.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn guarded_planning_stops_after_terminal_close() {
    for command in ["close", "closeall"] {
        let f = Fixture::new();
        let before = f.disk();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let result = f
            .call("run", json!({"basis":basis,"steps":[{"command":command,"params":{}},{"command":"color","params":{"color":"red"}}]}))
            .await
            .unwrap();
        assert_eq!(result["closed"], true, "{result}");
        assert_eq!(result["saved"], false, "{result}");
        assert!(result.get("document").is_none(), "{result}");
        assert_eq!(result["completed"].as_array().unwrap().len(), 1, "{result}");
        assert_eq!(result["completed"][0]["command"], command, "{result}");
        assert_eq!(result["notRun"], 1, "{result}");
        assert_eq!(before, f.disk());
        f.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_display_exception_holds_its_live_recipient() {
    for command in ["constraintbar", "dcdisplay"] {
        check(
            |f| (command, json!({"mode":"hide","handles":[f.circle]})),
            |f| vec![("erase", json!({"handles":[f.circle]}))],
            |f| vec![("properties.set", json!({"handles":[f.circle],"color":"blue"}))],
        )
        .await;
    }
}
