//! The command line's data for the editor: the prompt taken apart, the history the person's
//! entries write (and an agent's calls do not), and the command catalogue, driven as the editor
//! drives them (`command.runLine`, the tool events).
use super::*;

fn blank() -> CadDocument {
    let mut doc = CadDocument::create("Line.dxf", 0, 0).unwrap();
    doc.execute("drafting.objectSnap", json!({ "on": false })).unwrap();
    doc
}

fn line(doc: &mut CadDocument, text: &str) -> Value {
    doc.execute("command.runLine", json!({ "text": text })).unwrap()
}

fn state(doc: &CadDocument) -> Value {
    doc.inspect().unwrap()["drafting"]["commandLine"].clone()
}

fn history(doc: &CadDocument) -> Vec<String> {
    state(doc)["history"]["lines"].as_array().unwrap().iter().map(|line| line.as_str().unwrap().to_owned()).collect()
}

fn serial(doc: &CadDocument) -> u64 {
    state(doc)["history"]["serial"].as_u64().unwrap()
}

/// DIST typed and answered: the history has the command, each entry beside its prompt and the
/// distance (B7); Escape in a command writes "*Cancel*".
#[test]
fn the_history_shows_what_commands_write() {
    let mut doc = blank();
    assert_eq!((history(&doc), serial(&doc)), (Vec::<String>::new(), 0));
    line(&mut doc, "dist");
    line(&mut doc, "0,0");
    line(&mut doc, "30,40");
    let lines = history(&doc);
    assert_eq!(lines.first().map(String::as_str), Some("Command: DIST"), "{lines:?}");
    assert!(lines.iter().any(|line| line.ends_with(": 0,0") && line.starts_with("DIST ")), "{lines:?}");
    assert!(lines.iter().any(|line| line.starts_with("Distance = 50")), "{lines:?}");
    assert_eq!(serial(&doc), lines.len() as u64);
    line(&mut doc, "line");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(history(&doc).last().map(String::as_str), Some("*Cancel*"), "{:?}", history(&doc));
}

/// The prompt taken apart (N5): the command, the message, the options and the default, and what
/// it asks for; between commands it is the command prompt.
#[test]
fn the_prompt_is_taken_apart() {
    let mut doc = blank();
    assert_eq!(state(&doc)["prompt"], json!({ "command": null, "message": "Command", "keywords": [], "default": null, "kind": "command" }));
    line(&mut doc, "line");
    line(&mut doc, "0,0");
    line(&mut doc, "10,0");
    assert_eq!(state(&doc)["prompt"]["keywords"], json!(["Undo"]), "one segment: nothing to close yet");
    line(&mut doc, "10,10");
    let prompt = &state(&doc)["prompt"];
    assert_eq!((prompt["command"].clone(), prompt["kind"].clone()), (json!("LINE"), json!("point")), "{prompt}");
    let keywords: Vec<&str> = prompt["keywords"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(keywords.contains(&"Close") && keywords.contains(&"Undo"), "{prompt}");
    assert!(doc.prompt().contains(prompt["message"].as_str().unwrap()), "{prompt} / {}", doc.prompt());
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    line(&mut doc, "circle");
    line(&mut doc, "0,0");
    assert_eq!(state(&doc)["prompt"]["kind"], json!("distance"), "the radius is one value");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    line(&mut doc, "erase");
    assert_eq!(state(&doc)["prompt"]["kind"], json!("select"));
}

/// An entry the running command refuses comes back `rejected`, with its reason on the history;
/// one it takes does not (the editor shakes the command line on a refusal).
#[test]
fn a_refused_entry_is_rejected() {
    let mut doc = blank();
    assert_eq!(line(&mut doc, "line")["rejected"], json!(false));
    assert_eq!(line(&mut doc, "0,0")["rejected"], json!(false));
    let before = serial(&doc);
    assert_eq!(line(&mut doc, "nowhere")["rejected"], json!(true));
    assert!(serial(&doc) >= before + 2, "the entry and the reason: {:?}", history(&doc));
    assert_eq!(doc.inspect().unwrap()["running"], json!("line.add"));
}

/// The history is the person's: an agent's commands write nothing there, and the editor state
/// they leave is the one they found.
#[test]
fn an_agents_calls_write_no_history() {
    let mut doc = blank();
    line(&mut doc, "circle");
    line(&mut doc, "0,0");
    line(&mut doc, "5");
    let before = state(&doc);
    doc.execute("object.fillet", json!({})).ok();
    doc.execute("line.add", json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    doc.execute("command.list", json!({})).unwrap();
    assert_eq!(state(&doc), before);
    line(&mut doc, "line");
    assert_eq!(history(&doc).last().map(String::as_str), Some("Command: LINE"), "{:?}", history(&doc));
}

/// A tool event writes what it starts or ends: the Line tool starts LINE, clicks answer it.
#[test]
fn tool_events_write_the_history() {
    let mut doc = blank();
    doc.select_tool("line.add", 1.0).unwrap();
    assert_eq!(history(&doc), vec!["Command: LINE".to_owned()]);
    KitTools::text(&mut doc, "0,0", 1.0).unwrap();
    assert_eq!(history(&doc).len(), 2, "{:?}", history(&doc));
}

/// The history keeps its last lines and counts every one.
#[test]
fn the_history_keeps_its_last_lines() {
    let mut doc = blank();
    for _ in 0..(command_line::HISTORY_LINES + 30) {
        doc.write_history("x");
    }
    assert_eq!(history(&doc).len(), command_line::HISTORY_LINES);
    assert_eq!(serial(&doc), (command_line::HISTORY_LINES + 30) as u64);
    doc.write_history(&"y".repeat(100_000));
    assert!(history(&doc).last().unwrap().len() <= 2000);
}

/// `command.list` (N6, B6): each command with the name typed for it, its aliases, title and menu,
/// whether it prompts and whether the command line may start it; `search` narrows it.
#[test]
fn the_catalogue_names_and_flags_the_commands() {
    let mut doc = blank();
    let all = doc.execute("command.list", json!({})).unwrap();
    let find = |id: &str| all["commands"].as_array().unwrap().iter().find(|c| c["id"] == id).cloned().unwrap_or_else(|| panic!("no {id}"));
    let line = find("line.add");
    assert_eq!((line["name"].clone(), line["interactive"].clone(), line["commandLine"].clone()), (json!("line"), json!(true), json!(true)));
    assert!(line["aliases"].as_array().unwrap().contains(&json!("l")), "{line}");
    assert_eq!(find("donut.add")["interactive"], json!(true));
    assert_eq!(find("file.saveAs")["commandLine"], json!(false));
    assert!(all["commands"].as_array().unwrap().len() > 250);
    let found = doc.execute("command.list", json!({ "search": "DONUT" })).unwrap();
    assert!(found["commands"].as_array().unwrap().iter().all(|c| c.to_string().to_ascii_lowercase().contains("donut")), "{found}");
    assert!(!found["commands"].as_array().unwrap().is_empty());
    assert!(!doc.commands().into_iter().find(|c| c.id == "command.list").unwrap().mutates);
}

/// B6: what a menu button for an interactive command sends (its typed name on the command line)
/// starts the prompt and draws nothing, where the button used to run the command's JSON form.
#[test]
fn issue_a_button_starts_the_prompt() {
    let mut doc = blank();
    let entities = doc.inspect().unwrap()["entities"].clone();
    line(&mut doc, "donut");
    assert_eq!(doc.inspect().unwrap()["running"], json!("donut.add"));
    assert_eq!(doc.inspect().unwrap()["entities"], entities);
    assert!(doc.prompt().starts_with("DONUT "), "{}", doc.prompt());
}
