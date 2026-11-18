//! What [`KitDocument::check`](kition_kit::KitDocument::check) reports on a drawing (KHP
//! A21): the errors AUDIT finds in the drawing's database (`audit` fixes them), plus what
//! draws differently from what the author meant: degenerate polylines and hatches, blank texts,
//! dimensions whose text replaces the measurement, objects on layers that are off or frozen, text
//! fonts this build draws with a substitute, external references with nothing loaded, and objects
//! far away from the rest of the drawing. An object is named `entity:<handle>` (hex, as the
//! engine's commands take it).
//!
//! Codes: from AUDIT `cad.missing-layer`, `cad.missing-linetype`, `cad.missing-text-style`,
//! `cad.missing-block`, `cad.missing-table-entry`, `cad.missing-reference`,
//! `cad.missing-current`, `cad.dangling-reference`, `cad.invalid-geometry`, `cad.block-cycle`,
//! `cad.duplicate-handle` (and `cad.audit-<kind>` for a kind added later); then
//! `cad.degenerate-polyline`, `cad.degenerate-hatch`, `cad.empty-text`,
//! `cad.dimension-text-overridden`, `cad.hidden-layer`, `cad.font-substituted`,
//! `cad.unresolved-xref`, `cad.far-outside`, and `cad.truncated` when a code is found more often
//! than [`PER_CODE`] times.

use std::collections::BTreeMap;

use kition_cad_doc::geom::{Bounds2, PolyVertex, Vec2};
use kition_cad_doc::{Drawing, Entity, EntityKind, Handle, entity_bounds};
use kition_kit::diagnostics::{Diagnostic, Severity, Target};
use serde_json::json;

/// At most this many diagnostics of one code; the rest are counted in one `cad.truncated`.
pub const PER_CODE: usize = 100;

/// One code the check reports: its severity and a one-sentence summary for the catalogue.
pub struct Entry {
    pub code: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
}

const fn entry(code: &'static str, severity: Severity, summary: &'static str) -> Entry {
    Entry { code, severity, summary }
}

/// Every code [`check`] reports, errors first: the catalogue shipped as `agent/checks.json` (KHP
/// A38), with one conformance fixture per code (`conformance/check-<name>.dxf`). AUDIT kinds added
/// later than this list are reported as `cad.audit-<kind>` until they join it.
pub const CATALOGUE: &[Entry] = &[
    entry("cad.missing-layer", Severity::Error, "Objects are on a layer the drawing does not define; AUDIT creates it."),
    entry("cad.missing-linetype", Severity::Error, "A layer uses a linetype the drawing does not define; AUDIT sets it to Continuous."),
    entry(
        "cad.missing-text-style",
        Severity::Error,
        "A dimension or multileader style uses a text style the drawing does not define; AUDIT sets it to Standard.",
    ),
    entry("cad.missing-block", Severity::Error, "A block reference names a block the drawing does not define; AUDIT creates an empty definition."),
    entry("cad.missing-table-entry", Severity::Error, "A table entry every drawing has (layer 0, a standard style) is missing; AUDIT recreates it."),
    entry("cad.missing-reference", Severity::Error, "Objects use a linetype or style the drawing does not define; AUDIT sets them to the default."),
    entry(
        "cad.dangling-reference",
        Severity::Error,
        "A group, dimension, viewport or constraint refers to an object that does not exist; AUDIT removes the reference.",
    ),
    entry(
        "cad.invalid-geometry",
        Severity::Error,
        "An object's geometry is invalid (zero length, no area, not a number); erasing it changes nothing on the drawing.",
    ),
    entry("cad.block-cycle", Severity::Error, "A block contains itself through its references; AUDIT removes the reference that closes the loop."),
    entry("cad.duplicate-handle", Severity::Error, "Two objects share one handle; AUDIT gives one of them a new handle."),
    entry(
        "cad.missing-current",
        Severity::Warning,
        "The current layer, linetype or style names an entry that does not exist; AUDIT sets the default.",
    ),
    entry("cad.degenerate-polyline", Severity::Warning, "A polyline's vertices make a single point, so it draws nothing."),
    entry("cad.degenerate-hatch", Severity::Warning, "A hatch's boundary has no area, so it fills nothing."),
    entry("cad.unresolved-xref", Severity::Warning, "An external reference has nothing loaded, so its drawing does not show."),
    entry(
        "cad.far-outside",
        Severity::Warning,
        "An object lies far away from the rest of the drawing, so zooming to the extents shows almost nothing.",
    ),
    entry("cad.hidden-layer", Severity::Info, "Objects are on a layer that is off or frozen, so they do not show or plot."),
    entry("cad.font-substituted", Severity::Info, "A text style's font is not available here, so its text is drawn with a substitute."),
    entry("cad.empty-text", Severity::Info, "A text object has no text; erasing it changes nothing on the drawing."),
    entry(
        "cad.dimension-text-overridden",
        Severity::Info,
        "A dimension shows fixed text instead of its measurement, so it does not follow the geometry.",
    ),
    entry("cad.truncated", Severity::Info, "One code was found more often than the check lists; the rest are counted in one diagnostic."),
];

/// The catalogue as `agent/checks.json` holds it: code, severity, summary, and its fixture
/// (`check-<name>.dxf`).
pub fn catalogue_json() -> serde_json::Value {
    serde_json::Value::Array(
        CATALOGUE
            .iter()
            .map(|entry| {
                json!({
                    "code": entry.code,
                    "severity": entry.severity,
                    "summary": entry.summary,
                    "fixture": format!("check-{}.dxf", entry.code.trim_start_matches("cad.")),
                })
            })
            .collect(),
    )
}

/// How much farther from the rest of the drawing than its own spread an object must be to count
/// as far outside it.
const FAR: f64 = 1000.0;

/// Everything worth fixing in the drawing, errors first.
pub fn check(d: &Drawing) -> Vec<Diagnostic> {
    let mut found = Found::default();
    audit(d, &mut found);
    for e in every_entity(d) {
        entity(e, &mut found);
    }
    hidden_layers(d, &mut found);
    fonts(d, &mut found);
    xrefs(d, &mut found);
    far_outside(d, &mut found);
    found.finish()
}

#[derive(Default)]
struct Found {
    list: Vec<Diagnostic>,
    counts: BTreeMap<String, usize>,
}

impl Found {
    fn push(&mut self, diagnostic: Diagnostic) {
        let count = self.counts.entry(diagnostic.code.clone()).or_default();
        *count += 1;
        if *count <= PER_CODE {
            self.list.push(diagnostic);
        }
    }

    fn finish(mut self) -> Vec<Diagnostic> {
        for (code, count) in &self.counts {
            if *count > PER_CODE {
                self.list.push(Diagnostic::new(
                    "cad.truncated",
                    Severity::Info,
                    format!("{} more `{code}` diagnostics are not listed.", count - PER_CODE),
                ));
            }
        }
        self.list.sort_by_key(|diagnostic| diagnostic.severity);
        self.list
    }
}

fn path(path: String) -> Target {
    Target { path: Some(path), ..Target::default() }
}

fn of_entity(handle: Handle) -> Target {
    path(format!("entity:{}", handle.hex()))
}

fn erase(handle: Handle) -> serde_json::Value {
    json!({ "handles": [handle.hex()] })
}

/// Every entity: model space, the layouts, and the block definitions.
fn every_entity(d: &Drawing) -> impl Iterator<Item = &Entity> {
    std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|s| s.iter().map(AsRef::as_ref))
}

/// `missingLayer` → `missing-layer`.
fn kebab(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len() + 4);
    for c in kind.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// AUDIT's findings (`audit` with `fix` repairs every one of them).
fn audit(d: &Drawing, found: &mut Found) {
    const KNOWN: &[&str] = &[
        "missingLayer",
        "missingLinetype",
        "missingTextStyle",
        "missingBlock",
        "missingTableEntry",
        "missingReference",
        "missingCurrent",
        "danglingReference",
        "invalidGeometry",
        "blockCycle",
        "duplicateHandle",
    ];
    for issue in kition_cad_engine::audit::check(d).issues {
        // The next handle is the DXF writer's to keep above every handle, and a replica mints
        // handles from its own range above it (kdm_model::handle_range): nothing to fix.
        if issue.kind == "handseed" {
            continue;
        }
        let code = if KNOWN.contains(&issue.kind) { format!("cad.{}", kebab(issue.kind)) } else { format!("cad.audit-{}", kebab(issue.kind)) };
        let severity = match issue.kind {
            "missingCurrent" => Severity::Warning,
            _ => Severity::Error,
        };
        let handle = issue.handle.as_deref().and_then(Handle::parse_hex);
        let target = match handle {
            Some(h) => of_entity(h),
            None => path(issue.location.clone()),
        };
        let mut diagnostic =
            Diagnostic::new(code, severity, format!("{}: {} (AUDIT fixes it: {}).", issue.location, issue.message, issue.fix)).on(target);
        diagnostic = match (issue.kind, handle) {
            ("invalidGeometry", Some(h)) => diagnostic.fix("erase", Some(erase(h))),
            _ => diagnostic.fix("audit", Some(json!({ "fix": true }))),
        };
        found.push(diagnostic);
    }
}

/// Whether the vertices make at most one point.
fn one_point(points: impl IntoIterator<Item = Vec2>) -> bool {
    let mut points = points.into_iter();
    let Some(first) = points.next() else { return true };
    points.all(|p| (p - first).len() <= f64::EPSILON * first.len().max(1.0))
}

/// The area a loop of straight segments encloses (bulges ignored).
fn area(vertices: &[PolyVertex]) -> f64 {
    let n = vertices.len();
    (0..n)
        .map(|i| {
            let (a, b) = (vertices[i].p, vertices[(i + 1) % n].p);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// Checks on one entity's own content.
fn entity(e: &Entity, found: &mut Found) {
    let h = e.handle;
    match &e.kind {
        EntityKind::LwPolyline(p) if one_point(p.vertices.iter().map(|v| v.p)) => found.push(
            Diagnostic::new(
                "cad.degenerate-polyline",
                Severity::Warning,
                format!("Polyline {} has no length: all its vertices are one point.", h.hex()),
            )
            .on(of_entity(h))
            .fix("erase", Some(erase(h))),
        ),
        EntityKind::Polyline3d(p) if one_point(p.points.iter().map(|v| v.xy())) => found.push(
            Diagnostic::new(
                "cad.degenerate-polyline",
                Severity::Warning,
                format!("3D polyline {} has no length: all its vertices are one point.", h.hex()),
            )
            .on(of_entity(h))
            .fix("erase", Some(erase(h))),
        ),
        EntityKind::Hatch(hatch) => {
            // A loop with arcs (bulges) always encloses something; a straight one needs an area.
            let flat =
                |l: &kition_cad_doc::HatchLoop| l.vertices.iter().all(|v| v.bulge == 0.0) && (l.vertices.len() < 3 || area(&l.vertices) <= 0.0);
            if hatch.loops.iter().all(flat) {
                found.push(
                    Diagnostic::new(
                        "cad.degenerate-hatch",
                        Severity::Warning,
                        format!("Hatch {} has no boundary that encloses an area, so it fills nothing.", h.hex()),
                    )
                    .on(of_entity(h))
                    .fix("erase", Some(erase(h))),
                );
            }
        }
        EntityKind::Text(t) if blank(&t.value) => found.push(
            Diagnostic::new("cad.empty-text", Severity::Info, format!("Text {} is empty and draws nothing.", h.hex()))
                .on(of_entity(h))
                .fix("erase", Some(erase(h))),
        ),
        EntityKind::MText(t) if blank(&kition_cad_fonts::plain_mtext(&t.contents)) => found.push(
            Diagnostic::new("cad.empty-text", Severity::Info, format!("Multiline text {} is empty and draws nothing.", h.hex()))
                .on(of_entity(h))
                .fix("erase", Some(erase(h))),
        ),
        EntityKind::Dimension(dim) if overridden(&dim.text) => found.push(
            Diagnostic::new(
                "cad.dimension-text-overridden",
                Severity::Info,
                format!("Dimension {} shows \"{}\" instead of its measurement, so it won't follow edits to the geometry.", h.hex(), dim.text),
            )
            .on(of_entity(h))
            .fix("properties.set", Some(json!({ "handles": [h.hex()], "textOverride": "" }))),
        ),
        _ => {}
    }
}

/// A text that draws no glyph.
fn blank(s: &str) -> bool {
    s.replace("\\~", " ").chars().all(char::is_whitespace)
}

/// A dimension text that replaces the measured value: not empty, not suppressed, and without the
/// `<>` placeholder that shows the measurement.
fn overridden(text: &str) -> bool {
    !text.is_empty() && !text.trim().is_empty() && !text.contains("<>")
}

/// Objects drawn in model space or a layout on a layer that is off or frozen (one diagnostic per
/// layer).
fn hidden_layers(d: &Drawing, found: &mut Found) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()) {
        *counts.entry(e.common.layer.as_str()).or_default() += 1;
    }
    for layer in &d.layers {
        let Some(&n) = counts.get(layer.name.as_str()) else { continue };
        if layer.visible() {
            continue;
        }
        let (state, command) = if layer.frozen {
            ("frozen", json!({ "name": layer.name, "frozen": false }))
        } else {
            ("off", json!({ "name": layer.name, "on": true }))
        };
        found.push(
            Diagnostic::new(
                "cad.hidden-layer",
                Severity::Info,
                format!(
                    "{n} object{} on layer \"{}\" {} not shown: the layer is {state}.",
                    if n == 1 { "" } else { "s" },
                    layer.name,
                    if n == 1 { "is" } else { "are" }
                ),
            )
            .on(path(format!("layer:{}", layer.name)))
            .fix("layer.set", Some(command)),
        );
    }
}

/// Text styles in use whose font this build doesn't have: their text draws with a substitute.
fn fonts(d: &Drawing, found: &mut Found) {
    let mut used: BTreeMap<&str, usize> = BTreeMap::new();
    for e in every_entity(d) {
        if let EntityKind::Text(kition_cad_doc::Text { style, .. }) | EntityKind::MText(kition_cad_doc::MText { style, .. }) = &e.kind {
            *used.entry(style.as_str()).or_default() += 1;
        }
    }
    for style in &d.text_styles {
        if !used.contains_key(style.name.as_str()) {
            continue;
        }
        let font = style.font.trim();
        if font.is_empty() || font.eq_ignore_ascii_case(kition_cad_fonts::BUILTIN_FONT) || kition_cad_fonts::ttf::find(font).is_some() {
            continue;
        }
        found.push(
            Diagnostic::new(
                "cad.font-substituted",
                Severity::Info,
                format!("Text style \"{}\" asks for font \"{font}\", which isn't available; its text draws with a substitute.", style.name),
            )
            .on(path(format!("style:{}", style.name))),
        );
    }
}

/// External references whose drawing is not loaded: their references draw nothing.
fn xrefs(d: &Drawing, found: &mut Found) {
    for block in d.blocks.values() {
        let Some(file) = &block.xref_path else { continue };
        if !block.entities.is_empty() {
            continue;
        }
        found.push(
            Diagnostic::new(
                "cad.unresolved-xref",
                Severity::Warning,
                format!("External reference \"{}\" ({file}) is not loaded, so its references draw nothing.", block.name),
            )
            .on(path(format!("block:{}", block.name))),
        );
    }
}

/// Model-space objects far away from the rest of the drawing (a stray point at 1e9 makes ZOOM
/// Extents show nothing): farther from where most objects are than [`FAR`] times their spread.
fn far_outside(d: &Drawing, found: &mut Found) {
    let boxes: Vec<(Handle, Bounds2)> = d
        .model
        .iter()
        .filter(|e| !matches!(e.kind, EntityKind::Ray(_) | EntityKind::XLine(_)))
        .map(|e| (e.handle, entity_bounds(d, e, 0)))
        .filter(|(_, b)| !b.is_empty() && b.min.x.is_finite() && b.min.y.is_finite() && b.max.x.is_finite() && b.max.y.is_finite())
        .collect();
    if boxes.len() < 4 {
        return;
    }
    let quantile = |mut values: Vec<f64>, q: f64| {
        values.sort_by(f64::total_cmp);
        values[((values.len() - 1) as f64 * q).round() as usize]
    };
    let xs: Vec<f64> = boxes.iter().map(|(_, b)| b.center().x).collect();
    let ys: Vec<f64> = boxes.iter().map(|(_, b)| b.center().y).collect();
    let (x0, x1) = (quantile(xs.clone(), 0.1), quantile(xs, 0.9));
    let (y0, y1) = (quantile(ys.clone(), 0.1), quantile(ys, 0.9));
    let sizes: Vec<f64> = boxes.iter().map(|(_, b)| b.width().max(b.height())).collect();
    let spread = (x1 - x0).max(y1 - y0).max(quantile(sizes, 0.5)).max(1e-9);
    let reach = FAR * spread;
    for (h, b) in &boxes {
        let dx = (x0 - b.max.x).max(b.min.x - x1).max(0.0);
        let dy = (y0 - b.max.y).max(b.min.y - y1).max(0.0);
        if dx.hypot(dy) > reach {
            found.push(
                Diagnostic::new(
                    "cad.far-outside",
                    Severity::Warning,
                    format!("Object {} lies far away from the rest of the drawing, so ZOOM Extents shrinks the drawing to a dot.", h.hex()),
                )
                .on(of_entity(*h))
                .fix("erase", Some(erase(*h))),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_cad_doc::geom::Vec3;
    use kition_cad_doc::{Common, DimKind, Dimension, Hatch, HatchLoop, Line, LwPolyline, MText, Space, Text};

    fn line(d: &mut Drawing, a: (f64, f64), b: (f64, f64)) -> Handle {
        d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })).unwrap()
    }

    fn codes(d: &Drawing) -> Vec<String> {
        check(d).into_iter().map(|diagnostic| diagnostic.code).collect()
    }

    fn vertex(x: f64, y: f64) -> PolyVertex {
        PolyVertex { p: Vec2::new(x, y), ..PolyVertex::default() }
    }

    fn text(value: &str) -> Text {
        serde_json::from_value(json!({ "insert": [0.0, 0.0, 0.0], "height": 2.5, "value": value })).unwrap()
    }

    #[test]
    fn a_clean_drawing_has_no_diagnostics() {
        let mut d = Drawing::new_metric();
        for i in 0..5 {
            line(&mut d, (0.0, f64::from(i)), (10.0, f64::from(i)));
        }
        assert_eq!(check(&d), vec![]);
        kition_kit::diagnostics::check_diagnostics(&check(&d)).unwrap();
    }

    #[test]
    fn audit_findings_are_errors_with_the_audit_fix() {
        let mut d = Drawing::new_metric();
        let h = line(&mut d, (0.0, 0.0), (10.0, 0.0));
        d.modify_entity(h, |e| e.common.layer = "Gone".into()).unwrap();
        let zero = line(&mut d, (5.0, 5.0), (5.0, 5.0));
        let found = check(&d);
        kition_kit::diagnostics::check_diagnostics(&found).unwrap();
        let missing = found.iter().find(|x| x.code == "cad.missing-layer").expect("missing layer");
        assert_eq!(missing.severity, Severity::Error);
        assert_eq!(missing.fix.as_ref().unwrap().command, "audit");
        let invalid = found.iter().find(|x| x.code == "cad.invalid-geometry").expect("zero-length line");
        assert_eq!(invalid.target.path.as_deref(), Some(format!("entity:{}", zero.hex()).as_str()));
        assert_eq!(invalid.fix.as_ref().unwrap().command, "erase");
    }

    #[test]
    fn degenerate_polylines_and_hatches_are_warnings() {
        let mut d = Drawing::new_metric();
        let poly =
            LwPolyline { vertices: vec![vertex(1.0, 1.0), vertex(1.0, 1.0)], closed: false, const_width: 0.0, elevation: 0.0, plinegen: false };
        d.add(&Space::Model, Common::default(), EntityKind::LwPolyline(poly)).unwrap();
        let flat = HatchLoop { vertices: vec![vertex(0.0, 0.0), vertex(5.0, 0.0), vertex(10.0, 0.0)], outer: true };
        let hatch: Hatch = serde_json::from_value(json!({ "pattern": "SOLID", "solid": true, "loops": [flat] })).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch)).unwrap();
        let square = HatchLoop { vertices: vec![vertex(0.0, 0.0), vertex(5.0, 0.0), vertex(5.0, 5.0), vertex(0.0, 5.0)], outer: true };
        let good: Hatch = serde_json::from_value(json!({ "pattern": "SOLID", "solid": true, "loops": [square] })).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Hatch(good)).unwrap();
        let found = codes(&d);
        assert_eq!(found.iter().filter(|c| *c == "cad.degenerate-polyline").count(), 1);
        assert_eq!(found.iter().filter(|c| *c == "cad.degenerate-hatch").count(), 1, "{found:?}");
    }

    #[test]
    fn blank_texts_and_overridden_dimensions_are_info() {
        let mut d = Drawing::new_metric();
        d.add(&Space::Model, Common::default(), EntityKind::Text(text("  "))).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Text(text("Plan"))).unwrap();
        let mtext: MText = serde_json::from_value(json!({ "insert": [0.0, 0.0, 0.0], "height": 2.5, "contents": "\\P" })).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::MText(mtext)).unwrap();
        let dimension = |text: &str| -> Dimension {
            serde_json::from_value(json!({ "kind": DimKind::Aligned, "defpt": [0.0, 0.0, 0.0], "textMid": [0.0, 0.0, 0.0], "text": text })).unwrap()
        };
        let fixed = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension("1200"))).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension("<> mm"))).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(""))).unwrap();
        let found = check(&d);
        assert_eq!(found.iter().filter(|x| x.code == "cad.empty-text").count(), 2);
        let overridden: Vec<_> = found.iter().filter(|x| x.code == "cad.dimension-text-overridden").collect();
        assert_eq!(overridden.len(), 1);
        assert_eq!(overridden[0].severity, Severity::Info);
        assert_eq!(overridden[0].fix.as_ref().unwrap().params, Some(json!({ "handles": [fixed.hex()], "textOverride": "" })));
    }

    #[test]
    fn objects_on_hidden_layers_are_counted_per_layer() {
        let mut d = Drawing::new_metric();
        d.ensure_layer("Notes");
        d.layer_mut("Notes").unwrap().frozen = true;
        d.ensure_layer("Empty");
        d.layer_mut("Empty").unwrap().on = false;
        for i in 0..3 {
            let h = line(&mut d, (0.0, f64::from(i)), (1.0, f64::from(i)));
            d.modify_entity(h, |e| e.common.layer = "Notes".into()).unwrap();
        }
        let found: Vec<_> = check(&d).into_iter().filter(|x| x.code == "cad.hidden-layer").collect();
        assert_eq!(found.len(), 1, "a hidden layer without objects is not reported");
        assert_eq!(found[0].target.path.as_deref(), Some("layer:Notes"));
        assert_eq!(found[0].fix.as_ref().unwrap().params, Some(json!({ "name": "Notes", "frozen": false })));
    }

    #[test]
    fn unavailable_fonts_of_used_styles_are_reported() {
        let mut d = Drawing::new_metric();
        let mut style = kition_cad_doc::TextStyle { name: "Titles".into(), font: "No Such Font Family 9".into(), ..Default::default() };
        d.text_styles.push(style.clone());
        style.name = "Unused".into();
        d.text_styles.push(style);
        let mut titled = text("Plan");
        titled.style = "Titles".into();
        d.add(&Space::Model, Common::default(), EntityKind::Text(titled)).unwrap();
        let found: Vec<_> = check(&d).into_iter().filter(|x| x.code == "cad.font-substituted").collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target.path.as_deref(), Some("style:Titles"));
    }

    #[test]
    fn unloaded_external_references_are_warnings() {
        let mut d = Drawing::new_metric();
        let mut block = kition_cad_doc::Block::new("Site");
        block.xref_path = Some("site.dwg".into());
        d.blocks.insert("Site".into(), std::sync::Arc::new(block));
        assert!(codes(&d).contains(&"cad.unresolved-xref".to_owned()));
    }

    #[test]
    fn a_stray_object_far_away_is_a_warning() {
        let mut d = Drawing::new_metric();
        for i in 0..10 {
            line(&mut d, (0.0, f64::from(i)), (100.0, f64::from(i)));
        }
        let stray = line(&mut d, (1e9, 1e9), (1e9 + 1.0, 1e9));
        let found: Vec<_> = check(&d).into_iter().filter(|x| x.code == "cad.far-outside").collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target.path.as_deref(), Some(format!("entity:{}", stray.hex()).as_str()));
    }

    #[test]
    fn a_handle_from_a_replicas_own_range_is_not_a_handseed_finding() {
        // A replica mints handles from its actor's slot (kdm_model::handle_range), far above the
        // drawing's next handle; the DXF writer and reader keep the file's $HANDSEED above every
        // handle, so the finding is never something to fix.
        let mut d = Drawing::new_metric();
        for i in 0..4 {
            line(&mut d, (0.0, f64::from(i)), (10.0, f64::from(i)));
        }
        d.model.push(kition_cad_doc::Entity::new(
            Handle(0x000F_2654_0000_0000),
            EntityKind::Line(Line { a: Vec3::new(0.0, 9.0, 0.0), b: Vec3::new(10.0, 9.0, 0.0) }),
        ));
        assert_eq!(codes(&d), Vec::<String>::new());
    }

    #[test]
    fn each_code_is_capped_and_errors_come_first() {
        let mut d = Drawing::new_metric();
        for _ in 0..(PER_CODE + 5) {
            d.add(&Space::Model, Common::default(), EntityKind::Text(text(""))).unwrap();
        }
        let h = line(&mut d, (0.0, 0.0), (1.0, 0.0));
        d.modify_entity(h, |e| e.common.layer = "Gone".into()).unwrap();
        let found = check(&d);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found.iter().filter(|x| x.code == "cad.empty-text").count(), PER_CODE);
        let truncated = found.iter().find(|x| x.code == "cad.truncated").expect("truncated");
        assert!(truncated.message.starts_with("5 more `cad.empty-text`"));
        kition_kit::diagnostics::check_diagnostics(&found).unwrap();
    }
}
