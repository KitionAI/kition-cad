//! Explicit call context (kition-sdk `KitDocument::editor_state`, `set_context`; Kit Process
//! Protocol section 7a; task card C4/C5/C6-cad): the editor state a command may read besides its
//! parameters, taken and put back without an edit, set for one call from what the call says it
//! acts on, and kept out of an agent's commands.
//!
//! A call's **part** is a space: `{"layout": "<name>"}`, a layout by its name (its object-model id
//! is `layout:<name>`), or `{"layout": "Model"}` for model space. It becomes the tab shown, in
//! paper space (an MSPACE viewport ends).
//!
//! A call's **`on`** is entities by their handles (hex strings, `"1A2"`), as inspect, `object.query`
//! and receipts list them. They become the selection, which lives in one space: the call's part
//! when it has one, else the space the entities are in, which their handles name (no part is
//! needed). Entities of two spaces in one step are refused, as is an entity inside a block
//! definition (it is in no space). A command that takes `handles` acts in every space, and `on`
//! fills that parameter instead (`acts::targets`).
//!
//! Everything is checked before anything changes, and nothing of it is an edit: the revision, the
//! dirty state and the undo history stay as they are. A drawing has no timeline: `time` is not
//! available.
//!
//! The current layer (`CLAYER`) and the other current properties new objects take are the
//! drawing's header variables, which the file saves; no call context sets them. A drawing command
//! takes an explicit `layer` (and `layout`) instead ([`CadDocument::explicit`]).

use std::collections::VecDeque;

use kition_cad_doc::{Drawing, Handle, Space};
use kition_cad_engine::context::EditorState;
use kition_kit::CallContext;
use serde_json::Value;

use crate::CadDocument;

/// How many snapshots [`CadDocument::editor_state`](kition_kit::KitDocument::editor_state) keeps
/// for [`CadDocument::restore_editor_state`](kition_kit::KitDocument::restore_editor_state): a host
/// restores the one it took after a call, and an editor running an agent's steps holds two or
/// three at a time.
const KEPT: usize = 64;

/// The snapshots answered so far, by their fingerprint (the newest last).
#[derive(Default)]
pub(crate) struct Kept {
    states: VecDeque<(String, EditorState)>,
}

/// A fingerprint of `state`: the same for the same state, so taking the editor state twice
/// answers the same value (`testing::check_context` compares them).
fn fingerprint(state: &EditorState) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{state:?}").hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Commands whose work is moving the editor (the view, the tab shown, MSPACE, the selection):
/// they keep their effect for an agent too, for the rest of its call.
pub(crate) fn moves_the_editor(command: &str) -> bool {
    matches!(
        command,
        "view.zoomRealtime"
            | "view.pan"
            | "view.set"
            | "layout.set"
            | "viewport.modelSpace"
            | "viewport.paperSpace"
            | "select.set"
            | "select.quick"
            | "select.similar"
            | "edit.selectAll"
            | "edit.find"
            | "edit.deselectAll"
    ) || crate::acts::zooms_or_pans(command)
}

/// Navigation that changes a view: inside an MSPACE viewport it writes the viewport's view, which
/// the drawing saves.
fn changes_a_view(command: &str) -> bool {
    matches!(command, "view.zoomRealtime" | "view.pan" | "view.set") || crate::acts::zooms_or_pans(command)
}

/// Where the inspect tool lists what a call may name.
const LISTED: &str =
    "spaces are cad_inspect's `layouts` (`{\"layout\": \"Model\"}` or a layout's name), entities' handles are listed by the `object.query` command";

/// The space named `name`: `Model` (any case) or a layout's name.
pub(crate) fn space_named(doc: &Drawing, name: &str) -> Result<Space, String> {
    if name.eq_ignore_ascii_case("model") {
        return Ok(Space::Model);
    }
    if let Some(layout) = doc.layout(name) {
        return Ok(Space::Paper(layout.name.clone()));
    }
    let hint = name
        .strip_prefix("layout:")
        .filter(|bare| doc.layout(bare).is_some())
        .map_or_else(String::new, |bare| format!(" (`{name}` is the layout's object-model id: name the layout by its name, `{bare}`)"));
    let names: Vec<String> = std::iter::once("Model".to_owned()).chain(doc.layouts.iter().map(|l| l.name.clone())).collect();
    Err(format!("the drawing has no layout `{name}`{hint}: its spaces are {}", names.join(", ")))
}

/// The space a call's part names: `{"layout": "<name>"}`.
pub(crate) fn part_space(doc: &Drawing, part: &Value) -> Result<Space, String> {
    let Some(fields) = part.as_object() else { return Err(format!("the part {part} is not `{{\"layout\": \"<name>\"}}`: {LISTED}")) };
    if fields.len() != 1 {
        return Err(format!("the part {part} names {} members: a part is one space, `{{\"layout\": \"<name>\"}}`", fields.len()));
    }
    let (key, value) = fields.iter().next().ok_or_else(|| format!("the part {part} names no space"))?;
    if key != "layout" {
        return Err(format!(
            "the part has `{key}`, which names no part of a drawing: a part is a space, `{{\"layout\": \"Model\"}}` or `{{\"layout\": \"<layout name>\"}}`"
        ));
    }
    let name = value.as_str().ok_or_else(|| format!("`layout` is {value}: it is a layout's name, or \"Model\" for model space; {LISTED}"))?;
    space_named(doc, name)
}

/// The space name for messages.
pub(crate) fn space_name(space: &Space) -> &str {
    match space {
        Space::Model => "Model",
        Space::Paper(name) => name,
    }
}

/// The entity an `on` item names, and its space.
fn on_item(doc: &Drawing, item: &Value) -> Result<(Handle, Space), String> {
    let handle = item.as_str().and_then(Handle::parse_hex).ok_or_else(|| {
        format!("`on` holds {item}, which is not an entity handle: a handle is a hex string (\"1A2\"), as `object.query` lists it; {LISTED}")
    })?;
    if let Some(space) = doc.space_of(handle) {
        return Ok((handle, space));
    }
    if let Some((name, _)) = doc.blocks.iter().find(|(_, block)| block.entities.get(handle).is_some()) {
        return Err(format!(
            "entity {} is inside block `{name}`'s definition, in no space: `on` names entities in model space or a layout",
            handle.hex()
        ));
    }
    Err(format!("the drawing has no entity {}: {LISTED}", handle.hex()))
}

impl CadDocument {
    /// Whether the command running now runs for an agent: the host's dispatch says so (headless),
    /// or the document runs it as an agent's (`execute_as`).
    pub(crate) fn is_agent(&self) -> bool {
        kition_kit::dispatch::role() == Some(kition_kit::dispatch::Role::Agent) || self.session.origin().kind == kition_kit::kdm::OriginKind::Agent
    }

    /// The editor's state, kept for [`CadDocument::put_back`], as `{snapshot, layout, selection}`.
    pub(crate) fn take_editor_state(&self) -> Value {
        let Some(state) = self.session.editor_state() else { return Value::Null };
        let number = fingerprint(&state);
        let answer = serde_json::json!({
            "snapshot": number,
            "layout": space_name(&state.space),
            "selection": state.selection.iter().map(|h| h.hex()).collect::<Vec<_>>(),
            "part": {"layout": space_name(&state.space), "viewport": state.mspace},
            "active": {"settings": self.session.settings, "lastDimension": self.session.last_dim},
            "editor": state,
            "layoutIdentity": match &state.space {
                Space::Model => None,
                Space::Paper(name) => self.session.doc().ok().and_then(|d| d.record_id("layout", name, 0)),
            },
            "settings": self.session.settings,
            "lastDimension": self.session.last_dim,
        });
        let mut kept = self.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.states.retain(|(n, _)| *n != number);
        kept.states.push_back((number, state));
        while kept.states.len() > KEPT {
            kept.states.pop_front();
        }
        answer
    }

    /// Puts back what [`CadDocument::take_editor_state`] answered; no edit.
    pub(crate) fn put_back(&mut self, saved: &Value) -> Result<(), String> {
        if saved.is_null() {
            return Ok(());
        }
        let number = saved.get("snapshot").and_then(Value::as_str).ok_or_else(|| format!("{saved} is not an editor state this kit answered"))?;
        let state = {
            let kept = self.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            kept.states.iter().find(|(n, _)| *n == number).map(|(_, state)| state.clone())
        };
        let state = match state {
            Some(state) => state,
            None => {
                // A reread or a premise fork has another local snapshot cache. The wire
                // snapshot is complete, but every active identity must still resolve.
                let mut state: EditorState = serde_json::from_value(saved["editor"].clone()).map_err(|e| format!("invalid editor state: {e}"))?;
                let doc = self.session.doc().map_err(crate::engine_error)?;
                if let Space::Paper(name) = &state.space {
                    let layout = if let Some(id) = saved["layoutIdentity"].as_str() {
                        doc.layouts.iter().find(|l| doc.record_id("layout", &l.name, 0) == Some(id))
                    } else {
                        doc.layout(name)
                    }
                    .ok_or_else(|| format!("deleted active layout {name}"))?;
                    let previous = state.space.clone();
                    state.space = Space::Paper(layout.name.clone());
                    for (space, _) in &mut state.views {
                        if *space == previous {
                            *space = state.space.clone();
                        }
                    }
                }
                if let Some(h) = state.mspace
                    && !doc.space(&state.space).and_then(|s| s.get(h)).is_some_and(|e| matches!(e.kind, kition_cad_doc::EntityKind::Viewport(_)))
                {
                    return Err(format!("deleted active viewport {}", h.hex()));
                }
                let space = if state.mspace.is_some() { Space::Model } else { state.space.clone() };
                for h in &state.selection {
                    if doc.space_of(*h).as_ref() != Some(&space) {
                        return Err(format!("deleted or relocated active selection {}", h.hex()));
                    }
                }
                state
            }
        };
        let settings = serde_json::from_value(saved["settings"].clone()).map_err(|e| format!("invalid editor settings: {e}"))?;
        let last_dim = serde_json::from_value(saved["lastDimension"].clone()).map_err(|e| format!("invalid last dimension: {e}"))?;
        self.session.restore_editor_state(&state).map_err(crate::engine_error)?;
        self.session.settings = settings;
        self.session.last_dim = last_dim;
        Ok(())
    }

    /// Sets the space a call acts in (`part`) and the entities it acts on there (`on`), as the
    /// module docs say; refuses what the drawing lacks and `time`. No edit.
    pub(crate) fn set_call_context(&mut self, context: &CallContext) -> Result<(), String> {
        if context.time.is_some() {
            return Err("a drawing has no timeline: setting the current time (`at.time`) is not available in this kit".into());
        }
        let doc = self.session.doc().map_err(crate::engine_error)?;
        let part = context.part.as_ref().map(|part| part_space(doc, part)).transpose()?;
        let items = context.on.iter().map(|item| on_item(doc, item)).collect::<Result<Vec<_>, _>>()?;
        let mut named: Option<(Handle, Space)> = None;
        for (handle, space) in &items {
            if let Some(part) = &part
                && part != space
            {
                return Err(format!(
                    "entity {} is in {}, and the call's part is {}: give it with that space, or leave the part out (a handle names its space)",
                    handle.hex(),
                    space_name(space),
                    space_name(part)
                ));
            }
            match &named {
                Some((first, other)) if other != space => {
                    return Err(format!(
                        "`on` names entities of two spaces ({} in {}, {} in {}): the selection is in one space, so give them in one step per space, or pass them as the command's `handles` where it takes them",
                        first.hex(),
                        space_name(other),
                        handle.hex(),
                        space_name(space)
                    ));
                }
                Some(_) => {}
                None => named = Some((*handle, space.clone())),
            }
        }
        let space = part.or(named.map(|(_, space)| space));
        let mut selection: Vec<Handle> = Vec::new();
        for (handle, _) in items {
            if !selection.contains(&handle) {
                selection.push(handle);
            }
        }
        let selection = (!context.on.is_empty()).then_some(selection);
        self.session.set_call_context(space, selection).map_err(crate::engine_error)
    }

    /// Refuses an agent's navigation that would change the drawing: inside an MSPACE viewport,
    /// zooming and panning write the viewport's view, which the file saves (a locked viewport moves
    /// the sheet instead, which writes nothing).
    pub(crate) fn refuse_saved_navigation(&self, command: &str) -> Result<(), String> {
        if !changes_a_view(command) {
            return Ok(());
        }
        let viewport = self.session.state().ok().and_then(|st| st.zoom_viewport().and(st.mspace));
        match viewport {
            Some(handle) => Err(format!(
                "`{command}` inside the layout viewport {} (MSPACE) would change the viewport's view, which the drawing saves: run `viewport.paperSpace` first to navigate the sheet, set the viewport's view with `viewport.set`, or look at any area with `file.render` and a `window`",
                handle.hex()
            )),
            None => Ok(()),
        }
    }

    /// Runs `run` with the explicit `layer` and `layout` a drawing command was given
    /// ([`crate::acts::takes_place`]): new objects go on that layer and into that space, and the
    /// drawing's current layer and the tab shown stay as they were, also when it fails. `params`
    /// is the command's parameters without those two.
    pub(crate) fn explicit(
        &mut self,
        layer: Option<&Value>,
        layout: Option<&Value>,
        run: impl FnOnce(&mut Self) -> Result<Value, String>,
    ) -> Result<Value, String> {
        let doc = self.session.doc().map_err(crate::engine_error)?;
        let layer = match layer.filter(|value| !value.is_null()) {
            None => None,
            Some(value) => {
                let name = value.as_str().ok_or_else(|| format!("`layer` is {value}: it is a layer's name (a string)"))?;
                let found = doc.layer(name).ok_or_else(|| {
                    let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).take(20).collect();
                    format!("the drawing has no layer `{name}` (its layers: {}): make it with `layer.add` earlier in the run", names.join(", "))
                })?;
                Some(found.name.clone())
            }
        };
        let space = match layout.filter(|value| !value.is_null()) {
            None => None,
            Some(value) => {
                let name = value.as_str().ok_or_else(|| format!("`layout` is {value}: it is a layout's name, or \"Model\" for model space"))?;
                Some(space_named(doc, name)?)
            }
        };
        let kept = space.as_ref().and_then(|_| self.session.editor_state());
        if let Some(space) = space {
            self.session.set_call_context(Some(space), None).map_err(crate::engine_error)?;
        }
        let previous = self.session.layer_override.clone();
        if layer.is_some() {
            self.session.layer_override = layer;
        }
        let result = run(self);
        self.session.layer_override = previous;
        if let Some(kept) = kept {
            // Fails only when the command closed the drawing, which leaves nothing to put back.
            let _ = self.session.restore_editor_state(&kept);
        }
        result
    }

    /// Runs an agent's command (`run`) and puts the tab shown, MSPACE, the views, the selection and
    /// a pending window back afterwards, also when it fails; picks by a point use the agent's pick
    /// tolerance ([`CadDocument::agent_tolerance`]), which no zoom changes.
    pub(crate) fn bracketed(&mut self, run: impl FnOnce(&mut Self) -> Result<Value, String>) -> Result<Value, String> {
        let kept = self.session.editor_state();
        let previous = self.session.pick_tolerance.replace(self.agent_tolerance());
        let result = run(self);
        self.session.pick_tolerance = previous;
        if result.as_ref().is_ok_and(|value| value.get("closed") == Some(&Value::Bool(true))) {
            return result;
        }
        if let Some(kept) = kept {
            let _ = self.session.restore_editor_state(&kept);
        }
        result
    }

    /// The pick tolerance of an agent's call, in drawing units: 0.5% of the larger side of the
    /// extents of the space being edited (1 unit in an empty space), so a point picks the same
    /// object whatever the view's zoom. `select {at, aperture}` sets another.
    pub(crate) fn agent_tolerance(&self) -> f64 {
        let Ok(state) = self.session.state() else { return 1.0 };
        let extents = state.doc.extents(&state.edit_space());
        if extents.is_empty() {
            return 1.0;
        }
        let side = extents.width().max(extents.height());
        if side.is_finite() && side > 0.0 { side * AGENT_TOLERANCE } else { 1.0 }
    }
}

/// [`CadDocument::agent_tolerance`] as a fraction of the larger side of the space's extents.
pub const AGENT_TOLERANCE: f64 = 0.005;
