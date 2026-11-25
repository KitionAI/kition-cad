//! C8-land-11-cad: audit the command resolvers, including string handles and names.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_kit::{KitCommand, KitDocument};

fn commands() -> Vec<KitCommand> {
    CadDocument::create("Plan.dxf", 420, 297).unwrap().commands()
}

#[test]
fn id_parameters_are_marked_and_typed() {
    let commands = commands();
    kition_kit::testing::check_id_params(&commands, &[]).unwrap();
    let mut gaps = Vec::new();
    let mut count = 0;
    for c in &commands {
        for key in c.schema.as_ref().and_then(|s| s.as_object()).into_iter().flatten().map(|(k, _)| k.as_str()) {
            // All of these are entity handles in their implementations; no coordinates.
            let opaque = (c.id == "constraint.delete" || c.id.starts_with("constraint.bars") || c.id.starts_with("constraint.dynamicDimensions"))
                && matches!(key, "handle" | "handles")
                || c.id == "group.edit" && key == "remove";
            if opaque {
                assert_ne!(c.targets, Some(key), "{} {key} is membership data", c.id);
                assert!(!c.objects.contains(&key), "{} {key} must remain opaque", c.id);
                assert!(!c.object_types.iter().any(|(field, _)| *field == key), "{} {key} must not resolve an entity", c.id);
            }
            let entity = !opaque
                && (matches!(key, "handle" | "handles" | "h1" | "h2" | "h3" | "baseHandle" | "handles2")
                    || matches!(
                        (c.id.as_str(), key),
                        ("object.matchProperties", "source" | "targets")
                            | ("object.arrayPath", "path")
                            | ("object.changeSpace", "viewport")
                            | ("trim" | "extend", "edges")
                            | ("group.edit", "add")
                            | ("dim" | "dimension.addLinear" | "spline", "object")
                            | ("dimension.addAngular", "lines" | "arc")
                            | ("dimension.space", "base")
                            | ("object.measure", "bounds" | "intersections")
                    ));
            if entity {
                count += 1;
                if c.targets != Some(key) && !c.objects.contains(&key) {
                    gaps.push(format!("{} {key}: unmarked", c.id));
                }
                if !c.object_types.contains(&(key, &["Entity"][..])) {
                    gaps.push(format!("{} {key}: wrong type", c.id));
                }
            }
        }
    }
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
    println!("{count} entity parameters marked and typed; SDK scanner allowed: 0");
}

fn name_gaps(commands: &[KitCommand]) -> Vec<String> {
    // Resolver audit: source names only, never proposed new names, font names, patterns,
    // system variables, attribute tags, or enum words.
    let wanted = [
        ("layer.set", "name", "Layer"),
        ("layer.setCurrent", "name", "Layer"),
        ("layer.delete", "name", "Layer"),
        ("layer.match", "layer", "Layer"),
        ("object.setProperties", "dimStyle", "DimStyle"),
        ("textStyle.setCurrent", "name", "TextStyle"),
        ("textStyle.delete", "name", "TextStyle"),
        ("textStyle.rename", "from", "TextStyle"),
        ("dimensionStyle.setCurrent", "name", "DimStyle"),
        ("dimensionStyle.delete", "name", "DimStyle"),
        ("dimensionStyle.rename", "from", "DimStyle"),
        ("insert", "name", "Block"),
        ("attribute.manage", "name", "Block"),
        ("attribute.sync", "name", "Block"),
        ("divide", "block", "Block"),
        ("measure", "block", "Block"),
        ("group.edit", "name", "Group"),
        ("ungroup", "name", "Group"),
        ("layout.set", "name", "Layout"),
        ("layout.delete", "name", "Layout"),
        ("layout.rename", "from", "Layout"),
        ("layout.duplicate", "from", "Layout"),
        ("layerState.restore", "name", "LayerState"),
        ("layerState.delete", "name", "LayerState"),
        ("layerState.rename", "from", "LayerState"),
        ("multilineText.add", "style", "TextStyle"),
        ("multileaderStyle.open", "textStyle", "TextStyle"),
    ];
    let mut gaps = Vec::new();
    for (id, key, ty) in wanted {
        let c = commands.iter().find(|c| c.id == id).unwrap();
        if !(c.name_params.contains(&(key, "name")) || c.targets == Some(key) && c.name_field == Some("name"))
            || !c.object_types.contains(&(key, &[ty][..]))
        {
            gaps.push(format!("{id} {key}"));
        }
    }
    gaps
}

#[test]
fn name_parameters_are_declared_by_the_type_the_resolver_reads() {
    let commands = commands();
    let gaps = name_gaps(&commands);
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
    println!("{} non-target name declarations", commands.iter().map(|c| c.name_params.len()).sum::<usize>());
    let mut missing = commands;
    missing.iter_mut().find(|c| c.id == "layer.set").unwrap().name_params = &[];
    assert_eq!(name_gaps(&missing), ["layer.set name"], "scanner detects a removed declaration");
}

#[test]
fn a_table_cell_is_its_own_object_whose_tool_id_survives_a_reopen() {
    use kition_kit::session::model::Exchange;
    use serde_json::json;
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let table = doc.execute("table", json!({"at":[0,0],"rows":2,"cols":2,"cells":[["a","b"],["c","d"]]})).unwrap()["handle"].clone();
    // The table numbers its cells 1, 2, … row by row: the first cell of the second row is 3.
    let id = format!("cell:{}-3", table.as_str().unwrap());
    for saved in [false, true] {
        if saved {
            doc = CadDocument::open("Plan.dxf", &doc.save(Some("Plan.dxf")).unwrap()).unwrap();
        }
        let answer = Exchange::new().batch(&mut doc, &json!({"ops":[],"load":[{"target":id}]}), false).unwrap();
        assert_eq!(answer["loaded"][0]["id"], id, "{answer}");
        assert_eq!(answer["loaded"][0]["fields"]["text"], "c", "{answer}");
    }
}

#[test]
fn every_named_table_has_a_tool_id_that_survives_a_reopen() {
    let schema: serde_json::Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    for ty in [
        "Layer",
        "Linetype",
        "TextStyle",
        "DimStyle",
        "MLeaderStyle",
        "TableStyle",
        "View",
        "Ucs",
        "LayerState",
        "Group",
        "PlotStyleTable",
        "Block",
        "Layout",
    ] {
        assert_eq!(schema["types"][ty]["toolId"]["field"], "id", "{ty}");
    }
    use kition_kit::session::model::Exchange;
    use serde_json::json;
    let mut doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let line = doc.execute("line", json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].clone();
    doc.execute("constraint.horizontal", json!({"h1":line})).unwrap();
    let ids = ["linetype:Continuous", "textstyle:Standard", "dimstyle:Standard", "mleaderstyle:Standard", "tablestyle:Standard", "constraint:1"];
    for saved in [false, true] {
        if saved {
            doc = CadDocument::open("Plan.dxf", &doc.save(Some("Plan.dxf")).unwrap()).unwrap();
        }
        for id in ids {
            let answer = Exchange::new().batch(&mut doc, &json!({"ops":[],"load":[{"target":id}]}), false).unwrap();
            assert_eq!(answer["loaded"][0]["id"], id, "{answer}");
        }
    }
}
