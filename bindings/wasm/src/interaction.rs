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

    /// Draw a restrained grid behind opaque geometry. Work is bounded by the output pixel count,
    /// even for hostile spacing or viewport values; the grid gets coarser as the person zooms out.
    pub(super) fn draw_grid(&self, pixels: &mut [u8], x: f64, y: f64, scale: f64, width: u32, height: u32) {
        if !self.session.settings.gridmode {
            return;
        }
        let place = self.placement();
        let step = |unit: f64| {
            let pixels = unit * place.k * scale;
            if !pixels.is_finite() || pixels <= 0.0 {
                return None;
            }
            let factor = 10.0_f64.powf((12.0 / pixels).max(1.0).log10().ceil());
            let step = pixels * factor;
            (step.is_finite() && step >= 1.0).then_some(step)
        };
        let (Some(sx), Some(sy)) = (step(self.session.settings.gridunit.x), step(self.session.settings.gridunit.y)) else { return };
        let (ox, oy) = place.to_doc(Vec2::ZERO);
        let on_line = |pixel: f64, origin: f64, step: f64| (((pixel - origin) / step).round().mul_add(step, origin) - pixel).abs() < 0.5;
        let columns: Vec<bool> = (0..width).map(|i| on_line(f64::from(i), (ox - x) * scale, sx)).collect();
        for (row, rgba) in pixels.chunks_exact_mut(width as usize * 4).take(height as usize).enumerate() {
            let horizontal = on_line(row as f64, (oy - y) * scale, sy);
            for (col, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                if (horizontal || columns.get(col).copied().unwrap_or(false)) && *pixel == [255, 255, 255, 255] {
                    pixel.copy_from_slice(&[235, 233, 229, 255]);
                }
            }
        }
    }
}
