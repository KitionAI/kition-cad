//! C8-land-11-cad: the complete tool-id model before save equals a reopen, after each write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_kit::{KitDocument, session::model::Exchange};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn snapshot(doc: &mut CadDocument) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut pending = vec!["root".to_owned()];
    let mut seen = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    while let Some(target) = pending.pop() {
        if !seen.insert(target.clone()) {
            continue;
        }
        let result = Exchange::new().batch(doc, &json!({"ops":[],"load":[{"target":target}]}), false).unwrap();
        let object = &result["loaded"][0];
        let fields = object["fields"].clone();
        match object["type"].as_str().unwrap() {
            "Drawing" => {
                for key in ["model", "constraints"] {
                    pending.extend(fields[key].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned));
                }
                for key in [
                    "layers",
                    "linetypes",
                    "textStyles",
                    "dimStyles",
                    "mleaderStyles",
                    "tableStyles",
                    "views",
                    "ucss",
                    "layerStates",
                    "groups",
                    "plotStyleTables",
                    "blocks",
                    "layouts",
                ] {
                    pending.extend(fields[key].as_object().into_iter().flat_map(|m| m.values()).filter_map(Value::as_str).map(str::to_owned));
                }
            }
            "Block" | "Layout" => pending.extend(fields["entities"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned)),
            _ => {}
        }
        let id = object["id"].as_str().unwrap().to_owned();
        aliases.insert(target, id.clone());
        out.insert(id, fields);
    }
    fn pair(value: &mut Value, aliases: &BTreeMap<String, String>) {
        match value {
            Value::String(s) => {
                if let Some(id) = aliases.get(s) {
                    *s = id.clone();
                }
            }
            Value::Object(m) => m.values_mut().for_each(|v| pair(v, aliases)),
            Value::Array(a) => a.iter_mut().for_each(|v| pair(v, aliases)),
            _ => {}
        }
    }
    out.values_mut().for_each(|v| pair(v, &aliases));
    out
}

#[test]
fn representative_writes_equal_the_saved_fixture_model() {
    let files: [(&str, &[u8]); 3] = [
        ("act", include_bytes!("../../../cases/cad.agent.act/walls-layer/input/plan.dxf")),
        ("context", include_bytes!("../../../cases/cad.agent.context/two-spaces-one-run/input/plan.dxf")),
        ("observe", include_bytes!("../../../cases/cad.agent.observe/dim-bottom/input/plan.dxf")),
    ];
    for (fixture, bytes) in files {
        let mut doc = CadDocument::open("Plan.dxf", bytes).unwrap();
        let line = doc.execute("line.add", json!({"points":[[0.123456789123,0],[10.987654321987,20.1]]})).unwrap()["handles"][0].clone();
        let circle = doc.execute("circle.add", json!({"center":[13.123456789123,44.5],"radius":std::f64::consts::PI})).unwrap()["handle"].clone();
        let text = doc.execute("text.add", json!({"at":[10,30],"text":"original","height":2.123456789123})).unwrap()["handle"].clone();
        let table = doc.execute("table.add", json!({"at":[200,0],"rows":3,"cols":2})).unwrap()["handle"].clone();
        for (command, params) in [
            ("object.setProperties", json!({"handles":[line],"lineweight":0.123456789,"transparency":23,"color":"red"})),
            ("object.move", json!({"handles":[line],"delta":[0.123456789123,0.234567891234]})),
            ("object.rotate", json!({"handles":[line],"base":[0,0],"angle":13.123456789123})),
            ("object.setProperties", json!({"handles":[circle],"radius":std::f64::consts::E})),
            ("text.edit", json!({"handle":text,"text":"saved text"})),
            ("table.set", json!({"handle":table,"row":1,"col":0,"text":"cell"})),
            ("table.insertRow", json!({"handle":table,"row":1})),
            ("layer.add", json!({"name":"ReceiptLayer","color":"blue"})),
        ] {
            doc.execute(command, params).unwrap_or_else(|e| panic!("{fixture} {command}: {e}"));
            let memory = snapshot(&mut doc);
            let mut reopened = CadDocument::open("Plan.dxf", &doc.save(Some("Plan.dxf")).unwrap()).unwrap();
            let disk = snapshot(&mut reopened);
            for (id, fields) in &memory {
                assert_eq!(disk.get(id), Some(fields), "{fixture} {command} object {id}");
            }
            assert_eq!(memory.len(), disk.len(), "{fixture} {command}: all objects preserved");
        }
    }
}

#[test]
fn model_operations_preserve_the_reported_values_on_damaged_fixtures() {
    for file in ["check-missing-table-entry.dxf", "check-missing-linetype.dxf", "check-missing-current.dxf"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance").join(file);
        let bytes = std::fs::read(path).unwrap();
        let mut doc = CadDocument::open(file, &bytes).unwrap();
        let root = Exchange::new().batch(&mut doc, &json!({"ops":[],"load":[{"target":"root","fields":["model"]}]}), false).unwrap();
        let count = root["loaded"][0]["fields"]["model"].as_array().unwrap().len();
        let op = if count > 1 {
            json!({"op":"list.move","target":"root","field":"model","from":0,"to":1})
        } else {
            json!({"op":"delete","target":root["loaded"][0]["fields"]["model"][0]})
        };
        Exchange::new().batch(&mut doc, &json!({"ops":[op]}), false).unwrap();
        let memory = snapshot(&mut doc);
        let mut disk = CadDocument::open(file, &doc.save(Some(file)).unwrap()).unwrap();
        assert_eq!(snapshot(&mut disk), memory, "{file}: receipt values must survive reopening");
    }
}

#[test]
fn generated_header_caches_cannot_be_written_by_a_model_batch() {
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let before = doc.save(Some("Plan.dxf")).unwrap();
    let version = doc.version().unwrap();
    for key in ["EXTMIN", "EXTMAX", "HANDSEED"] {
        let result = Exchange::new().batch(
            &mut doc,
            &json!({"ops":[{"op":"map.set","target":"root","field":"header","key":key,"value":"not stored"}]}),
            false,
        );
        assert!(result.is_err(), "{key} is regenerated by the DXF writer, not stored as supplied");
        assert_eq!(doc.version().unwrap(), version);
        assert_eq!(doc.save(Some("Plan.dxf")).unwrap(), before);
    }
}
