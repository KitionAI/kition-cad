//! The embedded editor's grips, selected properties, grid and viewport synchronization.
use super::*;
use kition_cad_doc::{EntityKind, Handle};
use kition_cad_engine::grips::{GripMode, stretch_selection};
use kition_cad_engine::snap::tracking::{CursorQuery, PathShape};

#[derive(Clone, Copy)]
pub(super) struct HotGrip {
    handle: Handle,
    index: usize,
    pub base: Vec2,
}

/// How close (screen pixels) the cursor slides onto a tracking path.
const TRACK_PX: f64 = 6.0;
/// How far (screen pixels) a tracking path is drawn past the cursor.
const PATH_PX: f64 = 3000.0;
/// Half the arms of an acquired point's `+` and a crossing's `x` (screen pixels).
const MARK_PX: f64 = 4.0;
/// The fewest screen pixels between grid lines: closer, the spacing is multiplied by 5.
pub(super) const GRID_PX: f64 = 8.0;
/// The grid's colour on white paper.
const GRID: [u8; 4] = [235, 233, 229, 255];

/// GRIDUNIT along one axis, or SNAPUNIT's when it is 0 (the grid follows the snap spacing).
fn grid_unit(grid: f64, snap: f64) -> f64 {
    if grid == 0.0 { snap.abs() } else { grid.abs() }
}

/// Screen pixels between grid lines of `unit` drawing units at `scale` pixels per unit: the unit
/// times 5 until they are [`GRID_PX`] apart (at most 40 times), or `None` when there is no such
/// spacing.
fn grid_step(unit: f64, scale: f64) -> Option<f64> {
    let mut step = unit * scale;
    if !step.is_finite() || step <= 0.0 {
        return None;
    }
    for _ in 0..40 {
        if step >= GRID_PX {
            break;
        }
        step *= 5.0;
    }
    (step.is_finite() && step >= GRID_PX).then_some(step)
}

impl CadDocument {
    /// The pointer at `raw` (drawing units), with Shift held or not: runs the cursor pipeline
    /// there and returns the point a click there gives.
    pub(super) fn cursor_point(&mut self, raw: Vec2, shift: bool) -> Vec2 {
        self.cursor = Some(raw);
        self.shift = shift;
        self.resolve_cursor()
    }

    /// Forgets what the pointer resolved to (a tool or command changed under it).
    pub(super) fn forget_aim(&mut self) {
        self.snapped = None;
        self.hit = None;
        self.track = None;
        self.aim = None;
    }

    /// The cursor pipeline at the pointer, one order for commands and hot grips alike, as the
    /// engine defines it ([`kition_cad_engine::Session::snap_cursor`]): object snap, grid snap,
    /// tracking paths (polar, object snap tracking), then Ortho; a one-pick snap override (END,
    /// NON typed or chosen from the menu) instead snaps with that mode alone. It runs at point
    /// prompts and for a hot grip; elsewhere (no command, a "Select objects" prompt, a window being
    /// dragged) the point is the pointer itself. The result also goes to the session
    /// (`Session.cursor`, `cursor_deferred`), which direct distance entry reads: a typed `60` goes
    /// 60 units from the base point toward the cursor, or along the tracking path it is on.
    pub(super) fn resolve_cursor(&mut self) -> Vec2 {
        let Some(raw) = self.cursor else { return Vec2::ZERO };
        self.forget_aim();
        if !raw.is_finite() {
            return raw;
        }
        // Acquired tracking points belong to one command or one grip edit.
        let owner = if self.hot_grip.is_some() { Some("grip".to_owned()) } else { self.session.running.as_ref().map(|running| running.id.clone()) };
        if owner != self.tracking_for {
            self.session.tracking.clear();
            self.tracking_for = owner;
        }
        let Some((base, deferred)) = self.point_query() else {
            self.session.tracking.clear();
            self.aim = Some(raw);
            self.session.cursor = raw;
            self.session.cursor_deferred = None;
            return raw;
        };
        let pixel = self.session.pixel_size();
        let aperture = pixel * self.session.settings.aperture.max(1.0);
        let point = match self.session.snap_override().filter(|_| self.hot_grip.is_none()) {
            Some(mode) => {
                let hatch = if self.session.settings.osnaphatch { kition_cad_engine::snap::mode::HATCH } else { 0 };
                let hit =
                    self.session.state().ok().and_then(|state| {
                        kition_cad_engine::snap::osnap(&state.doc, &state.edit_space(), raw, aperture, mode | hatch, base, deferred)
                    });
                self.hit = hit;
                hit.map_or(raw, |hit| hit.point)
            }
            None => {
                let query = CursorQuery { base, deferred, aperture, tol: pixel * TRACK_PX, now: self.clock, shift: self.shift };
                let resolved = self.session.snap_cursor(raw, &query);
                self.hit = resolved.snap;
                self.track = resolved.track;
                resolved.point
            }
        };
        let point = if point.is_finite() { point } else { raw };
        self.snapped = self.hit.map(|hit| (hit.point, hit.name));
        self.aim = Some(point);
        self.session.cursor = point;
        self.session.cursor_deferred = self.hit.and_then(|hit| hit.deferred);
        point
    }

    /// The base point and deferred-snap flag of the pipeline's query, when it runs: a hot grip,
    /// or a prompt that asks for a point (not an object pick) with no selection window open.
    fn point_query(&self) -> Option<(Option<Vec2>, bool)> {
        if let Some(grip) = self.hot_grip {
            return Some((Some(grip.base), false));
        }
        if self.session.pending_window.is_some() {
            return None;
        }
        let prompt = self.session.current_prompt()?;
        kition_cad_engine::pointmod::accepts(&prompt).then_some((prompt.base, prompt.deferred))
    }

    /// The tracking guides at the cursor: each path the point lies on as a dashed line (a
    /// circle's extension as a dashed arc), a small `+` on each acquired point, an `x` where two
    /// paths cross or a path meets an object, and the tooltip ("Polar: 60.0000 < 45°").
    pub(super) fn tracking_overlays(&self) -> Vec<ToolOverlay> {
        let mut out = Vec::new();
        if self.point_query().is_none() {
            return out;
        }
        let place = self.placement();
        let pixel = self.session.pixel_size();
        let mark = pixel * MARK_PX;
        let cross = |out: &mut Vec<ToolOverlay>, at: Vec2, arms: [Vec2; 2]| {
            for arm in arms {
                let (ax, ay) = place.to_doc(at - arm);
                let (bx, by) = place.to_doc(at + arm);
                out.push(ToolOverlay::Line { ax, ay, bx, by, color: SNAP, dashed: false });
            }
        };
        for acquired in &self.session.tracking.acquired {
            cross(&mut out, acquired.at(), [Vec2::new(mark, 0.0), Vec2::new(0.0, mark)]);
        }
        let Some(track) = &self.track else { return out };
        let far = pixel * PATH_PX;
        let mut tips = Vec::new();
        for path in &track.paths {
            match path.shape {
                PathShape::Ray { origin, dir } => {
                    let back = (track.point - origin).dot(dir).clamp(0.0, far);
                    let (ax, ay) = place.to_doc(track.point - dir * back);
                    let (bx, by) = place.to_doc(track.point + dir * far);
                    out.push(ToolOverlay::Line { ax, ay, bx, by, color: SNAP, dashed: true });
                }
                PathShape::Line { dir, .. } => {
                    let (ax, ay) = place.to_doc(track.point - dir * far);
                    let (bx, by) = place.to_doc(track.point + dir * far);
                    out.push(ToolOverlay::Line { ax, ay, bx, by, color: SNAP, dashed: true });
                }
                PathShape::Circle { center, radius, .. } => {
                    let start = center.angle_to(track.point);
                    let span = (far / radius.max(1e-12)).min(std::f64::consts::PI);
                    let mut d = String::new();
                    for step in 0..=96 {
                        let at = center + Vec2::from_angle(start - span + span * f64::from(step) / 48.0) * radius;
                        let (x, y) = place.to_doc(at);
                        d.push_str(&format!("{}{x:.2},{y:.2}", if step == 0 { "M" } else { " L" }));
                    }
                    out.push(ToolOverlay::Path { d, color: SNAP, width: 1.0, dashed: true });
                }
            }
            tips.push(format!("{}: {}", path.name, self.distance_and_angle(path.from, track.point)));
        }
        if let Some(object) = track.object {
            tips.push(object.to_owned());
        }
        if track.paths.len() > 1 || track.object.is_some() {
            cross(&mut out, track.point, [Vec2::new(mark, mark), Vec2::new(mark, -mark)]);
        }
        if !tips.is_empty() {
            let (x, y) = place.to_doc(track.point);
            out.push(ToolOverlay::Measure { x, y, text: tips.join(", ") });
        }
        out
    }

    /// "60.0000 < 45°": the distance from `from` to `to` in the drawing's units (LUNITS, LUPREC)
    /// and the direction in whole degrees from the X axis.
    fn distance_and_angle(&self, from: Vec2, to: Vec2) -> String {
        let (lunits, luprec) = self.session.doc().map(|doc| (doc.header.i64("LUNITS", 2), doc.header.i64("LUPREC", 4))).unwrap_or((2, 4));
        let distance = kition_cad_engine::units::format_distance(from.dist(to), lunits, luprec);
        let degrees = from.angle_to(to).to_degrees();
        let degrees = if degrees.is_finite() { (degrees.round() as i64).rem_euclid(360) } else { 0 };
        format!("{distance} < {degrees}°")
    }

    /// What the cursor is now, as the tool answer's CSS cursor, which the editor draws as CAD does
    /// (`web/src/cursor.ts`): `text` at a prompt that takes free text (there Space types a space
    /// on the command line, `web/src/App.tsx`); `pointer` at a prompt that picks objects, where
    /// the pick box alone shows; `crosshair` at any other prompt, for a hot grip, while a selection
    /// window is open, and between commands with PICKFIRST off (the crosshair alone); `default`
    /// between commands (the crosshair with the pick box).
    pub(super) fn cursor_state(&self) -> &'static str {
        if self.free_text_prompt() {
            "text"
        } else if self.hot_grip.is_some() || self.session.pending_window.is_some() {
            "crosshair"
        } else if let Some(prompt) = self.session.current_prompt() {
            if prompt.picks_objects() { "pointer" } else { "crosshair" }
        } else if self.session.settings.pickfirst {
            "default"
        } else {
            "crosshair"
        }
    }

    /// Whether the object under the pick box is highlighted now: where the cursor picks objects
    /// (between commands with PICKFIRST, at "Select objects"), and not while a selection window is
    /// open, a drag goes on or a grip is hot.
    fn shows_hover(&self) -> bool {
        self.hot_grip.is_none() && self.session.pending_window.is_none() && !self.dragging && self.session.picking_objects()
    }

    /// Picks the object under the pointer at `at` again (on a pointer move or release), for the
    /// rollover highlight; nothing where the highlight does not show.
    pub(super) fn update_hover(&mut self, at: Vec2) {
        self.hover = if self.shows_hover() { self.session.hover(at, self.session.pick_aperture()) } else { None };
    }

    /// The object the rollover highlight shows, if any.
    pub(super) fn hovered(&self) -> Option<Handle> {
        self.hover.filter(|_| self.shows_hover())
    }

    /// The cursor, for the editor to draw (both overlays are fully transparent, [`CURSOR`]): an
    /// anchor where the crosshair goes, as large as the pick box (PICKBOX × 1.5 pixels each side
    /// of its centre, the pick range), and a label with the coordinate readout, the point a click
    /// gives in the drawing's units (LUNITS, LUPREC). The crosshair is on the pointer, or on the
    /// object snap or grid snap holding the point; tracking and Ortho move only the rubber band.
    pub(super) fn cursor_overlays(&self) -> Vec<ToolOverlay> {
        let Some(raw) = self.cursor.filter(|point| point.is_finite()) else { return Vec::new() };
        let aim = self.aim.filter(|point| point.is_finite()).unwrap_or(raw);
        let settings = &self.session.settings;
        let held = self.hit.is_some() || (settings.snapmode && !settings.orthomode && self.track.is_none() && self.point_query().is_some());
        let place = self.placement();
        let (x, y) = place.to_doc(if held { aim } else { raw });
        let size = (settings.pickbox.max(1.0) * 3.0).min(300.0) as f32;
        let (ax, ay) = place.to_doc(aim);
        vec![
            ToolOverlay::Anchor { x, y, color: CURSOR, filled: true, size },
            ToolOverlay::Label { x: ax, y: ay, text: self.coordinates(aim), color: CURSOR },
        ]
    }

    /// "120.0000, 45.5000, 0.0000": a point in the drawing's units (LUNITS, LUPREC), as the status
    /// bar shows the cursor.
    fn coordinates(&self, point: Vec2) -> String {
        let (lunits, luprec) = self.session.doc().map(|doc| (doc.header.i64("LUNITS", 2), doc.header.i64("LUPREC", 4))).unwrap_or((2, 4));
        let format = |value: f64| kition_cad_engine::units::format_distance(value, lunits, luprec);
        format!("{}, {}, {}", format(point.x), format(point.y), format(0.0))
    }

    pub(super) fn prompt(&self) -> String {
        if self.hot_grip.is_some() { "STRETCH Specify stretch point:".into() } else { self.session.prompt_text() }
    }

    pub(super) fn pick_grip(&self, point: Vec2, zoom: f64) -> Option<HotGrip> {
        let doc = self.session.doc().ok()?;
        let aperture = 6.0 / (self.placement().k * zoom.max(1e-9));
        self.session
            .selection()
            .into_iter()
            .filter_map(|handle| {
                let entity = doc.entity(handle)?;
                if doc.layer(&entity.common.layer).is_some_and(|layer| layer.locked || layer.frozen || !layer.on) {
                    return None;
                }
                entity
                    .kind
                    .grips()
                    .into_iter()
                    .enumerate()
                    .filter(|(_, base)| base.dist(point) <= aperture)
                    .min_by(|(_, a), (_, b)| a.dist(point).total_cmp(&b.dist(point)))
                    .map(|(index, base)| HotGrip { handle, index, base })
            })
            .min_by(|a, b| a.base.dist(point).total_cmp(&b.base.dist(point)))
    }

    pub(super) fn commit_grip(&mut self, point: Vec2) -> Result<(), String> {
        let Some(grip) = self.hot_grip else { return Ok(()) };
        self.session.grip_edit(grip.handle, grip.index, point, GripMode::Stretch).map_err(engine_error)?;
        self.hot_grip = None;
        self.forget_aim();
        Ok(())
    }

    pub(super) fn grip_overlays(&self) -> Vec<ToolOverlay> {
        let (Some(grip), Some(destination), Ok(doc)) = (self.hot_grip, self.aim, self.session.doc()) else { return Vec::new() };
        let Some(edits) = stretch_selection(doc, grip.handle, grip.index, destination, self.session.selection()) else {
            return Vec::new();
        };
        let previews: Vec<_> = edits
            .into_iter()
            .filter_map(|(handle, kind)| {
                let mut entity = (**doc.entity(handle)?).clone();
                entity.kind = kind;
                Some(entity)
            })
            .collect();
        let list =
            kition_cad_render::build_entities(doc, &previews, &kition_cad_render::Options { tolerance: self.tolerance(), ..Default::default() });
        let place = self.placement();
        let mut overlays = self.paths(&list, &place, ACCENT, 1.5, false);
        let (x, y) = place.to_doc(grip.base);
        overlays.push(ToolOverlay::Anchor { x, y, color: ACCENT, filled: true, size: 9.0 });
        // The tracking path itself is drawn with the command's ([`CadDocument::tracking_overlays`]);
        // the grip names what holds its point.
        if let Some(track) = &self.track {
            let (tx, ty) = place.to_doc(track.point);
            let name = track.object.or_else(|| track.paths.first().map(|path| path.name)).unwrap_or("Polar");
            overlays.push(ToolOverlay::Label { x: tx + 8.0, y: ty - 8.0, text: name.to_owned(), color: SNAP });
        }
        overlays
    }

    pub(super) fn selected_objects(&self) -> Vec<Value> {
        let Ok(doc) = self.session.doc() else { return Vec::new() };
        self.session
            .selection()
            .into_iter()
            .filter_map(|handle| {
                let e = doc.entity(handle)?;
                let mut value = json!({ "handle": handle.hex(), "type": e.kind.type_name(), "layer": e.common.layer,
                "color": e.common.color.name(), "locked": doc.layer(&e.common.layer).is_some_and(|l| l.locked) });
                if let EntityKind::Circle(circle) = &e.kind {
                    value["radius"] = json!(circle.radius);
                    value["center"] = json!([circle.center.x, circle.center.y]);
                }
                Some(value)
            })
            .collect()
    }

    pub(super) fn current_view(&self) -> Option<View> {
        self.session.state().ok().map(|s| s.view())
    }

    pub(super) fn record_view(&mut self, before: Option<View>) {
        let Some(view) = self.current_view().filter(|v| Some(*v) != before) else { return };
        let place = self.placement();
        let (x, y) = place.to_doc(view.center);
        self.view_serial = self.view_serial.saturating_add(1);
        self.view_request = Some(json!({ "serial": self.view_serial, "x": x, "y": y, "height": view.height * place.k }));
    }

    pub(super) fn sync_viewport(&mut self, params: &Value) -> Result<Value, String> {
        let number =
            |name: &str| params.get(name).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| format!("invalid viewport {name}"));
        let (x, y, zoom, width, height) = (number("x")?, number("y")?, number("zoom")?, number("width")?, number("height")?);
        if zoom <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Err("viewport dimensions and zoom must be positive".into());
        }
        let place = self.placement();
        let center = place.to_world(x + width / (2.0 * zoom), y + height / (2.0 * zoom));
        let world_height = height / (zoom * place.k);
        if !center.is_finite() || !world_height.is_finite() {
            return Err("viewport is out of range".into());
        }
        self.session.viewport_px = (width, height);
        self.session.state_mut().map_err(engine_error)?.set_view_quiet(View { center, height: world_height });
        self.viewport_synced = true;
        Ok(json!({}))
    }

    /// Draws the grid behind the drawing in pixels already rendered: `width × height` pixels whose
    /// top-left is document pixel (`x`, `y`), at `sx` × `sy` pixels per document pixel. Only for
    /// renders without the editor's canvas colour (previews, a host's thumbnails); the on-screen
    /// editor draws the grid over its whole canvas itself (`web/src/cursor.ts`), outside the
    /// drawing's area too and at the size shown. The spacing adapts as the grid display does:
    /// GRIDUNIT (SNAPUNIT when it is 0) times 5 until lines are [`GRID_PX`] apart, with every
    /// GRIDMAJOR line solid and the others dotted. Work is bounded by the pixel count, even for
    /// hostile spacing or viewport values.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_grid(&self, pixels: &mut [u8], x: f64, y: f64, sx: f64, sy: f64, width: u32, height: u32) {
        let settings = &self.session.settings;
        if !settings.gridmode || self.backdrop.is_some() || width == 0 || height == 0 {
            return;
        }
        let place = self.placement();
        let (Some(sx_step), Some(sy_step)) = (
            grid_step(grid_unit(settings.gridunit.x, settings.snapunit.x), place.k * sx),
            grid_step(grid_unit(settings.gridunit.y, settings.snapunit.y), place.k * sy),
        ) else {
            return;
        };
        let major = i64::from(settings.gridmajor.max(1));
        let (ox, oy) = place.to_doc(Vec2::ZERO);
        // Which grid line a pixel is on, if any: its index from the origin's line.
        let line = |pixel: f64, origin: f64, step: f64| {
            let index = ((pixel - origin) / step).round();
            ((index.mul_add(step, origin) - pixel).abs() < 0.5 && index.is_finite()).then_some(index as i64)
        };
        let columns: Vec<Option<i64>> = (0..width).map(|i| line(f64::from(i), (ox - x) * sx, sx_step)).collect();
        let background = self.background();
        let paper = [background.0, background.1, background.2, 255];
        for (row, rgba) in pixels.chunks_exact_mut(width as usize * 4).take(height as usize).enumerate() {
            let horizontal = line(row as f64, (oy - y) * sy, sy_step);
            for (col, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                if *pixel != paper {
                    continue;
                }
                // A major line is solid; a minor one dotted (every other pixel along it).
                let on = |index: Option<i64>, along: usize| index.is_some_and(|i| i.rem_euclid(major) == 0 || along.is_multiple_of(2));
                if on(horizontal, col) || on(columns.get(col).copied().flatten(), row) {
                    pixel.copy_from_slice(&GRID);
                }
            }
        }
    }
}
