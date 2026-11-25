//! Format fidelity scorecard (roadmap phase 7).
//!
//! `KITION_CAD_CORPUS=<dir>`: every drawing in that directory (DXF, and DWG through its DXF
//! conversion) is read, written as DXF and read again, and the test reports
//!
//! - per feature class (each entity type, entity properties, handles, layers, linetypes, text
//!   styles, dimension styles, blocks and inserts, layouts, header variables, …), which files keep
//!   it through import → export → import (the two drawings' leaves of that class are compared as
//!   multisets of `path=value`, so a moved entity is not a loss but a dropped property is), and
//! - what the written DXF lost against the original DXF: group codes counted per section and
//!   record type (`ENTITIES/LINE`, `TABLES/LAYER`, `OBJECTS/DICTIONARY`; header variables as
//!   `$NAME` and `$NAME@code`; subclass markers and `102` groups with their names; extended data
//!   as `xdata <application> <code>`).
//!
//! `KITION_FIDELITY_OUT=<dir>` writes the full report (`cad-fidelity.txt`, per file and
//! aggregated) and the aggregate scorecard (`cad-fidelity.json`, no file names) there.
//! `KITION_FIDELITY_BASELINE=<file>` fails when the corpus scores below such a scorecard: fewer
//! files keeping a class, more leaves lost, or less of the file retained (the non-regression
//! gate). Skipped when the corpus variable is unset. The KDM leg of the product's path (import
//! into a replica, read back) is covered by `kdm-model`'s `tests/roundtrip.rs`.
//!
//! The scorer below the kit-specific part is the same as in Kition Word, Slide and Sheet (the
//! package features are read from DXF group codes instead of a zip package, so `features` is
//! kit-specific here, as in Kition Vector).
// The shared scorer's `digest` names media by content; a drawing has none to compare.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::collections::BTreeMap;

use serde_json::{Value, json};

use kition_cad_doc::{Drawing, Entity, EntityStore};

// ---- Kit-specific -------------------------------------------------------------------------------

const KIT: &str = "cad";
const CORPUS_VAR: &str = "KITION_CAD_CORPUS";
const EXTENSIONS: &[&str] = &["dxf", "dwg"];
/// A class the self-test's document must have.
const PRIMARY_CLASS: &str = "lines";

type Model = Drawing;

fn import(bytes: &[u8]) -> Result<Model, String> {
    kition_cad_io::read_dxf(bytes).map_err(|error| error.to_string())
}

fn export(drawing: &Model) -> Result<Vec<u8>, String> {
    Ok(kition_cad_io::write_dxf(drawing).into_bytes())
}

/// The DXF a corpus file is scored by: a DXF file as it is, a DWG file converted to DXF (`None`:
/// the conversion failed).
fn source(bytes: &[u8]) -> Option<Vec<u8>> {
    if kition_cad_dwg::is_dwg(bytes) {
        return kition_cad_dwg::dwg_to_dxf(bytes).ok();
    }
    Some(bytes.to_vec())
}

/// An entity as `{"entity:<type>": {handle, common, …its fields}}`, so that its leaves name its
/// type.
fn entity_value(entity: &Entity) -> Value {
    let mut kind = serde_json::to_value(&entity.kind).unwrap_or(Value::Null);
    let mut ty = String::from("unknown");
    if let Value::Object(map) = &mut kind {
        if let Some(Value::String(name)) = map.remove("type") {
            ty = name;
        }
        map.insert("handle".into(), json!(entity.handle.0));
        map.insert("common".into(), serde_json::to_value(&entity.common).unwrap_or(Value::Null));
    }
    json!({ format!("entity:{ty}"): kind })
}

fn entities(store: &EntityStore) -> Value {
    Value::Array(store.iter().map(|e| entity_value(e)).collect())
}

/// The drawing as JSON: header variables by name, the tables, blocks and layouts with their
/// entities (blocks in name order), model space in draw order.
fn model_value(d: &Model) -> Value {
    let blocks: Vec<Value> = d
        .blocks
        .values()
        .map(|block| {
            let mut v = serde_json::to_value(block.as_ref()).unwrap_or(Value::Null);
            v["entities"] = entities(&block.entities);
            v
        })
        .collect();
    let layouts: Vec<Value> = d
        .layouts
        .iter()
        .map(|layout| {
            let mut v = serde_json::to_value(layout).unwrap_or(Value::Null);
            v["entities"] = entities(&layout.entities);
            v
        })
        .collect();
    json!({
        "header": to(&d.header),
        "layers": to(&d.layers),
        "linetypes": to(&d.linetypes),
        "text_styles": to(&d.text_styles),
        "dim_styles": to(&d.dim_styles),
        "mleader_styles": to(&d.mleader_styles),
        "table_styles": to(&d.table_styles),
        "blocks": blocks,
        "model": entities(&d.model),
        "layouts": layouts,
        "views": to(&d.views),
        "ucss": to(&d.ucss),
        "layer_states": to(&d.layer_states),
        "groups": to(&d.groups),
        "plot_style_tables": to(&d.plot_style_tables),
        "constraints": to(&d.constraints),
        "parametric": to(&d.parametric),
    })
}

/// Any serializable part of the model as JSON.
fn to<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Keys whose values are ids or caches a re-import may renumber without losing anything.
fn ignored(key: &str) -> bool {
    matches!(key, "handseed" | "id")
}

/// Whether two numbers of `class` are the same after a round trip (the DXF writer prints reals
/// with enough digits to read back; angles and colours go through conversions).
fn close(_class: &str, a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6_f64.max(a.abs() * 1e-9)
}

/// The feature class of a leaf, by its path (`.key` segments, `[]` for array items, `#` for
/// numeric map keys). Entities are `…[].entity:<type>.…`.
fn classify(path: &str) -> &'static str {
    if path.ends_with(".handle") || path.contains("handle") || path.starts_with(".groups[].members") {
        return "handles";
    }
    if let Some(at) = path.find("entity:") {
        let rest = &path[at + "entity:".len()..];
        let (ty, tail) = rest.split_once('.').unwrap_or((rest, ""));
        if tail.starts_with("common.") {
            return "entity properties";
        }
        return match ty {
            "line" => "lines",
            "point" => "points",
            "circle" | "arc" => "circles and arcs",
            "ellipse" => "ellipses",
            "lwPolyline" | "polyline3d" => "polylines",
            "spline" => "splines",
            "ray" | "xLine" => "rays and xlines",
            "text" => "text",
            "mText" => "mtext",
            "attDef" => "attributes",
            "insert" if tail.starts_with("attribs") => "attributes",
            "insert" => "blocks and inserts",
            "dimension" => "dimensions",
            "leader" | "mLeader" => "leaders",
            "hatch" => "hatches",
            "solid" | "trace" | "face3d" | "wipeout" => "solids and faces",
            "viewport" => "viewports",
            "image" => "images",
            "table" => "tables",
            _ => "other entities",
        };
    }
    const RULES: &[(&str, &str)] = &[
        (".header.", "header variables"),
        (".layer_states", "layers"),
        (".layers", "layers"),
        (".linetypes", "linetypes"),
        (".text_styles", "text styles"),
        (".dim_styles", "dimension styles"),
        (".mleader_styles", "leader and table styles"),
        (".table_styles", "leader and table styles"),
        (".blocks", "blocks and inserts"),
        (".layouts", "layouts and plotting"),
        (".plot_style_tables", "layouts and plotting"),
        (".views", "views and UCS"),
        (".ucss", "views and UCS"),
        (".groups", "groups"),
        (".constraints", "constraints and parameters"),
        (".parametric", "constraints and parameters"),
    ];
    for (needle, class) in RULES {
        if path.starts_with(needle) {
            return class;
        }
    }
    "other"
}

/// Group codes of a DXF by section and record type: `HEADER` holds `$NAME` (one per variable)
/// and `$NAME@code`; other records count their type, each group code, subclass markers and `102`
/// groups by name, and extended data by application (`xdata <app> <code>`).
fn features(dxf: &[u8]) -> Result<Features, String> {
    let tags = kition_cad_dxf::parse(dxf).map_err(|error| error.to_string())?;
    let mut out = Features::new();
    for section in kition_cad_dxf::sections(&tags) {
        *out.entry("(sections)".into()).or_default().entry(section.name.clone()).or_default() += 1;
        if section.name == "HEADER" {
            let counts = out.entry("HEADER".into()).or_default();
            let mut var = String::new();
            for tag in &section.tags {
                if tag.code == 9 {
                    var = tag.str();
                    *counts.entry(var.clone()).or_default() += 1;
                } else {
                    *counts.entry(format!("{var}@{}", tag.code)).or_default() += 1;
                }
            }
            continue;
        }
        for (ty, tags) in kition_cad_dxf::records(&section.tags) {
            let counts = out.entry(format!("{}/{ty}", section.name)).or_default();
            *counts.entry(ty.clone()).or_default() += 1;
            let mut app = String::new();
            for tag in &tags {
                let feature = match tag.code {
                    1001 => {
                        app = tag.str();
                        format!("xdata {app}")
                    }
                    1000..=1071 => format!("xdata {app} {}", tag.code),
                    100 | 102 => format!("{}:{}", tag.code, tag.str()),
                    code => code.to_string(),
                };
                *counts.entry(feature).or_default() += 1;
            }
        }
    }
    Ok(out)
}

/// A small drawing exercising most classes, for the self-test.
fn synthetic() -> Model {
    use kition_cad_doc::geom::Vec3;
    use kition_cad_doc::{Circle, Common, EntityKind, Layer, Line, Space};
    let mut d = Drawing::new_metric();
    d.layers.push(Layer::new("Walls"));
    let walls = Common { layer: "Walls".into(), ..Common::default() };
    let line = EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(100.0, 0.0, 0.0) });
    let circle = EntityKind::Circle(Circle { center: Vec3::new(50.0, 50.0, 0.0), radius: 25.0 });
    d.add(&Space::Model, walls, line).unwrap_or_else(|error| panic!("{error}"));
    d.add(&Space::Model, Common::default(), circle).unwrap_or_else(|error| panic!("{error}"));
    d
}

// ---- Shared scorer (as in kition-word, kition-slide, kition-sheet) -------------------------------

/// Leaves of a model by class: `path=value` → count.
type Leaves = BTreeMap<&'static str, BTreeMap<String, u64>>;
/// Features of a package by part kind: feature → count.
type Features = BTreeMap<String, BTreeMap<String, u64>>;

fn digest(bytes: &[u8]) -> String {
    // FNV-1a: enough to tell two media items apart in a report.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:016x}/{}", hash, bytes.len())
}

fn fold_digits(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_digits = false;
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

fn leaves(value: &Value) -> Leaves {
    fn walk(value: &Value, path: &mut String, out: &mut Leaves) {
        match value {
            Value::Null => {}
            Value::Object(map) => {
                for (key, item) in map {
                    if ignored(key) {
                        continue;
                    }
                    let len = path.len();
                    path.push('.');
                    path.push_str(if key.chars().all(|c| c.is_ascii_digit() || c == '-') { "#" } else { key });
                    walk(item, path, out);
                    path.truncate(len);
                }
            }
            Value::Array(items) if items.iter().any(|v| v.is_object() || v.is_array()) => {
                let len = path.len();
                path.push_str("[]");
                for item in items {
                    walk(item, path, out);
                }
                path.truncate(len);
            }
            scalar => {
                *out.entry(classify(path)).or_default().entry(format!("{path}={scalar}")).or_default() += 1;
            }
        }
    }
    let mut out = Leaves::new();
    walk(value, &mut String::new(), &mut out);
    out
}

#[derive(Default)]
struct ClassScore {
    files: u64,
    files_kept: u64,
    leaves: u64,
    kept: u64,
    added: u64,
}

#[derive(Default)]
struct Scorecard {
    files: u64,
    unreadable: u64,
    unwritable: u64,
    unrereadable: u64,
    classes: BTreeMap<&'static str, ClassScore>,
    /// (class, path with digits folded) → (files, leaves lost, an example: lost leaf → a leaf the
    /// second import has instead on that path, if any)
    lost_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    /// (class, path with digits folded) → (files, leaves added, an example)
    added_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    xml_original: u64,
    xml_kept: u64,
    /// (part kind, feature) → (files, count lost)
    lost_features: BTreeMap<(String, String), (u64, u64)>,
    part_kinds: BTreeMap<String, (u64, u64)>,
    lines: Vec<String>,
}

/// The number a leaf holds, if it holds one.
fn number_of(leaf: &str) -> Option<f64> {
    leaf.split_once('=').and_then(|(_, v)| if v.starts_with('"') { None } else { v.parse().ok() })
}

/// Folded path → (count, an example).
type Paths = BTreeMap<String, (u64, String)>;

/// How many of `left`'s leaves `right` keeps (equal, or a number [`close`] to a spare one on the
/// same path), how many it adds, what it lost and what it added.
fn compare(class: &str, left: &BTreeMap<String, u64>, right: &BTreeMap<String, u64>) -> (u64, u64, Paths, Paths) {
    let mut spare: BTreeMap<String, Vec<(f64, &str)>> = BTreeMap::new();
    let mut added = 0;
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        added += extra;
        if let Some(v) = number_of(leaf) {
            for _ in 0..extra {
                spare.entry(path_of(leaf)).or_default().push((v, leaf));
            }
        }
    }
    let mut kept = 0;
    let mut lost = Paths::new();
    for (leaf, n) in left {
        let m = right.get(leaf).copied().unwrap_or(0);
        kept += m.min(*n);
        let path = path_of(leaf);
        for _ in m..*n {
            if let (Some(v), Some(list)) = (number_of(leaf), spare.get_mut(&path))
                && let Some(i) = list.iter().position(|(w, _)| close(class, v, *w))
            {
                list.swap_remove(i);
                kept += 1;
                added -= 1;
                continue;
            }
            let entry = lost.entry(fold_digits(&path)).or_default();
            entry.0 += 1;
            if entry.1.is_empty() {
                let instead = right.keys().find(|r| path_of(r) == path && !left.contains_key(*r)).map(String::as_str).unwrap_or("nothing");
                entry.1 = format!("{} → {}", clip(leaf), clip(instead));
            }
        }
    }
    let mut gained = Paths::new();
    let mut note = |leaf: &str, n: u64| {
        let entry = gained.entry(fold_digits(&path_of(leaf))).or_default();
        entry.0 += n;
        if entry.1.is_empty() {
            entry.1 = clip(leaf);
        }
    };
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        if extra > 0 && number_of(leaf).is_none() {
            note(leaf, extra);
        }
    }
    for (_, leaf) in spare.values().flatten() {
        note(leaf, 1);
    }
    (kept, added, lost, gained)
}

/// `leaf`, cut to a readable length.
fn clip(leaf: &str) -> String {
    let mut out: String = leaf.chars().take(160).collect();
    if out.len() < leaf.len() {
        out.push('…');
    }
    out
}

fn path_of(leaf: &str) -> String {
    leaf.split_once('=').map(|(p, _)| p.to_owned()).unwrap_or_default()
}

impl Scorecard {
    fn file(&mut self, name: &str, bytes: &[u8]) {
        self.files += 1;
        let Ok(first) = import(bytes) else {
            self.unreadable += 1;
            self.lines.push(format!("{name}: not importable"));
            return;
        };
        let Ok(written) = export(&first) else {
            self.unwritable += 1;
            self.lines.push(format!("{name}: export failed"));
            return;
        };
        let Ok(second) = import(&written) else {
            self.unrereadable += 1;
            self.lines.push(format!("{name}: export not re-importable"));
            return;
        };
        let (a, b) = (leaves(&model_value(&first)), leaves(&model_value(&second)));
        let mut lost_classes = Vec::new();
        let empty = BTreeMap::new();
        for (class, left) in &a {
            let right = b.get(class).unwrap_or(&empty);
            let score = self.classes.entry(class).or_default();
            let total: u64 = left.values().sum();
            let (kept, added, lost_paths, added_paths) = compare(class, left, right);
            score.files += 1;
            score.leaves += total;
            score.kept += kept;
            score.added += added;
            if kept == total && added == 0 {
                score.files_kept += 1;
            } else {
                lost_classes.push(format!("{class} {kept}/{total}+{added}"));
            }
            for (paths, into) in [(lost_paths, &mut self.lost_leaves), (added_paths, &mut self.added_leaves)] {
                for (path, (n, example)) in paths {
                    let entry = into.entry((class, path)).or_default();
                    entry.0 += 1;
                    entry.1 += n;
                    if entry.2.is_empty() {
                        entry.2 = example;
                    }
                }
            }
        }
        for (class, right) in &b {
            if !a.contains_key(class) {
                let score = self.classes.entry(class).or_default();
                score.files += 1;
                score.added += right.values().sum::<u64>();
                lost_classes.push(format!("{class} 0/0+{}", right.values().sum::<u64>()));
            }
        }
        let (Ok(original), Ok(exported)) = (features(bytes), features(&written)) else {
            self.lines.push(format!("{name}: classes changed: {lost_classes:?}; package not comparable"));
            return;
        };
        let (mut total, mut kept) = (0u64, 0u64);
        for (kind, counts) in &original {
            let theirs = exported.get(kind);
            let kind_total: u64 = counts.values().sum();
            let mut kind_kept = 0;
            for (feature, n) in counts {
                let m = theirs.and_then(|t| t.get(feature)).copied().unwrap_or(0);
                kind_kept += m.min(*n);
                if m < *n {
                    let entry = self.lost_features.entry((kind.clone(), feature.clone())).or_default();
                    entry.0 += 1;
                    entry.1 += n - m;
                }
            }
            let entry = self.part_kinds.entry(kind.clone()).or_default();
            entry.0 += kind_total;
            entry.1 += kind_kept;
            total += kind_total;
            kept += kind_kept;
        }
        self.xml_original += total;
        self.xml_kept += kept;
        self.lines.push(format!("{name}: xml {:.1}%; classes changed: {lost_classes:?}", percent(kept, total)));
    }

    fn json(&self) -> Value {
        let classes: serde_json::Map<String, Value> = self
            .classes
            .iter()
            .map(|(class, s)| {
                ((*class).to_owned(), json!({ "files": s.files, "filesKept": s.files_kept, "leaves": s.leaves, "kept": s.kept, "added": s.added }))
            })
            .collect();
        json!({
            "kit": KIT,
            "files": self.files,
            "unreadable": self.unreadable,
            "unwritable": self.unwritable,
            "unrereadable": self.unrereadable,
            "classes": classes,
            "xml": { "original": self.xml_original, "kept": self.xml_kept },
        })
    }

    fn report(&self) -> String {
        let mut out = String::new();
        let ok = self.files - self.unreadable - self.unwritable - self.unrereadable;
        out.push_str(&format!(
            "{KIT} fidelity: {} files, {ok} round-tripped ({} unreadable, {} export failed, {} export unreadable)\n\n",
            self.files, self.unreadable, self.unwritable, self.unrereadable
        ));
        out.push_str("class                        files kept   leaves kept      added\n");
        for (class, s) in &self.classes {
            out.push_str(&format!(
                "{class:<28} {:>4}/{:<4}  {:>6.2}% of {:<9} {}\n",
                s.files_kept,
                s.files,
                percent(s.kept, s.leaves),
                s.leaves,
                s.added
            ));
        }
        out.push_str(&format!(
            "\nXML retained: {:.2}% of {} elements and attributes\n",
            percent(self.xml_kept, self.xml_original),
            self.xml_original
        ));
        let mut kinds: Vec<_> = self.part_kinds.iter().collect();
        kinds.sort_by_key(|(_, (total, kept))| std::cmp::Reverse(total - kept));
        out.push_str("\npart kinds by features lost:\n");
        for (kind, (total, kept)) in kinds.iter().take(25) {
            out.push_str(&format!("  {kind:<50} {:>9} lost of {:<9} ({:.1}% kept)\n", total - kept, total, percent(*kept, *total)));
        }
        let mut lost: Vec<_> = self.lost_features.iter().collect();
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost XML features by files (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*count, *files)));
        out.push_str("\ntop lost XML features by count (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        let mut lost: Vec<_> = self.lost_leaves.iter().collect();
        lost.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in lost.iter().take(300) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        let mut added: Vec<_> = self.added_leaves.iter().collect();
        added.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop added model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in added.iter().take(100) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        out.push_str("\nper file:\n");
        for line in &self.lines {
            out.push_str(&format!("  {line}\n"));
        }
        out
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 { 100.0 } else { part as f64 * 100.0 / whole as f64 }
}

/// The regressions of `now` against `baseline`, both [`Scorecard::json`] values.
fn regressions(now: &Value, baseline: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let number = |v: &Value, path: &[&str]| path.iter().fold(v, |v, key| &v[*key]).as_u64().unwrap_or(0);
    for key in ["unreadable", "unwritable", "unrereadable"] {
        if number(now, &[key]) > number(baseline, &[key]) {
            out.push(format!("{key}: {} > {}", number(now, &[key]), number(baseline, &[key])));
        }
    }
    let ratio = |kept: u64, total: u64| if total == 0 { 1.0 } else { kept as f64 / total as f64 };
    if let Some(classes) = baseline["classes"].as_object() {
        for class in classes.keys() {
            let (b, n) = (&baseline["classes"][class], &now["classes"][class]);
            if number(n, &["filesKept"]) < number(b, &["filesKept"]) {
                out.push(format!("{class}: {} files keep it, baseline {}", number(n, &["filesKept"]), number(b, &["filesKept"])));
            }
            // More leaves lost than the class grew by: a model that learns a property may also
            // count it on objects that were lost already, which is not a regression.
            let lost = |v: &Value| number(v, &["leaves"]).saturating_sub(number(v, &["kept"]));
            let grown = number(n, &["leaves"]).saturating_sub(number(b, &["leaves"]));
            if lost(n) > lost(b) + grown {
                out.push(format!("{class}: {} leaves lost, baseline {} (+{grown} leaves)", lost(n), lost(b)));
            }
        }
    }
    let (rb, rn) = (
        ratio(number(baseline, &["xml", "kept"]), number(baseline, &["xml", "original"])),
        ratio(number(now, &["xml", "kept"]), number(now, &["xml", "original"])),
    );
    if rn + 1e-9 < rb {
        out.push(format!("XML retained {:.4}%, baseline {:.4}%", rn * 100.0, rb * 100.0));
    }
    out
}

#[test]
fn corpus_fidelity() {
    let Ok(dir) = std::env::var(CORPUS_VAR) else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e))))
        // macOS keeps resource forks of copied files as `._<name>` beside them: not documents.
        .filter(|path| !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("._")))
        .collect();
    files.sort();
    let mut card = Scorecard::default();
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let Some(dxf) = source(&bytes) else {
            card.files += 1;
            card.unreadable += 1;
            card.lines.push(format!("{name}: not convertible"));
            continue;
        };
        card.file(&name, &dxf);
    }
    let report = card.report();
    let summary: String = report.split("\npart kinds").next().unwrap_or_default().to_owned();
    eprintln!("{summary}");
    let now = card.json();
    if let Ok(out) = std::env::var("KITION_FIDELITY_OUT") {
        let _ = std::fs::create_dir_all(&out);
        std::fs::write(format!("{out}/{KIT}-fidelity.txt"), &report).unwrap_or_else(|error| panic!("{out}: {error}"));
        let pretty = serde_json::to_string_pretty(&now).unwrap_or_default();
        std::fs::write(format!("{out}/{KIT}-fidelity.json"), pretty).unwrap_or_else(|error| panic!("{out}: {error}"));
    }
    if let Ok(path) = std::env::var("KITION_FIDELITY_BASELINE") {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let baseline: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
        let regressed = regressions(&now, &baseline);
        assert!(regressed.is_empty(), "fidelity regressed against {path}:\n{}", regressed.join("\n"));
    }
}

#[test]
fn scorer_passes_a_lossless_round_trip_and_flags_a_regression() {
    let doc = synthetic();
    let bytes = export(&doc).unwrap_or_else(|error| panic!("export: {error}"));
    let mut card = Scorecard::default();
    card.file("synthetic", &bytes);
    assert_eq!(card.unreadable + card.unwritable + card.unrereadable, 0, "{}", card.report());
    assert!(card.classes.values().all(|s| s.files_kept == s.files), "{}", card.report());
    assert!(card.classes.contains_key(PRIMARY_CLASS), "{}", card.report());
    assert!(card.xml_original > 0 && card.xml_kept == card.xml_original, "{}", card.report());
    // A baseline of this run accepts it; one that kept more flags it.
    let now = card.json();
    assert!(regressions(&now, &now).is_empty());
    let mut better = now.clone();
    better["xml"]["kept"] = json!(card.xml_kept + 1);
    better["xml"]["original"] = json!(card.xml_original);
    assert_eq!(regressions(&now, &better).len(), 1);
}
