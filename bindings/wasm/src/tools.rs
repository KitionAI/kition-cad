//! Pointer and keyboard routing for the embedded CAD editor.
use super::*;

impl KitTools for CadDocument {
    fn tools(&self) -> Vec<ToolInfo> {
        let mut tools =
            vec![ToolInfo { id: "select".into(), label: "Select".into(), shortcut: Some("Escape".into()), icon: "tool-selection".into(), group: 0 }];
        for (id, group, icon) in TOOLS {
            let Some(spec) = kition_cad_engine::cmd::find_command(id).filter(|spec| spec.interactive.is_some()) else { continue };
            let shortcut = spec.aliases.first().map(|alias| alias.to_ascii_uppercase()).filter(|alias| alias.len() == 1);
            tools.push(ToolInfo {
                id: spec.id.to_owned(),
                label: spec.label.trim_end_matches('…').to_owned(),
                shortcut,
                icon: (*icon).to_owned(),
                group: *group,
            });
        }
        tools
    }

    fn tool(&self) -> String {
        self.session.running.as_ref().map_or_else(|| "select".to_owned(), |running| running.id.clone())
    }

    fn select_tool(&mut self, id: &str, zoom: f64) -> Result<ToolResponse, String> {
        let r = self.select_tool_inner(id, zoom);
        self.session.sync_kdm();
        r
    }

    fn pointer(&mut self, event: &ToolPointer) -> Result<ToolResponse, String> {
        let r = self.pointer_inner(event);
        self.session.sync_kdm();
        r
    }

    fn key(&mut self, key: &str, mods: ToolMods, zoom: f64) -> Result<ToolResponse, String> {
        let r = self.key_inner(key, mods, zoom);
        self.session.sync_kdm();
        r
    }

    /// Typed text is a command-line entry.
    fn text(&mut self, text: &str, zoom: f64) -> Result<ToolResponse, String> {
        self.sync_view(zoom);
        let result = self.command_line(text);
        if self.cursor.is_some() {
            self.resolve_cursor();
        }
        self.session.sync_kdm();
        result?;
        Ok(self.respond())
    }

    fn state(&mut self, x: f64, y: f64, mods: ToolMods, zoom: f64) -> ToolResponse {
        self.sync_view(zoom);
        let at = self.placement().to_world(x, y);
        self.cursor_point(at, mods.shift);
        self.respond()
    }
}

/// The tool events, before the KDM bridge writes what they changed ([`KitTools`] wraps them).
impl CadDocument {
    fn select_tool_inner(&mut self, id: &str, zoom: f64) -> Result<ToolResponse, String> {
        self.sync_view(zoom);
        self.hot_grip = None;
        self.forget_aim();
        *self.reported.borrow_mut() = id.to_owned();
        // Already so: the host following a switch this engine asked for.
        if self.tool() == id {
            return Ok(self.respond());
        }
        if self.session.running.is_some() {
            self.session.cancel();
        }
        if id != "select" {
            kition_cad_engine::cmd::find_command(id).filter(|spec| spec.interactive.is_some()).ok_or_else(|| format!("no tool {id}"))?;
            self.session.start(id).map_err(engine_error)?;
        }
        Ok(self.respond())
    }

    fn pointer_inner(&mut self, event: &ToolPointer) -> Result<ToolResponse, String> {
        self.sync_view(event.zoom);
        // The tracking clock: the page repeats a move where the pointer rests (after the dwell),
        // and only that repeat counts as resting, so a point is acquired however fast events come.
        if event.action == PointerAction::Move {
            if self.rest == Some((event.x, event.y)) {
                self.clock += kition_cad_engine::snap::tracking::DWELL * 1.25;
            }
            self.rest = Some((event.x, event.y));
        } else {
            self.rest = None;
        }
        let place = self.placement();
        let at = place.to_world(event.x, event.y);
        let shift = event.mods.shift;
        let point = self.cursor_point(at, shift);
        match event.action {
            PointerAction::Move => {}
            PointerAction::Down => {
                self.press = Some((point, (event.x, event.y)));
                self.press_hit = self.hit;
                self.dragging = false;
            }
            PointerAction::Drag => {
                if let Some((from, (dx, dy))) = self.press
                    && self.hot_grip.is_none()
                    && !self.dragging
                    && ((event.x - dx).hypot(event.y - dy)) * event.zoom > DRAG_PX
                {
                    // A drag opens a selection window where it started, when objects are being selected.
                    self.dragging = self.session.begin_window(from);
                }
            }
            PointerAction::Up => {
                // Escape discards the press. Its later release is not a new click.
                let Some((from, _)) = self.press.take() else { return Ok(self.respond()) };
                let hit = self.press_hit.take();
                let target = if self.dragging { at } else { from };
                let dragged = self.dragging;
                self.dragging = false;
                if self.hot_grip.is_some() {
                    self.commit_grip(target)?;
                } else if self.session.running.is_some() {
                    // A snap goes in as the snap (a deferred tangent stays deferred until the
                    // line's other end is known), a plain point as the point.
                    let input = match hit {
                        Some(hit) if !dragged => hit.input(),
                        _ => Input::Point(target),
                    };
                    self.feed(input)?;
                } else if !shift
                    && self.session.pending_window.is_none()
                    && let Some(grip) = self.pick_grip(target, event.zoom)
                {
                    self.hot_grip = Some(grip);
                } else {
                    self.session.idle_click(target, shift).map_err(engine_error)?;
                }
                // The prompt (or grip) changed: the pointer resolves anew for it.
                self.resolve_cursor();
            }
            PointerAction::DoubleClick => {}
        }
        Ok(self.respond())
    }

    fn key_inner(&mut self, key: &str, _mods: ToolMods, zoom: f64) -> Result<ToolResponse, String> {
        self.sync_view(zoom);
        match key {
            "Enter" if self.session.running.is_some() => {
                self.feed(Input::Enter)?;
            }
            // Enter between commands repeats the last one, also with objects selected (when the
            // page hands editing keys to the engine); a hot grip keeps it.
            "Enter" if self.hot_grip.is_none() => {
                self.command_line("")?;
            }
            "Delete" | "Backspace" if self.session.running.is_none() && !self.session.selection().is_empty() => {
                self.hot_grip = None;
                self.forget_aim();
                self.session.execute("object.delete", &json!({})).map_err(engine_error)?;
            }
            "Escape" => {
                let cancelled_grip = self.hot_grip.take().is_some();
                self.press = None;
                self.press_hit = None;
                self.dragging = false;
                self.forget_aim();
                self.session.pending_window = None;
                // A grip is an adapter-owned command: first cancel it, then let a second
                // Escape clear the retained selection through the engine's idle path.
                if !cancelled_grip {
                    self.session.cancel();
                    if self.session.running.is_none() {
                        self.session.set_selection(Vec::new());
                    }
                }
            }
            _ => {}
        }
        if self.cursor.is_some() {
            self.resolve_cursor();
        }
        Ok(self.respond())
    }
}
