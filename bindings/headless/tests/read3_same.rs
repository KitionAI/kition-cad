//! C8-cad-read-3: same-field decision, with independent file writers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "read2/common.rs"]
mod common;
use common::*;
use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;
use serde_json::json;

async fn written_field(command: &'static str) {
    check(
        |f| match command {
            "text.edit" => (command, json!({"handle":f.text,"text":"Mine"})),
            "attribute.edit" => (command, json!({"handle":f.insert,"values":{"CODE":"Mine"}})),
            "plot.style" => (command, json!({"handles":[f.circle],"name":"ByLayer"})),
            "layer.changeToCurrent" => (command, json!({"handles":[f.circle]})),
            "object.matchProperties" => (command, json!({"source":f.line,"targets":[f.circle]})),
            "table.set" => (command, json!({"handle":f.table,"row":1,"col":1,"text":"Mine"})),
            _ => panic!("command"),
        },
        |f| match command {
            "text.edit" => vec![(command, json!({"handle":f.text,"text":"Colleague"}))],
            "attribute.edit" => vec![(command, json!({"handle":f.insert,"values":{"CODE":"Colleague"}}))],
            "plot.style" => vec![(command, json!({"handles":[f.circle],"name":"Normal"}))],
            "layer.changeToCurrent" => vec![("object.setProperties", json!({"handles":[f.circle],"layer":"Secondary"}))],
            "object.matchProperties" => vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
            "table.set" => vec![(command, json!({"handle":f.table,"row":1,"col":1,"text":"Colleague"}))],
            _ => panic!("command"),
        },
        |f| match command {
            "text.edit" => vec![("object.setProperties", json!({"handles":[f.text],"height":6}))],
            "attribute.edit" => vec![("move", json!({"handles":[f.insert],"delta":[1,0]}))],
            "table.set" => vec![(command, json!({"handle":f.table,"row":2,"col":2,"text":"Kept"}))],
            _ => vec![("object.setProperties", json!({"handles":[f.circle],"radius":7}))],
        },
    )
    .await;
}

macro_rules! probe {
    ($name:ident, $command:literal) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            written_field($command).await;
        }
    };
}
probe!(textedit_same_field, "text.edit");
probe!(attribute_same_field, "attribute.edit");
probe!(plotstyle_same_field, "plot.style");
probe!(layer_assignment_same_field, "layer.changeToCurrent");
probe!(matchprop_same_field, "object.matchProperties");
probe!(table_text_same_field, "table.set");

async fn current_setting(command: &'static str) {
    check(
        |f| match command {
            "attribute.display" => (command, json!({"mode":"on"})),
            "layer.setCurrent" => (command, json!({"name":"Editable"})),
            _ => (command, json!({"handles":[f.line]})),
        },
        |_| match command {
            "attribute.display" => vec![("attribute.display", json!({"mode":"off"}))],
            _ => vec![("layer.setCurrent", json!({"name":"Secondary"}))],
        },
        |f| vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))],
    )
    .await;
}
macro_rules! current_probe {
    ($name:ident,$command:literal) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            current_setting($command).await;
        }
    };
}
current_probe!(attribute_display_same_setting, "attribute.display");

/// The current layer is each person's own: after a colleague made Secondary their current layer
/// (or made an unrelated change), my command setting my current layer applies, and the saved file
/// is the colleague's with my command run on it (my current layer in it).
async fn my_current_layer(command: &'static str) {
    for colleague_current in [true, false] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        let colleague = if colleague_current {
            vec![("layer.setCurrent", json!({"name":"Secondary"}))]
        } else {
            vec![("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))]
        };
        f.colleague(&colleague);
        let before = f.disk();
        let params = match command {
            "layer.setCurrent" => json!({"name":"Editable"}),
            _ => json!({"handles":[f.line]}),
        };
        let mut oracle = CadDocument::open(FILE, &before).unwrap();
        oracle.execute(command, params.clone()).unwrap_or_else(|e| panic!("oracle {command}: {e}"));
        let expected = kition_cad_io::read(&oracle.save(Some(FILE)).unwrap(), FILE).unwrap();
        f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]}))
            .await
            .unwrap_or_else(|e| panic!("{command} after {colleague:?} must apply: {e}"));
        let actual = kition_cad_io::read(&f.disk(), FILE).unwrap();
        assert_eq!(content(&actual), content(&expected), "{command}: saved result");
        assert_eq!(actual.header.str("CLAYER", "?"), "Editable", "{command}: my current layer is saved");
        f.kits.shutdown().await;
    }
}
macro_rules! my_current_layer_probe {
    ($name:ident,$command:literal) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            my_current_layer($command).await;
        }
    };
}
my_current_layer_probe!(current_layer_is_my_own_setting, "layer.setCurrent");
my_current_layer_probe!(make_layer_current_is_my_own_setting, "layer.makeObjectCurrent");
my_current_layer_probe!(isolation_current_layer_is_my_own_setting, "layer.isolate");

/// `own`: the variable is each person's own (kdm_model::is_current_var), so a colleague's value is
/// theirs and my write applies; otherwise it is the drawing's, and replacing it holds the value I
/// observed.
async fn injected_header(command: &str, variable: &str, code: &str, value: &str, params: serde_json::Value, own: bool) {
    for relevant in [false, true] {
        let f = Fixture::new();
        let basis = f.call("inspect", json!({})).await.unwrap()["revision"].clone();
        if relevant {
            // The writer does not round-trip these header variables. A separate file
            // writer supplies the real header instead; verify it survived parsing.
            let before = String::from_utf8(f.disk()).unwrap();
            let mut lines: Vec<_> = before.lines().collect();
            let at = lines.iter().position(|line| line.trim() == "ENDSEC").unwrap() - 1;
            let key = format!("${variable}");
            lines.splice(at..at, ["9", key.as_str(), code, value]);
            let after = lines.join("\n") + "\n";
            assert_ne!(before, after);
            std::fs::write(f.path(), after).unwrap();
            let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
            assert!(d.header.get(variable).is_some(), "{variable} is a real saved colleague change");
        } else {
            f.colleague(&[("object.setProperties", json!({"handles":[f.circle],"color":"blue"}))]);
        }
        let before = f.disk();
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await;
        if relevant && !own {
            assert_eq!(answer.err().map(|e| e["error"]["code"].clone()), Some(json!("conflict")));
            assert_eq!(f.disk(), before);
        } else {
            answer.unwrap();
        }
        if relevant && own {
            let d = kition_cad_io::read(&f.disk(), FILE).unwrap();
            let colleague: f64 = value.parse().unwrap();
            assert_ne!(
                d.header.get(variable).and_then(kition_cad_doc::HVal::as_f64),
                Some(colleague),
                "the file never carries the colleague's {variable}"
            );
        }
        f.kits.shutdown().await;
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn injected_circle_radius_is_my_own_setting() {
    injected_header("circle.addCenterDiameter", "CIRCLERAD", "40", "6", json!({"center":[750,0],"diameter":10}), true).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn injected_wipeout_frame_same_setting() {
    injected_header("wipeout", "WIPEOUTFRAME", "70", "1", json!({"frames":"off"}), false).await;
}
