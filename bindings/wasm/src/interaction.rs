//! The embedded editor's grips, selected properties, grid and viewport synchronization.
use super::*;
use kition_cad_doc::{EntityKind, Handle};
use kition_cad_engine::grips::{GripMode, stretch_selection};
use kition_cad_engine::snap::tracking::{Track, TrackConfig, Tracker};

#[derive(Clone, Copy)]
pub(super) struct HotGrip {
    handle: Handle,
    index: usize,
    pub base: Vec2,
}

impl CadDocument {
    /// Keep the raw cursor for previews and use the same destination for pointer commits.
    pub(super) fn cursor_point(&mut self, raw: Vec2) -> Vec2 {
        self.cursor = Some(raw);
        self.snapped = if self.session.running.is_some() || self.hot_grip.is_some() { self.snap(raw) } else { None };
        if self.hot_grip.is_some() { self.grip_destination(raw) } else { self.snapped.map_or(raw, |(point, _)| point) }
    }

    fn grip_destination(&self, raw: Vec2) -> Vec2 {
        self.grip_resolution(raw).0
    }

    fn grip_resolution(&self, raw: Vec2) -> (Vec2, Option<Track>) {
        // Match the engine's point-entry ordering: object snap, grid, tracking, then Ortho.
        if let Some((point, _)) = self.snapped {
            return (point, None);
        }
        let mut point = raw;
        if self.session.settings.snapmode
            && let Ok(doc) = self.session.doc()
        {
            let (origin, angle) = kition_cad_engine::snap::grid_frame(doc);
            point = kition_cad_engine::snap::grid_snap(point, self.session.settings.snapunit, origin, angle);
        }
        if let Some(grip) = self.hot_grip
            && self.session.settings.polarmode
            && !self.session.settings.orthomode
            && let Ok(state) = self.session.state()
        {
            let pixel = self.session.pixel_size();
            let config = TrackConfig::new(&self.session.settings, self.session.angle_settings().angbase);
            // Polar rays need no acquisition timer. Dwell-based object tracking is separate;
            // a fresh tracker prevents points acquired by another command from leaking here.
            if let Some(track) = Tracker::default().track(
                &state.doc,
                &state.edit_space(),
                point,
                pixel * self.session.settings.aperture,
                pixel * 6.0,
                &config,
                Some(grip.base),
            ) {
                return (track.point, Some(track));
            }
        }
        if let Some(grip) = self.hot_grip
            && self.session.settings.orthomode
        {
            point = kition_cad_engine::snap::ortho(grip.base, point);
        }
        (point, None)
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
        self.snapped = None;
        Ok(())
    }

    pub(super) fn grip_overlays(&self) -> Vec<ToolOverlay> {
        let (Some(grip), Some(cursor), Ok(doc)) = (self.hot_grip, self.cursor, self.session.doc()) else { return Vec::new() };
        let (destination, track) = self.grip_resolution(cursor);
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
        if let Some(track) = track {
            let (tx, ty) = place.to_doc(track.point);
            overlays.push(ToolOverlay::Line { ax: x, ay: y, bx: tx, by: ty, color: SNAP, dashed: true });
            overlays.push(ToolOverlay::Label { x: tx + 8.0, y: ty - 8.0, text: track.object.unwrap_or("Polar").to_owned(), color: SNAP });
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
