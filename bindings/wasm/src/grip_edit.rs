//! Hot grips: what a click on a selected object's grip starts, as CAD does. The grip goes hot and
//! the next click places it; Space or Enter cycles the modes STRETCH, MOVE, ROTATE, SCALE and
//! MIRROR; the options are Base point, Copy, Undo, Reference (ROTATE, SCALE) and eXit, and a single
//! grip's own options (a line's or an arc's end: Lengthen; an arc's midpoint: Radius; a polyline's
//! vertex: Add vertex, Remove vertex; its segment's midpoint: Add vertex, and on an arc segment
//! Convert to Line). Ctrl (or Cmd) held while placing places a copy and
//! keeps editing copies until Enter. Shift+click makes several grips hot; a plain click on one of
//! them stretches them together.
//!
//! The edit is the editor's, not a running engine command (the tool stays Select); each placement
//! runs a `grip.*` command, `object.lengthen` or `polyline.edit`, so it is undoable and the same as
//! an agent's. The prompt is the engine's [`Prompt`], so typed options match as at any prompt.
use super::*;
use kition_cad_doc::{EntityKind, Handle};
use kition_cad_engine::grips::{GripAction, GripMode, grip_action_result, grip_actions, mode_matrix, stretch_grip, stretch_selection};
use kition_cad_engine::{Accept, Prompt};

/// A grip under the pointer (a click there picks it): warm.
pub const GRIP_WARM: [u8; 4] = [255, 127, 159, 255];
/// A hot grip.
pub const GRIP_HOT: [u8; 4] = [220, 38, 38, 255];
/// The fully transparent label that tells the editor which grip is under the pointer and what its
/// menu offers (`web/src/cursor.ts`); a host that draws overlays plainly shows nothing for it.
pub const GRIP_INFO: [u8; 4] = [255, 0, 0, 0];
/// How close (screen pixels) the pointer picks a grip: the grip's square and a margin.
pub const GRIP_PX: f64 = 6.0;
/// The most grips hot at once (a cap on the work per edit).
const MAX_HOT: usize = 1000;
/// The editor command the grip menu starts an edit with.
pub const EDIT: &str = "grip.edit";

/// A grip: grip `index` of object `handle`, at `point`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HotGrip {
    pub handle: Handle,
    pub index: usize,
    pub point: Vec2,
}

/// What the hot-grip prompt asks for besides the mode's own point.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ask {
    Mode,
    /// Base point: another point the mode measures from.
    Base,
    /// Reference (ROTATE, SCALE): the reference angle or length, typed or from two points.
    RefFirst,
    RefSecond(Vec2),
    /// The new angle or length; the reference (radians, drawing units) is known.
    RefNew(f64),
    /// A grip option that takes a point (Add vertex, Lengthen).
    Action(GripAction),
}

/// How a placement edits: to a point, or by a typed angle (radians) or scale factor.
#[derive(Clone, Copy, Debug)]
enum Edit {
    To(Vec2),
    Angle(f64),
    Factor(f64),
}

/// A grip edit in progress.
pub(super) struct GripEdit {
    /// The hot grips; the first is the base grip.
    grips: Vec<HotGrip>,
    /// The point the modes measure from: the base grip, or a picked base point.
    base: Vec2,
    mode: GripMode,
    copy: bool,
    /// The undo steps each copy made, in order (Undo takes the last copy back).
    copies: Vec<usize>,
    ask: Ask,
}

impl GripEdit {
    fn rotate_or_scale(&self) -> bool {
        matches!(self.mode, GripMode::Rotate | GripMode::Scale)
    }

    /// "STRETCH", or the option's title while it asks for its point ("LENGTHEN").
    fn title(&self) -> &'static str {
        match self.ask {
            Ask::Action(action) => action.title(),
            _ => self.mode.name(),
        }
    }
}

fn other(message: impl Into<String>) -> String {
    message.into()
}

/// A finite number greater than zero.
fn positive(value: Option<f64>) -> Result<f64, String> {
    value.filter(|v| v.is_finite() && *v > 1e-12).ok_or_else(|| other("Requires a positive number."))
}

impl CadDocument {
    /// The grip of a selected object at `point` (drawing units) a click there picks: the nearest
    /// within [`GRIP_PX`] screen pixels. Grips of objects on locked, frozen or off layers are drawn
    /// but not picked.
    pub(super) fn pick_grip(&self, point: Vec2, zoom: f64) -> Option<HotGrip> {
        let doc = self.session.doc().ok()?;
        let aperture = GRIP_PX / (self.placement().k * zoom.max(1e-9));
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
                    .filter(|(_, at)| at.dist(point) <= aperture)
                    .min_by(|(_, a), (_, b)| a.dist(point).total_cmp(&b.dist(point)))
                    .map(|(index, at)| HotGrip { handle, index, point: at })
            })
            .min_by(|a, b| a.point.dist(point).total_cmp(&b.point.dist(point)))
    }

    /// The grip under the pointer at `at`, warm, between commands with no grip edit going on and no
    /// window being dragged.
    pub(super) fn update_hover_grip(&mut self, at: Vec2, zoom: f64) {
        let shows = self.hot_grip.is_none() && self.session.running.is_none() && self.session.pending_window.is_none() && !self.dragging;
        self.hover_grip = if shows { self.pick_grip(at, zoom) } else { None };
    }

    /// A grip's colour: hot (being edited, or made hot with Shift), warm (under the pointer), or
    /// the selection's.
    pub(super) fn grip_color(&self, handle: Handle, index: usize) -> [u8; 4] {
        let is = |grip: &HotGrip| grip.handle == handle && grip.index == index;
        if self.hot_grip.as_ref().is_some_and(|edit| edit.grips.iter().any(is)) || self.marked.iter().any(is) {
            GRIP_HOT
        } else if self.hover_grip.as_ref().is_some_and(is) {
            GRIP_WARM
        } else {
            SELECTED
        }
    }

    /// A click on `grip` between commands: with Shift it goes hot (or cold again) and waits; plainly
    /// it starts the edit, with the other hot grips when it is one of them (it is the base grip).
    pub(super) fn click_grip(&mut self, grip: HotGrip, shift: bool) {
        let selection = self.session.selection();
        self.marked.retain(|hot| selection.contains(&hot.handle));
        let same = |hot: &HotGrip| hot.handle == grip.handle && hot.index == grip.index;
        if shift {
            match self.marked.iter().position(same) {
                Some(at) => {
                    self.marked.remove(at);
                }
                None if self.marked.len() < MAX_HOT => self.marked.push(grip),
                None => {}
            }
            return;
        }
        let mut grips = vec![grip];
        if self.marked.iter().any(same) {
            grips.extend(self.marked.iter().copied().filter(|hot| !same(hot)));
        }
        self.marked.clear();
        self.start_grip_edit(grips);
    }

    fn start_grip_edit(&mut self, grips: Vec<HotGrip>) {
        let Some(first) = grips.first().copied() else { return };
        self.hot_grip = Some(GripEdit { grips, base: first.point, mode: GripMode::Stretch, copy: false, copies: Vec::new(), ask: Ask::Mode });
    }

    /// Ends the grip edit (placed, eXit, Enter after copies); the selection stays.
    fn end_grip_edit(&mut self) {
        self.hot_grip = None;
        self.marked.clear();
        self.forget_aim();
    }

    /// The point the cursor pipeline measures from (polar tracking, Ortho) for the hot grip: the base
    /// point, the reference's first point, or none while the base point itself is asked.
    pub(super) fn grip_base(&self) -> Option<Option<Vec2>> {
        let edit = self.hot_grip.as_ref()?;
        Some(match edit.ask {
            Ask::Base | Ask::RefFirst => None,
            Ask::RefSecond(first) => Some(first),
            Ask::Mode | Ask::RefNew(_) | Ask::Action(_) => Some(edit.base),
        })
    }

    /// The hot grip's options (one grip in STRETCH: Lengthen, Radius, Add vertex, Remove vertex,
    /// Convert to Line).
    fn grip_actions(&self, edit: &GripEdit) -> Vec<GripAction> {
        match edit.grips.as_slice() {
            [grip] if edit.mode == GripMode::Stretch => {
                self.session.doc().ok().and_then(|doc| doc.entity(grip.handle)).map(|e| grip_actions(&e.kind, grip.index)).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// The centre of the arc whose grip is hot (Radius measures from it).
    fn arc_center(&self, edit: &GripEdit) -> Option<Vec2> {
        let grip = edit.grips.first()?;
        match &self.session.doc().ok()?.entity(grip.handle)?.kind {
            EntityKind::Arc(arc) => Some(arc.center.xy()),
            _ => None,
        }
    }

    /// The hot-grip prompt, as the engine's prompts are: its message, options and default.
    pub(super) fn grip_prompt(&self) -> Option<Prompt> {
        let edit = self.hot_grip.as_ref()?;
        let rotate = edit.mode == GripMode::Rotate;
        let base = edit.base;
        Some(match edit.ask {
            Ask::Base => Prompt::new("Specify base point", Accept::POINT),
            Ask::RefFirst if rotate => Prompt::new("Specify reference angle", Accept::POINT_OR_NUMBER).default("0"),
            Ask::RefFirst => Prompt::new("Specify reference length", Accept::POINT_OR_NUMBER).default("1"),
            Ask::RefSecond(first) => Prompt::new("Specify second point", Accept::POINT).base(first),
            Ask::RefNew(_) if rotate => Prompt::new("Specify new angle", Accept::POINT_OR_NUMBER).base(base),
            Ask::RefNew(_) => Prompt::new("Specify new length", Accept::POINT_OR_NUMBER).base(base),
            Ask::Action(GripAction::Lengthen) => {
                Prompt::new("Specify new end point or length change", Accept::POINT_OR_NUMBER).kw(&["eXit"]).base(base)
            }
            Ask::Action(GripAction::Radius) => {
                Prompt::new("Specify radius", Accept::POINT_OR_NUMBER).kw(&["eXit"]).base(self.arc_center(edit).unwrap_or(base))
            }
            Ask::Action(_) => Prompt::new("Specify new vertex point", Accept::POINT).kw(&["eXit"]).base(base),
            Ask::Mode => {
                let (message, accept) = match edit.mode {
                    GripMode::Stretch => ("Specify stretch point", Accept::POINT),
                    GripMode::Move => ("Specify move point", Accept::POINT),
                    GripMode::Rotate => ("Specify rotation angle", Accept::POINT_OR_NUMBER),
                    GripMode::Scale => ("Specify scale factor", Accept::POINT_OR_NUMBER),
                    GripMode::Mirror => ("Specify second point", Accept::POINT),
                };
                let mut keywords = vec!["Base point", "Copy", "Undo"];
                if edit.rotate_or_scale() {
                    keywords.push("Reference");
                }
                let actions = self.grip_actions(edit);
                keywords.extend(actions.iter().map(|action| action.keyword()));
                keywords.push("eXit");
                Prompt::new(message, accept).kw(&keywords).base(base)
            }
        })
    }

    /// The hot-grip prompt as one line: "STRETCH Specify stretch point or [Base point/Copy/Undo/eXit]:".
    pub(super) fn grip_prompt_text(&self) -> Option<String> {
        let edit = self.hot_grip.as_ref()?;
        Some(format!("{} {}", edit.title(), self.grip_prompt()?.display()))
    }

    /// The hot-grip prompt taken apart for the command line ([`CadDocument::command_line_state`]):
    /// the mode's title as the command ("** STRETCH **"), and `grip`, the mode and whether copies are
    /// placed.
    pub(super) fn grip_prompt_parts(&self) -> Option<Value> {
        let edit = self.hot_grip.as_ref()?;
        let prompt = self.grip_prompt()?;
        Some(json!({
            "command": format!("** {} **", edit.title()),
            "message": prompt.message,
            "keywords": prompt.keywords,
            "default": prompt.default,
            "kind": prompt.kind(),
            "base": prompt.base.filter(|base| base.is_finite()).map(|base| [base.x, base.y]),
            "grip": { "mode": edit.mode, "copy": edit.copy },
        }))
    }

    /// Enter (Space, a quick right-click, an empty command line) at the hot-grip prompt: the next
    /// mode, or the end of the edit after copies; back to the mode from an option's question.
    pub(super) fn grip_enter(&mut self) -> Result<(), String> {
        let Some(edit) = self.hot_grip.as_mut() else { return Ok(()) };
        if edit.ask != Ask::Mode {
            edit.ask = Ask::Mode;
            return Ok(());
        }
        if !edit.copies.is_empty() {
            self.end_grip_edit();
            return Ok(());
        }
        edit.mode = edit.mode.next();
        let title = format!("** {} **", edit.mode.name());
        self.write_history(&title);
        self.forget_aim();
        Ok(())
    }

    /// A command-line entry at the hot-grip prompt: Enter, an option, a mode's name (`rotate`), a
    /// point (`@45,12`, relative to the grip), a value (an angle, a scale factor, a length change),
    /// or a distance toward the cursor.
    pub(super) fn grip_text(&mut self, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return self.grip_enter();
        }
        let Some(prompt) = self.grip_prompt() else { return Ok(()) };
        if let Some(keyword) = prompt.match_keyword(text) {
            return self.grip_keyword(&keyword);
        }
        if let Some(mode) = GripMode::named(text)
            && let Some(edit) = self.hot_grip.as_mut()
        {
            edit.mode = mode;
            edit.ask = Ask::Mode;
            let title = format!("** {} **", mode.name());
            self.write_history(&title);
            self.forget_aim();
            return Ok(());
        }
        let angles = self.session.angle_settings();
        let last = prompt.base.or_else(|| self.hot_grip.as_ref().map(|edit| edit.base)).unwrap_or(Vec2::ZERO);
        if prompt.accept.point
            && let Some(point) = kition_cad_engine::prompt::parse_point_with(text, last, &angles).filter(|p| p.is_finite())
        {
            return self.grip_point(point, false);
        }
        if prompt.accept.number {
            return self.grip_value(text);
        }
        // Direct distance: a plain number goes that far from the grip toward the cursor (along
        // what holds it: Ortho, a tracking path, a locked box).
        if prompt.accept.point
            && let (Some(base), Some(distance)) = (prompt.base, kition_cad_engine::units::parse_distance(text).filter(|d| d.is_finite()))
        {
            let toward = self.aim.or(self.cursor).filter(|p| p.is_finite()).unwrap_or(base);
            let direction = (toward - base).normalized();
            let direction = if direction == Vec2::ZERO { Vec2::new(1.0, 0.0) } else { direction };
            return self.grip_point(base + direction * distance, false);
        }
        Err(other("Specify a point, or press Escape to cancel"))
    }

    /// An option chosen at the hot-grip prompt.
    fn grip_keyword(&mut self, keyword: &str) -> Result<(), String> {
        let Some(edit) = self.hot_grip.as_ref() else { return Ok(()) };
        let actions = self.grip_actions(edit);
        let rotate_or_scale = edit.rotate_or_scale();
        let ask = |doc: &mut Self, ask: Ask| {
            if let Some(edit) = doc.hot_grip.as_mut() {
                edit.ask = ask;
            }
        };
        match keyword {
            "eXit" => {
                self.end_grip_edit();
                return Ok(());
            }
            "Base point" => ask(self, Ask::Base),
            "Reference" if rotate_or_scale => ask(self, Ask::RefFirst),
            "Copy" => {
                if let Some(edit) = self.hot_grip.as_mut() {
                    edit.copy = true;
                }
            }
            "Undo" => {
                let steps = self.hot_grip.as_mut().and_then(|edit| edit.copies.pop()).ok_or_else(|| other("Nothing to undo."))?;
                let scope = self.session.undo_scope();
                for _ in 0..steps {
                    self.session.undo_in(scope).map_err(engine_error)?;
                }
            }
            word => {
                let Some(action) = actions.into_iter().find(|action| action.keyword() == word) else { return Ok(()) };
                if !action.needs_point() {
                    return self.grip_act(action, None, None);
                }
                ask(self, Ask::Action(action));
            }
        }
        self.forget_aim();
        Ok(())
    }

    /// A typed value: ROTATE's angle, SCALE's factor, a reference, or Lengthen's change.
    fn grip_value(&mut self, text: &str) -> Result<(), String> {
        let Some(edit) = self.hot_grip.as_ref() else { return Ok(()) };
        let angles = self.session.angle_settings();
        let number = || kition_cad_engine::units::parse_distance(text);
        let rotate = edit.mode == GripMode::Rotate;
        match (edit.ask, edit.mode) {
            (Ask::Mode, GripMode::Rotate) => {
                let angle = angles.rotation(text).filter(|a| a.is_finite()).ok_or_else(|| other("Requires an angle or point."))?;
                self.grip_apply(Edit::Angle(angle))
            }
            (Ask::Mode, GripMode::Scale) => self.grip_apply(Edit::Factor(positive(number())?)),
            (Ask::RefFirst, _) => {
                let reference = if rotate {
                    angles.direction(text).filter(|a| a.is_finite()).ok_or_else(|| other("Requires an angle or point."))?
                } else {
                    positive(number())?
                };
                if let Some(edit) = self.hot_grip.as_mut() {
                    edit.ask = Ask::RefNew(reference);
                }
                Ok(())
            }
            (Ask::RefNew(reference), _) if rotate => {
                let angle = angles.direction(text).filter(|a| a.is_finite()).ok_or_else(|| other("Requires an angle or point."))?;
                self.grip_apply(Edit::Angle(angle - reference))
            }
            (Ask::RefNew(reference), _) => self.grip_apply(Edit::Factor(positive(number().map(|v| v / reference))?)),
            (Ask::Action(action), _) => {
                let change = number().filter(|v| v.is_finite()).ok_or_else(|| other("Requires a number or point."))?;
                self.grip_act(action, None, Some(change))
            }
            _ => Err(other("Specify a point, or press Escape to cancel")),
        }
    }

    /// A point given at the hot-grip prompt (clicked, or typed): it places the grip in the mode, or
    /// answers the option asking. `copy_now` (Ctrl or Cmd held at the click) places a copy, and the
    /// edit goes on placing copies.
    pub(super) fn grip_point(&mut self, point: Vec2, copy_now: bool) -> Result<(), String> {
        let Some(edit) = self.hot_grip.as_mut() else { return Ok(()) };
        match edit.ask {
            Ask::Base => {
                edit.base = point;
                edit.ask = Ask::Mode;
                self.forget_aim();
                Ok(())
            }
            Ask::RefFirst => {
                edit.ask = Ask::RefSecond(point);
                self.forget_aim();
                Ok(())
            }
            Ask::RefSecond(first) => {
                if first.near(point, 1e-12) {
                    return Err(other("The two points must differ."));
                }
                edit.ask = Ask::RefNew(if edit.mode == GripMode::Rotate { first.angle_to(point) } else { first.dist(point) });
                self.forget_aim();
                Ok(())
            }
            Ask::Action(action) => self.grip_act(action, Some(point), None),
            Ask::Mode | Ask::RefNew(_) => {
                if copy_now {
                    edit.copy = true;
                }
                self.grip_apply(Edit::To(point))
            }
        }
    }

    /// The objects MOVE, ROTATE, SCALE and MIRROR transform: the hot grips' objects and the rest of
    /// the selection.
    fn grip_targets(&self, edit: &GripEdit) -> Vec<Handle> {
        let mut out: Vec<Handle> = Vec::new();
        for handle in edit.grips.iter().map(|grip| grip.handle).chain(self.session.selection()) {
            if !out.contains(&handle) {
                out.push(handle);
            }
        }
        out
    }

    /// The transform of the mode for `edit` (not STRETCH), as the commit makes it.
    fn grip_matrix(edit: &GripEdit, how: Edit) -> Option<kition_cad_geom::Mat3> {
        use kition_cad_geom::Mat3;
        let base = edit.base;
        match (edit.mode, how, edit.ask) {
            (GripMode::Rotate, Edit::Angle(angle), _) => Some(Mat3::rotate_about(base, angle)),
            (GripMode::Rotate, Edit::To(to), Ask::RefNew(reference)) => Some(Mat3::rotate_about(base, base.angle_to(to) - reference)),
            (GripMode::Scale, Edit::Factor(factor), _) => (factor.is_finite() && factor > 1e-12).then(|| Mat3::scale_about(base, factor)),
            (GripMode::Scale, Edit::To(to), Ask::RefNew(reference)) => {
                let factor = base.dist(to) / reference;
                (factor.is_finite() && factor > 1e-12).then(|| Mat3::scale_about(base, factor))
            }
            (mode, Edit::To(to), _) => mode_matrix(mode, base, to),
            _ => None,
        }
    }

    /// Places the grip edit: STRETCH moves the hot grips (and the selected objects' grips on the
    /// same points) by the base point's displacement; the other modes transform the objects. In Copy
    /// the objects stay and copies are placed, and the edit goes on; otherwise it ends.
    fn grip_apply(&mut self, how: Edit) -> Result<(), String> {
        if self.hot_grip.is_none() {
            return Ok(());
        }
        let mark = self.session.undo_mark();
        let steps_before = self.session.state().map(|state| state.undo.len()).unwrap_or(0);
        let done = self.grip_commit(how);
        // One placement is one undo step, however many commands it took (a copy and its
        // stretch, several hot grips), also when a later one of them was refused.
        self.session.fold_undo_since(mark);
        done?;
        let steps = self.session.state().map(|state| state.undo.len()).unwrap_or(0).saturating_sub(steps_before);
        match self.hot_grip.as_mut() {
            Some(edit) if edit.copy => {
                edit.copies.push(steps);
                edit.ask = Ask::Mode;
                self.forget_aim();
            }
            _ => self.end_grip_edit(),
        }
        Ok(())
    }

    /// The commands of one placement ([`CadDocument::grip_apply`]).
    fn grip_commit(&mut self, how: Edit) -> Result<(), String> {
        let Some(edit) = self.hot_grip.as_ref() else { return Ok(()) };
        let hex = |handles: &[Handle]| handles.iter().map(|handle| handle.hex()).collect::<Vec<_>>();
        let selection = self.session.selection();
        let copy = edit.copy;
        let base = [edit.base.x, edit.base.y];
        match (edit.mode, how) {
            (GripMode::Stretch, Edit::To(to)) => {
                let by = to - edit.base;
                let grips = edit.grips.clone();
                let originals = edit.base;
                // In Copy, the grips' objects are copied in place first and the copies stretched.
                let mut handles: Vec<Handle> = Vec::new();
                for grip in &grips {
                    if !handles.contains(&grip.handle) {
                        handles.push(grip.handle);
                    }
                }
                let copies: Vec<Handle> = if copy {
                    let made = self
                        .session
                        .execute(
                            "grip.move",
                            &json!({ "mode": "move", "handles": hex(&handles), "base": [0.0, 0.0], "to": [0.0, 0.0], "copy": true }),
                        )
                        .map_err(engine_error)?;
                    let made: Vec<Handle> = made["handles"]
                        .as_array()
                        .map(|list| list.iter().filter_map(|h| h.as_str().and_then(Handle::parse_hex)).collect())
                        .unwrap_or_default();
                    if made.len() != handles.len() {
                        return Err(other("The objects can't be copied (locked, or not in this space)."));
                    }
                    made
                } else {
                    handles.clone()
                };
                let followers = if copy { copies.clone() } else { selection.clone() };
                for grip in &grips {
                    let Some(at) = handles.iter().position(|handle| *handle == grip.handle) else { continue };
                    let Some(target) = copies.get(at).copied() else { continue };
                    // The base grip lands exactly on the point.
                    let destination = if grip.point == originals { to } else { grip.point + by };
                    let mut others = vec![target];
                    others.extend(followers.iter().copied().filter(|handle| *handle != target));
                    self.session
                        .execute(
                            "grip.move",
                            &json!({ "handle": target.hex(), "handles": hex(&others), "index": grip.index, "to": [destination.x, destination.y] }),
                        )
                        .map_err(engine_error)?;
                }
            }
            (GripMode::Stretch, _) => return Err(other("Specify a point, or press Escape to cancel")),
            (mode, how) => {
                let targets = hex(&self.grip_targets(edit));
                let params = match (mode, how) {
                    (GripMode::Move, Edit::To(to)) => json!({ "mode": "move", "handles": targets, "base": base, "to": [to.x, to.y], "copy": copy }),
                    (GripMode::Mirror, Edit::To(to)) => json!({ "handles": targets, "base": base, "to": [to.x, to.y], "copy": copy }),
                    (GripMode::Rotate, _) => {
                        let angle = match (how, edit.ask) {
                            (Edit::Angle(angle), _) => angle,
                            (Edit::To(to), Ask::RefNew(reference)) => edit.base.angle_to(to) - reference,
                            (Edit::To(to), _) => edit.base.angle_to(to),
                            (Edit::Factor(_), _) => return Err(other("Requires an angle or point.")),
                        };
                        json!({ "handles": targets, "base": base, "angle": angle.to_degrees(), "copy": copy })
                    }
                    (GripMode::Scale, _) => {
                        let factor = match (how, edit.ask) {
                            (Edit::Factor(factor), _) => factor,
                            (Edit::To(to), Ask::RefNew(reference)) => edit.base.dist(to) / reference,
                            (Edit::To(to), _) => edit.base.dist(to),
                            (Edit::Angle(_), _) => return Err(other("Requires a scale factor or point.")),
                        };
                        json!({ "handles": targets, "base": base, "factor": positive(Some(factor))?, "copy": copy })
                    }
                    _ => return Err(other("Specify a point, or press Escape to cancel")),
                };
                let id = match mode {
                    GripMode::Move => "grip.move",
                    GripMode::Rotate => "grip.rotate",
                    GripMode::Scale => "grip.scale",
                    _ => "grip.mirror",
                };
                self.session.execute(id, &params).map_err(engine_error)?;
            }
        }
        Ok(())
    }

    /// A grip option (Lengthen, Radius, Add vertex, Remove vertex, Convert to Line) on the base grip;
    /// the edit ends.
    fn grip_act(&mut self, action: GripAction, to: Option<Vec2>, value: Option<f64>) -> Result<(), String> {
        let Some(grip) = self.hot_grip.as_ref().and_then(|edit| edit.grips.first().copied()) else { return Ok(()) };
        self.session.grip_act(grip.handle, grip.index, action, to, value).map_err(engine_error)?;
        self.end_grip_edit();
        Ok(())
    }

    /// The objects as the hot-grip edit would leave them with the cursor at `to`.
    fn grip_preview(&self, edit: &GripEdit, to: Vec2) -> Vec<kition_cad_doc::Entity> {
        let Ok(doc) = self.session.doc() else { return Vec::new() };
        let edits: Vec<(Handle, EntityKind)> = match (edit.ask, edit.mode) {
            (Ask::Base | Ask::RefFirst | Ask::RefSecond(_), _) => Vec::new(),
            (Ask::Action(action), _) => edit
                .grips
                .first()
                .and_then(|grip| Some((grip.handle, grip_action_result(&doc.entity(grip.handle)?.kind, grip.index, action, Some(to), None)?)))
                .into_iter()
                .collect(),
            (_, GripMode::Stretch) => match edit.grips.as_slice() {
                [grip] if edit.base == grip.point => {
                    stretch_selection(doc, grip.handle, grip.index, to, self.session.selection()).unwrap_or_default()
                }
                grips => {
                    // Several hot grips (or another base point): each grip, and the selected objects'
                    // grips on the same point, moved by the base point's displacement.
                    let by = to - edit.base;
                    let editable = |handle: Handle| doc.entity(handle).filter(|e| !doc.layer(&e.common.layer).is_some_and(|layer| layer.locked));
                    let mut out: Vec<(Handle, EntityKind)> = Vec::new();
                    let selection = self.session.selection();
                    for grip in grips.iter().take(MAX_HOT) {
                        let destination = grip.point + by;
                        let mut moved = vec![(grip.handle, grip.index)];
                        for other in selection.iter().copied().filter(|handle| *handle != grip.handle) {
                            if let Some(entity) = editable(other)
                                && let Some(index) = entity.kind.grips().iter().position(|at| at.near(grip.point, 1e-9 * (1.0 + grip.point.len())))
                            {
                                moved.push((other, index));
                            }
                        }
                        for (handle, index) in moved {
                            let current = match out.iter().find(|(h, _)| *h == handle) {
                                Some((_, kind)) => kind.clone(),
                                None => match editable(handle) {
                                    Some(entity) => entity.kind.clone(),
                                    None => continue,
                                },
                            };
                            if let Some(kind) = stretch_grip(&current, index, destination) {
                                out.retain(|(h, _)| *h != handle);
                                out.push((handle, kind));
                            }
                        }
                    }
                    out
                }
            },
            (_, _) => match Self::grip_matrix(edit, Edit::To(to)) {
                Some(matrix) => self
                    .grip_targets(edit)
                    .into_iter()
                    .take(MAX_HOT)
                    .filter_map(|handle| doc.entity(handle))
                    .filter(|e| !doc.layer(&e.common.layer).is_some_and(|layer| layer.locked))
                    .map(|e| {
                        let mut kind = e.kind.clone();
                        kind.transform(&matrix);
                        (e.handle, kind)
                    })
                    .collect(),
                None => Vec::new(),
            },
        };
        edits
            .into_iter()
            .filter_map(|(handle, kind)| {
                let mut entity = (**doc.entity(handle)?).clone();
                entity.kind = kind;
                Some(entity)
            })
            .collect()
    }

    /// The hot-grip edit at the cursor: the objects as it would leave them, the hot grips, a guide
    /// from the base point to the cursor (not in STRETCH, where the grip itself is the guide), and
    /// what holds the point when tracking does ("Polar"); between edits, the grip under the pointer
    /// and its menu for the editor ([`GRIP_INFO`]).
    pub(super) fn grip_overlays(&self) -> Vec<ToolOverlay> {
        let place = self.placement();
        let Some(edit) = self.hot_grip.as_ref() else { return self.grip_info().into_iter().collect() };
        let (Some(destination), Ok(doc)) = (self.aim, self.session.doc()) else { return Vec::new() };
        let previews = self.grip_preview(edit, destination);
        // No edit there (a degenerate result, a locked object): nothing previews, not even part.
        if previews.is_empty() && matches!(edit.ask, Ask::Mode | Ask::RefNew(_) | Ask::Action(_)) {
            return Vec::new();
        }
        let list =
            kition_cad_render::build_entities(doc, &previews, &kition_cad_render::Options { tolerance: self.tolerance(), ..Default::default() });
        let mut overlays = self.paths(&list, &place, ACCENT, 1.5, false);
        let from = match edit.ask {
            Ask::RefSecond(first) => Some(first),
            Ask::Mode | Ask::RefNew(_) if edit.mode != GripMode::Stretch => Some(edit.base),
            Ask::Action(GripAction::Radius) => Some(self.arc_center(edit).unwrap_or(edit.base)),
            Ask::Action(_) => Some(edit.base),
            _ => None,
        };
        if let Some(from) = from {
            let (ax, ay) = place.to_doc(from);
            let (bx, by) = place.to_doc(destination);
            overlays.push(ToolOverlay::Line { ax, ay, bx, by, color: GUIDE, dashed: false });
        }
        for grip in &edit.grips {
            let (x, y) = place.to_doc(grip.point);
            overlays.push(ToolOverlay::Anchor { x, y, color: GRIP_HOT, filled: true, size: 9.0 });
        }
        // The tracking path itself is drawn with the command's ([`CadDocument::tracking_overlays`]);
        // the grip names what holds its point.
        if let Some(track) = &self.track {
            let (tx, ty) = place.to_doc(track.point);
            let name = track.object.or_else(|| track.paths.first().map(|path| path.name)).unwrap_or("Polar");
            overlays.push(ToolOverlay::Label { x: tx + 8.0, y: ty - 8.0, text: name.to_owned(), color: SNAP });
        }
        overlays
    }

    /// The grip under the pointer between edits, for the editor's grip menu: a transparent label at
    /// the grip with its object, index and options (`{"handle","index","actions":[{"label","keyword",
    /// "immediate"}]}`; an immediate option, Remove vertex or Convert to Line, changes the drawing as soon as it is
    /// answered, so the menu answers it at the prompt rather than through [`EDIT`]).
    fn grip_info(&self) -> Option<ToolOverlay> {
        let grip = self.hover_grip?;
        let doc = self.session.doc().ok()?;
        let actions: Vec<Value> = grip_actions(&doc.entity(grip.handle)?.kind, grip.index)
            .into_iter()
            .map(|action| json!({ "label": action.label(), "keyword": action.keyword(), "immediate": !action.needs_point() }))
            .collect();
        let (x, y) = self.placement().to_doc(grip.point);
        let text = json!({ "handle": grip.handle.hex(), "index": grip.index, "actions": actions }).to_string();
        Some(ToolOverlay::Label { x, y, text, color: GRIP_INFO })
    }

    /// [`EDIT`]: starts editing grip `index` of the selected object `handle`, as a click on it does,
    /// in `mode`, and answers `option` (one of the prompt's options that changes nothing yet: Base
    /// point, Copy, Reference, Lengthen, Radius, Add vertex; not Remove vertex or Convert to Line,
    /// which are answered at the prompt). It changes no drawing, only what the editor holds; it answers the prompt the edit
    /// asks now (`{}` when the option ended it, eXit).
    pub(super) fn grip_edit_command(&mut self, params: &Value) -> Result<Value, String> {
        let handle = params.get("handle").and_then(Value::as_str).and_then(Handle::parse_hex).ok_or("grip.edit needs `handle`, a selected object")?;
        let index =
            params.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok()).ok_or("grip.edit needs `index`, the grip's number")?;
        if !self.session.selection().contains(&handle) {
            return Err("grip.edit edits a grip of a selected object".into());
        }
        let mode = match params.get("mode").and_then(Value::as_str) {
            Some(name) => Some(GripMode::named(name).ok_or_else(|| format!("{name} is not a grip mode: stretch, move, rotate, scale or mirror"))?),
            None => None,
        };
        let doc = self.session.doc().map_err(engine_error)?;
        let entity = doc.entity(handle).ok_or("no such object")?;
        if doc.layer(&entity.common.layer).is_some_and(|layer| layer.locked || layer.frozen || !layer.on) {
            return Err("The object is on a locked layer.".into());
        }
        let point = entity.kind.grips().get(index).copied().ok_or("no such grip")?;
        if self.session.running.is_some() {
            self.session.cancel();
        }
        self.forget_dynamic_entry();
        self.marked.clear();
        self.start_grip_edit(vec![HotGrip { handle, index, point }]);
        if let (Some(mode), Some(edit)) = (mode, self.hot_grip.as_mut()) {
            edit.mode = mode;
        }
        if let Some(option) = params.get("option").and_then(Value::as_str) {
            let keyword = self.grip_prompt().and_then(|prompt| prompt.match_keyword(option));
            let immediate = self.hot_grip.as_ref().is_some_and(|edit| {
                self.grip_actions(edit).iter().any(|action| Some(action.keyword()) == keyword.as_deref() && !action.needs_point())
            });
            match keyword {
                Some(keyword) if immediate => {
                    self.end_grip_edit();
                    return Err(format!("{keyword} changes the drawing: start the grip edit, then answer it at the prompt"));
                }
                Some(keyword) => self.grip_keyword(&keyword)?,
                None => {
                    self.end_grip_edit();
                    return Err(format!("{option} is not an option of that grip"));
                }
            }
        }
        if self.cursor.is_some() {
            self.resolve_cursor();
        }
        Ok(match self.grip_prompt_text() {
            Some(prompt) => json!({ "prompt": prompt }),
            None => json!({}),
        })
    }
}

/// The catalogue entry of [`EDIT`].
pub(super) fn command() -> KitCommand {
    KitCommand {
        id: EDIT.into(),
        title: "Edit Grip".into(),
        params: Some(r#"{handle: string, index: number, mode?: "stretch"|"move"|"rotate"|"scale"|"mirror", option?: string}"#.into()),
        mutates: false,
        enabled: true,
        ..Default::default()
    }
    .with_kit_schema()
}
