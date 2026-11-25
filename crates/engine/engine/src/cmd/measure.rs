//! `object.measure`: what an agent that cannot see precisely needs measured, in one read-only
//! command (task card C4/C5/C6-cad). It changes nothing and reads no editor state: entities are
//! named by handle, a space by name.
//!
//! - `{bounds: [handles]}`: each entity's bounding box (`min`, `max`), as the drawing's extents
//!   count it.
//! - `{distance: [a, b]}`: the minimum distance between two entities or points (`[x, y]`), and the
//!   two nearest points (`from` on `a`, `to` on `b`). Exact for lines and points; along circles,
//!   arcs, ellipses and splines it is measured on the curve flattened to within a millionth of the
//!   two objects' size. Entities that cross are 0 apart.
//! - `{intersections: [a, b]}`: the points where two entities cross (exact for lines, circles and
//!   arcs; flattened curves otherwise), each once.
//! - `{overlaps: {layout?}}`: pairs of objects of a space (Model unless `layout` names one) that
//!   draw the same geometry: `duplicate` when their properties are the same too (what OVERKILL
//!   deletes), `same-geometry` when they differ (another layer or colour). Nothing is deleted.
//!
//! A block reference, a text or a dimension has no curve of its own here: measure the block's
//! entities or the text's insertion point instead.

use kition_cad_doc::{Drawing, Entity, Handle, Prim, Space};
use kition_cad_geom::{Bounds2, Line, Segment, Vec2};
use serde_json::{Value, json};

use super::{CommandSpec, bad, point_value};
use crate::{Result, Session};

const CMD: &str = "object.measure";

/// The most curve pieces one object is flattened into for a distance.
const MAX_PIECES: usize = 20_000;
/// The most objects `overlaps` compares in one space.
const MAX_OVERLAP_OBJECTS: usize = 200_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("object.measure", "Measure Entities", run)
            .params("{bounds?: [handles], distance?: [a, b] (a handle or [x, y] each), intersections?: [a, b] (handles), overlaps?: {layout?: string}} → bounds | {distance, from, to} | {points} | {pairs}")
            .noundo(),
    ]
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let asked: Vec<&str> =
        ["bounds", "distance", "intersections", "overlaps"].into_iter().filter(|k| p.get(*k).is_some_and(|v| !v.is_null())).collect();
    match asked.as_slice() {
        ["bounds"] => bounds(d, &p["bounds"]),
        ["distance"] => distance(d, &p["distance"]),
        ["intersections"] => intersections(d, &p["intersections"]),
        ["overlaps"] => overlaps(d, &p["overlaps"]),
        _ => Err(bad(CMD, "give one of `bounds` ([handles]), `distance` ([a, b]), `intersections` ([a, b]) or `overlaps` ({layout?})")),
    }
}

fn point_json(p: Vec2) -> Value {
    json!([p.x, p.y])
}

/// The entity `v` names, in model space or a layout.
fn entity<'d>(d: &'d Drawing, v: &Value) -> Result<&'d Entity> {
    let h = v.as_str().and_then(Handle::parse_hex).ok_or_else(|| bad(CMD, format!("{v} is not an entity handle (a hex string such as \"1A2\")")))?;
    if let Some(e) = d.entity(h) {
        return Ok(e);
    }
    if d.blocks.values().any(|b| b.entities.get(h).is_some()) {
        return Err(bad(CMD, format!("{} is inside a block definition: measure an entity in model space or a layout", h.hex())));
    }
    Err(bad(CMD, format!("the drawing has no entity {}", h.hex())))
}

fn pair(v: &Value, what: &str) -> Result<(Value, Value)> {
    match v.as_array().map(Vec::as_slice) {
        Some([a, b]) => Ok((a.clone(), b.clone())),
        _ => Err(bad(CMD, format!("`{what}` takes two items, [a, b]"))),
    }
}

fn bounds(d: &Drawing, v: &Value) -> Result<Value> {
    let items = v.as_array().ok_or_else(|| bad(CMD, "`bounds` takes a list of handles"))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let e = entity(d, item)?;
        let b = kition_cad_doc::entity_bounds(d, e, 0);
        if b.is_empty() {
            return Err(bad(CMD, format!("{} draws nothing, so it has no bounds", e.handle.hex())));
        }
        out.push(json!({ "handle": e.handle.hex(), "min": point_json(b.min), "max": point_json(b.max) }));
    }
    Ok(json!({ "bounds": out }))
}

/// An entity's curves as segments (lines and arcs; ellipses and splines flattened), and its lone
/// points.
fn pieces(e: &Entity, tol: f64) -> (Vec<Segment>, Vec<Vec2>) {
    let (mut segs, mut points) = (Vec::new(), Vec::new());
    let lines = |pts: &[Vec2], segs: &mut Vec<Segment>| {
        segs.extend(pts.windows(2).filter_map(|w| Some(Segment::Line(Line::new(*w.first()?, *w.get(1)?)))));
    };
    for prim in e.kind.prims() {
        match prim {
            Prim::Seg(s) => segs.push(s),
            Prim::Circle(c) => segs.push(Segment::Arc { arc: kition_cad_geom::Arc::new(c.center, c.radius, 0.0, std::f64::consts::TAU), ccw: true }),
            Prim::Ellipse(el) => {
                let mut pts = Vec::new();
                el.tessellate(tol, &mut pts);
                lines(&pts, &mut segs);
            }
            Prim::Spline(sp) => lines(&sp.tessellate(tol), &mut segs),
            Prim::Infinite { base, dir, .. } => segs.push(Segment::Line(Line::new(base - dir * 1e7, base + dir * 1e7))),
            Prim::Fill(f) => {
                let n = f.len();
                segs.extend((0..n).filter_map(|i| Some(Segment::Line(Line::new(*f.get(i)?, *f.get((i + 1) % n)?)))));
            }
            Prim::Point(p) => points.push(p),
        }
    }
    (segs, points)
}

/// What `v` is for a distance: an entity's pieces, or a point.
fn shape(d: &Drawing, v: &Value, tol: f64) -> Result<(Vec<Segment>, Vec<Vec2>)> {
    if v.is_array()
        && let Some(p) = point_value(v)
    {
        return Ok((Vec::new(), vec![p]));
    }
    let e = entity(d, v)?;
    let (segs, points) = pieces(e, tol);
    if segs.is_empty() && points.is_empty() {
        return Err(bad(CMD, format!("{} ({}) has no curve to measure: measure the entities it is made of", e.handle.hex(), e.kind.type_name())));
    }
    Ok((segs, points))
}

fn size_of(d: &Drawing, items: &[&Value]) -> f64 {
    let mut b = Bounds2::EMPTY;
    for item in items {
        if let Some(p) = point_value(item).filter(|_| item.is_array()) {
            b = b.union(&Bounds2::new(p, p));
        } else if let Ok(e) = entity(d, item) {
            b = b.union(&kition_cad_doc::entity_bounds(d, e, 0));
        }
    }
    if b.is_empty() { 1.0 } else { b.width().max(b.height()).max(1e-9) }
}

/// Points along `segs` to measure from: every end, and the curves' points at `tol`.
fn samples(segs: &[Segment], points: &[Vec2], tol: f64) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = points.to_vec();
    for s in segs {
        match s {
            Segment::Line(l) => out.extend([l.a, l.b]),
            Segment::Arc { .. } => s.tessellate(tol, &mut out),
        }
        if out.len() > MAX_PIECES {
            break;
        }
    }
    out
}

/// The point of `segs`/`points` nearest `p`, and its distance.
fn nearest(segs: &[Segment], points: &[Vec2], p: Vec2) -> Option<(Vec2, f64)> {
    segs.iter().map(|s| s.closest(p)).chain(points.iter().copied()).map(|q| (q, q.dist(p))).min_by(|a, b| a.1.total_cmp(&b.1))
}

fn crossings(a: &[Segment], b: &[Segment], eps: f64) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for sa in a {
        for sb in b {
            for p in kition_cad_geom::intersect_segments(sa, sb) {
                if p.is_finite() && !out.iter().any(|q| q.dist(p) <= eps) {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn distance(d: &Drawing, v: &Value) -> Result<Value> {
    let (a, b) = pair(v, "distance")?;
    let size = size_of(d, &[&a, &b]);
    let tol = size * 1e-6;
    let (sa, pa) = shape(d, &a, tol)?;
    let (sb, pb) = shape(d, &b, tol)?;
    if let Some(p) = crossings(&sa, &sb, tol).first() {
        return Ok(json!({ "distance": 0.0, "from": point_json(*p), "to": point_json(*p) }));
    }
    let mut best: Option<(f64, Vec2, Vec2)> = None;
    for from in samples(&sa, &pa, tol) {
        if let Some((to, dist)) = nearest(&sb, &pb, from)
            && best.is_none_or(|(b, ..)| dist < b)
        {
            best = Some((dist, from, to));
        }
    }
    for to in samples(&sb, &pb, tol) {
        if let Some((from, dist)) = nearest(&sa, &pa, to)
            && best.is_none_or(|(b, ..)| dist < b)
        {
            best = Some((dist, from, to));
        }
    }
    let (dist, from, to) = best.ok_or_else(|| bad(CMD, "nothing to measure between"))?;
    Ok(json!({ "distance": dist, "from": point_json(from), "to": point_json(to) }))
}

fn intersections(d: &Drawing, v: &Value) -> Result<Value> {
    let (a, b) = pair(v, "intersections")?;
    if !(a.is_string() && b.is_string()) {
        return Err(bad(CMD, "`intersections` takes two entities by handle"));
    }
    let tol = size_of(d, &[&a, &b]) * 1e-9;
    let (sa, _) = shape(d, &a, tol.max(1e-12) * 1e3)?;
    let (sb, _) = shape(d, &b, tol.max(1e-12) * 1e3)?;
    let points: Vec<Value> = crossings(&sa, &sb, tol.max(1e-12)).into_iter().map(point_json).collect();
    Ok(json!({ "points": points }))
}

fn overlaps(d: &Drawing, v: &Value) -> Result<Value> {
    let space = match v.get("layout").and_then(Value::as_str) {
        None => Space::Model,
        Some(name) if name.eq_ignore_ascii_case("model") => Space::Model,
        Some(name) => {
            let layout = d.layout(name).ok_or_else(|| bad(CMD, format!("the drawing has no layout `{name}`")))?;
            Space::Paper(layout.name.clone())
        }
    };
    let store = d.space(&space).ok_or_else(|| bad(CMD, "that space has no entities"))?;
    if store.len() > MAX_OVERLAP_OBJECTS {
        return Err(bad(CMD, format!("the space holds {} objects, more than {MAX_OVERLAP_OBJECTS} to compare", store.len())));
    }
    let mut first: std::collections::HashMap<String, (Handle, String)> = std::collections::HashMap::new();
    let mut pairs = Vec::new();
    for e in store.iter() {
        let geometry = serde_json::to_string(&e.kind).unwrap_or_default();
        let common = serde_json::to_string(&e.common).unwrap_or_default();
        match first.get(&geometry) {
            Some((h, c)) => {
                pairs.push(json!({ "a": h.hex(), "b": e.handle.hex(), "kind": if *c == common { "duplicate" } else { "same-geometry" } }));
            }
            None => {
                first.insert(geometry, (e.handle, common));
            }
        }
    }
    Ok(json!({ "pairs": pairs }))
}
