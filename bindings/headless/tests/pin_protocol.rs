//! C8-cad-pin: replacement planning and explicit null content regions through the CAD backend.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_cad_kit::CadDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::json;

#[test]
fn null_content_regions_reach_the_cad_callback() {
    let root = tempfile::tempdir().unwrap();
    let doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    std::fs::write(root.path().join("Plan.dxf"), doc.save(Some("Plan.dxf")).unwrap()).unwrap();
    let folder = HostFolder::new(root.path()).unwrap();
    let mut kit = Headless::<CadDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_cad_kit::EDITOR_STATE).unwrap(),
    );
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("Plan.dxf").unwrap(), read_only: false, create: None }).unwrap();
    let mut params = json!({"id":"drawing","field":"kind.attribs","region":null,"before":null}).as_object().unwrap().clone();
    assert_eq!(kit.context_content(&opened.id, &params).unwrap(), json!({"changed":null}));
    params.remove("region");
    assert!(kit.context_content(&opened.id, &params).is_err(), "an absent region is invalid");
}

#[test]
fn replacement_plans_advance_without_changing_the_live_drawing() {
    let root = tempfile::tempdir().unwrap();
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    for x in [0, 20] {
        doc.execute("circle", json!({"center":[x,10],"radius":5})).unwrap();
    }
    std::fs::write(root.path().join("Plan.dxf"), doc.save(Some("Plan.dxf")).unwrap()).unwrap();
    let folder = HostFolder::new(root.path()).unwrap();
    let mut kit = Headless::<CadDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_cad_kit::EDITOR_STATE).unwrap(),
    );
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("Plan.dxf").unwrap(), read_only: false, create: None }).unwrap();
    let token = kit.context_plan(&opened.id, &json!({"action":"begin","steps":[]}).as_object().unwrap().clone()).unwrap()["token"].clone();
    let advanced = kit
        .context_plan(
            &opened.id,
            &json!({"action":"advance","token":token,"steps":[
                {"command":"new"},{"command":"line","params":{"points":[[0,0],[10,0]]}}
            ]})
            .as_object()
            .unwrap()
            .clone(),
        )
        .unwrap();
    assert!(!advanced["changes"]["created"].as_array().unwrap().is_empty(), "{advanced}");
    let request = json!({"ops":[],"load":[{"target":"root","fields":["model"]}]});
    let planned = kit
        .context_plan(&opened.id, &json!({"action":"read","token":token,"method":"model.batch","params":request}).as_object().unwrap().clone())
        .unwrap();
    assert_eq!(planned["loaded"][0]["fields"]["model"].as_array().unwrap().len(), 1);
    let live = kit.model("model.batch", json!({"documentId":opened.id,"ops":[],"load":[{"target":"root","fields":["model"]}]})).unwrap();
    assert_eq!(live["loaded"][0]["fields"]["model"].as_array().unwrap().len(), 2);
}
