//! Grip editing: dragging an object's grips (the points from [`EntityKind::grips`]) the way
//! AutoCAD does, plus the multi-functional grip modes (move, rotate, scale, mirror about a grip).
//!
//! The UI calls [`Session::grip_edit`]; every edit runs through a `grip.*` command so it is
//! undoable and reachable from scripts, the control channel and MCP.

use kition_cad_doc::{DimKind, Drawing, EntityKind, Handle, LwPolyline};
use kition_cad_geom::{Arc, EPS, Mat3, Polyline, Vec2, Vec3, norm_angle};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{EngineError, Result, Session};

/// What dragging a hot grip does (AutoCAD's grip modes, cycled with Space/Enter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GripMode {
    /// Move the grip itself (the object's shape changes).
    #[default]
    Stretch,
    /// Move the whole object by the grip displacement.
    Move,
    /// Rotate the object about the grip; the angle is from the grip to the new point.
    Rotate,
    /// Scale the object about the grip; the factor is the distance from the grip to the new point.
    Scale,
    /// Mirror the object about the line from the grip to the new point.
    Mirror,
}

impl GripMode {
    /// The mode Space or Enter goes to next: STRETCH, MOVE, ROTATE, SCALE, MIRROR, and round.
    pub fn next(self) -> Self {
        match self {
            GripMode::Stretch => GripMode::Move,
            GripMode::Move => GripMode::Rotate,
            GripMode::Rotate => GripMode::Scale,
            GripMode::Scale => GripMode::Mirror,
            GripMode::Mirror => GripMode::Stretch,
        }
    }

    /// The mode's name as the command line shows it ("STRETCH").
    pub fn name(self) -> &'static str {
        match self {
            GripMode::Stretch => "STRETCH",
            GripMode::Move => "MOVE",
            GripMode::Rotate => "ROTATE",
            GripMode::Scale => "SCALE",
            GripMode::Mirror => "MIRROR",
        }
    }

    /// The mode a typed word names (`move`, `ROTATE`), whole and in any case.
    pub fn named(word: &str) -> Option<Self> {
        let word = word.trim();
        [GripMode::Stretch, GripMode::Move, GripMode::Rotate, GripMode::Scale, GripMode::Mirror]
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(word))
    }
}

/// What a hot grip can do besides its modes (the options of a multifunctional grip): offered at the
/// hot-grip prompt and in the grip's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GripAction {
    /// Polyline: a new vertex after this vertex, or on this segment (its midpoint grip).
    AddVertex,
    /// Polyline: this vertex goes (a polyline keeps two at least).
    RemoveVertex,
    /// Line or arc end: the object lengthened or shortened along itself at this end.
    Lengthen,
}

impl GripAction {
    /// The option as the hot-grip prompt offers it (typed by its capital or whole).
    pub fn keyword(self) -> &'static str {
        match self {
            GripAction::AddVertex => "Add vertex",
            GripAction::RemoveVertex => "Remove vertex",
            GripAction::Lengthen => "Lengthen",
        }
    }

    /// The grip menu's label.
    pub fn label(self) -> &'static str {
        match self {
            GripAction::AddVertex => "Add Vertex",
            GripAction::RemoveVertex => "Remove Vertex",
            GripAction::Lengthen => "Lengthen",
        }
    }

    /// The command line's title while the option asks for its point ("ADD VERTEX").
    pub fn title(self) -> &'static str {
        match self {
            GripAction::AddVertex => "ADD VERTEX",
            GripAction::RemoveVertex => "REMOVE VERTEX",
            GripAction::Lengthen => "LENGTHEN",
        }
    }

    /// Whether the option asks for a point (Remove vertex acts at once).
    pub fn needs_point(self) -> bool {
        !matches!(self, GripAction::RemoveVertex)
    }
}

/// The options of grip `index` of an object: a line's or an arc's ends lengthen; a polyline's
/// vertex adds a vertex after it or is removed (when more than two are left), its segment midpoint
/// adds a vertex on the segment.
pub fn grip_actions(kind: &EntityKind, index: usize) -> Vec<GripAction> {
    match kind {
        EntityKind::Line(_) | EntityKind::Arc(_) if index == 0 || index == 2 => vec![GripAction::Lengthen],
        EntityKind::LwPolyline(p) => match poly_grip(p, index) {
            Some(PolyGrip::Vertex(_)) if p.vertices.len() > 2 => vec![GripAction::AddVertex, GripAction::RemoveVertex],
            Some(PolyGrip::Vertex(_) | PolyGrip::Mid(_)) => vec![GripAction::AddVertex],
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The polyline vertex an option of grip `index` works on: the one a new vertex goes after (the
/// grip's vertex, or the start of the grip's segment), or the one removed.
fn action_vertex(p: &LwPolyline, index: usize) -> Option<usize> {
    match poly_grip(p, index)? {
        PolyGrip::Vertex(i) | PolyGrip::Mid(i) => Some(i),
    }
}

/// The object after option `action` of grip `index`, with the point `to` (Add vertex, Lengthen) or
/// the length change `value` (Lengthen); `None` when the option does not apply or the result would
/// be degenerate. The preview uses it; [`Session::grip_act`] commits the same shape.
pub fn grip_action_result(kind: &EntityKind, index: usize, action: GripAction, to: Option<Vec2>, value: Option<f64>) -> Option<EntityKind> {
    if !grip_actions(kind, index).contains(&action) {
        return None;
    }
    match (action, kind) {
        (GripAction::Lengthen, _) => {
            let pick = *kind.grips().get(index)?;
            crate::cmd::lengthened(kind, pick, to, value)
        }
        (GripAction::AddVertex, EntityKind::LwPolyline(p)) => {
            let at = to.filter(|q| q.is_finite())?;
            let i = action_vertex(p, index)?;
            let mut p = p.clone();
            if let Some(v) = p.vertices.get_mut(i) {
                v.bulge = 0.0;
            }
            p.vertices.insert(i.checked_add(1)?.min(p.vertices.len()), kition_cad_geom::PolyVertex::new(at));
            Some(EntityKind::LwPolyline(p))
        }
        (GripAction::RemoveVertex, EntityKind::LwPolyline(p)) => {
            let i = action_vertex(p, index)?;
            let n = p.vertices.len();
            let mut p = p.clone();
            let before = if i == 0 && p.closed { n.checked_sub(1)? } else { i.saturating_sub(1) };
            if before != i
                && let Some(v) = p.vertices.get_mut(before)
            {
                v.bulge = 0.0;
            }
            (i < n).then(|| p.vertices.remove(i))?;
            Some(EntityKind::LwPolyline(p))
        }
        _ => None,
    }
}

fn set_xy(p: &mut Vec3, q: Vec2) {
    p.x = q.x;
    p.y = q.y;
}

fn translated(k: &EntityKind, d: Vec2) -> EntityKind {
    let mut k = k.clone();
    k.transform(&Mat3::translate(d));
    k
}

/// What a polyline grip index refers to (matches the order of `EntityKind::grips`).
enum PolyGrip {
    Vertex(usize),
    /// Midpoint of the segment starting at this vertex.
    Mid(usize),
}

fn poly_grip(p: &LwPolyline, index: usize) -> Option<PolyGrip> {
    let n = p.vertices.len();
    // Start vertices of the non-degenerate segments, in the same order as Polyline::segments().
    let count = if p.closed { n } else { n.saturating_sub(1) };
    let mut seg_starts = Vec::with_capacity(count);
    for i in 0..count {
        let (Some(v), Some(w)) = (p.vertices.get(i), p.vertices.get((i + 1) % n.max(1))) else { continue };
        if v.p.near(w.p, EPS) && v.bulge.abs() < 1e-12 {
            continue;
        }
        seg_starts.push(i);
    }
    let mut k = 0usize;
    for i in 0..n {
        if k == index {
            return Some(PolyGrip::Vertex(i));
        }
        k += 1;
        if let Some(start) = seg_starts.get(i) {
            if k == index {
                return Some(PolyGrip::Mid(*start));
            }
            k += 1;
        }
    }
    None
}

/// The shape after dragging grip `index` to `to` (STRETCH mode). `None` when the grip does not
/// exist or the result would be degenerate.
pub fn stretch_grip(kind: &EntityKind, index: usize, to: Vec2) -> Option<EntityKind> {
    if !to.is_finite() {
        return None;
    }
    let grips = kind.grips();
    let from = *grips.get(index)?;
    let delta = to - from;
    let mut k = kind.clone();
    match &mut k {
        EntityKind::Line(l) => match index {
            0 => set_xy(&mut l.a, to),
            1 => return Some(translated(kind, delta)),
            _ => set_xy(&mut l.b, to),
        },
        EntityKind::Point(_) | EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_) | EntityKind::Image(_) | EntityKind::Table(_) => {
            return Some(translated(kind, delta));
        }
        EntityKind::Hatch(_) => return Some(translated(kind, delta)),
        EntityKind::Circle(c) => {
            if index == 0 {
                return Some(translated(kind, delta));
            }
            let r = c.center.xy().dist(to);
            if r <= EPS {
                return None;
            }
            c.radius = r;
        }
        EntityKind::Arc(a) => {
            let g = Arc::new(a.center.xy(), a.radius, a.start, a.end);
            let (mut s, mut m, mut e) = (g.start_point(), g.mid_point(), g.end_point());
            match index {
                0 => s = to,
                1 => m = to,
                2 => e = to,
                _ => return Some(translated(kind, delta)),
            }
            let na = Arc::from_3_points(s, m, e)?;
            return Some(crate::cmd::helpers::arc(&na));
        }
        EntityKind::Ellipse(e) => {
            let c = e.center.xy();
            let mj = e.major.xy();
            let a = mj.len();
            if a <= EPS {
                return None;
            }
            let b = a * e.ratio;
            match index {
                0 => return Some(translated(kind, delta)),
                1 | 3 => {
                    let v = if index == 1 { to - c } else { c - to };
                    let l = v.len();
                    if l <= EPS {
                        return None;
                    }
                    if l >= b {
                        e.major = v.to3(0.0);
                        e.ratio = (b / l).clamp(1e-9, 1.0);
                    } else {
                        e.major = (v.normalized().perp() * b).to3(0.0);
                        e.ratio = (l / b).clamp(1e-9, 1.0);
                        shift_params(e, -std::f64::consts::FRAC_PI_2);
                    }
                }
                _ => {
                    let u = mj / a;
                    let l = (to - c).dot(u.perp()).abs();
                    if l <= EPS {
                        return None;
                    }
                    if l <= a {
                        e.ratio = (l / a).clamp(1e-9, 1.0);
                    } else {
                        e.major = (u.perp() * l).to3(0.0);
                        e.ratio = (a / l).clamp(1e-9, 1.0);
                        shift_params(e, -std::f64::consts::FRAC_PI_2);
                    }
                }
            }
        }
        EntityKind::LwPolyline(p) => {
            let n = p.vertices.len();
            match poly_grip(p, index)? {
                PolyGrip::Vertex(i) => p.vertices.get_mut(i)?.p = to,
                PolyGrip::Mid(i) => {
                    let j = (i + 1) % n.max(1);
                    let (a, b) = (p.vertices.get(i)?.p, p.vertices.get(j)?.p);
                    if p.vertices.get(i)?.bulge.abs() > 1e-12 {
                        // Arc segment: reshape through the dragged midpoint.
                        p.vertices.get_mut(i)?.bulge = crate::cmd::curves::bulge_through(a, to, b);
                    } else {
                        p.vertices.get_mut(i)?.p = a + delta;
                        p.vertices.get_mut(j)?.p = b + delta;
                    }
                }
            }
        }
        EntityKind::Polyline3d(p) => set_xy(p.points.get_mut(index)?, to),
        EntityKind::Spline(sp) => {
            if sp.fit.is_empty() {
                *sp.control.get_mut(index)? = to;
            } else {
                let mut fit = sp.fit.clone();
                *fit.get_mut(index)? = to;
                *sp = sp.refit(&fit, sp.closed);
            }
        }
        EntityKind::Ray(r) | EntityKind::XLine(r) => {
            if index == 0 {
                return Some(translated(kind, delta));
            }
            let d = (to - r.base.xy()).normalized();
            if d == Vec2::ZERO {
                return None;
            }
            r.dir = d.to3(0.0);
        }
        EntityKind::Insert(i) => {
            if index == 0 {
                return Some(translated(kind, delta));
            }
            let a = i.attribs.get_mut(index - 1)?;
            let ip = a.text.insert.xy() + delta;
            set_xy(&mut a.text.insert, ip);
            if let Some(al) = &mut a.text.align_pt {
                let q = al.xy() + delta;
                set_xy(al, q);
            }
        }
        EntityKind::Dimension(d) => {
            match index {
                0 => set_xy(&mut d.p13, to),
                1 => set_xy(&mut d.p14, to),
                2 => {
                    set_xy(&mut d.defpt, to);
                    if matches!(d.kind, DimKind::Radius | DimKind::Diameter) {
                        d.user_text_pos = false;
                    }
                }
                _ => {
                    set_xy(&mut d.text_mid, to);
                    d.user_text_pos = true;
                }
            }
            d.block = None;
        }
        EntityKind::Leader(l) => set_xy(l.vertices.get_mut(index)?, to),
        EntityKind::MLeader(m) => {
            let mut idx = 0usize;
            let mut done = false;
            'outer: for l in &mut m.leaders {
                for v in l.iter_mut() {
                    if idx == index {
                        set_xy(v, to);
                        done = true;
                        break 'outer;
                    }
                    idx += 1;
                }
            }
            if !done {
                // The landing grip moves the landing and its content.
                let ld = m.landing.xy() + delta;
                set_xy(&mut m.landing, ld);
                if let Some(t) = &mut m.text {
                    let q = t.insert.xy() + delta;
                    set_xy(&mut t.insert, q);
                }
                if let Some(b) = &mut m.block {
                    let q = b.insert.xy() + delta;
                    set_xy(&mut b.insert, q);
                }
            }
        }
        EntityKind::Solid(s) | EntityKind::Trace(s) => set_xy(s.corners.get_mut(index)?, to),
        EntityKind::Face3d(f) => set_xy(f.corners.get_mut(index)?, to),
        EntityKind::Viewport(v) => {
            let opposite = *grips.get((index + 2) % 4)?;
            let w = (to.x - opposite.x).abs();
            let h = (to.y - opposite.y).abs();
            if w <= EPS || h <= EPS {
                return None;
            }
            let c = to.mid(opposite);
            set_xy(&mut v.center, c);
            v.width = w;
            v.height = h;
        }
        EntityKind::Wipeout(w) => *w.boundary.get_mut(index)? = to,
        EntityKind::Unknown(_) => return None,
    }
    Some(k)
}

fn shift_params(e: &mut kition_cad_doc::Ellipse, by: f64) {
    let full = (kition_cad_geom::ccw_sweep(e.start, e.end) - kition_cad_geom::TAU).abs() < 1e-9;
    if !full {
        e.start = norm_angle(e.start + by);
        e.end = norm_angle(e.end + by);
    }
}

/// The complete stretch edit, shared by preview and commit. Connected selected objects
/// follow the primary grip; locked members and degenerate secondary edits stay unchanged.
/// An invalid or locked primary refuses the whole edit without mutating the drawing.
pub fn stretch_selection(
    doc: &Drawing,
    handle: Handle,
    index: usize,
    to: Vec2,
    handles: impl IntoIterator<Item = Handle>,
) -> Option<Vec<(Handle, EntityKind)>> {
    let editable = |h| doc.entity(h).filter(|e| !doc.layer(&e.common.layer).is_some_and(|l| l.locked));
    let primary = editable(handle)?;
    let kind = stretch_grip(&primary.kind, index, to)?;
    let from = *primary.kind.grips().get(index)?;
    let tolerance = 1e-9 * (1.0 + from.len());
    let mut edits = vec![(handle, kind)];
    for other in handles {
        if edits.iter().any(|(h, _)| *h == other) {
            continue;
        }
        let Some(entity) = editable(other) else { continue };
        if let Some(i) = entity.kind.grips().iter().position(|g| g.near(from, tolerance))
            && let Some(kind) = stretch_grip(&entity.kind, i, to)
        {
            edits.push((other, kind));
        }
    }
    Some(edits)
}

/// The transform for a multi-functional grip mode about `base` with the dragged point `to`.
pub fn mode_matrix(mode: GripMode, base: Vec2, to: Vec2) -> Option<Mat3> {
    match mode {
        GripMode::Stretch => None,
        GripMode::Move => Some(Mat3::translate(to - base)),
        GripMode::Rotate => Some(Mat3::rotate_about(base, base.angle_to(to))),
        GripMode::Scale => {
            let f = base.dist(to);
            (f > EPS && f.is_finite()).then(|| Mat3::scale_about(base, f))
        }
        GripMode::Mirror => (!base.near(to, EPS)).then(|| Mat3::mirror(base, to)),
    }
}

impl Session {
    /// Drag grip `grip_index` of `handle` to `new_point` in `mode` (one undo step).
    /// The base point of the rotate/scale/mirror modes is the grip itself. As in AutoCAD the edit
    /// covers the selection: a stretch drags the grips of other selected objects that sit on the
    /// same point, and the other modes transform every selected object.
    pub fn grip_edit(&mut self, handle: Handle, grip_index: usize, new_point: Vec2, mode: GripMode) -> Result<()> {
        let to = [new_point.x, new_point.y];
        let hs: Vec<String> = std::iter::once(handle).chain(self.selection().into_iter().filter(|h| *h != handle)).map(|h| h.hex()).collect();
        let r = match mode {
            GripMode::Stretch => self.execute("grip.move", &json!({ "handle": handle.hex(), "handles": hs, "index": grip_index, "to": to })),
            GripMode::Move => {
                self.execute("grip.move", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to, "mode": "move" }))
            }
            GripMode::Rotate => self.execute("grip.rotate", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to })),
            GripMode::Scale => self.execute("grip.scale", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to })),
            GripMode::Mirror => self.execute("grip.mirror", &json!({ "handles": hs, "baseHandle": handle.hex(), "index": grip_index, "to": to })),
        };
        r.map(|_| ())
    }

    /// Option `action` of grip `grip_index` of `handle` with the point `to` or the length change
    /// `value` (one undo step), through the commands that do it: `object.lengthen` from the grip's
    /// end, `polyline.edit`'s vertex insert or remove.
    pub fn grip_act(&mut self, handle: Handle, grip_index: usize, action: GripAction, to: Option<Vec2>, value: Option<f64>) -> Result<()> {
        let entity = self.doc()?.entity(handle).cloned().ok_or_else(|| EngineError::Other("no such object".into()))?;
        if !grip_actions(&entity.kind, grip_index).contains(&action) {
            return Err(EngineError::Other(format!("{} is not an option of that grip", action.label())));
        }
        let (h, at) = (handle.hex(), to.filter(|p| p.is_finite()).map(|p| [p.x, p.y]));
        let r = match (action, &entity.kind) {
            (GripAction::Lengthen, kind) => {
                let pick = kind.grips().get(grip_index).map(|p| [p.x, p.y]);
                match (value.filter(|v| v.is_finite()), at) {
                    (Some(delta), _) => self.execute("object.lengthen", &json!({ "handle": h, "pick": pick, "delta": delta })),
                    (None, Some(to)) => self.execute("object.lengthen", &json!({ "handle": h, "pick": pick, "to": to })),
                    (None, None) => return Err(EngineError::Other("Lengthen needs a point or a length change".into())),
                }
            }
            (GripAction::AddVertex | GripAction::RemoveVertex, EntityKind::LwPolyline(p)) => {
                let index = action_vertex(p, grip_index).ok_or_else(|| EngineError::Other("no such grip".into()))?;
                if action == GripAction::AddVertex {
                    let to = at.ok_or_else(|| EngineError::Other("Add vertex needs a point".into()))?;
                    self.execute("polyline.edit", &json!({ "handle": h, "option": "vertex", "action": "insert", "index": index, "to": to }))
                } else {
                    self.execute("polyline.edit", &json!({ "handle": h, "option": "vertex", "action": "remove", "index": index }))
                }
            }
            _ => return Err(EngineError::Other("no such grip option".into())),
        };
        r.map(|_| ())
    }

    /// Grip points of an object (the indices `grip_edit` takes).
    pub fn grips_of(&self, handle: Handle) -> Result<Vec<Vec2>> {
        self.doc()?.entity(handle).map(|e| e.kind.grips()).ok_or_else(|| EngineError::Other("no such object".into()))
    }
}

/// Polyline helper kept public for the UI: segment midpoints are grips too.
pub fn polyline_grip_count(p: &LwPolyline) -> usize {
    p.vertices.len() + Polyline { vertices: p.vertices.clone(), closed: p.closed }.segments().len().min(p.vertices.len())
}

#[cfg(test)]
#[path = "grips_tests.rs"]
mod tests;
