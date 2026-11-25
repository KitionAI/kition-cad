//! Every command says what it does and every read command an agent is offered says what it
//! answers (kition-sdk `testing::check_commands`, KHP A1, A5).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;

/// Read commands an agent is offered that answer nothing worth a schema, with why.
const NO_OUTPUT: &[(&str, &str)] = &[];

fn doc() -> CadDocument {
    CadDocument::create("Drawing.dxf", 800, 600).unwrap()
}

#[test]
fn every_command_says_what_it_does() {
    let commands = doc().commands();
    if let Ok(path) = std::env::var("KITION_COMMANDS_DUMP") {
        std::fs::write(path, serde_json::to_string(&commands).unwrap()).unwrap();
    }
    if let Err(problems) = kition_kit::testing::check_commands(&commands, NO_OUTPUT) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// With `KITION_COMMANDS_OBSERVE` set to a file, runs every offered read command with no
/// parameters on the conformance drawing (everything selected) and writes what each answered:
/// the evidence the output schemas in `agent-commands.json` are written from.
#[test]
fn observe_read_commands() {
    let Ok(path) = std::env::var("KITION_COMMANDS_OBSERVE") else { return };
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.dxf")).unwrap();
    let mut seen = serde_json::Map::new();
    let reads: Vec<String> =
        CadDocument::open("basic.dxf", &bytes).unwrap().commands().into_iter().filter(|c| c.schema.is_some() && !c.mutates).map(|c| c.id).collect();
    for id in reads {
        let mut doc = CadDocument::open("basic.dxf", &bytes).unwrap();
        let _ = doc.execute("edit.selectAll", serde_json::json!({}));
        let answer = match doc.execute(&id, serde_json::json!({})) {
            Ok(value) => value,
            Err(error) => serde_json::json!({ "__error": error }),
        };
        seen.insert(id, answer);
    }
    std::fs::write(path, serde_json::to_string(&seen).unwrap()).unwrap();
}

/// Every entry of `agent-commands.json` names a command, types only parameters the command's
/// notation has, and gives output schemas the SDK accepts.
#[test]
fn agent_text_names_real_commands_and_parameters() {
    let table: serde_json::Map<String, serde_json::Value> = serde_json::from_str(kition_cad_kit::agent_text::SOURCE).unwrap();
    let notation: std::collections::HashMap<String, Option<serde_json::Value>> = doc()
        .commands()
        .into_iter()
        .map(|c| (c.id.clone(), c.params.as_deref().and_then(|p| kition_kit::schema::from_kit_params(p).ok()).map(|c| c.schema)))
        .collect();
    let mut problems = Vec::new();
    for (id, entry) in &table {
        let Some(schema) = notation.get(id) else {
            problems.push(format!("{id}: not a command"));
            continue;
        };
        for key in entry.get("params").and_then(|p| p.as_object()).map(|p| p.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
            if schema.as_ref().and_then(|s| s.get(&key)).is_none() {
                problems.push(format!("{id}: types `{key}`, which its notation doesn't have"));
            }
        }
        if let Some(output) = entry.get("output")
            && let Err(error) = kition_kit::schema::validate_output(output)
        {
            problems.push(format!("{id}: output schema: {error}"));
        }
    }
    for command in doc().commands() {
        if let Some(schema) = &command.schema
            && let Err(error) = kition_kit::schema::validate(schema)
        {
            problems.push(format!("{}: parameter schema: {error}", command.id));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// What a read command answers on the conformance drawing fits the output schema it declares.
#[test]
fn read_commands_answer_what_their_output_schemas_say() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.dxf")).unwrap();
    let mut problems = Vec::new();
    for command in CadDocument::open("basic.dxf", &bytes).unwrap().commands() {
        let (Some(output), true) = (command.output_schema.as_ref(), command.schema.is_some() && !command.mutates) else { continue };
        let mut doc = CadDocument::open("basic.dxf", &bytes).unwrap();
        let _ = doc.execute("edit.selectAll", serde_json::json!({}));
        if let Ok(answer) = doc.execute(&command.id, serde_json::json!({}))
            && let Err(error) = fits(&answer, output, &command.id)
        {
            problems.push(error);
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn fits(value: &serde_json::Value, schema: &serde_json::Value, at: &str) -> Result<(), String> {
    use serde_json::Value;
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        return if branches.iter().any(|b| fits(value, b, at).is_ok()) { Ok(()) } else { Err(format!("{at}: {value} fits no branch")) };
    }
    let kind = schema.get("type").and_then(Value::as_str).unwrap_or("json");
    let ok = match kind {
        "json" => true,
        "null" => value.is_null(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    };
    if !ok {
        return Err(format!("{at}: expected {kind}, got {value}"));
    }
    if let (Some(items), Some(list)) = (schema.get("items"), value.as_array()) {
        for (i, item) in list.iter().enumerate() {
            fits(item, items, &format!("{at}[{i}]"))?;
        }
    }
    if let (Some(props), Some(object)) = (schema.get("properties").and_then(Value::as_object), value.as_object()) {
        for (key, inner) in props {
            if let Some(v) = object.get(key)
                && !v.is_null()
            {
                fits(v, inner, &format!("{at}.{key}"))?;
            }
        }
    }
    Ok(())
}

/// A drawing's layers are named, and every command that takes `layer` reads it as a name
/// (`str_param(p, "layer")`), so its schema says string: an integer schema refused every real layer
/// name and let a number through that the engine ignored (W2-27, `cad.agent.act/walls-layer`).
#[test]
fn a_layer_parameter_is_a_layer_name() {
    let mut wrong = Vec::new();
    for command in doc().commands() {
        let Some(schema) = &command.schema else { continue };
        if let Some(layer) = schema.get("layer")
            && layer.get("type").and_then(serde_json::Value::as_str) != Some("string")
        {
            wrong.push(format!("{}: {layer}", command.id));
        }
    }
    assert!(wrong.is_empty(), "layer is not a string in: {wrong:#?}");
}

/// `object.setProperties {layer}` moves the objects onto the named layer.
#[test]
fn properties_set_moves_objects_to_a_named_layer() {
    let mut doc = doc();
    doc.execute("line", serde_json::json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    let handle = doc.execute("object.query", serde_json::json!({})).unwrap()["object.query"][0]["handle"].clone();
    doc.execute("layer.add", serde_json::json!({ "name": "Walls" })).unwrap();
    doc.execute("object.setProperties", serde_json::json!({ "handles": [handle], "layer": "Walls" })).unwrap();
    assert_eq!(doc.execute("object.query", serde_json::json!({ "layer": "Walls" })).unwrap()["count"], 1);
}

/// At least 95% of the parameter keys agents are offered say their type (kit requirement AX14): an
/// untyped key leaves an agent guessing between a number, a string and a list.
#[test]
fn parameter_keys_say_their_type() {
    // `"type": "json"` is the notation's "any value": an agent sees no type.
    let typed = |node: &serde_json::Value| {
        node.get("type").is_some_and(|kind| kind != "json")
            || ["oneOf", "anyOf", "allOf", "enum", "const", "$ref"].iter().any(|key| node.get(key).is_some())
    };
    let mut total = 0usize;
    let mut untyped = Vec::new();
    for command in doc().commands() {
        let Some(serde_json::Value::Object(schema)) = &command.schema else { continue };
        for (name, node) in schema {
            total += 1;
            if !typed(node) {
                untyped.push(format!("{}.{name}", command.id));
            }
        }
    }
    let share = (total - untyped.len()) as f64 / total as f64;
    assert!(share >= 0.95, "{:.1}% of {total} parameter keys typed; untyped: {untyped:#?}", share * 100.0);
}

/// Parameters that take objects' handles say so; none is described as a colour.
#[test]
fn handle_parameters_are_not_colours() {
    let mut problems = Vec::new();
    for command in doc().commands() {
        let Some(schema) = &command.schema else { continue };
        for (key, node) in schema.as_object().into_iter().flatten() {
            let text = node.to_string();
            if text.contains("colour such as") && !key.to_ascii_lowercase().contains("colo") {
                problems.push(format!("{}.{key}: {text}", command.id));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    let dimspace = doc().commands().into_iter().find(|c| c.id == "dimension.space").unwrap().schema.unwrap();
    assert_eq!(dimspace["base"]["description"], "a handle");
}
