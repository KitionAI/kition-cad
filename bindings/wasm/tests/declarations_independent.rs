//! The independent reading of every write command (task card C8-cad-read): for each of the 247
//! commands the kit offers that change the drawing, its JSON handler and the engine operation
//! behind it were read by readers who did not open the declarations (`src/place.rs`, `query.rs`,
//! `scan.rs`, `acts.rs`, `params.rs`, their tests, the guide). Each row is one code path of a
//! command on [`fixture`] (or [`constrained`]), written from that reading before any comparison:
//! the editor state the path reads (`active`: current properties in the header, `part`: the
//! current space, `selection`) and, for each object, the fields held (`*` every field and derived
//! value, `-` existence alone) and whether its place in its list matters (`*`) or not (`-`).
//! `~place` rows separately require historical query replay inputs. This marker never
//! discharges a stored geometry, text, bounds or field requirement on another object.
//!
//! What a write holds is computed as the SDK's guard computes it ([`held`]); the comparison is
//! mechanical. Symbolic parent paths retain absent/future members. A replay membership
//! witness is reported separately from geometry and model bounds; it cannot discharge
//! those facts merely by being present.
//!
//! The rule for every row is necessity: hold a fact iff another writer changing it between the
//! observation and the write changes what the write lands on (a positional or named address names
//! what was there) or a fact its result depends on. An insertion index or destination name
//! identifies the observed gap; a named address identifies the observed object. Assigned fields
//! retain their observed values even for absolute replacements. Reply-only formatting is not
//! a dependency of the written result.
//!
//! The original rows were written blind. Source-reviewed corrections are annotated below;
//! table.set was subsequently reviewed from its handler. Before this card
//! the declarations differed from the reading for 234 of the 247 commands.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::collections::{BTreeMap, BTreeSet};

use kition_cad_kit::CadDocument;
use kition_kit::{KitCommand, KitDocument};
use serde_json::{Value, json};

const FILE: &str = "Fixture.dxf";

fn run(doc: &mut CadDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn raw(doc: &mut CadDocument, edit: impl FnOnce(&mut kition_cad_doc::Drawing)) {
    let mut drawing = kition_cad_io::read(&doc.save(Some(FILE)).unwrap(), FILE).unwrap();
    edit(&mut drawing);
    *doc = CadDocument::open(FILE, &kition_cad_io::write(&drawing, FILE).unwrap()).unwrap();
}

fn hex(v: &Value) -> String {
    v.as_str().unwrap_or_else(|| panic!("not a handle: {v}")).to_owned()
}

pub fn fixture() -> (CadDocument, Vec<(&'static str, String)>) {
    let mut d = <CadDocument as KitDocument>::create(FILE, 420, 297).unwrap();
    run(&mut d, "layer.add", json!({"name":"Walls","color":1}));
    run(&mut d, "layer.add", json!({"name":"Notes","color":3}));
    run(&mut d, "linetype", json!({"load":"DASHED"}));
    run(&mut d, "style", json!({"name":"S","height":2,"current":false}));
    // A metric drawing starts on ISO-25; the readers were told the fixture's current dimension
    // style is Standard.
    run(&mut d, "dimensionStyle.setCurrent", json!({"name":"Standard"}));
    // Rows written for a second dimension style and a viewport (added after review: the readers
    // assumed them, the first fixture had neither).
    run(&mut d, "dimensionStyle.open", json!({"name":"D2","current":false}));
    // Named views and UCSs have no command; they are written into the file, which reopens it
    // (before the entities, so that the session's last dimension is `$Dim`).
    raw(&mut d, |d| {
        d.views.push(serde_json::from_value(json!({"name":"V1","center":{"x":0,"y":0},"height":10,"width":10})).unwrap());
        d.ucss.push(
            serde_json::from_value(json!({"name":"U1","origin":{"x":0,"y":0,"z":0},"xAxis":{"x":1,"y":0,"z":0},"yAxis":{"x":0,"y":1,"z":0}}))
                .unwrap(),
        );
    });
    let member = run(&mut d, "line", json!({"points":[[0,80],[10,80]],"layer":"0"}))["handles"][0].clone();
    run(&mut d, "block", json!({"name":"Door","base":[0,80],"handles":[member],"keep":"delete"}));
    let rect = run(&mut d, "rectangle.add", json!({"p1":[0,0],"p2":[10,10],"layer":"Walls"}))["handle"].clone();
    let l1 = run(&mut d, "line", json!({"points":[[20,0],[30,0]],"layer":"Walls"}))["handles"][0].clone();
    let l2 = run(&mut d, "line", json!({"points":[[30,0],[30,10]],"layer":"Walls"}))["handles"][0].clone();
    let circ = run(&mut d, "circle", json!({"center":[50,5],"radius":5}))["handle"].clone();
    let arc = run(&mut d, "arc", json!({"center":[70,5],"radius":5,"start":0,"end":180}))["handle"].clone();
    let text = run(&mut d, "text", json!({"at":[0,20],"text":"Hello","height":2.5,"layer":"Notes"}))["handle"].clone();
    let mtext = run(&mut d, "multilineText.add", json!({"at":[20,20],"text":"World","height":2.5,"layer":"Notes"}))["handle"].clone();
    let spline = run(&mut d, "spline", json!({"fit":[[0,40],[10,45],[20,40]]}))["handle"].clone();
    let ins = run(&mut d, "insert", json!({"name":"Door","at":[100,0]}))["handle"].clone();
    let tab = run(&mut d, "table", json!({"at":[150,0],"rows":2,"cols":2}))["handle"].clone();
    let dim = run(&mut d, "dimension.addLinear", json!({"object":l1,"at":[25,-5]}))["handle"].clone();
    let hatch = run(&mut d, "hatch", json!({"handles":[rect]}))["handle"].clone();
    let vp = run(&mut d, "viewport.add", json!({"layout":"Layout1","p1":[10,10],"p2":[100,80]}))["viewports"][0].clone();
    run(&mut d, "group", json!({"name":"G1","handles":[l1, l2]}));
    run(&mut d, "layerState.save", json!({"name":"LS1"}));
    run(&mut d, "select", json!({"handles":[circ]}));
    let saved = kition_cad_io::read(&d.save(Some(FILE)).unwrap(), FILE).unwrap();
    let door_line = saved.block("Door").unwrap().entities.iter().next().unwrap().handle.hex();
    let mut roles: Vec<(&'static str, String)> = vec![
        ("$Rect", hex(&rect)),
        ("$L1", hex(&l1)),
        ("$L2", hex(&l2)),
        ("$Circ", hex(&circ)),
        ("$Arc", hex(&arc)),
        ("$Text", hex(&text)),
        ("$MText", hex(&mtext)),
        ("$Spline", hex(&spline)),
        ("$Ins", hex(&ins)),
        ("$Tab", hex(&tab)),
        ("$Dim", hex(&dim)),
        ("$Hatch", hex(&hatch)),
        ("$Vp", hex(&vp)),
        ("$DoorLine", door_line),
    ];
    // A table cell's text is its own object's: `cell:<table handle>-<cell id>`.
    let tab_cell = |row: usize, col: usize| {
        let h = kition_cad_doc::Handle::parse_hex(&hex(&tab)).unwrap();
        let kition_cad_doc::EntityKind::Table(t) = &saved.entity(h).unwrap().kind else { panic!("$Tab is no table") };
        format!("cell:{}-{}", hex(&tab), t.cells[row][col].id)
    };
    roles.push(("$TabCell10", tab_cell(1, 0)));
    roles.push(("$TabCell11", tab_cell(1, 1)));
    for (role, ty, name) in [
        ("$0", "layer", "0"),
        ("$Walls", "layer", "Walls"),
        ("$Notes", "layer", "Notes"),
        ("$Defpoints", "layer", "Defpoints"),
        ("$Continuous", "linetype", "Continuous"),
        ("$Dashed", "linetype", "DASHED"),
        ("$Standard", "textstyle", "Standard"),
        ("$S", "textstyle", "S"),
        ("$DimStandard", "dimensionStyle.open", "Standard"),
        ("$Door", "block", "Door"),
        ("$Layout1", "layout", "Layout1"),
        ("$Layout2", "layout", "Layout2"),
        ("$G1", "group", "G1"),
        ("$LS1", "layerstate", "LS1"),
        ("$V1", "view", "V1"),
        ("$U1", "ucs", "U1"),
        ("$TableStandard", "tableStyle.open", "Standard"),
        ("$MLeaderStandard", "multileaderStyle.open", "Standard"),
    ] {
        if let Some(id) = saved.record_id(ty, name, 0) {
            roles.push((role, format!("{ty}:{id}")));
        }
    }
    (d, roles)
}

/// [`fixture`] with a perpendicular constraint between `$L1` and `$L2`.
pub fn constrained() -> (CadDocument, Vec<(&'static str, String)>) {
    let (mut d, roles) = fixture();
    let l1 = roles.iter().find(|(r, _)| *r == "$L1").unwrap().1.clone();
    let l2 = roles.iter().find(|(r, _)| *r == "$L2").unwrap().1.clone();
    run(&mut d, "constraint.perpendicular", json!({"h1": l1, "h2": l2}));
    run(&mut d, "select", json!({"handles":[roles.iter().find(|(r, _)| *r == "$Circ").unwrap().1]}));
    (d, roles)
}

/// One code path of a command: its parameters (roles in place of handles), the editor state it
/// reads (sorted, comma-separated) and what a write with `basis` holds of each object (role,
/// fields, positions).
pub struct Row {
    command: &'static str,
    params: &'static str,
    editor: &'static str,
    holds: &'static [(&'static str, &'static str, &'static str)],
}

const fn r(command: &'static str, params: &'static str, editor: &'static str, holds: &'static [(&'static str, &'static str, &'static str)]) -> Row {
    Row { command, params, editor, holds }
}

/// A row on [`constrained`].
const fn r_c(command: &'static str, params: &'static str, editor: &'static str, holds: &'static [(&'static str, &'static str, &'static str)]) -> Row {
    r(command, params, editor, holds)
}

fn with_roles(value: &Value, roles: &[(&str, String)]) -> Value {
    match value {
        Value::String(text) => roles.iter().find(|(role, _)| role == text).map_or_else(|| value.clone(), |(_, id)| json!(id)),
        Value::Array(items) => Value::Array(items.iter().map(|item| with_roles(item, roles)).collect()),
        Value::Object(fields) => Value::Object(fields.iter().map(|(key, item)| (key.clone(), with_roles(item, roles))).collect()),
        _ => value.clone(),
    }
}

/// Fields as a set: `None` every field, else the paths (empty: existence alone).
type Fields = Option<BTreeSet<String>>;

fn fields_of(text: &str) -> Fields {
    match text {
        "*" => None,
        "-" | "" => Some(BTreeSet::new()),
        text => Some(text.split(',').map(|f| f.trim().to_owned()).collect()),
    }
}

fn spelled(fields: &Fields) -> String {
    match fields {
        None => "*".into(),
        Some(set) if set.is_empty() => "-".into(),
        Some(set) => set.iter().cloned().collect::<Vec<_>>().join(","),
    }
}

/// review: keep symbolic paths, including absent members. Enumerating
/// today's leaves loses absence and future map/array members (an empty overrides
/// map today can gain DIMTXT tomorrow). Parent and leaf lists are not equivalent.
fn expand(fields: &Fields, _stored: Option<&Value>) -> Fields {
    fields.as_ref().map(|set| {
        // C8-cad-read-2: place.rs::derived publishes these exact same two
        // scalar PickFact members independently. This encoding does not hold
        // rank/blocks/layers, or substitute membership for geometry or bounds.
        let mut out: BTreeSet<String> = set
            .iter()
            .map(|field| match field.as_str() {
                "~visible" => "~pick.visible".to_owned(),
                "~space" => "~pick.space".to_owned(),
                _ => field.clone(),
            })
            .collect();
        if set.iter().any(|f| f.starts_with("kind.")) {
            out.insert("kind.type".to_owned());
        }
        out
    })
}

/// Whether `held` holds at least what `needed` does: a path holds itself and every path below it.
fn covers(held: &Fields, needed: &Fields) -> bool {
    match (held, needed) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(held), Some(needed)) => needed.iter().all(|n| held.iter().any(|h| n == h || n.starts_with(&format!("{h}.")))),
    }
}

/// What a write with `basis` of `command` holds, computed as the SDK's guard (kition-kits
/// `tools/guard.rs` at 1935d52) computes it: the target parameter's ids, whole (no position for
/// a position-independent command); then `under`'s premises replacing what they name and the
/// objects its place names; then every other parameter string naming an object (a handle, a
/// record by a declared name parameter), whole, unless a premise already says what is held.
fn held(
    doc: &CadDocument,
    command: &KitCommand,
    params: &Value,
    entities: &kition_cad_doc::Drawing,
) -> Result<BTreeMap<String, (Fields, bool)>, String> {
    let mut holds: BTreeMap<String, (Fields, bool)> = BTreeMap::new();
    let is_entity = |text: &str| {
        kition_cad_doc::Handle::parse_hex(text)
            .is_some_and(|h| entities.entity(h).is_some() || entities.blocks.values().any(|b| b.entities.get(h).is_some()))
    };
    let target = command.targets;
    if let Some(target) = target {
        let values: Vec<&Value> = match params.get(target) {
            Some(Value::Array(items)) => items.iter().collect(),
            Some(Value::Null) | None => vec![],
            Some(v) => vec![v],
        };
        for value in values.into_iter().filter_map(Value::as_str) {
            holds.insert(value.to_owned(), (None, !command.position_independent));
        }
    }
    if let Some(under) = doc.under(&command.id, params)? {
        for premise in &under.premises {
            let id = premise.id.as_str().map_or_else(|| premise.id.to_string(), str::to_owned);
            let fields = premise.fields.as_ref().map(|f| f.iter().cloned().collect::<BTreeSet<_>>());
            let positions = premise.positions.as_ref().is_none_or(|p| !p.is_empty());
            holds.insert(id, (fields, positions));
        }
        // SDK content regions over stored attributes have the same semantic input as
        // the independently read tag constants/values. This preserves absent tags.
        for content in &under.content {
            if command.id != "attribute.edit" || content.field != "kind.attribs" {
                return Err(format!("unrecognized content premise: {content:?}"));
            }
            let id = content.id.as_str().ok_or("attribute content id must be a handle")?.to_owned();
            let entry = holds.entry(id).or_insert((Some(BTreeSet::new()), false));
            if let Some(fields) = &mut entry.0 {
                for (tag, value) in content.region.as_object().into_iter().flatten() {
                    if value.is_string() || value.is_number() || value.is_boolean() {
                        let tag = json!(tag.to_ascii_lowercase()).to_string().replace(']', "\\u005d");
                        fields.insert(format!("~attributes[tag={tag}].constants"));
                        fields.insert(format!("~attributes[tag={tag}].values"));
                    }
                }
            }
        }
        // A place replayed by `covers` holds what the space's membership puts under it.
        if let Some(covers) = &under.covers {
            holds.insert("~place".into(), (Some(covers.fields.iter().cloned().collect()), false));
            // C8-cad-read-2: this is an actual typed-map replay witness, not R2's
            // advertised name parameter. place.rs::record_address/covered projects
            // exactly this key from historical ~recordNames. read2_names proves
            // case-folded collisions refuse while other types/names merge.
            if covers.fields.iter().any(|f| f == "~recordNames")
                && command.places.contains(&"name")
                && matches!(
                    command.id.as_str(),
                    "block" | "block.add" | "insert" | "style" | "dimensionStyle.open" | "dimensionStyle.openManager" | "layerState.save"
                )
                && let Some(name) = params.get("name").and_then(Value::as_str)
                && let Some((_, types)) = command.object_types.iter().find(|(key, _)| *key == "name")
                && let Some(ty) = types.first()
            {
                let prefix = format!("{}:", ty.to_ascii_lowercase());
                let resolved = name.strip_prefix(&prefix).and_then(|id| {
                    entities
                        .record_names()
                        .into_iter()
                        .find(|(table, record)| table == &ty.to_ascii_lowercase() && entities.record_id(table, record, 0) == Some(id))
                        .map(|(_, name)| name)
                });
                let name = resolved.as_deref().unwrap_or(name);
                let name =
                    if matches!(command.id.as_str(), "style" | "dimensionStyle.open" | "dimensionStyle.openManager") { name.trim() } else { name };
                let name = if command.id == "insert" { name.strip_prefix('*').unwrap_or(name) } else { name };
                let key = name.to_ascii_lowercase().bytes().map(|b| format!("{b:02x}")).collect::<String>();
                let entry = holds.entry("drawing".into()).or_insert((Some(BTreeSet::new()), false));
                if let Some(fields) = &mut entry.0 {
                    fields.insert(format!("~recordNames.{}:{key}", ty.to_ascii_lowercase()));
                }
            }
        }
        for object in &under.objects {
            if let Some(id) = object.as_str() {
                holds.entry(id.to_owned()).or_insert((None, true));
            }
        }
    }
    let mut mentioned: Vec<(String, Option<&'static [&'static str]>)> = Vec::new();
    for (name, value) in params.as_object().into_iter().flatten() {
        if Some(name.as_str()) == target || command.places.contains(&name.as_str()) {
            continue;
        }
        // Public SDK: literal strings never resolve objects. Follow only declared roles.
        if !command.objects.iter().any(|at| at.split('.').next().unwrap_or(at).trim_end_matches("[]") == name)
            && !command.name_params.iter().any(|(at, _)| at.split('.').next().unwrap_or(at).trim_end_matches("[]") == name)
        {
            continue;
        }
        let types = command
            .name_params
            .iter()
            .any(|(at, _)| at.trim_end_matches("[]") == name)
            .then(|| command.object_types.iter().find(|(at, _)| at.trim_end_matches("[]") == name).map_or(&[][..], |(_, types)| *types));
        let mut found = Vec::new();
        match value {
            Value::String(text) => found.push(text.clone()),
            Value::Array(items) => {
                for item in items {
                    match item {
                        Value::String(text) => found.push(text.clone()),
                        Value::Object(map) => found.extend(map.values().filter_map(Value::as_str).map(str::to_owned)),
                        _ => {}
                    }
                }
            }
            Value::Object(map) => found.extend(map.values().filter_map(Value::as_str).map(str::to_owned)),
            _ => {}
        }
        mentioned.extend(found.into_iter().map(|text| (text, types)));
    }
    for (text, types) in mentioned {
        let id = if is_entity(&text) {
            Some(text.clone())
        } else if let Some(types) = types {
            types.iter().find_map(|ty| {
                let prefix = ty.to_ascii_lowercase();
                entities
                    .record_names()
                    .into_iter()
                    .find(|(t, n)| *t == prefix && n.eq_ignore_ascii_case(&text))
                    .and_then(|(t, n)| entities.record_id(&t, &n, 0).map(|id| format!("{t}:{id}")))
            })
        } else {
            None
        };
        if let Some(id) = id {
            holds.entry(id).or_insert((None, true));
        }
    }
    Ok(holds)
}

/// The scenes the rows run on and their rows.
type Fixture = (&'static str, fn() -> (CadDocument, Vec<(&'static str, String)>), &'static [Row]);

fn selected_group_member() -> (CadDocument, Vec<(&'static str, String)>) {
    let (mut doc, roles) = fixture();
    let member = roles.iter().find(|(role, _)| *role == "$L1").unwrap().1.clone();
    run(&mut doc, "select", json!({"handles":[member]}));
    (doc, roles)
}

const FIXTURES: &[Fixture] = &[
    ("fixture", fixture, ROWS),
    ("constrained", constrained, CONSTRAINED),
    // review: restore the removed selection path on an actual group
    // member. The shared fixture's selected circle belongs to no group.
    ("selected-group-member", selected_group_member, &[r("ungroup", r##"{}"##, "selection", &[("$L1", "~groups", "-")])]),
];

#[test]
fn every_write_rests_on_what_its_handler_reads() {
    let mut wrong = Vec::new();
    let (mut differing, mut less, mut more) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut editors: BTreeMap<&str, (BTreeSet<&str>, String)> = BTreeMap::new();
    let mut rows: BTreeSet<&str> = BTreeSet::new();
    let mut count = 0;
    let mut writes = BTreeSet::new();
    for (scene, build, table) in FIXTURES {
        let (doc, roles) = build();
        let commands = doc.commands();
        writes.extend(commands.iter().filter(|c| c.mutates).map(|c| c.id.clone()));
        let drawing = kition_cad_io::read(&doc.save(Some(FILE)).unwrap(), FILE).unwrap();
        let role_of = |id: &str| roles.iter().find(|(_, i)| i == id).map_or_else(|| id.to_owned(), |(role, _)| (*role).to_owned());
        let id_of = |role: &str| roles.iter().find(|(r, _)| *r == role).map_or_else(|| role.to_owned(), |(_, id)| id.clone());
        let stored = |id: &str| -> Option<Value> {
            let h = kition_cad_doc::Handle::parse_hex(id)?;
            let e = drawing.entity(h).cloned().or_else(|| drawing.blocks.values().find_map(|b| b.entities.get(h).cloned()))?;
            Some(json!({"kind": serde_json::to_value(&e.kind).ok()?, "common": serde_json::to_value(&e.common).ok()?}))
        };
        for row in *table {
            count += 1;
            rows.insert(row.command);
            let command = commands.iter().find(|c| c.id == row.command).unwrap_or_else(|| panic!("no command {}", row.command));
            let params = with_roles(&serde_json::from_str(row.params).unwrap_or_else(|e| panic!("{} {}: {e}", row.command, row.params)), &roles);
            let editor = {
                let mut e: Vec<&str> = command.reads.iter().map(|s| s.as_str()).collect();
                e.sort_unstable();
                e.join(",")
            };
            let entry = editors.entry(row.command).or_insert_with(|| (BTreeSet::new(), editor));
            // Review R12: metadata declares the handler-wide union, while blind rows are
            // per-call samples. Fill omitted paths only from these source reads, not from
            // the current declaration. Guard premises below still use per-call necessity.
            let supplemental = match row.command {
                // table.rs:145-150; annotate.rs:320,660-667; attribs.rs:171-176;
                // modify.rs::run_breakpoint/run_join, trimextend.rs::run, spline_opts::run_object;
                // modify2.rs::run_ncopy; annotate.rs::run_qdim; groups.rs::ungroup_names.
                "attribute.sync"
                | "object.breakAtPoint"
                | "dimension.addArcLength"
                | "dimension.editText"
                | "dimension.textAngle"
                | "dimension.textCenter"
                | "dimension.textHome"
                | "dimension.textLeft"
                | "dimension.textRight"
                | "extend"
                | "join"
                | "object.copyNested"
                | "dimension.addQuick"
                | "spline"
                | "table.deleteColumn"
                | "table.deleteRow"
                | "table.insertColumn"
                | "table.insertRow"
                | "table.merge"
                | "table.set"
                | "table.unmerge"
                | "ungroup" => "selection",
                // layout.rs:234-258,345-376,617-630: default current layout/viewport.
                "layout" | "layout.delete" | "layout.rename" | "viewport.add" | "viewport.new" | "viewport.one" | "viewport.two"
                | "viewport.three" | "viewport.four" => "part",
                "viewport.freezeLayer" => "part,selection",
                // Literal current-header reads, including validation and reply-only reads:
                // props.rs::styles_json/dimstyles_json/run_dimstyle_rename/run_tablestyle;
                // layer.rs::run_set/run_delete/obj_layers; merge.rs::run_laydel/run_laymrg;
                // annotate.rs::run_update/style; blocks/purge.rs::candidates; sysvars.rs::get/set.
                "dimensionStyle.setCurrent"
                | "dimensionStyle.delete"
                | "dimensionStyle.rename"
                | "dimension.updateStyle"
                | "layer.changeToCurrent"
                | "layer.deleteWithObjects"
                | "layer"
                | "layer.delete"
                | "layer.set"
                | "layer.freeze"
                | "layer.lock"
                | "layer.merge"
                | "layer.off"
                | "layer.unlock"
                | "multileaderStyle.open"
                | "purge"
                | "style"
                | "textStyle.delete"
                | "textStyle.rename"
                | "tableStyle.open"
                | "rename" => "active",
                "variable.set" => "active,part",
                // targets() with explicit null falls back (cmd/mod.rs:293-300), even when
                // normal non-null explicit samples don't consume the selection.
                "attribute.editGlobal" | "object.deleteDuplicates" => "selection",
                // lib.rs::command_line reaches arbitrary engine commands.
                "command.runLine" => "active,part,selection",
                _ => "",
            };
            entry.0.extend(supplemental.split(',').filter(|s| !s.is_empty()));
            entry.0.extend(row.editor.split(',').filter(|s| !s.is_empty()));
            let held = match held(&doc, command, &params, &drawing) {
                Ok(held) => held,
                Err(e) => {
                    differing.insert(row.command);
                    wrong.push(format!("ERROR {} {}: {e}", row.command, row.params));
                    continue;
                }
            };
            let holds: BTreeMap<String, (Fields, bool)> = held
                .into_iter()
                .map(|(id, (fields, at))| {
                    let s = stored(&id);
                    (role_of(&id), (expand(&fields, s.as_ref()), at))
                })
                .collect();
            let mut expected: BTreeMap<String, (Fields, bool)> = BTreeMap::new();
            for (object, fields, at) in row.holds {
                let s = stored(&id_of(object));
                let (fields, at) = (expand(&fields_of(fields), s.as_ref()), *at == "*");
                let entry = expected.entry((*object).to_owned()).or_insert((Some(BTreeSet::new()), false));
                entry.0 = match (entry.0.take(), fields) {
                    (Some(mut a), Some(b)) => {
                        a.extend(b);
                        Some(a)
                    }
                    _ => None,
                };
                entry.1 |= at;
            }
            // review / R13: discarded stored values and containing
            // groups matter; visibility does not. Locks are marked separately as
            // the transaction-level lock policy is accounted for separately below.
            if matches!(row.command, "join" | "polyline.edit") {
                for (object, (fields, _)) in &mut expected {
                    if fields.is_none() && stored(&id_of(object)).is_some() {
                        *fields = fields_of("common,kind,~groups");
                    }
                }
            }
            // C8-cad-read-2: locks.rs::refuse_locked_changes now rejects changes
            // to existing locked entities after the handler and propagation hooks.
            // These paths assign fields on the addressed entity (not snap sources).
            let lock_policy = matches!(
                row.command,
                "object.setProperties"
                    | "attribute.edit"
                    | "attribute.editGlobal"
                    | "dimension.override"
                    | "dimensionStyle.override"
                    | "dimension.updateStyle"
                    | "dimension.disassociate"
                    | "dimension.editText"
                    | "dimension.textAngle"
                    | "dimension.textCenter"
                    | "dimension.textHome"
                    | "dimension.textLeft"
                    | "dimension.textRight"
            ) || row.command.starts_with("table.");
            if lock_policy
                || matches!(row.command, "join" | "polyline.edit" | "layer.changeToCurrent" | "layer.match" | "object.matchProperties" | "region")
            {
                for (object, (fields, _)) in &mut expected {
                    let is_source =
                        row.command == "object.matchProperties" && params.get("source").and_then(Value::as_str) == Some(id_of(object).as_str());
                    // C8-cad-read-4: annotate.rs:674-677 filters non-dimensions
                    // before modify_entity, so the lock policy does not apply to them.
                    let ignored = row.command == "dimension.disassociate" && stored(&id_of(object)).is_some_and(|e| e["kind"]["type"] != "dimension");
                    if !is_source
                        && !ignored
                        && stored(&id_of(object)).is_some()
                        && let Some(fields) = fields
                    {
                        fields.insert("~locked".into());
                    }
                }
            }
            // R1 is a source-reading correction, not a spelling equivalence.
            // current_common copies CELTYPE; resolve the actual named record instead
            // of assuming the fixture can only ever use ByLayer.
            if row.command != "linetype" && expected.get("drawing").is_some_and(|(f, _)| f.as_ref().is_some_and(|f| f.contains("header.CELTYPE"))) {
                let name = drawing.header.str("CELTYPE", "ByLayer");
                let id = drawing.record_id("linetype", &name, 0).map_or_else(|| name.clone(), str::to_owned);
                expected.entry(role_of(&format!("linetype:{id}"))).or_insert((fields_of("name"), false));
            }
            // R5: the readings spelled a layer's members by its lowercase name; the schema keys
            // `~layerMembers` by its hex-encoded lowercase name.
            if let Some((Some(fields), _)) = expected.get_mut("drawing") {
                *fields = fields
                    .iter()
                    .map(|f| match f.strip_prefix("~layerMembers.") {
                        Some(name) => format!("~layerMembers.{}", name.bytes().map(|b| format!("{b:02x}")).collect::<String>()),
                        None => f.clone(),
                    })
                    .collect();
            }
            // R10: named-record map keys use hex-encoded names so dots in names do not
            // become field separators (e.g. monochrome.ctb); the reader uses plain names.
            if let Some((Some(fields), _)) = expected.get_mut("drawing") {
                *fields = fields
                    .iter()
                    .map(|f| {
                        if f == "~recordNames.linetype:center" {
                            return "~centerLinetype".to_owned();
                        }
                        if let Some((ty, name)) = f.strip_prefix("~recordNames.").and_then(|f| f.split_once(':')) {
                            format!("~recordNames.{ty}:{}", name.bytes().map(|b| format!("{b:02x}")).collect::<String>())
                        } else {
                            f.clone()
                        }
                    })
                    .collect();
            }
            // review / R2: retain typed, case-folded name facts.
            // SDK name-parameter checks are not this same fact in every case:
            // the kit resolves B1/b1 alike, whereas guard name edits/creations
            // compare the supplied spelling. A declaration cannot discharge the
            // independently read absence merely by advertising a name parameter.
            // R3 and place replay are not equivalences: replay membership cannot
            // discharge geometry, bounds or fields. Source-reviewed scan rows name
            // their historical replay separately as ~place and keep required values
            // on the actual entities/drawing. No expected fact is synthesized here.
            let mut holds = holds;
            if !expected.contains_key("~place") {
                holds.remove("~place");
            }
            // R11: resolving a file already establishes the root's existence; a name-only
            // absence check discharged above leaves no additional root fact to hold.
            if expected.get("drawing").is_some_and(|(f, at)| f.as_ref().is_some_and(BTreeSet::is_empty) && !at) {
                expected.remove("drawing");
            }
            // R9: an entity's ~groups witness stores its complete containing groups
            // (scan.rs:12-19), including their data and identity. A whole-entity premise includes it.
            for (object, (fields, _)) in &expected.clone() {
                if stored(&id_of(object)).is_some()
                    && covers(fields, &Some(BTreeSet::from(["~groups".to_owned()])))
                    && let Some(h) = kition_cad_doc::Handle::parse_hex(&id_of(object))
                {
                    for g in drawing.groups.iter().filter(|g| g.members.contains(&h)) {
                        if let Some(id) = drawing.record_id("group", &g.name, 0) {
                            expected.remove(&role_of(&format!("group:{id}")));
                        }
                    }
                }
            }
            let fewer = expected.iter().any(|(o, (f, at))| holds.get(o).is_none_or(|(hf, hat)| !covers(hf, f) || (*at && !hat)));
            let wider = holds.iter().any(|(o, (f, at))| expected.get(o).is_none_or(|(ef, eat)| !covers(ef, f) || (*at && !eat)));
            if fewer || wider {
                differing.insert(row.command);
                if fewer {
                    less.insert(row.command);
                }
                if wider {
                    more.insert(row.command);
                }
                // Per object, what the declaration lacks (-) and holds beyond the reading (+).
                let minus = |a: &Fields, b: &Fields| -> String {
                    match (a, b) {
                        (_, None) => String::new(),
                        (None, Some(_)) => "*".into(),
                        (Some(a), Some(b)) => a
                            .iter()
                            .filter(|n| !covers(&Some(b.clone()), &Some(BTreeSet::from([(*n).clone()]))))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(","),
                    }
                };
                let mut delta = Vec::new();
                for (o, (f, at)) in &expected {
                    match holds.get(o) {
                        None => delta.push(format!("-{o}[{}{}]", spelled(f), if *at { " @" } else { "" })),
                        Some((hf, hat)) => {
                            let lack = minus(f, hf);
                            if !lack.is_empty() || (*at && !hat) {
                                delta.push(format!("-{o}[{lack}{}]", if *at && !hat { " @" } else { "" }));
                            }
                        }
                    }
                }
                for (o, (f, at)) in &holds {
                    match expected.get(o) {
                        None => delta.push(format!("+{o}[{}{}]", spelled(f), if *at { " @" } else { "" })),
                        Some((ef, eat)) => {
                            let extra = minus(f, ef);
                            if !extra.is_empty() || (*at && !eat) {
                                delta.push(format!("+{o}[{extra}{}]", if *at && !eat { " @" } else { "" }));
                            }
                        }
                    }
                }
                wrong.push(format!(
                    "{}{} {} {} ({scene}): {}",
                    if fewer { "LESS " } else { "" },
                    if wider { "MORE " } else { "" },
                    row.command,
                    row.params,
                    delta.join(" ")
                ));
                let _ = &params;
            }
        }
    }
    for (command, (read, declared)) in &editors {
        let read = read.iter().copied().collect::<Vec<_>>().join(",");
        if &read != declared {
            differing.insert(command);
            let d: BTreeSet<&str> = declared.split(',').filter(|s| !s.is_empty()).collect();
            let r: BTreeSet<&str> = read.split(',').filter(|s| !s.is_empty()).collect();
            if !r.is_subset(&d) {
                less.insert(command);
            }
            if !d.is_subset(&r) {
                more.insert(command);
            }
            wrong.push(format!("EDITOR {command}: declares {declared:?}, reading {read:?}"));
        }
    }
    let read: BTreeSet<String> = rows.iter().map(|s| (*s).to_owned()).collect();
    for missing in writes.difference(&read) {
        wrong.push(format!("{missing}: a write command with no reading"));
    }
    println!("{}", wrong.join("\n"));
    println!(
        "rows {count}; commands differing {} of {}: holding less {}, holding more {}, both {}",
        differing.len(),
        writes.len(),
        less.len(),
        more.len(),
        less.intersection(&more).count()
    );
    println!("LESS {less:?}");
    println!("MORE {more:?}");
    assert_eq!(writes.len(), 247, "the write commands read");
    assert!(writes.is_subset(&read), "every registered write must have a reading");
    assert!(less.is_empty(), "held-LESS commands cannot return: {less:?}");
    assert!(differing.len() <= FLOOR, "{} commands differ from their readings (floor {FLOOR})", differing.len());
}

/// Commands whose declarations differ from their reading; the ceiling only goes down.
// C8-cad-pin: source-reviewed rows and behavior fixes tighten the remaining conservative ceiling.
const FLOOR: usize = 70;

const ROWS: &[Row] = &[
    // ---- Reading A ----
    // Group A rows (dimensions, leaders, attributes) from blind reading of the handlers.
    // dimlinear: points given, rotation from `at`; DIMASSOC scan finds nothing; current props, DIMSTYLE + DIM* overrides
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addLinear",
        r##"{"p1":[200,200],"p2":[210,200],"at":[205,190]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimlinear: points given on $L1 start / $L2 end; DIMASSOC scan attaches p13->$L1, p14->$L2
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addLinear",
        r##"{"p1":[20,0],"p2":[30,10],"at":[25,-5],"rotation":0}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimlinear: object by handle: extent and both endpoint attachments come from $L1
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addLinear",
        r##"{"object":"$L1","at":[25,-5]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
        ],
    ),
    // dimlinear: explicit text: same holdings as the points path
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addLinear",
        r##"{"p1":[200,200],"p2":[210,200],"at":[205,190],"text":"T"}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dim: same handler as dimlinear (run_linear), points path
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dim",
        r##"{"p1":[200,200],"p2":[210,200],"at":[205,190]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dim: object path, as dimlinear
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dim",
        r##"{"object":"$L1","at":[25,-5]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
        ],
    ),
    // dimaligned: points only; scan finds nothing
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addAligned",
        r##"{"p1":[200,200],"p2":[210,210],"at":[200,210]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimaligned: points on $L1 start/$L2 end; scan attaches both
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addAligned",
        r##"{"p1":[20,0],"p2":[30,10],"at":[20,10]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimradius: circle by handle; radius, centre and attachments come from that circle
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addRadius",
        r##"{"handle":"$Circ","at":[60,10]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.type,kind.center,kind.radius", "-"),
        ],
    ),
    // dimradius: no handle: first of the selection ([$Circ])
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addRadius",
        r##"{"at":[60,10]}"##,
        "active,part,selection",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.type,kind.center,kind.radius", "-"),
        ],
    ),
    // dimradius: centre+point given; scan finds $Circ by centre/radius
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addRadius",
        r##"{"center":[50,5],"point":[55,5]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.center,kind.radius", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimdiameter: circle by handle; radius, centre and attachments come from that circle
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addDiameter",
        r##"{"handle":"$Circ","at":[60,10]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.type,kind.center,kind.radius", "-"),
        ],
    ),
    // dimdiameter: no handle: first of the selection ([$Circ])
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addDiameter",
        r##"{"at":[60,10]}"##,
        "active,part,selection",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.type,kind.center,kind.radius", "-"),
        ],
    ),
    // dimdiameter: centre+point given; scan finds $Circ by centre/radius
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addDiameter",
        r##"{"center":[50,5],"point":[55,5]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.center,kind.radius", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimangular: two lines by handle: vertex = their intersection, assoc set explicitly to $L1/$L2 (no scan)
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addAngular",
        r##"{"lines":["$L1","$L2"],"at":[25,5]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
            ("$L2", "kind.type,kind.a,kind.b", "-"),
        ],
    ),
    // dimangular: arc by handle; points and attachments come from $Arc
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addAngular",
        r##"{"arc":"$Arc","at":[70,15]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Arc", "kind.type,kind.center,kind.radius,kind.start,kind.end", "-"),
        ],
    ),
    // dimangular: three points; scan finds nothing
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addAngular",
        r##"{"vertex":[300,300],"p1":[310,300],"p2":[300,310],"at":[305,305]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimarc: arc by handle; points and attachments come from $Arc
    // C8-cad-read-2: annotate.rs::source_associations binds this explicit/selected
    // source directly; add_dim does not scan other objects or their order on this path.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addArcLength",
        r##"{"handle":"$Arc","at":[70,15]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$Arc", "kind.type,kind.center,kind.radius,kind.start,kind.end", "-"),
        ],
    ),
    // dimordinate: feature point free; xtype from leader direction
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addOrdinate",
        r##"{"feature":[300,300],"leader":[300,320]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // dimordinate: feature on $L1 start; scan attaches p13->$L1
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addOrdinate",
        r##"{"feature":[20,0],"leader":[20,-10],"xtype":true}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // Review R6: annotate.rs:341 reads Session::last_dim, not the selection; held as $Dim below.
    // dimcontinue: continue from last_dim ($Dim): copies its kind/style/overrides, p13 = its p14 (30,0) -> scan attaches $L2 start
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addContinue",
        r##"{"points":[[40,-2]]}"##,
        "active,part",
        &[
            ("$DimStandard", "name,data", "-"),
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMASSOC",
                "-",
            ),
            ("$0", "name", "-"),
            (
                "$Dim",
                "kind.type,kind.kind,kind.defpt,kind.textMid,kind.p13,kind.p14,kind.p15,kind.p16,kind.style,kind.measurement,kind.textRotation,kind.overrides",
                "-",
            ),
            ("$L2", "kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // Review R6: annotate.rs:341 reads Session::last_dim, not the selection; held as $Dim below.
    // dimbaseline: baseline from last_dim ($Dim): p13 = its p13 (20,0)->$L1, new point (30,10)->$L2 end; offset DIMDLI x scale of $Dim's style+overrides
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addBaseline",
        r##"{"points":[[30,10]]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMASSOC,header.DIMSCALE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$DimStandard", "name,data", "-"),
            (
                "$Dim",
                "kind.type,kind.kind,kind.defpt,kind.textMid,kind.p13,kind.p14,kind.p15,kind.p16,kind.style,kind.measurement,kind.textRotation,kind.overrides",
                "-",
            ),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // qdim: objects by handle: endpoints x-sorted/deduped -> one dim 20..30; scan attaches $L1 start / $L2 start
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addQuick",
        r##"{"handles":["$L1","$L2"],"at":[25,-10]}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
            ("$L2", "kind.type,kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // qdim: vertical: y-sorted points (20,0)/(30,10) -> one dim; scan attaches $L1 start / $L2 end
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "dimension.addQuick",
        r##"{"handles":["$L1","$L2"],"at":[35,5],"vertical":true}"##,
        "active,part",
        &[
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2,header.DIMASSOC",
                "-",
            ),
            ("$DimStandard", "name,data", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
            ("$L2", "kind.type,kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // mleader: straight leader with text; CMLEADERSTYLE (falls back to the first style), DIMSCALE scales sizes
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/annotate.rs:499-528 stores MText.style from the multileader style; add $Standard name identity]
    r(
        "multileader.add",
        r##"{"points":[[0,0],[10,10]],"text":"N"}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.CMLEADERSTYLE,header.DIMSCALE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$MLeaderStandard", "name,data.arrowSize,data.textHeight,data.landingGap,data.dogleg,data.textStyle", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // mleader: block content by NAME, centre attach: block extents of Door decide the insert point
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/mleader_content.rs:52-66 consumes only the native bounds center; extents.rs:35-44,85-104 includes member visibility and layer state, not member colour or ordered identities]
    r(
        "multileader.add",
        r##"{"points":[[0,0],[10,10]],"block":"Door"}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.CMLEADERSTYLE,header.DIMSCALE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$MLeaderStandard", "name,data.arrowSize,data.dogleg", "-"),
            ("$Door", "name,~blockCenter", "-"),
        ],
    ),
    // mleader: block content, insertion attach: only the block's existence by name
    r(
        "multileader.add",
        r##"{"points":[[0,0],[10,10]],"block":"Door","blockAttach":"insertion"}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.CMLEADERSTYLE,header.DIMSCALE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$MLeaderStandard", "name,data.arrowSize,data.dogleg", "-"),
            ("$Door", "name", "-"),
        ],
    ),
    // mleader: leader type none: content at the one point
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/annotate.rs:499-528 stores MText.style from the multileader style; add $Standard name identity]
    r(
        "multileader.add",
        r##"{"points":[[5,5]],"leaderType":"none","text":"N"}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.CMLEADERSTYLE,header.DIMSCALE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$MLeaderStandard", "name,data.arrowSize,data.textHeight,data.textStyle", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // leader: leader entity; style name copied from DIMSTYLE
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/annotate.rs:551-558 stores the current dimension-style reference; add $DimStandard name identity]
    r(
        "leader",
        r##"{"points":[[0,0],[10,10]]}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$DimStandard", "name", "-"),
        ],
    ),
    // leader: plus an MText whose height is the DIMSTYLE record's text height (style "Standard" hard-coded)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/annotate.rs:560-572 stores the Standard text-style reference; add $Standard name identity]
    r(
        "leader",
        r##"{"points":[[0,0],[10,10]],"text":"N"}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,header.CECOLOR_RGB,header.DIMSTYLE",
                "-",
            ),
            ("$0", "name", "-"),
            ("$DimStandard", "name,data.textHeight", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // dimension.updateStyle: dimension by handle: kind.style := DIMSTYLE, kind.block cleared
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.updateStyle",
        r##"{"handles":["$Dim"]}"##,
        "",
        &[("drawing", "header.DIMSTYLE", "-"), ("$Dim", "kind.type", "-"), ("$Dim", "kind.style", "-")],
    ),
    // dimension.updateStyle: selection fallback ([$Circ]): not a dimension, nothing written
    r("dimension.updateStyle", r##"{}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // dimoverride: merge override fields into kind.overrides (read-modify-write of the map), clear kind.block
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.override",
        r##"{"handles":["$Dim"],"DIMASZ":2}"##,
        "",
        &[("$Dim", "kind.type,~dimOverride_arrowSize", "-"), ("$Dim", "~dimOverride_arrowSize", "-")],
    ),
    // dimoverride: clear: kind.overrides replaced by {} (nothing read)
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r("dimension.override", r##"{"handles":["$Dim"],"clear":true}"##, "", &[("$Dim", "kind.type", "-"), ("$Dim", "kind.overrides", "-")]),
    // C8-cad-read-4: annotate.rs:605-620 assigns text as well as clearing its cache.
    // dimoverride: text path: kind.text := text, kind.block cleared
    r("dimension.override", r##"{"handles":["$Dim"],"text":"X"}"##, "", &[("$Dim", "kind.type,kind.text", "-")]),
    // dimoverride: selection fallback ([$Circ]): not a dimension, nothing changed
    r("dimension.override", r##"{"DIMASZ":2}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // C8-cad-read-2: annotate.rs::run_reassociate scans each target's owning space.
    // dimreassociate: dimension by handle: assoc replaced by a scan of its owning space at p13/p14
    // C8-cad-read-3 source correction: assoc.rs:92-112 / annotate.rs:149-160,646-668: ranked snap membership is replayed; selected source geometry remains explicit; absolute list indices are not inputs.
    // C8-cad-read-4 audit: locks.rs:9-48 checks the reassociated target, not snap-source locks.
    r(
        "dimension.reassociate",
        r##"{"handles":["$Dim"]}"##,
        "",
        &[
            ("$Dim", "kind.type,kind.kind,kind.p13,kind.p14", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("~place", "kind,~pick", "-"),
            ("$Dim", "kind.assoc,~space,~locked", "-"),
        ],
    ),
    // dimreassociate: selection fallback ([$Circ]): not a dimension, nothing written
    r("dimension.reassociate", r##"{}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // dimdisassociate: dimension by handle: kind.assoc cleared
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r("dimension.disassociate", r##"{"handles":["$Dim"]}"##, "", &[("$Dim", "kind.type", "-"), ("$Dim", "kind.assoc", "-")]),
    // dimdisassociate: selection fallback ([$Circ]): not a dimension, nothing written
    r("dimension.disassociate", r##"{}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // dimtedit: move text: kind.textMid, kind.userTextPos, kind.block written
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.editText",
        r##"{"handles":["$Dim"],"at":[26,-8]}"##,
        "",
        &[("$Dim", "kind.type", "-"), ("$Dim", "kind.textMid,kind.userTextPos", "-")],
    ),
    // dimtedit: justify: textJust inserted into kind.overrides (map read-modify-write)
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.editText",
        r##"{"handles":["$Dim"],"mode":"left"}"##,
        "",
        &[("$Dim", "kind.type,~dimOverride_textJust", "-"), ("$Dim", "kind.userTextPos,~dimOverride_textJust", "-")],
    ),
    // dimtedit: angle: kind.textRotation
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.editText",
        r##"{"handles":["$Dim"],"mode":"angle","angle":30}"##,
        "",
        &[("$Dim", "kind.type", "-"), ("$Dim", "kind.textRotation", "-")],
    ),
    // dimension.textHome: home: run_dimtedit mode home; userTextPos/textRotation reset, textJust removed from kind.overrides
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.textHome",
        r##"{"handles":["$Dim"]}"##,
        "",
        &[("$Dim", "kind.type,~dimOverride_textJust", "-"), ("$Dim", "kind.userTextPos,kind.textRotation,~dimOverride_textJust", "-")],
    ),
    // dimension.textLeft: left: run_dimtedit mode left; textJust set in kind.overrides
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.textLeft",
        r##"{"handles":["$Dim"]}"##,
        "",
        &[("$Dim", "kind.type,~dimOverride_textJust", "-"), ("$Dim", "kind.userTextPos,~dimOverride_textJust", "-")],
    ),
    // dimension.textCenter: center: run_dimtedit mode center; textJust set in kind.overrides
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.textCenter",
        r##"{"handles":["$Dim"]}"##,
        "",
        &[("$Dim", "kind.type,~dimOverride_textJust", "-"), ("$Dim", "kind.userTextPos,~dimOverride_textJust", "-")],
    ),
    // dimension.textRight: right: run_dimtedit mode right; textJust set in kind.overrides
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r(
        "dimension.textRight",
        r##"{"handles":["$Dim"]}"##,
        "",
        &[("$Dim", "kind.type,~dimOverride_textJust", "-"), ("$Dim", "kind.userTextPos,~dimOverride_textJust", "-")],
    ),
    // dimension.textAngle: angle: kind.textRotation
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r("dimension.textAngle", r##"{"handles":["$Dim"],"angle":30}"##, "", &[("$Dim", "kind.type", "-"), ("$Dim", "kind.textRotation", "-")]),
    // C8-cad-read-4: annotate.rs:742-789 filters the base and non-linear targets;
    // eligibility alone controls success when no dimension is moved.
    // The fixture has one dimension so no other dimension is spaced; with others the base's defpt/p13/p14/style/overrides, DIMSCALE (spacing omitted) and every other's kind.defpt would be held
    // dimspace: base by handle; base filtered out of the others -> nothing moved; only base kind decides success
    r("dimension.space", r##"{"base":"$Dim","handles":["$Dim"]}"##, "", &[("$Dim", "kind.type,~linearDimension", "-")]),
    // dimspace: others from the selection ([$Circ]): not a dimension, nothing moved
    r(
        "dimension.space",
        r##"{"base":"$Dim","spacing":5}"##,
        "selection",
        &[("$Dim", "kind.type,~linearDimension", "-"), ("$Circ", "kind.type", "-")],
    ),
    // attedit: insert by handle: values written into kind.attribs[*].text.value by tag (Door has no attributes: changed 0)
    // same-field decision: hold the values this write replaces.
    r(
        "attribute.edit",
        r##"{"handle":"$Ins","values":{"TAG":"v"}}"##,
        "",
        &[("$Ins", "kind.type,~attributes[tag=\"tag\"].constants", "-"), ("$Ins", "~attributes[tag=\"tag\"].values", "-")],
    ),
    // attedit: selection fallback: first selected ([$Circ]), not an insert -> changed 0
    r("attribute.edit", r##"{"values":{"TAG":"v"}}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // UNSURE: the space scan is held as ~queries; every entity's kind.type also decides membership
    // -attedit: no handle: every object of the current space; inserts matching block/tag/value specs get find->replace
    r(
        "attribute.editGlobal",
        r##"{"find":"a","replace":"b"}"##,
        "part",
        &[("drawing", "~queries", "-"), ("$Ins", "kind.type,kind.block,~attributes", "-")],
    ),
    // -attedit: one insert by handle
    r(
        "attribute.editGlobal",
        r##"{"handle":"$Ins","find":"a","replace":"b","block":"Door"}"##,
        "",
        &[("$Ins", "kind.type,kind.block,~attributes", "-")],
    ),
    // attdisp: sets header.ATTMODE = 2, reads nothing
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r("attribute.display", r##"{"mode":"on"}"##, "", &[("drawing", "header.ATTMODE", "-")]),
    // UNSURE: with attdefs present also $Door data.base and each reference's kind.insert,kind.scale,kind.rotation,kind.attribs (values kept by tag)
    // attsync: block by NAME: filters nonconstant AttDefs; Door has none -> native error before any reference writes.
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/blocks/attribs.rs:114-136 filters nonconstant AttDefs and copies their fields; ordinary member identities and geometry are unused]
    r(
        "attribute.sync",
        r##"{"name":"Door"}"##,
        "",
        &[("drawing", "~insertRefs", "-"), ("$Door", "name,~insertAttributes", "-"), ("$Ins", "kind.block", "-")],
    ),
    // attsync: block taken from the insert named by handle
    r(
        "attribute.sync",
        r##"{"handle":"$Ins"}"##,
        "",
        // [corrected C8-cad-pin: crates/engine/engine/src/cmd/blocks/attribs.rs:114-136 filters nonconstant AttDefs; ordinary block members are unused]
        &[("drawing", "~insertRefs", "-"), ("$Door", "name,~insertAttributes", "-"), ("$Ins", "kind.type,kind.block", "-")],
    ),
    // ---- Reading B ----
    // Group B expected rows (from blind reading of draw.rs, draw2.rs, spline_opts.rs, hatch.rs, props.rs, table.rs)
    // table.* selection fallback is not writable on this fixture (selection [$Circ] is not a table -> error).
    // spline {object:true} without handles falls back to the selection [$Circ] -> error (not a polyline); not written.
    // arc: 3-point arc with current properties
    r(
        "arc",
        r##"{"p1":[0,60],"p2":[5,65],"p3":[10,60]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc: center/radius/angles
    r(
        "arc",
        r##"{"center":[0,60],"radius":3,"start":0,"end":90}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // circle: center+radius with current properties
    r(
        "circle",
        r##"{"center":[0,60],"radius":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // circle: two points
    r(
        "circle",
        r##"{"p1":[0,60],"p2":[4,60]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // donut: closed wide lwpolyline
    r(
        "donut",
        r##"{"center":[0,60],"inside":1,"outside":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // ellipse: ellipse from params
    r(
        "ellipse",
        r##"{"center":[0,60],"major":[4,0],"ratio":0.5}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // line: lines between points
    r(
        "line",
        r##"{"points":[[0,60],[10,60],[10,70]]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // mtext: style by name, height given
    r(
        "multilineText.add",
        r##"{"at":[0,60],"text":"Hi","style":"S","height":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$S", "name", "-"),
        ],
    ),
    // mtext: style from TEXTSTYLE, height from TEXTSIZE
    r(
        "multilineText.add",
        r##"{"at":[0,60],"text":"Hi"}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.TEXTSTYLE,header.TEXTSIZE", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // mtext: style from TEXTSTYLE, height given
    r(
        "multilineText.add",
        r##"{"at":[0,60],"text":"Hi","height":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.TEXTSTYLE", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // pline: lwpolyline from vertices
    r(
        "polyline.add",
        r##"{"vertices":[[0,60],[10,60],[10,70]],"width":0.5}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // point: single point
    r(
        "point",
        r##"{"at":[0,60]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // point.addMultiple: several points
    r(
        "point.addMultiple",
        r##"{"points":[[0,60],[1,60]]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // polygon: inscribed polygon
    r(
        "polygon",
        r##"{"sides":6,"center":[0,60],"radius":3}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // polygon: polygon from edge
    r(
        "polygon",
        r##"{"sides":5,"edge":[[0,60],[4,60]]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // ray: ray base+through
    r(
        "ray",
        r##"{"base":[0,60],"through":[5,65]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // rectang: two corners
    r(
        "rectangle.add",
        r##"{"p1":[0,60],"p2":[10,70]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // rectang: dimensions, thickness on the new entity
    r(
        "rectangle.add",
        r##"{"p1":[0,60],"dimensions":[5,3],"thickness":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // spline: fit points, knots from SPLKNOTS
    r(
        "spline",
        r##"{"fit":[[0,60],[5,65],[10,60]]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.SPLKNOTS", "-"),
        ],
    ),
    // spline: fit points, knots given
    r(
        "spline",
        r##"{"fit":[[0,60],[5,65],[10,60]],"knots":"chord"}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // spline: control points
    r(
        "spline",
        r##"{"control":[[0,60],[5,65],[10,60],[15,65]]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // spline: convert polyline by handle (refuses locked), knots from SPLKNOTS
    r("spline", r##"{"object":"$Rect"}"##, "", &[("$Rect", "kind.type,kind.vertices,kind.closed,~locked", "-"), ("drawing", "header.SPLKNOTS", "-")]),
    // spline: convert polyline via object:true + handles, knots given
    r("spline", r##"{"object":true,"handles":["$Rect"],"knots":"uniform"}"##, "", &[("$Rect", "kind.type,kind.vertices,kind.closed,~locked", "-")]),
    // text: height given, style from TEXTSTYLE
    r(
        "text",
        r##"{"at":[0,60],"text":"Hi","height":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.TEXTSTYLE", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // text: height from TEXTSIZE
    r(
        "text",
        r##"{"at":[0,60],"text":"Hi"}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.TEXTSTYLE,header.TEXTSIZE", "-"),
            ("$Standard", "name", "-"),
        ],
    ),
    // xline: base+through
    r(
        "constructionLine.add",
        r##"{"base":[0,60],"through":[5,65]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // xline: bisect
    r(
        "constructionLine.add",
        r##"{"vertex":[0,60],"start":[5,60],"end":[0,65]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // xline: offset from a line by handle
    r(
        "constructionLine.add",
        r##"{"handle":"$L1","distance":5,"side":[25,5]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
        ],
    ),
    // xline: through point parallel to a line by handle
    r(
        "constructionLine.add",
        r##"{"handle":"$L1","through":[0,50]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$L1", "kind.type,kind.a,kind.b", "-"),
        ],
    ),
    // boundary: pick by internal point: scan of the current space
    // C8-cad-read-3 source correction: hatch.rs:24-57,102-154: replay visible boundary membership; retain chosen Rect geometry and exact extent-dependent sampled outlines; other candidates are query inputs.
    r(
        "boundary",
        r##"{"points":[[5,5]]}"##,
        "active,part",
        &[
            // C8-cad-read-4 audit: lib.rs:718-722 / hatch.rs:269: new entities reference the current layer identity.
            ("$0", "name", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$Rect", "kind,~pick,~boundaryOutline", "-"),
            ("drawing", "", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // hatch: boundary by handle, pattern/scale/angle from HPNAME/HPSCALE/HPANG
    r(
        "hatch",
        r##"{"handles":["$Rect"]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$Rect", "kind.type,kind.vertices,kind.closed", "-"),
            ("drawing", "header.HPNAME,header.HPSCALE,header.HPANG", "-"),
        ],
    ),
    // hatch: boundary by handle, all settings given
    r(
        "hatch",
        r##"{"handles":["$Rect"],"pattern":"ANSI37","scale":2,"angle":45}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$Rect", "kind.type,kind.vertices,kind.closed", "-"),
        ],
    ),
    // hatch: selection fallback ($Circ), header defaults
    r(
        "hatch",
        r##"{}"##,
        "active,part,selection",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.type,kind.center,kind.radius", "-"),
            ("drawing", "header.HPNAME,header.HPSCALE,header.HPANG", "-"),
        ],
    ),
    // hatch: pick by internal point, settings given
    // C8-cad-read-3 source correction: hatch.rs:24-57,102-154: replay visible boundary membership; retain chosen Rect geometry and exact extent-dependent sampled outlines; other candidates are query inputs.
    r(
        "hatch",
        r##"{"points":[[5,5]],"pattern":"ANSI31","scale":1,"angle":0}"##,
        "active,part",
        &[
            // C8-cad-read-4 audit: lib.rs:718-722 / hatch.rs:269: new entities reference the current layer identity.
            ("$0", "name", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$Rect", "kind,~pick,~boundaryOutline", "-"),
            ("drawing", "", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // hatch: pick by internal point, header defaults
    // C8-cad-read-3 source correction: hatch.rs:24-57,102-154: replay visible boundary membership; retain chosen Rect geometry and exact extent-dependent sampled outlines; other candidates are query inputs.
    r(
        "hatch",
        r##"{"points":[[5,5]]}"##,
        "active,part",
        &[
            // C8-cad-read-4 audit: lib.rs:718-722 / hatch.rs:269: new entities reference the current layer identity.
            ("$0", "name", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$Rect", "kind,~pick,~boundaryOutline", "-"),
            ("drawing", "", "-"),
            ("drawing", "header.HPNAME,header.HPSCALE,header.HPANG", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // gradient: boundary by handle, scale/angle from HPSCALE/HPANG (HPNAME overridden by SOLID)
    r(
        "gradient",
        r##"{"handles":["$Rect"],"color1":"1","color2":"3"}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$Rect", "kind.type,kind.vertices,kind.closed", "-"),
            ("drawing", "header.HPSCALE,header.HPANG", "-"),
        ],
    ),
    // gradient: boundary by handle, scale and angle given
    r(
        "gradient",
        r##"{"handles":["$Rect"],"scale":1,"angle":30}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$Rect", "kind.type,kind.vertices,kind.closed", "-"),
        ],
    ),
    // gradient: selection fallback ($Circ)
    r(
        "gradient",
        r##"{}"##,
        "active,part,selection",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("$Circ", "kind.type,kind.center,kind.radius", "-"),
            ("drawing", "header.HPSCALE,header.HPANG", "-"),
        ],
    ),
    // gradient: pick by internal point, scale and angle given
    // C8-cad-read-3 source correction: hatch.rs:24-57,102-154: replay visible boundary membership; retain chosen Rect geometry and exact extent-dependent sampled outlines; other candidates are query inputs.
    r(
        "gradient",
        r##"{"points":[[5,5]],"scale":1,"angle":0}"##,
        "active,part",
        &[
            // C8-cad-read-4 audit: lib.rs:718-722 / hatch.rs:269: new entities reference the current layer identity.
            ("$0", "name", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$Rect", "kind,~pick,~boundaryOutline", "-"),
            ("drawing", "", "-"),
            ("~place", "kind,~pick", "-"),
        ],
    ),
    // hatchedit: hatch by handle via object.setProperties: writes pattern/solid/gradient/scale/angle, reads nothing old
    r(
        "hatch.edit",
        r##"{"handles":["$Hatch"],"pattern":"ANSI37","scale":2,"angle":30}"##,
        "",
        &[("$Hatch", "kind.type,kind.pattern,kind.solid,kind.gradient,kind.scale,kind.angle,~locked", "-")],
    ),
    // hatchedit: selection fallback ($Circ): object.setProperties dispatches on kind, circle unchanged
    r("hatch.edit", r##"{"scale":2}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // region: closed object by handle -> closed lwpolyline (kind replaced)
    r("region", r##"{"handles":["$Circ"]}"##, "", &[("$Circ", "kind.type,kind.center,kind.radius", "-")]),
    // region: closed polyline by handle
    r("region", r##"{"handles":["$Rect"]}"##, "", &[("$Rect", "kind.type,kind.vertices,kind.closed", "-")]),
    // region: non-closed object skipped silently
    r("region", r##"{"handles":["$L1"]}"##, "", &[("$L1", "kind.type", "-")]),
    // region: selection fallback ($Circ)
    r("region", r##"{}"##, "selection", &[("$Circ", "kind.type,kind.center,kind.radius", "-")]),
    // table: style from CTABLESTYLE, text height from style, col width from MEASUREMENT
    r(
        "table",
        r##"{"at":[200,0],"rows":2,"cols":2}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CTABLESTYLE", "-"),
            ("drawing", "header.MEASUREMENT", "-"),
            ("$TableStandard", "name,data.textHeight,data.margin", "-"),
        ],
    ),
    // table: textHeight and colWidth given: style margin still read
    r(
        "table",
        r##"{"at":[200,0],"rows":2,"cols":2,"textHeight":2.5,"colWidth":20,"title":"T","header":["a","b"]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CTABLESTYLE", "-"),
            ("$TableStandard", "name,data.margin", "-"),
        ],
    ),
    // review: table.rs::edit normalizes the grid; run_set redirects
    // through merge anchors and assigns text. Other edits read topology, addressed
    // identities and dimension defaults, not unrelated cell text or row/column sizes.
    // C8-cad-read-2: locks.rs enforces the owner lock policy on table writes.
    // table.set: cell by index in table by handle; kind replaced whole
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    // review: table.rs::run_set assigns text without consuming its old value;
    // cell identity and merge anchors decide where the assignment lands.
    // same-field decision: hold the values this write replaces.
    r(
        "table.set",
        r##"{"handle":"$Tab","row":1,"col":0,"text":"x"}"##,
        "",
        &[("$Tab", "kind.type,kind.cells.1.0.id,kind.cells.0.0.merged,kind.cells.1.0.merged", "-"), ("$TabCell10", "text", "-")],
    ),
    // table.insertRow: row inserted before index; kind replaced whole
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.insertRow",
        r##"{"handle":"$Tab","row":1}"##,
        "",
        &[(
            "$Tab",
            "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols,kind.rowHeights.1",
            "-",
        )],
    ),
    // table.insertRow: append with default height (last row's)
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.insertRow",
        r##"{"handle":"$Tab"}"##,
        "",
        &[(
            "$Tab",
            "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols,kind.rowHeights.1",
            "-",
        )],
    ),
    // table.deleteRow: row by index; kind replaced whole
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.deleteRow",
        r##"{"handle":"$Tab","row":1}"##,
        "",
        &[(
            "$Tab",
            "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols",
            "-",
        )],
    ),
    // table.insertColumn: column inserted before index
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.insertColumn",
        r##"{"handle":"$Tab","col":1,"width":5}"##,
        "",
        &[(
            "$Tab",
            "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols",
            "-",
        )],
    ),
    // table.insertColumn: append with default width (last column's)
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.insertColumn",
        r##"{"handle":"$Tab"}"##,
        "",
        &[(
            "$Tab",
            "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols,kind.colWidths.1",
            "-",
        )],
    ),
    // table.deleteColumn: column by index
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.deleteColumn",
        r##"{"handle":"$Tab","col":1}"##,
        "",
        &[(
            "$Tab",
            "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols",
            "-",
        )],
    ),
    // table.merge: merge range by index
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r(
        "table.merge",
        r##"{"handle":"$Tab","row":1,"col":0,"rows":1,"cols":2}"##,
        "",
        &[
            (
                "$Tab",
                "kind.type,kind.cells.0.0.id,kind.cells.0.1.id,kind.cells.1.0.id,kind.cells.1.1.id,kind.cells.0.0.merged,kind.cells.0.1.merged,kind.cells.1.0.merged,kind.cells.1.1.merged,~grid.rows,~grid.cols",
                "-",
            ),
            ("$TabCell10", "text", "-"),
            ("$TabCell11", "text", "-"),
        ],
    ),
    // table.unmerge: unmerge the merge covering a cell
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    r("table.unmerge", r##"{"handle":"$Tab","row":0,"col":0}"##, "", &[("$Tab", "kind.type,kind.cells.0.0.id,kind.cells.0.0.merged", "-")]),
    // ---- Reading C ----
    // arc.addStartCenterEnd: start, center, end -> new arc with current properties
    r(
        "arc.addStartCenterEnd",
        r##"{"start":[10,0],"center":[0,0],"end":[0,10]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addStartCenterAngle: start, center, included angle
    r(
        "arc.addStartCenterAngle",
        r##"{"start":[10,0],"center":[0,0],"angle":90}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addStartCenterLength: start, center, chord length
    r(
        "arc.addStartCenterLength",
        r##"{"start":[10,0],"center":[0,0],"length":5}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addStartEndAngle: start, end, included angle
    r(
        "arc.addStartEndAngle",
        r##"{"start":[10,0],"end":[0,10],"angle":90}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addStartEndDirection: start, end, start tangent direction
    r(
        "arc.addStartEndDirection",
        r##"{"start":[10,0],"end":[0,10],"direction":90}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addStartEndRadius: start, end, radius
    r(
        "arc.addStartEndRadius",
        r##"{"start":[10,0],"end":[0,10],"radius":10}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addCenterStartEnd: center, start, end
    r(
        "arc.addCenterStartEnd",
        r##"{"center":[0,0],"start":[10,0],"end":[0,10]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addCenterStartAngle: center, start, included angle
    r(
        "arc.addCenterStartAngle",
        r##"{"center":[0,0],"start":[10,0],"angle":90}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addCenterStartLength: center, start, chord length
    r(
        "arc.addCenterStartLength",
        r##"{"center":[0,0],"start":[10,0],"length":5}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addContinue: only end -> continues from the DRAWING's last line/arc/open polyline in the current space (storage order, not the session's last curve): $Arc
    // UNSURE: whether a later-added line in model space is covered by $Arc's position "*" or needs a drawing-level ordered-membership fact (~queries)
    // C8-cad-read-3 source correction: curves.rs:118-142: last supported curve is a replayed identity; moving past a non-curve does not change the continuation source.
    r(
        "arc.addContinue",
        r##"{"end":[55,5]}"##,
        "active,part",
        &[
            ("$Arc", "kind.center,kind.end,kind.radius,kind.type", "-"),
            ("~place", "kind,~pick", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // arc.addContinue: explicit start + direction -> no scan
    r(
        "arc.addContinue",
        r##"{"start":[0,0],"direction":0,"end":[10,10]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // circle.addCenterDiameter: center + diameter
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "circle.addCenterDiameter",
        r##"{"center":[0,0],"diameter":10}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CIRCLERAD", "-"),
        ],
    ),
    // circle.addTwoPoints: two diameter end points
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "circle.addTwoPoints",
        r##"{"p1":[0,0],"p2":[10,0]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CIRCLERAD", "-"),
        ],
    ),
    // circle.addThreePoints: three points
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "circle.addThreePoints",
        r##"{"p1":[0,0],"p2":[10,0],"p3":[5,5]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CIRCLERAD", "-"),
        ],
    ),
    // circle.addTanTanRadius: tangent to $L1 (as infinite line) and $Circ, radius 3
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "circle.addTanTanRadius",
        r##"{"h1":"$L1","p1":[25,0],"h2":"$Circ","p2":[50,0],"radius":3}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b,kind.type", "-"),
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CIRCLERAD", "-"),
        ],
    ),
    // circle.addTanTanTan: tangent to $L1, $L2 and $Circ
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "circle.addTanTanTan",
        r##"{"h1":"$L1","p1":[25,0],"h2":"$L2","p2":[30,5],"h3":"$Circ","p3":[45,5]}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b,kind.type", "-"),
            ("$L2", "kind.a,kind.b,kind.type", "-"),
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CIRCLERAD", "-"),
        ],
    ),
    // ellipse.addAxisEnd: axis end points + half other axis
    r(
        "ellipse.addAxisEnd",
        r##"{"p1":[0,0],"p2":[10,0],"distance":3}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // ellipse.addArc: center, major, ratio, start/end angles
    r(
        "ellipse.addArc",
        r##"{"center":[0,0],"major":[10,0],"ratio":0.5,"start":0,"end":90}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // divide: handle given -> point markers along $Circ
    r(
        "divide",
        r##"{"handle":"$Circ","segments":4}"##,
        "active,part",
        &[
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // divide: no handle -> first object of the session selection ($Circ)
    r(
        "divide",
        r##"{"segments":4}"##,
        "active,part,selection",
        &[
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // [corrected C8-cad-pin: blocks.rs:151-184 / draw2.rs:713-718 copy only ordered nonconstant AttDefs, their text/tag/visibility and the base when such definitions exist; ordinary members and their order are not copied.]
    // divide: block markers -> inserts of Door (attdefs of its members copied as attribs)
    r(
        "divide",
        r##"{"handle":"$L1","segments":3,"block":"Door"}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b,kind.type", "-"),
            ("$Door", "~insertAttributes,name", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // measure: handle given -> point markers every 3 units along $L1
    r(
        "measure",
        r##"{"handle":"$L1","length":3}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b,kind.type", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // measure: no handle -> first selected object ($Circ)
    r(
        "measure",
        r##"{"length":5}"##,
        "active,part,selection",
        &[
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // [corrected C8-cad-pin: blocks.rs:151-184 / draw2.rs:713-718 copy only ordered nonconstant AttDefs, their text/tag/visibility and the base when such definitions exist; ordinary members and their order are not copied.]
    // measure: block Door, measured from the end nearest `from`
    r(
        "measure",
        r##"{"handle":"$L1","length":3,"block":"Door","from":[30,0]}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b,kind.type", "-"),
            ("$Door", "~insertAttributes,name", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // revcloud: rectangle, arc length given
    r(
        "revisionCloud.add",
        r##"{"p1":[0,0],"p2":[10,10],"arcLength":2}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // revcloud: rectangle, arc length from REVCLOUDARCLEN (MEASUREMENT picks the fallback)
    r(
        "revisionCloud.add",
        r##"{"p1":[0,0],"p2":[10,10]}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE,header.MEASUREMENT,header.REVCLOUDARCLEN",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // revcloud: object -> cloud around $Rect takes $Rect's whole common, $Rect is deleted (no lock check)
    // UNSURE: add_entity still runs ensure_layer(CLAYER) before the common is overwritten; header vars not held because the cloud's common comes from $Rect (editor "active" left out for the same reason)
    r("revisionCloud.add", r##"{"handle":"$Rect","arcLength":2}"##, "part", &[("$Rect", "*", "-")]),
    // revisionCloud.addPolygonal: polygon points
    r(
        "revisionCloud.addPolygonal",
        r##"{"points":[[0,0],[10,0],[5,8]],"arcLength":2}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // revisionCloud.addFreehand: freehand path
    r(
        "revisionCloud.addFreehand",
        r##"{"freehand":[[0,0],[10,0],[10,10],[0,10]],"arcLength":2}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // wipeout: frames switch -> only writes WIPEOUTFRAME
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r("wipeout", r##"{"frames":"off"}"##, "", &[("drawing", "header.WIPEOUTFRAME", "-")]),
    // wipeout: from closed polyline $Rect (kept)
    r(
        "wipeout",
        r##"{"handle":"$Rect"}"##,
        "active,part",
        &[
            ("$Rect", "kind.closed,kind.type,kind.vertices", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // wipeout: from $Rect and erase it (no lock check)
    r(
        "wipeout",
        r##"{"handle":"$Rect","erase":true}"##,
        "active,part",
        &[
            ("$Rect", "*", "-"),
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // wipeout: from points
    r(
        "wipeout",
        r##"{"points":[[0,0],[10,0],[10,10]]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // 3dpoly: 3D polyline from points
    r(
        "polyline.add3d",
        r##"{"points":[[0,0,0],[10,0,5]]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // mline: scale/justification from CMLSCALE/CMLJUST (MEASUREMENT fallback); CMLSTYLE not read
    r(
        "multiline.add",
        r##"{"points":[[0,0],[10,0]]}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CMLJUST,header.CMLSCALE,header.CPLOTSTYLE,header.MEASUREMENT",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // mline: scale and justification given
    r(
        "multiline.add",
        r##"{"points":[[0,0],[10,0]],"scale":2,"justification":"zero"}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // helix: 3D polyline approximation
    r(
        "helix",
        r##"{"center":[0,0],"baseRadius":5}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // spline.addControlVertices: spline from control vertices
    r(
        "spline.addControlVertices",
        r##"{"control":[[0,0],[5,5],[10,0]]}"##,
        "active,part",
        &[
            ("drawing", "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELTYPE,header.CELWEIGHT,header.CLAYER,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // centermark: handle -> two CENTER lines through $Circ; CENTER linetype loaded from the library unless a record of that name exists
    r(
        "centerMark.add",
        r##"{"handle":"$Circ"}"##,
        "active,part",
        &[
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            (
                "drawing",
                "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELWEIGHT,header.CENTEREXE,header.CLAYER,header.CPLOTSTYLE,header.MEASUREMENT,~recordNames.linetype:center",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // centermark: no handle -> every object of the session selection ($Circ)
    r(
        "centerMark.add",
        r##"{}"##,
        "active,part,selection",
        &[
            ("$Circ", "kind.center,kind.radius,kind.type", "-"),
            (
                "drawing",
                "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELWEIGHT,header.CENTEREXE,header.CLAYER,header.CPLOTSTYLE,header.MEASUREMENT,~recordNames.linetype:center",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // centerline: between two lines $L1 and $L2
    r(
        "centerLine.add",
        r##"{"h1":"$L1","h2":"$L2"}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b,kind.type", "-"),
            ("$L2", "kind.a,kind.b,kind.type", "-"),
            (
                "drawing",
                "header.CECOLOR,header.CECOLOR_RGB,header.CELTSCALE,header.CELWEIGHT,header.CENTEREXE,header.CLAYER,header.CPLOTSTYLE,header.MEASUREMENT,~recordNames.linetype:center",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // ---- Reading D ----
    // ---------------- group D ----------------

    // erase: by handle; locked objects skipped silently (~locked), entity removed whole
    r("erase", r##"{"handles":["$L1"]}"##, "", &[("$L1", "*", "-")]),
    // erase: selection fallback
    r("erase", r##"{}"##, "selection", &[("$Circ", "*", "-")]),
    // move: by handle, in-place transform, locked skipped silently
    r("move", r##"{"handles":["$L1"],"delta":[5,0]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // move: selection fallback with from/to
    // [corrected C8-cad-pin: crates/engine/doc/src/entity.rs:889-906 transforms and assigns circle radius even for unit scale; add kind.radius]
    r("move", r##"{"from":[0,0],"to":[5,5]}"##, "selection", &[("$Circ", "kind.center,~locked,kind.radius", "-")]),
    // copy: clones the whole entity into its own space (no lock check)
    // C8-cad-read-4 sampled audit: modify.rs:108-124 clones stored common/kind into the source space; derived picks/groups are not copied.
    r("copy", r##"{"handles":["$L1"],"delta":[0,20],"count":2}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // copy: selection fallback
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("copy", r##"{"delta":[0,20]}"##, "selection", &[("$Circ", "common,kind,~space", "-")]),
    // rotate: in place, locked skipped silently
    r("rotate", r##"{"handles":["$L1"],"base":[20,0],"angle":90}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // rotate: copy=true clones whole
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("rotate", r##"{"handles":["$L1"],"base":[20,0],"angle":90,"copy":true}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // rotate: selection fallback
    // C8-cad-read-4 sampled audit: doc/entity.rs:903-905 rewrites radius even under rotation.
    r("rotate", r##"{"base":[0,0],"angle":45}"##, "selection", &[("$Circ", "kind.center,kind.radius,~locked", "-")]),
    // scale: in place
    r("scale", r##"{"handles":["$Circ"],"base":[50,5],"factor":2}"##, "", &[("$Circ", "kind.center,kind.radius,~locked", "-")]),
    // scale: copy=true
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("scale", r##"{"handles":["$L1"],"base":[20,0],"factor":2,"copy":true}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // scale: selection fallback
    r("scale", r##"{"base":[50,5],"factor":2}"##, "selection", &[("$Circ", "kind.center,kind.radius,~locked", "-")]),
    // mirror: default keeps the source and adds a mirrored clone (no lock check)
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("mirror", r##"{"handles":["$L1"],"p1":[0,0],"p2":[0,10]}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // mirror: erase:true mirrors in place, locked skipped silently
    r("mirror", r##"{"handles":["$L1"],"p1":[0,0],"p2":[0,10],"erase":true}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // mirror: selection fallback
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("mirror", r##"{"p1":[0,0],"p2":[0,10]}"##, "selection", &[("$Circ", "common,kind,~space", "-")]),
    // stretch: crossing-window scan of the current space finds $L1 and $L2; locked skipped silently
    r(
        "stretch",
        r##"{"window":[[25,-1],[35,11]],"delta":[5,0]}"##,
        "part",
        &[("$L1", "kind.a,kind.b,~pick,~locked", "-"), ("$L2", "kind.a,kind.b,~pick,~locked", "-")],
    ),
    // stretch: window over a $Rect corner also finds the hatch (moved whole if its first grip is inside)
    // UNSURE: whether $Hatch's first grip is inside the window (only whether it moves, not what is held)
    r(
        "stretch",
        r##"{"window":[[8,8],[12,12]],"delta":[2,2]}"##,
        "part",
        &[("$Rect", "kind.vertices,~pick,~locked", "-"), ("$Hatch", "kind,~pick,~locked", "-")],
    ),
    // offset: copy keeps the source's common, pushed into the current space
    r("offset", r##"{"handle":"$L1","distance":2,"side":[25,5]}"##, "part", &[("$L1", "common,kind.a,kind.b", "-")]),
    // offset: layer current -> CLAYER replaces the source layer
    r(
        "offset",
        r##"{"handle":"$L1","distance":2,"side":[25,5],"layer":"current"}"##,
        "active,part",
        &[
            (
                "$L1",
                "common.color,common.linetype,common.lineweight,common.ltscale,common.transparency,common.visible,common.thickness,common.extrusion,common.plotStyle,kind.a,kind.b",
                "-",
            ),
            ("drawing", "header.CLAYER", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // offset: erase:true removes the source unless locked (skip silently)
    r("offset", r##"{"handle":"$L1","distance":2,"side":[25,5],"erase":true}"##, "part", &[("$L1", "*", "-")]),
    // offset: selection fallback (first selected)
    r("offset", r##"{"distance":2,"side":[50,5]}"##, "part,selection", &[("$Circ", "common,kind.center,kind.radius", "-")]),
    // trim: handle, implicit edges = every visible cutter of the current space; quick mode erases a target no edge crosses; locked target -> error
    r(
        "trim",
        r##"{"handle":"$L1","pick":[25,0],"mode":"quick"}"##,
        "part",
        &[
            ("$L1", "*", "-"),
            ("drawing", "~cutters", "-"),
            ("$Rect", "kind.vertices,kind.closed", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end", "-"),
            ("$Spline", "kind", "-"),
            ("$Hatch", "kind.loops", "-"),
        ],
    ),
    // trim: explicit edges (kept only if visible and in the current space)
    r(
        "trim",
        r##"{"handle":"$L1","pick":[25,0],"edges":["$L2"],"mode":"quick"}"##,
        "part",
        &[("$L1", "*", "-"), ("$L2", "kind.a,kind.b,~pick.visible,~pick.space", "-")],
    ),
    // trim: fence scan of the current space finds $L1
    r(
        "trim",
        r##"{"fence":[[25,-1],[25,1]],"edges":["$L2"],"mode":"quick"}"##,
        "part",
        &[("$L1", "*", "-"), ("$L2", "kind.a,kind.b,~pick.visible,~pick.space", "-")],
    ),
    // trim: crossing window scan finds $L1
    r(
        "trim",
        r##"{"crossing":[[24,-1],[26,1]],"edges":["$L2"],"mode":"quick"}"##,
        "part",
        &[("$L1", "*", "-"), ("$L2", "kind.a,kind.b,~pick.visible,~pick.space", "-")],
    ),
    // trim: selection fallback; a circle with < 2 crossings is erased in quick mode
    r(
        "trim",
        r##"{"pick":[55,5],"edges":["$L2"],"mode":"quick"}"##,
        "part,selection",
        &[("$Circ", "*", "-"), ("$L2", "kind.a,kind.b,~pick.visible,~pick.space", "-")],
    ),
    // extend: explicit boundary edge; locked target -> error; kind rewritten in place
    r(
        "extend",
        r##"{"handle":"$L1","pick":[21,0],"edges":["$Rect"]}"##,
        "part",
        &[("$L1", "kind.a,kind.b,~locked", "-"), ("$Rect", "kind.vertices,kind.closed,~pick.visible,~pick.space", "-")],
    ),
    // extend: implicit edges = every visible cutter of the current space
    r(
        "extend",
        r##"{"handle":"$L1","pick":[21,0]}"##,
        "part",
        &[
            ("$L1", "kind.a,kind.b,~locked", "-"),
            ("drawing", "~cutters", "-"),
            ("$Rect", "kind.vertices,kind.closed", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end", "-"),
            ("$Spline", "kind", "-"),
            ("$Hatch", "kind.loops", "-"),
        ],
    ),
    // extend: fence scan finds $L1
    r(
        "extend",
        r##"{"fence":[[21,-1],[21,1]],"edges":["$Rect"]}"##,
        "part",
        &[("$L1", "kind.a,kind.b,~locked,~pick", "-"), ("$Rect", "kind.vertices,kind.closed,~pick.visible,~pick.space", "-")],
    ),
    // fillet: two lines, radius given; arc copies h1's common, pushed into the current space; locks -> error
    r(
        "fillet",
        r##"{"h1":"$L1","p1":[22,0],"h2":"$L2","p2":[30,8],"radius":2}"##,
        "part",
        &[("$L1", "common,kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b,~locked", "-")],
    ),
    // fillet: radius omitted -> FILLETRAD
    r(
        "fillet",
        r##"{"h1":"$L1","p1":[22,0],"h2":"$L2","p2":[30,8]}"##,
        "part",
        &[("$L1", "common,kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b,~locked", "-"), ("drawing", "header.FILLETRAD", "-")],
    ),
    // fillet: line + circle (fillet_curves): arc goes to h1's own space
    r(
        "fillet",
        r##"{"h1":"$L2","p1":[30,8],"h2":"$Circ","p2":[46,5],"radius":8}"##,
        "",
        &[("$L2", "common,kind.a,kind.b,~locked,~pick.space", "-"), ("$Circ", "kind.center,kind.radius,~locked", "-")],
    ),
    // fillet: polyline option, radius given
    r("fillet", r##"{"handle":"$Rect","polyline":true,"radius":1}"##, "", &[("$Rect", "kind,~locked", "-")]),
    // fillet: polyline option, FILLETRAD default
    r("fillet", r##"{"handle":"$Rect","polyline":true}"##, "", &[("$Rect", "kind,~locked", "-"), ("drawing", "header.FILLETRAD", "-")]),
    // chamfer: two lines, distances given
    r(
        "chamfer",
        r##"{"h1":"$L1","p1":[22,0],"h2":"$L2","p2":[30,8],"d1":1,"d2":2}"##,
        "part",
        &[("$L1", "common,kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b,~locked", "-")],
    ),
    // chamfer: distances omitted -> CHAMFERA for both (CHAMFERB not read)
    r(
        "chamfer",
        r##"{"h1":"$L1","p1":[22,0],"h2":"$L2","p2":[30,8]}"##,
        "part",
        &[("$L1", "common,kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b,~locked", "-"), ("drawing", "header.CHAMFERA", "-")],
    ),
    // chamfer: length + angle
    r(
        "chamfer",
        r##"{"h1":"$L1","p1":[22,0],"h2":"$L2","p2":[30,8],"length":1,"angle":30}"##,
        "part",
        &[("$L1", "common,kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b,~locked", "-")],
    ),
    // chamfer: polyline option with d1
    r("chamfer", r##"{"handle":"$Rect","polyline":true,"d1":1}"##, "", &[("$Rect", "kind,~locked", "-")]),
    // chamfer: polyline option, CHAMFERA default
    r("chamfer", r##"{"handle":"$Rect","polyline":true}"##, "", &[("$Rect", "kind,~locked", "-"), ("drawing", "header.CHAMFERA", "-")]),
    // explode: insert -> block by name, its base and members (in block order), parts into the current space
    r(
        "explode",
        r##"{"handles":["$Ins"]}"##,
        "part",
        &[("$Ins", "*", "-"), ("$Door", "name,data.base,~memberOrder", "-"), ("$DoorLine", "common,kind", "-")],
    ),
    // explode: polyline into lines
    r("explode", r##"{"handles":["$Rect"]}"##, "part", &[("$Rect", "*", "-")]),
    // explode: dimension -> rendered geometry with its dim style (+ text style)
    // UNSURE: kition_cad_render::dimension_in reads more drawing state (DIMSCALE/header, text style font) than read here
    // C8-cad-read-4 source correction: io/dxf_write.rs:1033-1037,1323-1337 generates dimension blocks; render/lib.rs:817-826 reads these four font metrics.
    r(
        "explode",
        r##"{"handles":["$Dim"]}"##,
        "part",
        &[("$Dim", "*", "-"), ("$DimStandard", "name,data", "-"), ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-")],
    ),
    // explode: selection fallback; a circle is not explodable (silent no-op), lock checked first
    r("explode", r##"{}"##, "selection", &[("$Circ", "kind.type,~locked", "-")]),
    // C8-cad-read-2: modify.rs::run_front/run_back resolve space_of(handle);
    // explicit handles no longer read the current part.
    // C8-cad-read-4 source correction: modify.rs:1301-1319 changes order in space; display/pick eligibility is unused.
    r("arrange.bringToFront", r##"{"handles":["$Circ"]}"##, "", &[("$Circ", "~pick.order,~space", "*")]),
    // arrange.bringToFront: selection fallback
    // C8-cad-read-4 source correction: modify.rs:1301-1319 changes order in space; display/pick eligibility is unused.
    r("arrange.bringToFront", r##"{}"##, "selection", &[("$Circ", "~pick.order,~space", "*")]),
    // arrange.sendToBack: by handle
    // C8-cad-read-4 source correction: modify.rs:1301-1319 changes order in space; display/pick eligibility is unused.
    r("arrange.sendToBack", r##"{"handles":["$Circ"]}"##, "", &[("$Circ", "~pick.order,~space", "*")]),
    // arrange.sendToBack: selection fallback
    // C8-cad-read-4 source correction: modify.rs:1301-1319 changes order in space; display/pick eligibility is unused.
    r("arrange.sendToBack", r##"{}"##, "selection", &[("$Circ", "~pick.order,~space", "*")]),
    // break: no lock check; target replaced by pieces in the current space
    r("break", r##"{"handle":"$L1","p1":[22,0],"p2":[24,0]}"##, "part", &[("$L1", "*", "-")]),
    // break: selection fallback (circle -> arc)
    r("break", r##"{"p1":[55,5],"p2":[50,10]}"##, "part,selection", &[("$Circ", "*", "-")]),
    // breakatpoint: by handle
    r("object.breakAtPoint", r##"{"handle":"$L1","at":[25,0]}"##, "part", &[("$L1", "*", "-")]),
    // join: two connected lines -> polyline (first's common), no lock check
    r("join", r##"{"handles":["$L1","$L2"]}"##, "part", &[("$L1", "*", "-"), ("$L2", "*", "-")]),
    // join: an unconnected arc is read (decides the chain) but kept
    r(
        "join",
        r##"{"handles":["$L1","$L2","$Arc"]}"##,
        "part",
        &[("$L1", "*", "-"), ("$L2", "*", "-"), ("$Arc", "kind.center,kind.radius,kind.start,kind.end", "-")],
    ),
    // overkill: scan of the current space (no visibility filter); store order decides survivors
    r("object.deleteDuplicates", r##"{}"##, "part", &[("drawing", "~overkill", "-")]),
    // overkill: given handles, compared in parameter order
    r("object.deleteDuplicates", r##"{"handles":["$L1","$L2"]}"##, "", &[("$L1", "common,kind", "-"), ("$L2", "common,kind", "-")]),
    // array: dispatch to arrayrect (default type)
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("array", r##"{"handles":["$L1"],"rows":2,"cols":2,"rowSpacing":10,"colSpacing":15}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // array: dispatch to arraypolar
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("array", r##"{"type":"polar","handles":["$Circ"],"center":[50,20],"count":4}"##, "", &[("$Circ", "common,kind,~space", "-")]),
    // array: dispatch to arraypath, path curve geometry read
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r(
        "array",
        r##"{"type":"path","handles":["$Circ"],"path":"$L1","count":3}"##,
        "",
        &[("$Circ", "common,kind,~space", "-"), ("$L1", "kind.a,kind.b", "-")],
    ),
    // array: selection fallback (rectangular defaults 3x4)
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("array", r##"{}"##, "selection", &[("$Circ", "common,kind,~space", "-")]),
    // arrayrect: by handle
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r(
        "object.arrayRectangular",
        r##"{"handles":["$L1"],"rows":2,"cols":3,"rowSpacing":10,"colSpacing":15}"##,
        "",
        &[("$L1", "common,kind,~space", "-")],
    ),
    // arrayrect: selection fallback
    // C8-cad-read-4 sampled audit: modify.rs:108-124 clones stored common/kind into the source space; derived picks/groups are not copied.
    r("object.arrayRectangular", r##"{"rows":2,"cols":2,"rowSpacing":12,"colSpacing":12}"##, "selection", &[("$Circ", "common,kind,~space", "-")]),
    // arraypolar: rotate (default)
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("object.arrayPolar", r##"{"handles":["$Circ"],"center":[50,20],"count":4}"##, "", &[("$Circ", "common,kind,~space", "-")]),
    // arraypolar: rotate:false moves the bounds centre of the first object around
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r(
        "object.arrayPolar",
        r##"{"handles":["$Circ"],"center":[50,20],"count":4,"angle":180,"rotate":false}"##,
        "",
        &[("$Circ", "common,kind,~space", "-")],
    ),
    // arraypolar: selection fallback
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("object.arrayPolar", r##"{"center":[50,20],"count":3}"##, "selection", &[("$Circ", "common,kind,~space", "-")]),
    // grip.stretch: handle only -> no secondaries; locked primary -> error
    r("grip.stretch", r##"{"handle":"$L1","index":0,"to":[18,0]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.stretch: secondaries whose grip coincides with the dragged one follow (locked secondaries skipped silently)
    r(
        "grip.stretch",
        r##"{"handle":"$L1","handles":["$L1","$L2"],"index":2,"to":[32,0]}"##,
        "",
        &[("$L1", "kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b,~locked", "-")],
    ),
    // grip.stretch: selection fallback (primary = first selected, secondaries = selection)
    r("grip.stretch", r##"{"index":0,"to":[52,5]}"##, "selection", &[("$Circ", "kind.center,kind.radius,~locked", "-")]),
    // grip.move: stretch form
    r("grip.move", r##"{"handle":"$L1","index":0,"to":[18,0]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.move: move mode with explicit base
    r("grip.move", r##"{"handles":["$L1"],"base":[20,0],"to":[25,0],"mode":"move"}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.move: move mode, base = grip of baseHandle
    r(
        "grip.move",
        r##"{"handles":["$L1"],"baseHandle":"$L2","index":0,"to":[35,0],"mode":"move"}"##,
        "",
        &[("$L1", "kind.a,kind.b,~locked", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // grip.move: move mode copy
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("grip.move", r##"{"handles":["$L1"],"base":[20,0],"to":[25,5],"mode":"move","copy":true}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // grip.move: move mode, selection fallback
    // [corrected C8-cad-pin: crates/engine/doc/src/entity.rs:889-906 transforms and assigns circle radius even for unit scale; add kind.radius]
    r("grip.move", r##"{"base":[50,5],"to":[55,5],"mode":"move"}"##, "selection", &[("$Circ", "kind.center,~locked,kind.radius", "-")]),
    // grip.rotate: explicit base + angle
    r("grip.rotate", r##"{"handles":["$L1"],"base":[20,0],"angle":90}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.rotate: base = grip index of first target (no baseHandle), angle from `to`
    r("grip.rotate", r##"{"handles":["$L1"],"index":0,"to":[20,10]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.rotate: copy
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("grip.rotate", r##"{"handles":["$L1"],"base":[20,0],"angle":90,"copy":true}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // grip.rotate: selection fallback
    // [corrected C8-cad-pin: crates/engine/doc/src/entity.rs:889-906 transforms and assigns circle radius even for unit scale; add kind.radius]
    r("grip.rotate", r##"{"base":[0,0],"angle":30}"##, "selection", &[("$Circ", "kind.center,~locked,kind.radius", "-")]),
    // grip.scale: explicit base + factor
    r("grip.scale", r##"{"handles":["$Circ"],"base":[50,5],"factor":2}"##, "", &[("$Circ", "kind.center,kind.radius,~locked", "-")]),
    // grip.scale: base = grip of baseHandle, factor from `to`
    r(
        "grip.scale",
        r##"{"handles":["$Circ"],"baseHandle":"$L1","index":0,"to":[22,0]}"##,
        "",
        &[("$Circ", "kind.center,kind.radius,~locked", "-"), ("$L1", "kind.a,kind.b", "-")],
    ),
    // grip.scale: copy
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("grip.scale", r##"{"handles":["$Circ"],"base":[50,5],"factor":2,"copy":true}"##, "", &[("$Circ", "common,kind,~space", "-")]),
    // grip.scale: selection fallback
    r("grip.scale", r##"{"base":[50,5],"factor":2}"##, "selection", &[("$Circ", "kind.center,kind.radius,~locked", "-")]),
    // grip.mirror: default copy=false mirrors in place (locked skipped silently)
    r("grip.mirror", r##"{"handles":["$L1"],"base":[0,0],"to":[0,10]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.mirror: copy
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("grip.mirror", r##"{"handles":["$L1"],"base":[0,0],"to":[0,10],"copy":true}"##, "", &[("$L1", "common,kind,~space", "-")]),
    // grip.mirror: first mirror point = grip of first target
    r("grip.mirror", r##"{"handles":["$L1"],"index":0,"to":[20,10]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // grip.mirror: selection fallback
    // [corrected C8-cad-pin: crates/engine/doc/src/entity.rs:889-906 transforms and assigns circle radius even for unit scale; add kind.radius]
    r("grip.mirror", r##"{"base":[0,0],"to":[0,10]}"##, "selection", &[("$Circ", "kind.center,~locked,kind.radius", "-")]),
    // ---- Reading E ----
    // ---------------- group E ----------------
    // audit: report only, no fix — nothing written
    r("audit", r##"{"fix":false}"##, "", &[]),
    // audit: fix — the whole drawing is replaced by its repaired copy
    r("audit", r##"{"fix":true}"##, "", &[("drawing", "~audit", "-")]),
    // UNSURE: whether the DXF reader accepts this minimal file; recover opens a NEW document and never reads the current one
    // recover: base64 data opened as a new drawing
    r("recover", r##"{"data":"MApTRUNUSU9OCjIKRU5USVRJRVMKMApFTkRTRUMKMApFT0YK","name":"r.dxf"}"##, "", &[]),
    // -block: handles given, default keep=delete — copies and erases the target; an existing name would be redefined silently
    r("block.add", r##"{"name":"B1","base":[20,0],"handles":["$L1"]}"##, "", &[("$L1", "*", "-"), ("drawing", "~recordNames.block:b1", "-")]),
    // -block: selection fallback
    r("block.add", r##"{"name":"B1","base":[50,5]}"##, "selection", &[("$Circ", "*", "-"), ("drawing", "~recordNames.block:b1", "-")]),
    // -block: keep convert — also inserts the new block with the current properties in the current space
    r(
        "block.add",
        r##"{"name":"B1","base":[20,0],"handles":["$L1"],"keep":"convert"}"##,
        "active,part",
        &[
            ("$L1", "*", "-"),
            (
                "drawing",
                "~recordNames.block:b1,header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // block: default keep=convert — block from the targets, targets erased, insert with current props in current space
    r(
        "block",
        r##"{"name":"B1","base":[20,0],"handles":["$L1","$L2"]}"##,
        "active,part",
        &[
            ("$L1", "*", "-"),
            ("$L2", "*", "-"),
            (
                "drawing",
                "~recordNames.block:b1,header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // block: keep=retain — only the definition is made
    r(
        "block",
        r##"{"name":"B1","base":[20,0],"handles":["$L1"],"keep":"retain"}"##,
        "",
        &[("$L1", "*", "-"), ("drawing", "~recordNames.block:b1", "-")],
    ),
    // block: selection fallback, convert
    r(
        "block",
        r##"{"name":"B1","base":[50,5]}"##,
        "active,part,selection",
        &[
            ("$Circ", "*", "-"),
            (
                "drawing",
                "~recordNames.block:b1,header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // block: existing name (case-insensitive) — silently redefines Door
    r(
        "block",
        r##"{"name":"door","base":[20,0],"handles":["$L1"],"keep":"retain"}"##,
        "",
        &[("$L1", "*", "-"), ("$Door", "name", "-"), ("drawing", "~recordNames.block:door", "-")],
    ),
    // insert: block by name, current properties, current space (attdefs of the block decide the attributes)
    r(
        "insert",
        r##"{"name":"Door","at":[200,0]}"##,
        "active,part",
        &[
            ("$Door", "name,~memberOrder", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // insert: basePoint — the block's base point decides where the reference lands
    r(
        "insert",
        r##"{"name":"Door","at":[200,0],"basePoint":[5,80],"scale":2,"rotation":90}"##,
        "active,part",
        &[
            ("$Door", "name,data.base,~memberOrder", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // insert: *name — inserted then exploded: the block's members are copied (layer-0/ByBlock members take the insert's current props)
    r(
        "insert",
        r##"{"name":"*Door","at":[200,0]}"##,
        "active,part",
        &[
            ("$Door", "name,data.base,~memberOrder", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // UNSURE: host-gated (files_by_path); file contents unknown — every layer/linetype/style/block name of the file would also be a ~recordNames hold
    // insert: file — defines/redefines block Part from the file, then inserts it
    r(
        "insert",
        r##"{"name":"Part","file":"/tmp/part.dxf","at":[0,0]}"##,
        "active,part",
        &[
            (
                "drawing",
                "~recordNames.block:part,header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE",
                "-",
            ),
            ("$0", "name", "-"),
        ],
    ),
    // attdef: explicit height — TEXTSTYLE copied as the style, current props
    r(
        "attribute.define",
        r##"{"tag":"T1","at":[0,60],"height":2}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.TEXTSTYLE,header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE",
                "-",
            ),
            ("$Standard", "name", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // attdef: height omitted — TEXTSIZE taken
    r(
        "attribute.define",
        r##"{"tag":"T1","at":[0,60]}"##,
        "active,part",
        &[
            (
                "drawing",
                "header.TEXTSIZE,header.TEXTSTYLE,header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE",
                "-",
            ),
            ("$Standard", "name", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // base: writes INSBASE, reads nothing
    // same-field decision: hold the values this write replaces.
    r("base", r##"{"at":[5,5]}"##, "", &[("drawing", "header.INSBASE", "-")]),
    // purge: default all — cascading named purges
    r("purge", r##"{}"##, "", &[("drawing", "~purge_all", "-")]),
    // purge: blocks by name pattern
    r("purge", r##"{"type":"blocks","names":"*"}"##, "", &[("drawing", "~purge_blocks", "-")]),
    // purge: zero-length geometry (scan of model + layouts, no lock check)
    r("purge", r##"{"type":"zerolength"}"##, "", &[("drawing", "~purge_zeroLength", "-")]),
    // purge: layers
    r("purge", r##"{"type":"layers"}"##, "", &[("drawing", "~purge_layers", "-")]),
    // UNSURE: host-gated (path); the drawing is not written, only the file — holds are what the file's content depends on
    // wblock: a block by name, with the drawing's tables
    r(
        "block.write",
        r##"{"path":"/tmp/w.dxf","name":"Door"}"##,
        "",
        &[("$Door", "*", "-"), ("drawing", "layers,linetypes,textStyles,dimStyles,blocks", "-")],
    ),
    // UNSURE: host-gated
    // wblock: objects by handle
    r(
        "block.write",
        r##"{"path":"/tmp/w.dxf","handles":["$L1"]}"##,
        "",
        &[("$L1", "*", "-"), ("drawing", "layers,linetypes,textStyles,dimStyles,blocks", "-")],
    ),
    // UNSURE: host-gated
    // wblock: selection fallback
    r(
        "block.write",
        r##"{"path":"/tmp/w.dxf"}"##,
        "selection",
        &[("$Circ", "*", "-"), ("drawing", "layers,linetypes,textStyles,dimStyles,blocks", "-")],
    ),
    // cutclip: by handle — copied whole to the session clipboard and erased (locked skipped)
    r("edit.cut", r##"{"handles":["$L1"]}"##, "", &[("$L1", "*", "-")]),
    // cutclip: selection fallback
    r("edit.cut", r##"{}"##, "selection", &[("$Circ", "*", "-")]),
    // edit.delete: always the session selection, locked skipped
    r("edit.delete", r##"{}"##, "selection", &[("$Circ", "*", "-")]),
    // UNSURE: the paste reads only the SESSION clipboard (empty in the fixture ⇒ command disabled, "the clipboard is empty"); with content it would hold ~recordNames of every layer/linetype/style/block name the clipboard carries
    // pasteclip: clipboard moved so its base lands on `at`, into the current space
    r("edit.paste", r##"{"at":[0,0]}"##, "part", &[]),
    // UNSURE: same — empty clipboard ⇒ disabled; the A$C… block name is only made unique
    // pasteblock: clipboard as a new block, inserted with current props in current space
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/clipboard.rs:99-103 errors on the empty fixture clipboard before reading drawing defaults; remove defaults and layer identity]
    r("edit.pasteAsBlock", r##"{"at":[0,0]}"##, "active,part", &[]),
    // UNSURE: needs a clipboard copied from ANOTHER drawing; disabled in the fixture
    // pasteorig: clipboard at its original coordinates
    r("edit.pasteToOriginalCoordinates", r##"{}"##, "part", &[]),
    // u: last step of the caller's undo scope — the drawing swaps back to the snapshot
    r("u", r##"{}"##, "", &[("drawing", "~history", "-")]),
    // undo: count steps
    r("undo", r##"{"count":2}"##, "", &[("drawing", "~history", "-")]),
    // undo: other scope
    r("undo", r##"{"scope":"agent"}"##, "", &[("drawing", "~history", "-")]),
    // redo: redo stack of the scope (empty right after the fixture ⇒ "Nothing to redo")
    r("redo", r##"{}"##, "", &[("drawing", "~history", "-")]),
    // group: named, handles — members must exist (missing ones silently dropped); a name clash errors
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/groups.rs:70-90,145-155 checks the typed destination name; add name-gap replay]
    r("group", r##"{"name":"G2","handles":["$Circ","$Arc"]}"##, "", &[("$Circ", "-", "-"), ("$Arc", "-", "-"), ("~place", "name", "-")]),
    // group: unnamed, selection fallback
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/groups.rs:60-62 chooses the first unused *A name; add ~nextGroupName]
    r("group", r##"{}"##, "selection", &[("$Circ", "-", "-"), ("drawing", "~nextGroupName", "-")]),
    // groupedit: add — members read for de-duplication, added objects must exist
    r("group.edit", r##"{"name":"G1","add":["$Circ"]}"##, "", &[("$G1", "name,data.members", "-"), ("$Circ", "-", "-")]),
    // groupedit: remove — group deleted when it ends empty
    r("group.edit", r##"{"name":"G1","remove":["$L1"]}"##, "", &[("$G1", "name,data.members", "-")]),
    // groupedit: rename (clash errors)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/groups.rs:70-90,145-155 checks the typed destination name; add name-gap replay]
    r("group.edit", r##"{"name":"G1","rename":"Walls group"}"##, "", &[("$G1", "name", "-"), ("~place", "name", "-")]),
    // ungroup: by name — the group record is removed
    r("ungroup", r##"{"name":"G1"}"##, "", &[("$G1", "name,data", "-")]),
    // ungroup: by handle — every group holding the object
    r("ungroup", r##"{"handles":["$L1"]}"##, "", &[("$L1", "~groups", "-")]),
    // ungroup: selection fallback ($Circ is in no group ⇒ error "not in a group")
    // Review: removed `ungroup {}` on the selection [$Circ], which is in no group (the call fails
    // on this fixture); the selection form is the `handles` row's.
    // align: transform targets in place (locked skipped)
    r("align", r##"{"handles":["$L1"],"s1":[20,0],"d1":[0,100],"s2":[30,0],"d2":[0,110]}"##, "", &[("$L1", "kind,~locked", "-")]),
    // align: selection fallback
    r("align", r##"{"s1":[50,5],"d1":[60,5]}"##, "selection", &[("$Circ", "kind,~locked", "-")]),
    // arraypath: copies of the objects along the path (copies in each object's own space)
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r(
        "object.arrayPath",
        r##"{"handles":["$Circ"],"path":"$Spline","count":4}"##,
        "",
        &[("$Circ", "common,kind,~space", "-"), ("$Spline", "kind", "-")],
    ),
    // arraypath: selection fallback, spacing
    // C8-cad-read-4 source correction: modify.rs:108-124 clones stored common/kind into the source space; derived eligibility and groups are not copied.
    r("object.arrayPath", r##"{"path":"$L1","spacing":2}"##, "selection", &[("$Circ", "common,kind,~space", "-"), ("$L1", "kind.a,kind.b", "-")]),
    // blend: spline between the picked ends, current props, current space
    r(
        "blend",
        r##"{"h1":"$L1","p1":[30,0],"h2":"$Arc","p2":[75,5]}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // blend: smooth continuity
    r(
        "blend",
        r##"{"h1":"$L1","p1":[30,0],"h2":"$Spline","p2":[20,40],"continuity":"smooth"}"##,
        "active,part",
        &[
            ("$L1", "kind.a,kind.b", "-"),
            ("$Spline", "kind", "-"),
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // C8-cad-read-4: modify2.rs:1494-1558 chooses the first valid viewport, transforms
    // geometry and preserves common properties; assoc.rs:247-313 and io/dxf_write.rs:1323-1337
    // remeasure and serialize attached dimensions using their layer and rendered text metrics.
    r(
        "object.changeSpace",
        r##"{"handles":["$L1"],"to":"Layout1"}"##,
        "part",
        &[
            ("$L1", "kind.type,kind.a,kind.b,~space,~locked,~solve", "-"),
            ("$Dim", "kind,common.layer", "-"),
            ("$0", "name,data.locked", "-"),
            ("$DimStandard", "name,data", "-"),
            ("$Standard", "name,data.font,data.height,data.widthFactor,data.oblique", "-"),
            ("$Layout1", "name,~chspaceViewport", "-"),
            ("$Vp", "kind.center,kind.height,kind.viewCenter,kind.viewHeight", "-"),
        ],
    ),
    // The omitted destination also consumes tab order, including the unchosen layout.
    r(
        "object.changeSpace",
        r##"{}"##,
        "part,selection",
        &[
            ("$Circ", "kind.type,kind.center,kind.radius,~space,~locked,~solve", "-"),
            ("drawing", "layouts", "-"),
            ("$Layout1", "name,data.tabOrder,~chspaceViewport", "-"),
            ("$Layout2", "data.tabOrder", "-"),
            ("$Vp", "kind.center,kind.height,kind.viewCenter,kind.viewHeight", "-"),
        ],
    ),
    // flatten: z zeroed, thickness 0 (locked skipped)
    r("flatten", r##"{"handles":["$L1"]}"##, "", &[("$L1", "kind.type,kind.a.z,kind.b.z,common.thickness,~locked", "-")]),
    // flatten: selection fallback
    r("flatten", r##"{}"##, "selection", &[("$Circ", "kind.type,kind.center.z,common.thickness,~locked", "-")]),
    // lengthen: measure only (answer), no write
    r("object.lengthen", r##"{"handle":"$L1"}"##, "", &[]),
    // lengthen: delta on a line at the end nearest pick (locked ⇒ error)
    r("object.lengthen", r##"{"handle":"$L1","delta":5,"pick":[30,0]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // lengthen: arc by angle
    r("object.lengthen", r##"{"handle":"$Arc","deltaAngle":10}"##, "", &[("$Arc", "kind.center,kind.radius,kind.start,kind.end,~locked", "-")]),
    // lengthen: selection fallback ($Circ is not lengthenable ⇒ error)
    r("object.lengthen", r##"{"delta":5}"##, "selection", &[("$Circ", "kind.type,~locked", "-")]),
    // ncopy: all leaves of the reference, moved by delta, into the current space with the leaves' own props
    r(
        "object.copyNested",
        r##"{"handle":"$Ins","delta":[0,20]}"##,
        "part",
        &[("$Ins", "kind.block,kind.insert,kind.scale,kind.rotation", "-"), ("$Door", "name,data.base,~memberOrder", "-")],
    ),
    // ncopy: nearest leaf to pick
    r(
        "object.copyNested",
        r##"{"handle":"$Ins","pick":[105,80],"from":[0,0],"to":[0,10]}"##,
        "part",
        &[("$Ins", "kind.block,kind.insert,kind.scale,kind.rotation", "-"), ("$Door", "name,data.base,~memberOrder", "-")],
    ),
    // pedit: open a polyline (locked ⇒ error)
    // C8-cad-read-4: modify2.rs:272-273 assigns closed even for OPEN; coordinates are preserved.
    r("polyline.edit", r##"{"handle":"$Rect","option":"open"}"##, "", &[("$Rect", "kind.type,kind.closed,~locked", "-")]),
    // pedit: line converted to a polyline and closed
    r("polyline.edit", r##"{"handle":"$L1","option":"close"}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // pedit: vertex move by index
    r(
        "polyline.edit",
        r##"{"handle":"$Rect","option":"vertex","index":1,"action":"move","to":[12,0]}"##,
        "",
        &[("$Rect", "kind.vertices,~locked", "-")],
    ),
    // pedit: join — connected others are merged and erased (their lock is not checked)
    r("polyline.edit", r##"{"handle":"$L1","option":"join","handles2":["$L2"]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-"), ("$L2", "*", "-")]),
    // pedit: selection fallback ($Circ ⇒ "not a polyline" error)
    r("polyline.edit", r##"{"option":"width","width":1}"##, "selection", &[("$Circ", "kind.type,~locked", "-")]),
    // reverse: line endpoints swapped (locked skipped)
    r("reverse", r##"{"handles":["$L1"]}"##, "", &[("$L1", "kind.a,kind.b,~locked", "-")]),
    // reverse: spline
    r("reverse", r##"{"handles":["$Spline"]}"##, "", &[("$Spline", "kind,~locked", "-")]),
    // reverse: selection fallback (circle not reversible ⇒ nothing written)
    r("reverse", r##"{}"##, "selection", &[("$Circ", "kind.type,~locked", "-")]),
    // splinedit: reverse
    r("spline.edit", r##"{"handle":"$Spline","option":"reverse"}"##, "", &[("$Spline", "kind,~locked", "-")]),
    // splinedit: move a fit point by index (refit)
    r("spline.edit", r##"{"handle":"$Spline","option":"move","index":1,"to":[10,50]}"##, "", &[("$Spline", "kind,~locked", "-")]),
    // splinedit: convert to polyline
    r("spline.edit", r##"{"handle":"$Spline","option":"polyline","precision":4}"##, "", &[("$Spline", "kind,~locked", "-")]),
    // splinedit: selection fallback ($Circ ⇒ "not a spline" error)
    r("spline.edit", r##"{"option":"reverse"}"##, "selection", &[("$Circ", "kind.type,~locked", "-")]),
    // textedit: text value
    // same-field decision: hold the values this write replaces.
    r("text.edit", r##"{"handle":"$Text","text":"Bye"}"##, "", &[("$Text", "kind.type,~locked", "-"), ("$Text", "kind.value", "-")]),
    // textedit: dimension override (dimension block cleared)
    // same-field decision: hold the values this write replaces.
    r("text.edit", r##"{"handle":"$Dim","text":"<> mm"}"##, "", &[("$Dim", "kind.type,~locked", "-"), ("$Dim", "kind.text", "-")]),
    // textedit: selection fallback ($Circ ⇒ error)
    r("text.edit", r##"{"text":"x"}"##, "selection", &[("$Circ", "kind.type,~locked", "-")]),
    // find: matches in the current space become the selection (no drawing write)
    // C8-cad-read-3 source correction: utility.rs:246-301: matching membership and matched words are required; nonmatching text is preserved, and the matching predicate is replayed.
    // C8-cad-read-4 source correction: utility.rs:296-300 writes only the matching handle selection;
    // changed words that keep matching affect only reply text/count. The wider guard is waiting on SDK selection-consumer planning.
    // [corrected C8-cad-pin: utility.rs:247-280 reads words from stored kind in the active space; public SDK coverage replays kind/~space instead of a bounded aggregate ~texts receipt.]
    r("find", r##"{"find":"Hello"}"##, "part", &[("~place", "kind,~space", "-")]),
    // find: replace in every match in the current space (no lock check)
    // C8-cad-read-3 source correction: utility.rs:246-301: matching membership and matched words are required; nonmatching text is preserved, and the matching predicate is replayed.
    // C8-cad-read-4 audit: utility.rs:291-297 assigns matched kinds; locks.rs:9-48 rejects a locked target.
    // [corrected C8-cad-pin: utility.rs:247-297 uses the same stored-kind/space membership predicate for replacement.]
    r("find", r##"{"find":"Hello","replace":"Bye"}"##, "part", &[("$Text", "~words,~locked", "-"), ("~place", "kind,~space", "-")]),
    // ---- Reading F ----
    // ---------------- group F: layers ----------------
    // layer: read-only listing, writes nothing
    r("layer", r##"{}"##, "", &[]),
    // layer.add: plain new layer (name clash is an error, not a silent change)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-193 checks the destination layer name; add name-gap replay]
    r("layer.add", r##"{"name":"Red","color":1}"##, "", &[("~place", "name", "-")]),
    // layer.add: linetype given by name; the record's canonical name is copied
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-193 checks the destination layer name; add name-gap replay]
    r("layer.add", r##"{"name":"Hidden","linetype":"DASHED"}"##, "", &[("$Dashed", "name", "-"), ("~place", "name", "-")]),
    // layer.add: current:true writes CLAYER (overwrites, does not read)
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r("layer.add", r##"{"name":"Cur","current":true}"##, "", &[("drawing", "header.CLAYER", "-"), ("~place", "name", "-")]),
    // layer.set: colour edit; the whole layer record is cloned and written back (lost update)
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    // same-field decision: hold the values this write replaces.
    r("layer.set", r##"{"name":"Walls","color":5}"##, "", &[("$Walls", "name", "-"), ("$Walls", "data.color", "-")]),
    // layer.set: frozen:true is refused on the current layer (reads CLAYER)
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    // review: layer.rs::run_set requires CLAYER != Walls; changing
    // that predicate changes whether the requested result can exist.
    // same-field decision: hold the values this write replaces.
    r("layer.set", r##"{"name":"Walls","frozen":true}"##, "", &[("$Walls", "name,~canFreeze", "-"), ("$Walls", "data.frozen", "-")]),
    // layer.set: linetype by name + whole-record write
    // Review R4 (lost update): the handler replaces the whole value on the drawing as it is when the
    // write runs, so no other writer's field is lost; the row holds what decides the result.
    // same-field decision: hold the values this write replaces.
    r(
        "layer.set",
        r##"{"name":"Notes","linetype":"DASHED"}"##,
        "",
        &[("$Notes", "name", "-"), ("$Dashed", "name", "-"), ("$Notes", "data.linetype", "-")],
    ),
    // layer.set: rename repoints every entity on the layer (model, layouts, blocks) and CLAYER if it named it
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r(
        "layer.set",
        r##"{"name":"Walls","newName":"Wall2"}"##,
        "",
        // Review R4: the members and CLAYER follow the rename on the drawing as it is then.
        &[("$Walls", "name", "-"), ("~place", "name", "-")],
    ),
    // layer.setCurrent: thaws the layer (single field), snapshots all layers to LAYERP, writes CLAYER
    // UNSURE: holding `layers` for the LAYERP snapshot (prev_push serialises every layer record whole)
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.setCurrent",
        r##"{"name":"Walls"}"##,
        "",
        &[("$Walls", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.CLAYER,header.KITION_CAD_LAYERP", "-")],
    ),
    // layer.delete: refuses 0/Defpoints/current and used layers; deletes the record
    // UNSURE: every fixture layer is protected, current (0) or used, so the JSON form always refuses here
    r("layer.delete", r##"{"name":"Notes"}"##, "", &[("$Notes", "*", "-"), ("drawing", "header.CLAYER,~layerMembers.notes", "-")]),
    // laymcur: handle → its common.layer becomes CLAYER (record not consulted)
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r("layer.makeObjectCurrent", r##"{"handle":"$L1"}"##, "", &[("$L1", "common.layer", "-"), ("drawing", "header.CLAYER", "-")]),
    // laymcur: selection fallback
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r("layer.makeObjectCurrent", r##"{}"##, "selection", &[("$Circ", "common.layer", "-"), ("drawing", "header.CLAYER", "-")]),
    // laymch: target layer by name (created silently if missing); writes common.layer of each target
    // same-field decision: hold the values this write replaces.
    r("layer.match", r##"{"handle":"$Circ","layer":"Walls"}"##, "", &[("$Circ", "-", "-"), ("$Walls", "name", "-"), ("$Circ", "common.layer", "-")]),
    // laymch: selection fallback
    // same-field decision: hold the values this write replaces.
    r("layer.match", r##"{"layer":"Notes"}"##, "selection", &[("$Circ", "-", "-"), ("$Notes", "name", "-"), ("$Circ", "common.layer", "-")]),
    // laycur: writes CLAYER into each target's common.layer (locks ignored)
    // same-field decision: hold the values this write replaces.
    r("layer.changeToCurrent", r##"{"handle":"$L1"}"##, "", &[("$L1", "-", "-"), ("drawing", "header.CLAYER", "-"), ("$L1", "common.layer", "-")]),
    // laycur: selection fallback
    // same-field decision: hold the values this write replaces.
    r("layer.changeToCurrent", r##"{}"##, "selection", &[("$Circ", "-", "-"), ("drawing", "header.CLAYER", "-"), ("$Circ", "common.layer", "-")]),
    // layiso: keep the targets' layers, turn every other layer off; LAYERP+LAYISO snapshots; CLAYER = kept layer
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.isolate",
        r##"{"handle":"$L1"}"##,
        "",
        &[
            ("$L1", "common.layer", "-"),
            ("drawing", "layers", "-"),
            ("drawing", "header.CLAYER,header.KITION_CAD_LAYERP,header.KITION_CAD_LAYISO", "-"),
        ],
    ),
    // layiso: selection fallback
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.isolate",
        r##"{}"##,
        "selection",
        &[
            ("$Circ", "common.layer", "-"),
            ("drawing", "layers", "-"),
            ("drawing", "header.CLAYER,header.KITION_CAD_LAYERP,header.KITION_CAD_LAYISO", "-"),
        ],
    ),
    // layuniso: replaces the whole layers list by the LAYISO snapshot
    // UNSURE: `layers` held because the whole-list replace drops layers/changes made since the snapshot
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:281-286,364-368 errors on the absent fixture history before replacing any layer; remove layer contents, retain the history key]
    r("layer.unisolate", r##"{}"##, "", &[("drawing", "header.KITION_CAD_LAYISO", "-")]),
    // layfrz: target's layer frozen (single field) unless it is CLAYER; LAYERP snapshot of all layers
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.freeze",
        r##"{"handle":"$L1"}"##,
        "",
        &[
            ("$L1", "common.layer", "-"),
            ("$Walls", "name", "-"),
            ("drawing", "header.CLAYER,layers", "-"),
            ("drawing", "header.KITION_CAD_LAYERP", "-"),
        ],
    ),
    // layfrz: selection fallback ($Circ on current layer 0: freeze silently reverted)
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.freeze",
        r##"{}"##,
        "selection",
        &[
            ("$Circ", "common.layer", "-"),
            ("$0", "name", "-"),
            ("drawing", "header.CLAYER,layers", "-"),
            ("drawing", "header.KITION_CAD_LAYERP", "-"),
        ],
    ),
    // layoff: target's layer data.on=false; LAYERP snapshot
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.off",
        r##"{"handle":"$Text"}"##,
        "",
        &[("$Text", "common.layer", "-"), ("$Notes", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")],
    ),
    // layoff: selection fallback
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.off",
        r##"{}"##,
        "selection",
        &[("$Circ", "common.layer", "-"), ("$0", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")],
    ),
    // laylck: target's layer data.locked=true; LAYERP snapshot
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.lock",
        r##"{"handle":"$L1"}"##,
        "",
        &[("$L1", "common.layer", "-"), ("$Walls", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")],
    ),
    // laylck: selection fallback
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.lock",
        r##"{}"##,
        "selection",
        &[("$Circ", "common.layer", "-"), ("$0", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")],
    ),
    // layulk: target's layer data.locked=false; LAYERP snapshot
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.unlock",
        r##"{"handle":"$L1"}"##,
        "",
        &[("$L1", "common.layer", "-"), ("$Walls", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")],
    ),
    // layulk: selection fallback
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "layer.unlock",
        r##"{}"##,
        "selection",
        &[("$Circ", "common.layer", "-"), ("$0", "name", "-"), ("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")],
    ),
    // layon: every layer data.on=true; LAYERP snapshot
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r("layer.allOn", r##"{}"##, "", &[("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")]),
    // laythw: every layer data.frozen=false; LAYERP snapshot
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r("layer.thawAll", r##"{}"##, "", &[("drawing", "layers", "-"), ("drawing", "header.KITION_CAD_LAYERP", "-")]),
    // layerp: replaces the whole layers list by the LAYERP snapshot
    // UNSURE: needs a prior LAYERP snapshot (fixture may have none ⇒ "No previous layer state"); `layers` held for the whole-list replace
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:281-286,364-368 errors on the absent fixture history before replacing any layer; remove layer contents, retain the history key]
    r("layer.previous", r##"{}"##, "", &[("drawing", "header.KITION_CAD_LAYERP", "-")]),
    // layerState.save: new state copies every layer whole
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:397-402 copies every current layer value into the saved state; add all four layer data premises]
    r(
        "layerState.save",
        r##"{"name":"LS2"}"##,
        "",
        &[
            ("drawing", "layers,~recordNames.layerstate:ls2", "-"),
            ("$0", "name,data", "-"),
            ("$Walls", "name,data", "-"),
            ("$Notes", "name,data", "-"),
            ("$Defpoints", "name,data", "-"),
        ],
    ),
    // layerState.save: an existing name is silently overwritten
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:397-402 copies every current layer value into the saved state; add all four layer data premises]
    r(
        "layerState.save",
        r##"{"name":"LS1"}"##,
        "",
        &[
            ("$LS1", "*", "-"),
            ("drawing", "layers", "-"),
            ("$0", "name,data", "-"),
            ("$Walls", "name,data", "-"),
            ("$Notes", "name,data", "-"),
            ("$Defpoints", "name,data", "-"),
        ],
    ),
    // layerState.restore: each saved layer replaces the same-named layer record whole
    // UNSURE: assumes LS1 was saved from the fixture's four layers
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:406-414 assigns every field of each saved layer; add assigned data for all saved layer identities]
    r(
        "layerState.restore",
        r##"{"name":"LS1"}"##,
        "",
        &[
            ("$LS1", "name,data.layers", "-"),
            ("$0", "name", "-"),
            ("$Walls", "name", "-"),
            ("$Notes", "name", "-"),
            ("$Defpoints", "name", "-"),
            ("$0", "name,data", "-"),
            ("$Walls", "name,data", "-"),
            ("$Notes", "name,data", "-"),
            ("$Defpoints", "name,data", "-"),
        ],
    ),
    // layerState.delete: removes the named state
    r("layerState.delete", r##"{"name":"LS1"}"##, "", &[("$LS1", "*", "-")]),
    // layerState.rename: renames the named state (clash with `to` is an error)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:440-451 rejects an occupied destination layer-state name; add name-gap replay]
    r("layerState.rename", r##"{"from":"LS1","to":"LS9"}"##, "", &[("$LS1", "name", "-"), ("~place", "name", "-")]),
    // laymrg: move every object on Notes to Walls, drop Notes from viewports, delete Notes; CLAYER follows if it was Notes
    r(
        "layer.merge",
        r##"{"from":["Notes"],"to":"Walls"}"##,
        "",
        &[("$Notes", "*", "-"), ("$Walls", "name", "-"), ("drawing", "header.CLAYER,~layerMembers.notes", "-")],
    ),
    // laydel: delete Notes and every object on it (model, layouts, block definitions); refuses CLAYER
    r("layer.deleteWithObjects", r##"{"names":["Notes"]}"##, "", &[("$Notes", "*", "-"), ("drawing", "header.CLAYER,~layerMembers.notes", "-")]),
    // ---------------- group F: layouts ----------------
    // layout option new: as layout.add with a name (default viewport)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout.rs:149-164 validates the destination layout name against its typed table; add name-gap replay]
    r(
        "layout",
        r##"{"option":"new","name":"Sheet"}"##,
        "active",
        &[
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~layoutSeed,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
            ("~place", "name", "-"),
        ],
    ),
    // layout option copy: name → from; copies Layout1 whole
    r(
        "layout",
        r##"{"option":"copy","name":"Layout1","to":"Sheet"}"##,
        "",
        &[
            ("$Layout1", "name,data.page,data.view,~memberOrder", "-"),
            ("$Vp", "common,kind", "-"),
            ("drawing", "~layoutSeed", "-"),
            ("~place", "name", "-"),
        ],
    ),
    // layout option delete
    // UNSURE: the "keep at least one layout" refusal makes the other layout's existence matter
    r("layout", r##"{"option":"delete","name":"Layout2"}"##, "", &[("$Layout2", "*", "-"), ("drawing", "~layoutCanDelete", "-")]),
    // layout option rename: name → from
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout.rs:149-164 validates the destination layout name against its typed table; add name-gap replay]
    r("layout", r##"{"option":"rename","name":"Layout1","to":"Sheet"}"##, "", &[("$Layout1", "name", "-"), ("~place", "name", "-")]),
    // C8-cad-read-4: view.rs:247 resolves the named sheet. lib.rs:621-627
    // brackets the agent dispatcher, preserving page/view state but not erasing
    // the observed address. Only existence and case-insensitive name replay matter.
    r("layout", r##"{"option":"set","name":"Layout1"}"##, "", &[("$Layout1", "", "-"), ("~place", "name", "-")]),
    // layout.add: named, default viewport over the printable area zoomed to model extents
    r(
        "layout.add",
        r##"{"name":"Sheet"}"##,
        "active",
        &[
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~layoutSeed,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
            ("~place", "name", "-"),
        ],
    ),
    // layout.add: name omitted ⇒ first free LayoutN (Layout3) chosen silently
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout.rs:173-174 chooses the first free Layout name; replace absence of Layout3 with ~nextLayoutName (renaming Layout1 frees an earlier candidate)]
    r(
        "layout.add",
        r##"{}"##,
        "active",
        &[
            ("$0", "name", "-"),
            (
                "drawing",
                "~nextLayoutName,header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~layoutSeed,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    // layout.add: no viewport
    r("layout.add", r##"{"name":"Sheet","viewport":false}"##, "", &[("drawing", "header.MEASUREMENT,~layoutSeed", "-"), ("~place", "name", "-")]),
    // layout.delete: removes the layout and its entities
    // UNSURE: the "keep at least one layout" refusal makes the other layout's existence matter
    r("layout.delete", r##"{"name":"Layout2"}"##, "", &[("$Layout2", "*", "-"), ("drawing", "~layoutCanDelete", "-")]),
    // layout.rename: from by name (from omitted needs a current layout; Model is current in the fixture)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout.rs:149-164 validates the destination layout name against its typed table; add name-gap replay]
    r("layout.rename", r##"{"from":"Layout1","to":"Sheet"}"##, "", &[("$Layout1", "name", "-"), ("~place", "name", "-")]),
    // C8-cad-read-4: layout.rs:280-298 copies page/view and ordered stored members,
    // replacing name/tab order. ~memberOrder is that exact ordered identity list.
    // layout.duplicate: named target
    r(
        "layout.duplicate",
        r##"{"from":"Layout1","to":"Sheet"}"##,
        "",
        &[
            ("$Layout1", "name,data.page,data.view,~memberOrder", "-"),
            ("$Vp", "common,kind", "-"),
            ("drawing", "~layoutSeed", "-"),
            ("~place", "name", "-"),
        ],
    ),
    // layout.duplicate: `to` omitted ⇒ "Layout1 (2)" chosen silently
    r(
        "layout.duplicate",
        r##"{"from":"Layout1"}"##,
        "",
        &[
            ("$Layout1", "name,data.page,data.view,~memberOrder", "-"),
            ("$Vp", "common,kind", "-"),
            ("drawing", "~recordNames.layout:layout1 (2),~layoutSeed", "-"),
        ],
    ),
    // mview: one viewport over Layout1's printable area (page + MEASUREMENT)
    r(
        "viewport.add",
        r##"{"layout":"Layout1"}"##,
        "active",
        &[
            ("$Layout1", "name,data.page", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    // mview: explicit corners, 2 viewports (page not read)
    r(
        "viewport.add",
        r##"{"layout":"Layout1","p1":[0,0],"p2":[9,6],"count":2}"##,
        "active",
        &[
            ("$Layout1", "name", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    // vports: same handler as mview
    r(
        "viewport.new",
        r##"{"layout":"Layout1","count":4}"##,
        "active",
        &[
            ("$Layout1", "name,data.page", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    r(
        "viewport.new",
        r##"{"layout":"Layout1","p1":[0,0],"p2":[9,6],"count":3,"arrangement":"left"}"##,
        "active",
        &[
            ("$Layout1", "name", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    // viewport.one .. viewport.four: fixed count over the printable area or the given corners
    r(
        "viewport.one",
        r##"{"layout":"Layout1"}"##,
        "active",
        &[
            ("$Layout1", "name,data.page", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    r(
        "viewport.two",
        r##"{"layout":"Layout1","arrangement":"horizontal"}"##,
        "active",
        &[
            ("$Layout1", "name,data.page", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    r(
        "viewport.three",
        r##"{"layout":"Layout1","p1":[0,0],"p2":[9,6]}"##,
        "active",
        &[
            ("$Layout1", "name", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    r(
        "viewport.four",
        r##"{"layout":"Layout1"}"##,
        "active",
        &[
            ("$Layout1", "name,data.page", "-"),
            ("$0", "name", "-"),
            (
                "drawing",
                "header.MEASUREMENT,header.CLAYER,header.CECOLOR,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE,~viewportSeed,~newVpLayers,~modelBounds",
                "-",
            ),
        ],
    ),
    // viewport.set: viewport by handle; its kind is cloned and replaced whole
    // UNSURE: the fixture has no paper-space viewport; `$Vp` stands for one (e.g. made by mview on Layout1)
    // C8-cad-read-4 source correction: layout.rs:617-710 reads id/height/lock and assigns viewHeight; other viewport fields survive.
    r(
        "viewport.set",
        r##"{"handle":"$Vp","scale":"1:50"}"##,
        "",
        &[("$Vp", "kind.type,kind.id,kind.height,kind.viewHeight,kind.locked,~locked", "-")],
    ),
    // viewport.set: no handle ⇒ selected viewports, else the active MSPACE viewport
    // UNSURE: on the fixture the selection [$Circ] is filtered out and there is no active viewport ⇒ error
    r("viewport.set", r##"{"locked":true}"##, "part,selection", &[("$Circ", "kind.type", "-")]),
    // vplayer: same handler; layer names are stored as typed, not resolved
    // UNSURE: needs a viewport (`$Vp`), absent from the fixture
    // C8-cad-read-4 source correction: layout.rs:692-706 edits only the named layer list and resolves its named reference.
    r(
        "viewport.freezeLayer",
        r##"{"handle":"$Vp","freeze":["Walls"]}"##,
        "",
        &[("$Vp", "kind.type,kind.id,kind.frozenLayers,~locked", "-"), ("$Walls", "name", "-")],
    ),
    // C8-cad-read-4 source correction: layout.rs:692-706 edits only the named layer list and resolves its named reference.
    r(
        "viewport.freezeLayer",
        r##"{"handle":"$Vp","colors":{"Walls":"red"}}"##,
        "",
        &[("$Vp", "kind.type,kind.id,kind.layerColors,~locked", "-"), ("$Walls", "name", "-")],
    ),
    // pagesetup: page of Layout1 cloned, edited and written back whole
    r("plot.pageSetup", r##"{"layout":"Layout1","paper":"A3","landscape":true}"##, "", &[("$Layout1", "name,data.page", "-")]),
    // pagesetup: Model current, no changes ⇒ report only
    r("plot.pageSetup", r##"{}"##, "part", &[]),
    // ---- Reading G ----
    // commandLine: runs typed text; holds what the typed command holds (LTSCALE: a header write that reads nothing)
    // UNSURE: holdings depend entirely on the typed text; this row uses a command that reads no drawing state
    r("command.runLine", r##"{"text":"ltscale 2"}"##, "", &[]),
    // [corrected C8-cad-pin: crates/engine/engine/src/lib.rs:1120-1131 delegates name {json} to the native handler; props.rs:433-438 assigns LTSCALE, so hold the assigned field.]
    r("command.runLine", r##"{"text":"ltscale {\"scale\":2}"}"##, "", &[("drawing", "header.LTSCALE", "-")]),
    // edit.undo: restores the drawing from the top undo step of the scope (whole drawing replaced, selection cleared)
    r("edit.undo", r##"{}"##, "", &[("drawing", "~history", "-")]),
    // edit.redo: restores the drawing from the top redo step of the scope
    // UNSURE: on the fixture the redo stack may be empty (then the call returns null and writes nothing)
    r("edit.redo", r##"{}"##, "", &[("drawing", "~history", "-")]),
    // close: removes the active drawing's whole document state (drawing + history), unsaved edits discarded
    r("close", r##"{}"##, "", &[("drawing", "*", "-")]),
    // closeall: removes every open drawing
    r("file.closeAll", r##"{}"##, "", &[("drawing", "*", "-")]),
    // file.switch: replaces only the session's active-document index; the drawing is untouched
    r("file.switch", r##"{"index":0}"##, "", &[]),
    // new: adds a new drawing and makes it active; the current drawing is untouched (replaces the active index)
    r("new", r##"{}"##, "", &[]),
    // open: reads a file into a new drawing and makes it active; the current drawing is untouched
    // UNSURE: needs the io hooks installed and valid DXF bytes
    r("open", r##"{"data":"MApTRUNUSU9OCjIKRU5USVRJRVMKMApFTkRTRUMKMApFT0YK","name":"a.dxf"}"##, "", &[]),
    // qsave: serialises the whole drawing to the file and marks it saved (drawing content not changed)
    // UNSURE: on the web host files may be host-owned (files_by_path refuses a path)
    r("file.save", r##"{"path":"/tmp/g-qsave.dxf"}"##, "", &[("drawing", "*", "*")]),
    // saveas: same, to an explicit path
    // UNSURE: same host-files caveat
    r("file.saveAs", r##"{"path":"/tmp/g-saveas.dxf"}"##, "", &[("drawing", "*", "*")]),
    // plotstyle: by handle; skips a target on a locked layer silently, writes common.plotStyle
    // same-field decision: hold the values this write replaces.
    r("plot.style", r##"{"name":"ByLayer","handles":["$Circ"]}"##, "", &[("$Circ", "~locked", "-"), ("$Circ", "common.plotStyle", "-")]),
    // plotstyle: no handles -> falls back to the session selection
    // same-field decision: hold the values this write replaces.
    r("plot.style", r##"{"name":"ByLayer"}"##, "selection", &[("$Circ", "~locked", "-"), ("$Circ", "common.plotStyle", "-")]),
    // plotstyle: empty handle list -> sets header CPLOTSTYLE only
    // same-field decision: hold the values this write replaces.
    r("plot.style", r##"{"name":"ByLayer","handles":[]}"##, "", &[("drawing", "header.CPLOTSTYLE", "-")]),
    // stylesmanager: set edits a copy of the table (drawing copy shadows the built-in) and stores it whole
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout/plotstyle.rs:113-114 resolves the builtin or stored table by name; encode the same source gap as typed name replay rather than an aggregate registry subpath (representation only)]
    r("plot.styleManager", r##"{"action":"set","name":"monochrome.ctb","style":{"name":"Color_1","screening":50}}"##, "", &[("~place", "name", "-")]),
    // stylesmanager: new copied from a table named by `from`
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout/plotstyle.rs:210-242 validates the destination .ctb/.stb name; add typed destination-gap replay, retaining the source lookup]
    r("plot.styleManager", r##"{"action":"new","name":"mine.ctb","from":"monochrome.ctb"}"##, "", &[("~place", "name", "-")]),
    // stylesmanager: new from scratch (a name clash errors; nothing read)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layout/plotstyle.rs:210-242 validates the destination .ctb/.stb name; add typed destination-gap replay, retaining the source lookup]
    r("plot.styleManager", r##"{"action":"new","name":"mine2.ctb"}"##, "", &[("~place", "name", "-")]),
    // color: sets header CECOLOR (and CECOLOR_RGB); reads nothing
    // same-field decision: hold the values this write replaces.
    r("color", r##"{"color":1}"##, "", &[("drawing", "header.CECOLOR,header.CECOLOR_RGB", "-")]),
    // ddptype: sets header PDMODE / PDSIZE
    // same-field decision: hold the values this write replaces.
    r("point.style", r##"{"pdmode":3,"pdsize":1}"##, "", &[("drawing", "header.PDMODE,header.PDSIZE", "-")]),
    // dimstyle: edit an existing style by name, make it current, sync DIM* header vars from the whole style
    // same-field decision: hold the values this write replaces.
    r(
        "dimensionStyle.open",
        r##"{"name":"Standard","arrowSize":3}"##,
        "active",
        &[
            ("$DimStandard", "name,data", "-"),
            (
                "drawing",
                "header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2",
                "-",
            ),
        ],
    ),
    // dimstyle: a new style copies the current DIMSTYLE style whole as its base
    // same-field decision: hold the values this write replaces.
    r(
        "dimensionStyle.open",
        r##"{"name":"NewDim","arrowSize":3}"##,
        "active",
        &[
            ("drawing", "header.DIMSTYLE,~recordNames.dimstyle:newdim", "-"),
            ("$DimStandard", "name,data", "-"),
            (
                "drawing",
                "header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2",
                "-",
            ),
        ],
    ),
    // dimstyle: existing style, current:false -> only the named style's given fields change
    // same-field decision: hold the values this write replaces.
    r(
        "dimensionStyle.open",
        r##"{"name":"Standard","arrowSize":3,"current":false}"##,
        "active",
        &[
            ("drawing", "header.DIMSTYLE", "-"),
            ("$DimStandard", "name,data", "-"),
            (
                "drawing",
                "header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2",
                "-",
            ),
        ],
    ),
    // dimensionStyle.openManager: same handler as dimstyle (Dimension menu entry)
    // same-field decision: hold the values this write replaces.
    r(
        "dimensionStyle.openManager",
        r##"{"name":"Standard","arrowSize":3}"##,
        "active",
        &[
            ("$DimStandard", "name,data", "-"),
            (
                "drawing",
                "header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2",
                "-",
            ),
        ],
    ),
    // dimensionStyle.setCurrent: by name; sets DIMSTYLE and syncs DIM* vars from the whole style
    // same-field decision: hold the values this write replaces.
    r(
        "dimensionStyle.setCurrent",
        r##"{"name":"Standard"}"##,
        "",
        &[
            ("$DimStandard", "name,data", "-"),
            (
                "drawing",
                "header.DIMSTYLE,header.DIMSCALE,header.DIMASZ,header.DIMEXO,header.DIMEXE,header.DIMTXT,header.DIMGAP,header.DIMDEC,header.DIMADEC,header.DIMLFAC,header.DIMTAD,header.DIMTIH,header.DIMTOH,header.DIMBLK,header.DIMBLK1,header.DIMBLK2,header.DIMTSZ,header.DIMCLRD,header.DIMCLRE,header.DIMCLRT,header.DIMTXSTY,header.DIMPOST,header.DIMCEN,header.DIMZIN,header.DIMLUNIT,header.DIMTOL,header.DIMTP,header.DIMTM,header.DIMDLI,header.DIMDLE,header.DIMJUST,header.DIMRND,header.DIMDSEP,header.DIMFRAC,header.DIMLIM,header.DIMTDEC,header.DIMTFAC,header.DIMTZIN,header.DIMALT,header.DIMALTF,header.DIMALTD,header.DIMAPOST,header.DIMALTRND,header.DIMALTU,header.DIMALTZ,header.DIMAUNIT,header.DIMSE1,header.DIMSE2",
                "-",
            ),
        ],
    ),
    // dimensionStyle.delete: props.rs::record_eligibility publishes the same current/usage
    // predicate as ~canDelete on the stable D2 record, included in the whole hold.
    r("dimensionStyle.delete", r##"{"name":"D2"}"##, "", &[("dimstyle:D2", "*", "-"), ("dimstyle:D2", "~canDelete", "-")]),
    // dimensionStyle.override: by handle; only a Dimension's overrides map is merged
    // same-field decision; props.rs:810-829 / annotate.rs:714-737: only assigned dimension fields.
    r("dimensionStyle.override", r##"{"handles":["$Dim"],"DIMASZ":3}"##, "", &[("$Dim", "kind.type", "-"), ("$Dim", "~dimOverride_arrowSize", "-")]),
    // dimensionStyle.override: selection fallback ($Circ is not a dimension: skipped, nothing written)
    r("dimensionStyle.override", r##"{"DIMASZ":3}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // dimensionStyle.rename: renames the record, DIMSTYLE if current, and dims/leaders in model+layouts using it
    // UNSURE: Standard cannot be renamed and the fixture has no other dimension style
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r("dimensionStyle.rename", r##"{"from":"D2","to":"D3"}"##, "", &[("dimstyle:D2", "name", "-"), ("~place", "name", "-")]),
    // limits: sets header LIMMIN / LIMMAX
    // same-field decision: hold the values this write replaces.
    r("limits", r##"{"min":[0,0],"max":[420,297]}"##, "", &[("drawing", "header.LIMMIN,header.LIMMAX", "-")]),
    // linetype: current -> must name a loaded linetype; sets CELTYPE
    // same-field decision: hold the values this write replaces.
    r("linetype", r##"{"current":"DASHED"}"##, "", &[("$Dashed", "name", "-"), ("drawing", "header.CELTYPE", "-")]),
    // linetype: load -> pushes the library linetype unless one of that name is loaded (name clash skips it)
    // [corrected C8-cad-pin: crates/engine/doc/src/library.rs:52-63 scales newly loaded linetypes by MEASUREMENT; add header.MEASUREMENT]
    r("linetype", r##"{"load":"HIDDEN"}"##, "", &[("drawing", "~recordNames.linetype:hidden", "-"), ("drawing", "header.MEASUREMENT", "-")]),
    // ltscale: sets header LTSCALE
    // same-field decision: hold the values this write replaces.
    r("property.linetypeScale", r##"{"scale":2}"##, "", &[("drawing", "header.LTSCALE", "-")]),
    // lweight: sets header CELWEIGHT
    // same-field decision: hold the values this write replaces.
    r("property.lineweight", r##"{"lineweight":0.5}"##, "", &[("drawing", "header.CELWEIGHT", "-")]),
    // matchprop: copies the source's layer/color/linetype/lineweight/ltscale/transparency onto targets (handles only)
    // same-field decision: hold the values this write replaces.
    r(
        "object.matchProperties",
        r##"{"source":"$L1","targets":["$Circ"]}"##,
        "",
        &[
            ("$L1", "common.layer,common.color,common.linetype,common.lineweight,common.ltscale,common.transparency", "-"),
            ("$Circ", "-", "-"),
            ("$Circ", "common.layer,common.color,common.linetype,common.lineweight,common.ltscale,common.transparency", "-"),
        ],
    ),
    // mleaderstyle: edit (or create) a style by name; makes it current
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "multileaderStyle.open",
        r##"{"name":"Standard","arrowSize":3}"##,
        "",
        &[("$MLeaderStandard", "name", "-"), ("$MLeaderStandard", "data.arrowSize", "-"), ("drawing", "header.CMLEADERSTYLE", "-")],
    ),
    // object.setProperties: colour only, by handle (writes common.color, reads nothing of the target)
    // same-field decision: hold the values this write replaces.
    r("object.setProperties", r##"{"handles":["$L1"],"color":1}"##, "", &[("$L1", "-", "-"), ("$L1", "common.color", "-")]),
    // object.setProperties: colour only, selection fallback
    // same-field decision: hold the values this write replaces.
    r("object.setProperties", r##"{"color":1}"##, "selection", &[("$Circ", "-", "-"), ("$Circ", "common.color", "-")]),
    // object.setProperties: geometry — circle radius and line start; each key applies only to the matching kind
    // same-field decision: hold the values this write replaces.
    r(
        "object.setProperties",
        r##"{"handles":["$Circ","$L1"],"radius":8,"start":[20,5]}"##,
        "",
        &[("$Circ", "kind.type", "-"), ("$L1", "kind.type", "-"), ("$Circ", "kind.radius", "-"), ("$L1", "kind.a", "-")],
    ),
    // object.setProperties: layer reassignment by name (ensure_layer creates it when missing)
    // same-field decision: hold the values this write replaces.
    r(
        "object.setProperties",
        r##"{"handles":["$Circ"],"layer":"Walls"}"##,
        "",
        &[("$Circ", "-", "-"), ("$Walls", "name", "-"), ("$Circ", "common.layer", "-")],
    ),
    // object.setProperties: text write on a TEXT (kind.value)
    // same-field decision: hold the values this write replaces.
    r("object.setProperties", r##"{"handles":["$Text"],"text":"Bye"}"##, "", &[("$Text", "kind.type", "-"), ("$Text", "kind.value", "-")]),
    // rename: layer -> layer.set {name,newName}: copies the whole layer record back, repoints members, CLAYER
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r(
        "rename",
        r##"{"table":"layer","from":"Walls","to":"W2"}"##,
        "",
        // Review R4: the members and CLAYER follow the rename on the drawing as it is then.
        &[("$Walls", "name", "-"), ("~place", "name", "-")],
    ),
    // rename: block -> inserts/mleaders referencing it, INSNAME and dim style arrow blocks follow
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r(
        "rename",
        r##"{"table":"block","from":"Door","to":"D2"}"##,
        "",
        // Review R4: the references follow the rename on the drawing as it is then.
        &[("$Door", "name", "-"), ("~place", "name", "-")],
    ),
    // rename: linetype -> layers, CELTYPE and every entity's common.linetype using it follow
    // UNSURE: no derived fact names "entities using linetype X"; the layer list stands in for the layer scan
    // Review R4: the references follow the rename on the drawing as it is then.
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r("rename", r##"{"table":"linetype","from":"DASHED","to":"DASH2"}"##, "", &[("$Dashed", "name", "-"), ("~place", "name", "-")]),
    // rename: view (simple table)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/props/rename.rs:83-102 rejects occupied destination view names; add typed destination-gap replay]
    r("rename", r##"{"table":"view","from":"V1","to":"V2"}"##, "", &[("$V1", "name", "-"), ("~place", "name", "-")]),
    // rename: text style -> textStyle.rename
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r(
        "rename",
        r##"{"table":"style","from":"S","to":"S2"}"##,
        "",
        // Review R4: the references follow the rename on the drawing as it is then.
        &[("$S", "name", "-"), ("~place", "name", "-")],
    ),
    // style: edit an existing text style by name; makes it current (TEXTSTYLE)
    // same-field decision: hold the values this write replaces.
    r("style", r##"{"name":"S","height":3}"##, "", &[("$S", "name", "-"), ("drawing", "header.TEXTSTYLE", "-"), ("$S", "data.height", "-")]),
    // style: a new style (name clash would silently turn the create into an edit)
    // same-field decision: hold the values this write replaces.
    r("style", r##"{"name":"T2","height":3}"##, "", &[("drawing", "~recordNames.textstyle:t2", "-"), ("drawing", "header.TEXTSTYLE", "-")]),
    // textStyle.setCurrent: by name -> TEXTSTYLE
    // same-field decision: hold the values this write replaces.
    r("textStyle.setCurrent", r##"{"name":"S"}"##, "", &[("$S", "name", "-"), ("drawing", "header.TEXTSTYLE", "-")]),
    // review / R8: eligibility is an input, not an irrelevant
    // refusal-only read. This predicate is !current && !in-use (props.rs:617).
    // The original ~texts/styles/block scans were broader than that predicate.
    // C8-cad-read-2: per-record scalar avoids holding eligibility of other styles.
    // C8-cad-read-2: place.rs::derived now publishes canDelete on each stable
    // style record; the whole-record hold includes that precise predicate.
    r("textStyle.delete", r##"{"name":"S"}"##, "", &[("$S", "*", "-"), ("$S", "~canDelete", "-")]),
    // textStyle.rename: renames, follows TEXTSTYLE, dim/mleader styles' textStyle and texts in model+layouts
    // UNSURE: ~texts used for the text-style scan of model and layouts
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/layer.rs:185-233; crates/engine/engine/src/cmd/props.rs:565-578,744-757,927-980; crates/engine/engine/src/cmd/props/rename.rs:41-55,83-102 check typed destination occupancy; add name-gap replay]
    r(
        "textStyle.rename",
        r##"{"from":"S","to":"S2"}"##,
        "",
        // Review R4: the references follow the rename on the drawing as it is then.
        &[("$S", "name", "-"), ("~place", "name", "-")],
    ),
    // tablestyle: edit (or create) a table style by name; makes it current (CTABLESTYLE)
    // same-field decision; layer.rs:264-392 / props.rs:838-905 / draw2.rs:509-559,1160-1168 / blocks/attribs.rs:50: hold assigned settings.
    r(
        "tableStyle.open",
        r##"{"name":"Standard","textHeight":3}"##,
        "",
        &[("$TableStandard", "name", "-"), ("$TableStandard", "data.textHeight", "-"), ("drawing", "header.CTABLESTYLE", "-")],
    ),
    // units: sets header LUNITS/LUPREC/AUNITS/AUPREC/INSUNITS
    // same-field decision: hold the values this write replaces.
    r("units", r##"{"lunits":2,"luprec":2}"##, "", &[("drawing", "header.LUNITS,header.LUPREC", "-")]),
    // setvar: a session setting (no drawing write)
    r("variable.set", r##"{"name":"ORTHOMODE","value":1}"##, "", &[]),
    // setvar: a header variable; the existing value's type decides the coercion, value replaced whole
    r("variable.set", r##"{"name":"FILLETRAD","value":2}"##, "", &[("drawing", "header.FILLETRAD", "-")]),
];
const CONSTRAINED: &[Row] = &[
    // Group H: parametric constraints (crates/engine/engine/src/cmd/constraints.rs, crates/engine/constraints).
    //
    // Shared rule used below ("G"): any command that changes `drawing.constraints` or `drawing.parametric`
    // makes the post-command hook take the GLOBAL path (constraints.rs:958 `global = ...`): it purges every
    // invalid constraint, checks every constraint for satisfaction and, if any is unsatisfied (a drawing can
    // keep unsatisfied constraints after a failed re-solve, constraints.rs:1004), re-solves EVERY constraint
    // (`cs::solve`, constraints.rs:1001), moving any constrained geometry. So these writes hold the whole
    // constraint list, constraint:1 and the geometry it references ($L1, $L2), not only the component
    // they touch. `cs::add` itself (constraints/src/lib.rs:503) solves only the component holding the new
    // constraint; that component is what `~solve` on the targets stands for.
    // Constraint commands never check layer locks (no ~locked held; see the defects in reading-H.md).

    // autoconstrain: handles given, tolerances/types from the stored parametric settings
    r_c(
        "constraint.auto",
        r##"{"handles":["$L1","$L2"]}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.distanceTolerance,parametric.settings.angleTolerance,parametric.settings.autoTypes", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b,~solve", "-"),
            ("$L2", "kind.a,kind.b,~solve", "-"),
        ],
    ),
    // autoconstrain: tolerance, angleTolerance and types given (settings not read)
    r_c(
        "constraint.auto",
        r##"{"handles":["$L1","$L2"],"tolerance":0.1,"angleTolerance":2,"types":["horizontal"]}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b,~solve", "-")],
    ),
    // autoconstrain: no handles -> selection [$Circ]; a lone circle infers nothing, so no write
    // [corrected C8-cad-pin: crates/engine/constraints/src/auto.rs:32-57,97-139,192-199 yields no candidate for one circle; constraints.rs:794-797 writes only when a candidate was added; retain kind, remove geometry and options]
    r_c("constraint.auto", r##"{}"##, "selection", &[("$Circ", "kind.type", "-")]),
    // constraintbar: hide given handles (exception list edited against barsVisible); parametric change -> global re-solve
    r_c(
        "constraint.bars",
        r##"{"mode":"hide","handles":["$L1"]}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.barsVisible,parametric.settings.barExceptions", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // constraintbar: default mode show, selection fallback; bars visible and no exception -> no change on the fixture
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/constraints.rs:871-916 compares raw exception handles without resolving entities; remove unconstrained circle existence, retain assigned visibility/exception settings]
    r_c("constraint.bars", r##"{}"##, "selection", &[("drawing", "parametric.settings.barsVisible,parametric.settings.barExceptions", "-")]),
    // constraintbar: hideall (sets barsVisible=false, clears exceptions; selection is read by targets() but unused)
    // C8-cad-read-4 sampled audit: constraints.rs:881-884 assigns both settings.
    r_c(
        "constraint.bars",
        r##"{"mode":"hideall"}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.barsVisible,parametric.settings.barExceptions", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // constraint.hideAllBars: barsVisible=false, exceptions cleared -> global re-solve
    r_c(
        "constraint.hideAllBars",
        r##"{}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.barsVisible,parametric.settings.barExceptions", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // UNSURE: on the fixture this is a no-op (bars already visible, no exceptions); whether the global re-solve runs depends on the stored settings
    // constraint.showAllBars: barsVisible=true, exceptions cleared
    // C8-cad-read-4 source correction: constraints.rs:955-966 returns before solving
    // when this all-mode assignment leaves the fixture's stored settings unchanged.
    r_c("constraint.showAllBars", r##"{}"##, "", &[("drawing", "parametric.settings.barsVisible,parametric.settings.barExceptions", "-")]),
    // C8-cad-read-4 source correction: constraints.rs:919-935 preserves omitted settings; only supplied values are assigned, then a global solve runs if changed.
    r_c(
        "constraint.settings",
        r##"{"distanceTolerance":0.1}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.distanceTolerance", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // constraintsettings: value equal to the stored one -> no write
    r_c("constraint.settings", r##"{"barsVisible":true}"##, "", &[("drawing", "parametric.settings.barsVisible", "-")]),
    // dcaligned: one line -> its length (expr measured from geometry, name d<n> auto)
    r_c(
        "constraint.dimensionAligned",
        r##"{"h1":"$L1"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // dcaligned: two points on two objects, value given (the solve moves $Arc)
    r_c(
        "constraint.dimensionAligned",
        r##"{"h1":"$L1","p1":"start","h2":"$Arc","p2":"center","value":40}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b,~solve", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // dcangular: two lines (L1 and the rectangle's segment 1), angle measured
    r_c(
        "constraint.dimensionAngular",
        r##"{"h1":"$L1","h2":"$Rect","p2":"seg1"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b,~solve", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Rect", "kind.vertices,kind.closed,~solve", "-"),
        ],
    ),
    // dcangular: one arc -> its included angle
    r_c(
        "constraint.dimensionAngular",
        r##"{"h1":"$Arc"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // dcdiameter: circle diameter measured
    r_c(
        "constraint.dimensionDiameter",
        r##"{"h1":"$Circ"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
        ],
    ),
    // dcdisplay: hide given handles (dim exception list vs dimsVisible) -> global re-solve
    r_c(
        "constraint.dynamicDimensions",
        r##"{"mode":"hide","handles":["$L1"]}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.dimsVisible,parametric.settings.dimExceptions", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // dcdisplay: show, selection fallback (no change on the fixture)
    // [corrected C8-cad-pin: crates/engine/engine/src/cmd/constraints.rs:871-916 compares raw exception handles without resolving entities; remove unconstrained circle existence, retain assigned visibility/exception settings]
    r_c(
        "constraint.dynamicDimensions",
        r##"{}"##,
        "selection",
        &[("drawing", "parametric.settings.dimsVisible,parametric.settings.dimExceptions", "-")],
    ),
    // dcdisplay: hideall
    r_c(
        "constraint.dynamicDimensions",
        r##"{"mode":"hideall"}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.dimsVisible,parametric.settings.dimExceptions", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // constraint.hideAllDynamicDimensions: dimsVisible=false, exceptions cleared -> global re-solve
    r_c(
        "constraint.hideAllDynamicDimensions",
        r##"{}"##,
        "",
        &[
            ("drawing", "constraints,parametric.settings.dimsVisible,parametric.settings.dimExceptions", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // UNSURE: no-op on the fixture (dims already visible); the global re-solve runs only if the settings change
    // constraint.showAllDynamicDimensions: dimsVisible=true, exceptions cleared
    // C8-cad-read-4 source correction: constraints.rs:955-966 returns before solving
    // when this all-mode assignment leaves the fixture's stored settings unchanged.
    r_c(
        "constraint.showAllDynamicDimensions",
        r##"{}"##,
        "",
        &[("drawing", "parametric.settings.dimsVisible,parametric.settings.dimExceptions", "-")],
    ),
    // dchorizontal: one line -> horizontal distance between its ends
    r_c(
        "constraint.dimensionHorizontal",
        r##"{"h1":"$L1"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // dclinear: one line -> horizontal or vertical picked from the measured geometry
    r_c(
        "constraint.dimensionLinear",
        r##"{"h1":"$L1"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // dclinear: two points on two objects (axis from the larger of dx/dy at the current geometry)
    r_c(
        "constraint.dimensionLinear",
        r##"{"h1":"$L1","p1":"start","h2":"$L2","p2":"end"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b,~solve", "-")],
    ),
    // dcradius: arc radius, value given (the solve changes the radius)
    r_c(
        "constraint.dimensionRadius",
        r##"{"h1":"$Arc","value":6}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // dcvertical: one line -> vertical distance
    r_c(
        "constraint.dimensionVertical",
        r##"{"h1":"$L2"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b", "-"), ("$L2", "kind.a,kind.b,~solve", "-")],
    ),
    // delconstraint: by id (constraint:1 removed; parameter-expression guard; global hook over the rest)
    r_c("constraint.delete", r##"{"ids":[1]}"##, "", &[("drawing", "constraints", "-"), ("constraint:1", "*", "-")]),
    // delconstraint: by handles -> every constraint whose refs name $L1 (scan of the list)
    r_c("constraint.delete", r##"{"handles":["$L1"]}"##, "", &[("drawing", "constraints", "-"), ("constraint:1", "*", "-")]),
    // delconstraint: selection fallback [$Circ] -> no constraint references it, nothing removed
    // [corrected C8-cad-pin: constraints.rs:801-810 compares handles only with saved constraint refs; the unconstrained circle is an opaque membership value with no existence read.]
    r_c("constraint.delete", r##"{}"##, "selection", &[("drawing", "constraints", "-")]),
    // dimconstraint: type omitted -> aligned, one line
    r_c(
        "constraint.addDimensional",
        r##"{"h1":"$L1"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // dimconstraint: radius of a circle, value given
    r_c(
        "constraint.addDimensional",
        r##"{"type":"radius","h1":"$Circ","value":6}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
        ],
    ),
    // dimconstraint: explicit name + expr (a clash with an existing name is an error, not a silent change)
    r_c(
        "constraint.addDimensional",
        r##"{"type":"linear","h1":"$L1","name":"w","expr":"12"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // gccoincident: explicit points ($L1 end, $L2 start)
    r_c(
        "constraint.coincident",
        r##"{"h1":"$L1","p1":"end","h2":"$L2","p2":"start"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b,~solve", "-")],
    ),
    // gccoincident: no hints -> closest pair of characteristic points (circle centre, arc end)
    r_c(
        "constraint.coincident",
        r##"{"h1":"$Circ","h2":"$Arc"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // gccollinear: second object by pick point -> nearest rectangle segment
    r_c(
        "constraint.collinear",
        r##"{"h1":"$L1","h2":"$Rect","p2":[5,0]}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b,~solve", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Rect", "kind.vertices,kind.closed,~solve", "-"),
        ],
    ),
    // gcconcentric: circle and arc (the arc moves)
    r_c(
        "constraint.concentric",
        r##"{"h1":"$Circ","h2":"$Arc"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // gcequal: circle and arc radii
    r_c(
        "constraint.equal",
        r##"{"h1":"$Circ","h2":"$Arc"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // gcfix: whole object (no hint -> Sub::Whole)
    r_c(
        "constraint.fix",
        r##"{"h1":"$Circ"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
        ],
    ),
    // gcfix: one point of a line already in constraint:1's component
    r_c(
        "constraint.fix",
        r##"{"h1":"$L1","p1":"start"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // gchorizontal: one line
    r_c(
        "constraint.horizontal",
        r##"{"h1":"$L1"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // gchorizontal: two points of the same object ({h1, p1, p2} reuses h1)
    r_c(
        "constraint.horizontal",
        r##"{"h1":"$L1","p1":"start","p2":"end"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b,~solve", "-"), ("$L2", "kind.a,kind.b", "-")],
    ),
    // gcparallel: line and a named polyline segment
    r_c(
        "constraint.parallel",
        r##"{"h1":"$L1","h2":"$Rect","p2":"seg0"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b,~solve", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Rect", "kind.vertices,kind.closed,~solve", "-"),
        ],
    ),
    // gcperpendicular: polyline segment and $L2
    r_c(
        "constraint.perpendicular",
        r##"{"h1":"$Rect","p1":"seg0","h2":"$L2"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b,~solve", "-"),
            ("$Rect", "kind.vertices,kind.closed,~solve", "-"),
        ],
    ),
    // gcsmooth: line and arc (line-circle form)
    r_c(
        "constraint.smooth",
        r##"{"h1":"$L1","h2":"$Arc"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b,~solve", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // gcsymmetric: circle and arc about $L2
    r_c(
        "constraint.symmetric",
        r##"{"h1":"$Circ","h2":"$Arc","h3":"$L2"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b,~solve", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // gctangent: two circles/arcs (external/internal sign from current geometry)
    r_c(
        "constraint.tangent",
        r##"{"h1":"$Circ","h2":"$Arc"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
            ("$Arc", "kind.center,kind.radius,kind.start,kind.end,~solve", "-"),
        ],
    ),
    // gcvertical: one line
    r_c(
        "constraint.vertical",
        r##"{"h1":"$L2"}"##,
        "",
        &[("drawing", "constraints", "-"), ("constraint:1", "*", "-"), ("$L1", "kind.a,kind.b", "-"), ("$L2", "kind.a,kind.b,~solve", "-")],
    ),
    // geomconstraint: type by name, polyline segment
    r_c(
        "constraint.addGeometric",
        r##"{"type":"horizontal","h1":"$Rect","p1":"seg0"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Rect", "kind.vertices,kind.closed,~solve", "-"),
        ],
    ),
    // geomconstraint: fix a whole circle
    r_c(
        "constraint.addGeometric",
        r##"{"type":"fix","h1":"$Circ"}"##,
        "",
        &[
            ("drawing", "constraints", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
            ("$Circ", "kind.center,kind.radius,~solve", "-"),
        ],
    ),
    // parameters: no keys -> lists the parameters (no write)
    r_c("parameters", r##"{}"##, "", &[]),
    // parameters: set a new user parameter -> name looked up among dimensional constraints and parameters,
    // every expression must evaluate, then cs::solve re-solves EVERY constraint (lib.rs:604)
    r_c(
        "parameters",
        r##"{"name":"w","value":10}"##,
        "",
        &[
            ("drawing", "constraints,parametric.parameters", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // parameters: set with expr and description
    r_c(
        "parameters",
        r##"{"name":"w","expr":"5*2","description":"width"}"##,
        "",
        &[
            ("drawing", "constraints,parametric.parameters", "-"),
            ("constraint:1", "*", "-"),
            ("$L1", "kind.a,kind.b", "-"),
            ("$L2", "kind.a,kind.b", "-"),
        ],
    ),
    // ---- other commands on the constrained fixture ----
    // Non-constraint commands leave constraints/parametric unchanged, so the hook takes the LOCAL path:
    // only constraint refs whose entity `kind` changed or vanished are "touched"; with none it returns at once
    // (constraints.rs:969). Otherwise it purges invalid constraints, checks all constraints and, if any is
    // unsatisfied, re-solves only the components connected to the touched entities (`solve_connected`).

    // line: new entity with the current properties in the current space; no constrained kind changes -> no solve
    // [corrected C8-cad-pin: crates/engine/engine/src/lib.rs:718-746 copies the current layer into the new line reference; add $0 name identity]
    r_c(
        "line",
        r##"{"points":[[0,50],[5,50]]}"##,
        "active,part",
        &[
            ("drawing", "header.CLAYER,header.CECOLOR,header.CECOLOR_RGB,header.CELTYPE,header.CELWEIGHT,header.CELTSCALE,header.CPLOTSTYLE", "-"),
            ("$0", "name", "-"),
        ],
    ),
    // layer.set: the Notes record is read, edited and written back whole (no entity kind changes -> no solve)
    // same-field decision: hold the values this write replaces.
    r_c("layer.set", r##"{"name":"Notes","color":2}"##, "", &[("$Notes", "name", "-"), ("$Notes", "data.color", "-")]),
    // move: $L1 is touched (kind changed) -> its component {constraint:1} is checked and, if unsatisfied, re-solved
    // (keeping $L1, moving $L2). Translation keeps the perpendicular, so on this fixture nothing else moves.
    // UNSURE: $Dim (associative to $L1's endpoints) is re-measured by assoc::after_command; outside group H's reading
    r_c(
        "move",
        r##"{"handles":["$L1"],"delta":[1,0]}"##,
        "",
        &[("$L1", "kind.a,kind.b,~locked,~solve", "-"), ("constraint:1", "*", "-"), ("$L2", "kind.a,kind.b", "-"), ("$Dim", "kind", "-")],
    ),
    // move: $Circ is in no constraint -> not touched, no solve; ~solve held because a constraint on $Circ
    // would make this move re-solve its component
    // [corrected C8-cad-pin: crates/engine/doc/src/entity.rs:889-906 transforms and assigns circle radius even for unit scale; add kind.radius]
    r_c("move", r##"{"handles":["$Circ"],"delta":[1,0]}"##, "", &[("$Circ", "kind.center,~locked,~solve,kind.radius", "-")]),
    // object.setProperties: colour only, kind unchanged -> no solve; no lock check in object.setProperties
    // same-field decision: hold the values this write replaces.
    r_c("object.setProperties", r##"{"handles":["$Circ"],"color":"red"}"##, "", &[("$Circ", "-", "-"), ("$Circ", "common.color", "-")]),
    // erase: $L2 removed (lock honoured: skipped silently if Walls locked); the hook purges constraint:1
    // (its ref is gone) and writes the constraint list without it; no solve (no constraint left in the component)
    // UNSURE: groups::after_command also drops $L2 from G1 (outside group H's reading)
    r_c("erase", r##"{"handles":["$L2"]}"##, "", &[("$L2", "*", "-"), ("constraint:1", "*", "-"), ("$G1", "data", "-")]),
];
