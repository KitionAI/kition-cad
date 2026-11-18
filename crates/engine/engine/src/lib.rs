//! The Kition CAD engine.
//!
//! Every user-visible action is a command with a stable id (AutoCAD's command names in lower
//! case: `line`, `circle`, `zoom`, `layer`…, plus dotted ids for UI operations). Commands run two
//! ways: programmatically with JSON parameters ([`Session::execute`], no dialogs, used by the
//! control channel, MCP, CLI and scripts) and interactively through prompts ([`Session::start`],
//! [`Session::input`], [`Session::cmdline`]) exactly like typing at AutoCAD's command line.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod assoc;
pub mod audit;
pub mod cmd;
pub mod context;
mod finite;
pub mod grips;
mod guard;
pub mod kdm_bridge;
mod locks;
pub mod pointmod;
pub mod prompt;
pub mod sample;
pub mod select;
pub mod snap;
pub mod spatial;
pub mod sysvars;
pub mod units;

use std::sync::Arc;

use kition_cad_doc::{Common, Drawing, Entity, EntityKind, Handle, Space};
use kition_cad_geom::{Bounds2, Vec2};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
pub use kdm_model::kition_kit::kdm::{Origin, UndoScope, Version};
pub use kdm_model::kition_kit::session::Damage;
pub use kition_cad_doc as doc;
pub use kition_cad_geom as geom;
pub use kition_cad_render as render;
pub use prompt::{Accept, Input, Interactive, Prompt, Step};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no active document")]
    NoDocument,
    #[error("{0}")]
    Other(String),
    /// A command panicked; the guard kept the drawing as it was (a bug: please report it).
    #[error("internal error in `{0}` (the drawing was kept as it was): {1}")]
    Internal(String, String),
}

impl From<kition_cad_doc::DocError> for EngineError {
    fn from(e: kition_cad_doc::DocError) -> Self {
        EngineError::Other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// A 2D view of a space: world centre and visible height in drawing units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub center: Vec2,
    pub height: f64,
}

impl Default for View {
    fn default() -> Self {
        View { center: Vec2::new(6.0, 4.5), height: 10.0 }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub label: String,
    pub doc: Arc<Drawing>,
    pub selection: Vec<Handle>,
    /// How many of the drawing's KDM replica undo steps this step took ([`kdm_bridge`]): 0 for a
    /// step that changed nothing the replica keeps, or that was taken before a reset.
    pub steps: u32,
    /// Whose history the step is in (KitModel M12a): the person's, or the agents' working for
    /// them, as the origin of the command that made it.
    pub scope: UndoScope,
}

/// One open drawing.
#[derive(Debug)]
pub struct DocState {
    pub doc: Arc<Drawing>,
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    pub selection: Vec<Handle>,
    pub previous_selection: Vec<Handle>,
    pub path: Option<String>,
    pub title: String,
    pub saved: Arc<Drawing>,
    pub revision: u64,
    pub space: Space,
    /// MSPACE: the layout viewport whose model space is being edited (None = paper space).
    pub mspace: Option<Handle>,
    pub views: Vec<(Space, View)>,
    pub view_history: Vec<View>,
    pub uid: u64,
    /// How many steps [`DocState::push_undo`] ever pushed (only grows): the KDM bridge finds the
    /// steps a command pushed by it, since at the limit a push leaves the stack's length as it was.
    pub pushed: u64,
    /// The drawing's KDM replica and what it last saw ([`kdm_bridge`]).
    pub(crate) link: kdm_bridge::Link,
}

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl DocState {
    pub fn new(doc: Drawing, title: &str, path: Option<String>) -> Self {
        let doc = Arc::new(doc);
        DocState {
            link: kdm_bridge::Link::new(&doc),
            pushed: 0,
            saved: doc.clone(),
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            selection: Vec::new(),
            previous_selection: Vec::new(),
            path,
            title: title.into(),
            revision: 1,
            space: Space::Model,
            mspace: None,
            views: Vec::new(),
            view_history: Vec::new(),
            uid: NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
    pub fn is_dirty(&self) -> bool {
        !Arc::ptr_eq(&self.doc, &self.saved)
    }
    /// Pushes a new undo step (clearing its own history's redo steps, never the other scope's;
    /// dropping the oldest past 2000).
    pub fn push_undo(&mut self, snapshot: Snapshot) {
        let scope = snapshot.scope;
        self.undo.push(snapshot);
        self.redo.retain(|u| u.scope != scope);
        self.pushed = self.pushed.saturating_add(1);
        let excess = self.undo.len().saturating_sub(2000);
        self.undo.drain(..excess);
    }
    /// [`Session::undo_in`] for a step under the other scope's later ones (index `i` of the undo,
    /// or with `redo` the redo, stack): the replica undoes (or redoes) its transactions and the
    /// drawing becomes the replica's view, keeping the later steps. A step the replica does not
    /// hold can't be taken back without the later steps, so there is nothing to do (`None`).
    fn restore_under(&mut self, redo: bool, i: usize, scope: UndoScope) -> Option<String> {
        let steps = (if redo { &self.redo } else { &self.undo }).get(i)?.steps;
        let before = self.doc.clone();
        if !self.replay_kdm(steps, !redo, scope) {
            return None;
        }
        let u = if redo { self.redo.remove(i) } else { self.undo.remove(i) };
        let label = u.label.clone();
        let cur = Snapshot { label: u.label, doc: before.clone(), selection: self.selection.clone(), steps: u.steps, scope };
        if redo {
            self.undo.push(cur)
        } else {
            self.redo.push(cur)
        }
        self.trim_kdm();
        self.follow_layout(&before);
        self.selection.retain(|h| self.doc.entity(*h).is_some());
        self.revision += 1;
        Some(label)
    }
    /// The undo stack's indexes of the steps pushed since [`DocState::pushed`] was `mark` that are
    /// still on it (the newest ones).
    pub(crate) fn pushed_since(&self, mark: u64) -> std::ops::Range<usize> {
        let new = usize::try_from(self.pushed.saturating_sub(mark)).unwrap_or(usize::MAX);
        let len = self.undo.len();
        len.saturating_sub(new)..len
    }
    /// After undo/redo replaced the drawing (`before` is the one shown until then): if the current
    /// layout no longer exists, follow it to its other name when it was renamed (same tab order,
    /// a name `before` did not have), else go to the Model tab.
    fn follow_layout(&mut self, before: &Drawing) {
        let Space::Paper(name) = &self.space else { return };
        if self.doc.layouts.iter().any(|l| l.name == *name) {
            return;
        }
        let renamed = before
            .layouts
            .iter()
            .find(|l| l.name == *name)
            .and_then(|old| self.doc.layouts.iter().find(|l| l.tab_order == old.tab_order && !before.layouts.iter().any(|b| b.name == l.name)));
        match renamed.map(|l| Space::Paper(l.name.clone())) {
            Some(to) => {
                let from = self.space.clone();
                for (sp, _) in self.views.iter_mut() {
                    if *sp == from {
                        *sp = to.clone();
                    }
                }
                self.space = to;
            }
            None => {
                self.space = Space::Model;
                self.mspace = None;
            }
        }
    }
    /// The space edits, picks and snaps act on: model space inside an active viewport.
    pub fn edit_space(&self) -> Space {
        if self.active_viewport().is_some() { Space::Model } else { self.space.clone() }
    }
    /// The active MSPACE viewport, if it still exists in the current layout.
    pub fn active_viewport(&self) -> Option<(Handle, kition_cad_doc::Viewport)> {
        let h = self.mspace?;
        let e = self.doc.space(&self.space)?.get(h)?;
        match &e.kind {
            kition_cad_doc::EntityKind::Viewport(v) if v.height > 1e-12 && v.view_height > 1e-12 => Some((h, v.clone())),
            _ => None,
        }
    }
    pub fn paper_view(&self) -> View {
        self.views.iter().find(|(s, _)| *s == self.space).map(|(_, v)| *v).unwrap_or_default()
    }
    /// The screen view. Inside an MSPACE viewport this is the model-space view that maps the
    /// screen consistently with the paper view, so picks, snaps and previews work in model units.
    pub fn view(&self) -> View {
        let pv = self.paper_view();
        match self.active_viewport() {
            Some((_, vp)) => {
                let k = vp.view_height / vp.height;
                View { center: vp.view_center + (pv.center - vp.center.xy()) * k, height: pv.height * k }
            }
            None => pv,
        }
    }
    /// Map a requested screen view to the paper view, or (in MSPACE) to the viewport's model view.
    fn store_view(&mut self, v: View) {
        if let Some((h, vp)) = self.active_viewport() {
            let pv = self.paper_view();
            let k = v.height / pv.height.max(1e-12);
            if vp.locked {
                // A locked viewport keeps its scale: pan/zoom the sheet instead.
                let k0 = vp.view_height / vp.height;
                let pc = vp.center.xy() + (v.center - vp.view_center) / k0;
                self.put_paper_view(View { center: pc, height: v.height / k0 });
            } else {
                let center = v.center - (pv.center - vp.center.xy()) * k;
                let height = vp.height * k;
                let doc = Arc::make_mut(&mut self.doc);
                if let Some(store) = doc.space_mut(&self.space.clone()) {
                    store.modify(h, |e| {
                        if let kition_cad_doc::EntityKind::Viewport(x) = &mut e.kind {
                            x.view_center = center;
                            x.view_height = height;
                        }
                    });
                }
            }
            return;
        }
        self.put_paper_view(v);
    }
    fn put_paper_view(&mut self, v: View) {
        match self.views.iter_mut().find(|(s, _)| *s == self.space) {
            Some((_, slot)) => *slot = v,
            None => self.views.push((self.space.clone(), v)),
        }
    }
    pub fn set_view(&mut self, v: View) {
        if !(v.center.is_finite() && v.height.is_finite() && v.height > 1e-12) {
            return;
        }
        let old = self.view();
        if old != v {
            self.view_history.push(old);
            if self.view_history.len() > 50 {
                self.view_history.remove(0);
            }
        }
        self.store_view(v);
    }
    /// The active MSPACE viewport when ZOOM works on its own view, as in AutoCAD: extents and
    /// windows fit the viewport, and zooms keep its centre. A locked viewport keeps its scale, so
    /// zooming it moves the sheet instead (see `store_view`).
    pub fn zoom_viewport(&self) -> Option<kition_cad_doc::Viewport> {
        self.active_viewport().map(|(_, v)| v).filter(|v| !v.locked && v.width > 1e-12)
    }
    /// The view ZOOM starts from: the model view of the active unlocked viewport, else [`Self::view`].
    pub fn zoom_frame(&self) -> View {
        match self.zoom_viewport() {
            Some(v) => View { center: v.view_center, height: v.view_height },
            None => self.view(),
        }
    }
    /// Width / height of the area ZOOM fits into: the active unlocked viewport, else `screen`.
    pub fn zoom_aspect(&self, screen: f64) -> f64 {
        self.zoom_viewport().map_or(screen, |v| v.width / v.height)
    }
    /// Set the view ZOOM works on (see [`Self::zoom_frame`]), recording view history.
    pub fn set_zoom_frame(&mut self, f: View) {
        let v = match self.zoom_viewport() {
            // The screen view that `store_view` maps to this model view of the viewport.
            Some(vp) => {
                let pv = self.paper_view();
                let k = f.height / vp.height;
                View { center: f.center + (pv.center - vp.center.xy()) * k, height: pv.height * k }
            }
            None => f,
        };
        self.set_view(v);
    }
    /// Set without recording view history (realtime pan/zoom frames).
    pub fn set_view_quiet(&mut self, v: View) {
        if !(v.center.is_finite() && v.height.is_finite() && v.height > 1e-12) {
            return;
        }
        self.store_view(v);
    }
}

/// Session-level drafting settings (the AutoCAD system variables stored in the registry/profile).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub osmode: u32,
    pub orthomode: bool,
    pub polarmode: bool,
    /// POLARANG (radians).
    pub polarang: f64,
    pub gridmode: bool,
    pub snapmode: bool,
    pub snapunit: Vec2,
    /// OSNAPHATCH: object snaps find hatch objects (off by default, as in AutoCAD).
    pub osnaphatch: bool,
    pub gridunit: Vec2,
    /// Major grid line every N minor lines (GRIDMAJOR).
    pub gridmajor: u32,
    pub dynmode: bool,
    /// DYNPIFORMAT = 1: Dynamic Input shows second and next points as Cartesian `x,y` instead of
    /// polar `distance<angle` (the default).
    pub dynpi_cartesian: bool,
    /// DYNPICOORDS = 1: second and next points typed into Dynamic Input are absolute instead of
    /// relative to the last point (the default).
    pub dynpi_absolute: bool,
    pub lwdisplay: bool,
    pub transparency_display: bool,
    pub selection_cycling: bool,
    pub otrack: bool,
    /// PICKBOX in pixels.
    pub pickbox: f64,
    /// APERTURE in pixels.
    pub aperture: f64,
    pub pickfirst: bool,
    pub pickadd: bool,
    pub gripsize: f64,
    pub cursorsize: f64,
    /// MAXARRAY: the most objects one array command creates (items × selected objects).
    pub maxarray: u64,
    pub isodraft: bool,
    pub annoallvisible: bool,
    pub annoautoscale: bool,
    /// QPMODE: show the Quick Properties palette when objects are selected.
    pub qpmode: bool,
    /// POLARMODE bits ([`snap::tracking::polarmode`]); polar tracking itself is on while
    /// `polarmode` is (AUTOSNAP bit 8, F10).
    pub polar_flags: u32,
    /// POLARADDANG: additional polar tracking angles (radians), used with POLARMODE bit 4.
    pub polaraddang: Vec<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            osmode: 4133,
            orthomode: false,
            polarmode: true,
            polarang: std::f64::consts::FRAC_PI_2,
            gridmode: true,
            snapmode: false,
            snapunit: Vec2::new(0.5, 0.5),
            osnaphatch: false,
            gridunit: Vec2::new(0.5, 0.5),
            gridmajor: 5,
            dynmode: true,
            dynpi_cartesian: false,
            dynpi_absolute: false,
            lwdisplay: false,
            transparency_display: false,
            selection_cycling: false,
            otrack: true,
            pickbox: 3.0,
            aperture: 10.0,
            pickfirst: true,
            pickadd: true,
            gripsize: 5.0,
            cursorsize: 5.0,
            maxarray: 100_000,
            isodraft: false,
            annoallvisible: true,
            annoautoscale: false,
            qpmode: false,
            polar_flags: 0,
            polaraddang: Vec::new(),
        }
    }
}

/// An interactive command in progress.
pub struct Running {
    pub id: String,
    pub machine: Box<dyn Interactive>,
    pub before: Arc<Drawing>,
    pub selection_before: Vec<Handle>,
}

/// A pending window/crossing selection started by a click on empty space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingWindow {
    pub corner: Vec2,
    pub during_command: bool,
}

/// Options and values the interactive commands remember for the rest of the session, offered as
/// the `<default>` the next time they ask (as AutoCAD does). Values AutoCAD keeps in a drawing
/// system variable (HPNAME, OFFSETDIST, FILLETRAD, CHAMFERA, POLYSIDES…) live in the drawing
/// header instead. JSON calls never read these: their explicit parameters and documented defaults
/// stay authoritative.
#[derive(Clone, Debug, PartialEq)]
pub struct LastUsed {
    /// HATCH/GRADIENT starts at "Select objects" instead of "Pick internal point".
    pub hatch_select: bool,
    /// POLYGON: circumscribed about the circle (the `C` option) instead of inscribed.
    pub polygon_circumscribed: bool,
    /// ROTATE angle (radians).
    pub rotate_angle: f64,
    /// SCALE factor.
    pub scale_factor: f64,
    /// MLEADER placement order and Options.
    pub mleader: MLeaderOptions,
    /// MLEADER's block content name (Options > Content type > Block).
    pub mleader_block: String,
}

impl Default for LastUsed {
    fn default() -> Self {
        LastUsed {
            hatch_select: false,
            polygon_circumscribed: false,
            rotate_angle: 0.0,
            scale_factor: 1.0,
            mleader: MLeaderOptions::default(),
            mleader_block: String::new(),
        }
    }
}

/// Which part of a multileader MLEADER places first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeaderOrder {
    #[default]
    Arrowhead,
    Landing,
    Content,
}

/// The MLEADER placement order and the values set at its Options prompt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MLeaderOptions {
    pub order: LeaderOrder,
    /// Maximum points of the leader line (2..=64).
    pub max_points: usize,
    /// First and second angle constraints (radians; 0 = free).
    pub angles: [f64; 2],
    /// Draw the landing line (dogleg).
    pub landing: bool,
    /// Ask for multiline text content (Content type None turns it off).
    pub content: bool,
    /// Leader lines: straight, spline, or none (content only).
    pub leader_type: LeaderType,
    /// The content is a block (`LastUsed::mleader_block`) instead of multiline text.
    pub block: bool,
    /// The block's extents centre (not its insertion point) sits at the end of the landing.
    pub block_center: bool,
}

/// MLEADER's leader line type (Options > Leader type).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeaderType {
    #[default]
    Straight,
    Spline,
    None,
}

impl Default for MLeaderOptions {
    fn default() -> Self {
        MLeaderOptions {
            order: LeaderOrder::Arrowhead,
            max_points: 2,
            angles: [0.0; 2],
            landing: true,
            content: true,
            leader_type: LeaderType::Straight,
            block: false,
            block_center: true,
        }
    }
}

/// The editor session: open drawings, settings, the running command and the command log.
pub struct Session {
    pub docs: Vec<DocState>,
    pub active: usize,
    pub settings: Settings,
    pub running: Option<Running>,
    pub log: Vec<String>,
    pub last_command: Option<String>,
    /// LASTPOINT.
    pub last_point: Vec2,
    /// Point modifiers pending at the current point prompt (`FROM`, `M2P`, `.x`, `<a`…).
    pub point_mods: Vec<pointmod::Frame>,
    /// Cursor position in world coordinates (from the UI; used for direct distance entry).
    pub cursor: Vec2,
    /// The deferred tangent/perpendicular snap under the cursor, if any (from the UI; lets the
    /// rubber band show the line it resolves to).
    pub cursor_deferred: Option<snap::Deferred>,
    /// Object snap tracking: acquired points and the path under the cursor (from the UI's
    /// [`Session::snap_cursor`] calls; direct distance entry follows the path).
    pub tracking: snap::tracking::Tracker,
    /// Viewport size in pixels (from the UI; used for zoom and pick apertures).
    pub viewport_px: (f64, f64),
    pub clipboard: Vec<Entity>,
    pub clipboard_base: Vec2,
    pub pending_window: Option<PendingWindow>,
    pub untitled_counter: u32,
    /// The last dimension created (DIMCONTINUE / DIMBASELINE).
    pub last_dim: Option<Handle>,
    /// Options remembered between invocations of interactive commands.
    pub last_used: LastUsed,
    /// Where `clipboard` came from and the layers, styles and blocks its objects need.
    pub clipboard_source: Option<cmd::clipboard::ClipSource>,
    /// Calls into the session in progress: the KDM bridge writes a top-level call's changes when
    /// it returns ([`kdm_bridge`]).
    depth: u32,
    /// Who the commands act for on the replicas ([`kdm_bridge`]); `None`: the local person.
    origin: Option<Origin>,
    /// The pick tolerance in drawing units while set ([`Session::pick_aperture`]): a host sets it
    /// for an agent's call, so a point picks the same object whatever the view's zoom.
    pub pick_tolerance: Option<f64>,
    /// The layer new objects go on while set, instead of the current layer (`CLAYER`): a command
    /// given an explicit `layer` ([`Session::current_common`]). The drawing's `CLAYER` stays.
    pub layer_override: Option<String>,
    /// A host owns every file (KHP A29, section 10.6): no command reaches a file by its path
    /// ([`Session::files_by_path`]). OPEN, RECOVER and INSERT read files only from `data`; SAVE,
    /// SAVEAS, WBLOCK, PLOT, EXPORTPDF and STYLESMANAGER write none (their bytes or text come back
    /// in the answer); the kit sets it for every session a host serves.
    pub host_files_only: bool,
    /// What may run from typed text ([`Session::cmdline`], scripts, a transparent `'command`), asked
    /// before such a command starts (with no parameters) or runs (`name {json}`, with them); `Err`
    /// says why not. A host sets it so that the command line runs nothing its caller could not
    /// run directly.
    pub step_gate: Option<StepGate>,
}

/// The check [`Session::step_gate`] holds: the command id, and its parameters when typed as
/// `name {json}` (`None`: it starts and prompts).
pub type StepGate = Arc<dyn Fn(&str, Option<&Value>) -> std::result::Result<(), String> + Send + Sync>;

/// Why a command reaches no file by path while a host owns every file.
pub const HOST_OWNS_FILES: &str = "files are not reached by path here: the host opens, saves, places and exports drawings (KHP A29); pass the file's contents as `data` or `text`, or leave out `path` to get the bytes back";

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Self {
        let mut s = Session::empty();
        s.new_drawing(false);
        s
    }
    /// A session with no open drawing (the Start tab).
    pub fn empty() -> Self {
        Session {
            docs: Vec::new(),
            active: 0,
            settings: Settings::default(),
            running: None,
            log: Vec::new(),
            last_command: None,
            last_point: Vec2::ZERO,
            point_mods: Vec::new(),
            cursor: Vec2::ZERO,
            cursor_deferred: None,
            tracking: snap::tracking::Tracker::default(),
            viewport_px: (1200.0, 800.0),
            clipboard: Vec::new(),
            clipboard_base: Vec2::ZERO,
            pending_window: None,
            untitled_counter: 0,
            last_dim: None,
            last_used: LastUsed::default(),
            clipboard_source: None,
            depth: 0,
            origin: None,
            pick_tolerance: None,
            layer_override: None,
            host_files_only: false,
            step_gate: None,
        }
    }

    /// Whether command `cmd` may reach a file by its path: not while a host owns every file
    /// ([`Session::host_files_only`]). Asked before the path is touched, so the answer is the
    /// same whether a file is there or not.
    pub fn files_by_path(&self, cmd: &str) -> Result<()> {
        if self.host_files_only {
            return Err(EngineError::BadParams { cmd: cmd.into(), msg: HOST_OWNS_FILES.into() });
        }
        Ok(())
    }

    /// Asks [`Session::step_gate`] whether typed text may start (`params: None`) or run command `id`.
    fn may_run_typed(&self, id: &str, params: Option<&Value>) -> Result<()> {
        match &self.step_gate {
            Some(gate) => gate(id, params).map_err(EngineError::Other),
            None => Ok(()),
        }
    }

    /// Starts the command typed as `name` (a prefix `_`, `.`, `-` or `'` as [`Session::start`]
    /// reads it), once [`Session::step_gate`] allows it.
    fn start_typed(&mut self, name: &str) -> Result<()> {
        if let Some(spec) = cmd::resolve_typed(name) {
            self.may_run_typed(spec.id, None)?;
        }
        self.start(name)
    }
    pub fn new_drawing(&mut self, metric: bool) -> usize {
        self.untitled_counter += 1;
        let mut d = if metric { Drawing::new_metric() } else { Drawing::new_imperial() };
        d.ensure_record_ids();
        let mut st = DocState::new(d, &format!("Drawing{}", self.untitled_counter), None);
        st.set_view_quiet(if metric { View { center: Vec2::new(210.0, 148.5), height: 297.0 } } else { View::default() });
        self.docs.push(st);
        self.active = self.docs.len() - 1;
        self.active
    }
    pub fn open_drawing(&mut self, mut d: Drawing, title: &str, path: Option<String>) -> usize {
        d.ensure_record_ids();
        let mut st = DocState::new(d, title, path);
        let ext = st.doc.extents(&Space::Model);
        if !ext.is_empty() {
            st.set_view_quiet(View {
                center: ext.center(),
                height: (ext.height().max(ext.width() * self.viewport_px.1 / self.viewport_px.0.max(1.0)) * 1.1).max(1e-6),
            });
        }
        self.docs.push(st);
        self.active = self.docs.len() - 1;
        self.active
    }

    pub fn state(&self) -> Result<&DocState> {
        self.docs.get(self.active).ok_or(EngineError::NoDocument)
    }
    pub fn state_mut(&mut self) -> Result<&mut DocState> {
        self.docs.get_mut(self.active).ok_or(EngineError::NoDocument)
    }
    pub fn doc(&self) -> Result<&Drawing> {
        Ok(&self.state()?.doc)
    }
    /// Copy-on-write access to the drawing.
    pub fn doc_mut(&mut self) -> Result<&mut Drawing> {
        Ok(Arc::make_mut(&mut self.state_mut()?.doc))
    }
    /// The space commands act on (model space while working inside a layout viewport).
    pub fn space(&self) -> Space {
        self.state().map(|s| s.edit_space()).unwrap_or_default()
    }
    /// The tab being displayed: Model or a layout, regardless of MSPACE.
    pub fn layout_space(&self) -> Space {
        self.state().map(|s| s.space.clone()).unwrap_or_default()
    }
    pub fn selection(&self) -> Vec<Handle> {
        self.state().map(|s| s.selection.clone()).unwrap_or_default()
    }
    pub fn set_selection(&mut self, sel: Vec<Handle>) {
        if let Ok(st) = self.state_mut() {
            let mut seen = std::collections::HashSet::new();
            st.selection = sel.into_iter().filter(|h| seen.insert(*h)).collect();
        }
    }

    /// Message on the command line history.
    pub fn echo(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        log::debug!("{m}");
        self.log.push(m);
        if self.log.len() > 5000 {
            self.log.drain(..1000);
        }
    }

    /// Common properties for new objects (CLAYER, CECOLOR, CELTYPE, CELWEIGHT, CELTSCALE); the
    /// layer is [`Session::layer_override`] while one is set.
    pub fn current_common(&self) -> Common {
        let Ok(d) = self.doc() else { return Common::default() };
        let h = &d.header;
        Common {
            layer: self.layer_override.clone().unwrap_or_else(|| h.str("CLAYER", "0")),
            color: kition_cad_color::Color::from_aci(h.i64("CECOLOR", 256) as i16),
            linetype: h.str("CELTYPE", "ByLayer"),
            lineweight: kition_cad_doc::Lineweight::from_dxf(h.i64("CELWEIGHT", -1) as i16),
            ltscale: h.f64("CELTSCALE", 1.0),
            plot_style: h.str("CPLOTSTYLE", "ByLayer"),
            ..Common::default()
        }
    }

    /// Add an entity to the current space with current properties.
    pub fn add_entity(&mut self, kind: EntityKind) -> Result<Handle> {
        let common = self.current_common();
        let space = self.space();
        let true_color = self.doc()?.header.get("CECOLOR_RGB").and_then(|v| v.as_i64());
        let mut common = common;
        if let Some(rgb) = true_color {
            common.color = kition_cad_color::Color::True(kition_cad_color::Rgb::from_u32(rgb as u32));
        }
        let h = self.doc_mut()?.add(&space, common, kind)?;
        Ok(h)
    }

    /// World units per pixel at the current view.
    pub fn pixel_size(&self) -> f64 {
        let v = self.state().map(|s| s.view()).unwrap_or_default();
        v.height / self.viewport_px.1.max(1.0)
    }

    /// How far (drawing units) a picked point may be from an object to pick it: the pick box
    /// (PICKBOX pixels, with a margin) at the current view, or [`Session::pick_tolerance`] while
    /// one is set, which no zoom changes.
    pub fn pick_aperture(&self) -> f64 {
        match self.pick_tolerance {
            Some(t) if t.is_finite() && t > 0.0 => t,
            _ => self.pixel_size() * self.settings.pickbox.max(1.0) * 1.5,
        }
    }

    // ---------------- programmatic execution ----------------

    /// Run a command with JSON parameters (no dialogs). Records undo when the drawing changes.
    /// Afterwards (at the top level) what it changed is written to the drawing's replica.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        self.top(|s| s.execute_inner(id, params))
    }

    fn execute_inner(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        (spec.enabled)(self).map_err(|m| EngineError::Disabled(spec.id.into(), m))?;
        let before = self.state().ok().map(|s| (s.doc.clone(), s.selection.clone(), s.uid));
        let run = spec.run;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(self, params)));
        let result = match result {
            Ok(r) => r,
            Err(p) => {
                let msg =
                    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into());
                // Restore the drawing as it was.
                if let Some((doc, sel, uid)) = &before
                    && let Some(st) = self.docs.iter_mut().find(|d| d.uid == *uid)
                {
                    st.doc = doc.clone();
                    st.selection = sel.clone();
                }
                return Err(EngineError::Internal(id.into(), msg));
            }
        };
        // A command that fails changes nothing: what it did before failing is put back, as after a
        // panic, so no partial change stays (and no undo step records one).
        if result.is_err()
            && let Some((doc, sel, uid)) = &before
            && let Some(st) = self.docs.iter_mut().find(|d| d.uid == *uid)
        {
            st.doc = doc.clone();
            st.selection = sel.clone();
            return result;
        }
        cmd::groups::after_command(self, before.as_ref().map(|b| &b.0));
        cmd::constraints::after_command(self, before.as_ref().map(|b| &b.0), spec.undoable && result.is_ok());
        assoc::after_command(self, before.as_ref().map(|b| &b.0), before.as_ref().map(|b| b.2));
        if spec.undoable
            && result.is_ok()
            && let Some((doc, selection, uid)) = &before
        {
            self.refuse_locked_changes(Some(*uid), doc, selection, spec.id)?;
        }
        if spec.undoable && result.is_ok() && before.as_ref().is_some_and(|(doc, _, _)| self.state().is_ok_and(|st| !Arc::ptr_eq(doc, &st.doc))) {
            self.doc_mut()?.ensure_record_ids();
        }
        let result = match (result, &before) {
            (Ok(v), Some((doc, sel, uid))) if spec.undoable => self.refuse_non_finite(Some(*uid), doc, sel, spec.id).map(|()| v),
            (r, _) => r,
        };
        let scope = self.undo_scope();
        if spec.undoable
            && let Some((doc, sel, uid)) = before
            && let Some(st) = self.docs.iter_mut().find(|d| d.uid == uid)
            && !Arc::ptr_eq(&doc, &st.doc)
        {
            st.push_undo(Snapshot { label: spec.label.to_string(), doc, selection: sel, steps: 0, scope });
            st.revision += 1;
        }
        result
    }

    /// Bump the revision without an undo step (view-only or settings changes that redraw).
    pub fn touch(&mut self) {
        if let Ok(st) = self.state_mut() {
            st.revision += 1;
        }
    }

    // ---------------- interactive commands ----------------

    /// Start a command by name or alias as if typed at the command line.
    pub fn start(&mut self, name: &str) -> Result<()> {
        self.top(|s| s.start_inner(name))
    }

    fn start_inner(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        let transparent = name.starts_with('\'');
        let spec = cmd::resolve_typed(name).ok_or_else(|| EngineError::UnknownCommand(name.strip_prefix('\'').unwrap_or(name).to_string()))?;
        if transparent && spec.transparent && self.running.is_some() {
            // Transparent commands (zoom, pan…) run without cancelling the active one.
            if spec.interactive.is_none() {
                self.echo(format!(">>{}", spec.label));
                return self.execute(spec.id, &Value::Null).map(|_| ());
            }
        }
        if self.running.is_some() {
            self.cancel();
        }
        (spec.enabled)(self).map_err(|m| EngineError::Disabled(spec.id.into(), m))?;
        self.last_command = Some(spec.id.to_string());
        self.echo(format!("Command: {}", spec.id.to_ascii_uppercase()));
        match spec.interactive {
            Some(factory) => {
                let machine = guard::Guarded::create(spec.id, factory, self)?;
                let st = self.state()?;
                let before = st.doc.clone();
                let selection_before = st.selection.clone();
                self.running = Some(Running { id: spec.id.to_string(), machine, before, selection_before });
                self.pending_window = None;
                // Some commands complete immediately (e.g. ERASE with a pickfirst selection).
                self.feed(None)?;
                Ok(())
            }
            None => {
                let r = self.execute(spec.id, &Value::Null)?;
                if let Some(s) = r.as_str() {
                    self.echo(s.to_string());
                } else if !r.is_null()
                    && let Some(msg) = r.get("message").and_then(Value::as_str)
                {
                    for l in msg.lines() {
                        self.echo(l.to_string());
                    }
                }
                Ok(())
            }
        }
    }

    /// Give an input to the running command. With no command running, `Enter` repeats the last
    /// command (as in AutoCAD) and points do nothing.
    pub fn input(&mut self, input: Input) -> Result<()> {
        self.top(|s| s.input_inner(input))
    }

    fn input_inner(&mut self, input: Input) -> Result<()> {
        if self.running.is_none() {
            if input == Input::Enter
                && let Some(last) = self.last_command.clone()
            {
                return self.start_typed(&last);
            }
            return Ok(());
        }
        if !self.point_mods.is_empty() {
            return self.modifier_input(input);
        }
        // A deferred snap is only meaningful where the prompt resolves it; elsewhere it is the
        // point it was picked at.
        let input = match input {
            Input::Deferred(d) if !self.current_prompt().is_some_and(|p| p.deferred) => Input::Point(d.at),
            other => other,
        };
        // Generic selection handling during "Select objects:" prompts.
        let input = self.preprocess_selection(input)?;
        match input {
            Some(i) => self.feed(Some(i)),
            None => Ok(()),
        }
    }

    fn preprocess_selection(&mut self, input: Input) -> Result<Option<Input>> {
        let Some(prompt) = self.current_prompt() else { return Ok(Some(input)) };
        if !prompt.accept.select {
            return Ok(Some(input));
        }
        let space = self.space();
        match input {
            Input::Point(p) => {
                let ap = self.pick_aperture();
                if let Some(pw) = self.pending_window.take() {
                    let crossing = p.x < pw.corner.x;
                    let hs = select::select_window(self.doc()?, &space, Bounds2::new(pw.corner, p), crossing);
                    return Ok(Some(Input::Pick(hs)));
                }
                match select::pick(self.doc()?, &space, p, ap) {
                    Some(h) => Ok(Some(Input::Pick(vec![h]))),
                    None => {
                        self.pending_window = Some(PendingWindow { corner: p, during_command: true });
                        Ok(None)
                    }
                }
            }
            Input::Text(ref t) | Input::Keyword(ref t) => {
                let tl = t.trim().to_ascii_lowercase();
                let d = self.doc()?;
                let store = d.space(&space);
                let picked = match tl.as_str() {
                    "all" => store.map(|s| s.iter().filter(|e| d.is_visible(e)).map(|e| e.handle).collect()),
                    "l" | "last" => store.and_then(|s| s.last()).map(|e| vec![e.handle]),
                    "p" | "previous" => Some(self.state()?.previous_selection.clone()),
                    _ => None,
                };
                match picked {
                    Some(hs) => Ok(Some(Input::Pick(hs))),
                    None => Ok(Some(input)),
                }
            }
            Input::Cancel => {
                self.pending_window = None;
                Ok(Some(Input::Cancel))
            }
            other => Ok(Some(other)),
        }
    }

    fn feed(&mut self, input: Option<Input>) -> Result<()> {
        let Some(mut run) = self.running.take() else { return Ok(()) };
        let step = match input {
            Some(Input::Cancel) => Ok(Step::Cancel),
            Some(i) => {
                if let Input::Point(p) = &i {
                    self.last_point = *p;
                }
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run.machine.input(self, i)));
                match r {
                    Ok(r) => r,
                    Err(_) => Err(EngineError::Internal(run.id.clone(), "panic".into())),
                }
            }
            // A command starts with no modifiers pending.
            None => {
                self.point_mods.clear();
                run.machine.begin(self)
            }
        };
        let step = step.and_then(|step| {
            self.refuse_locked_changes(None, &run.before, &run.selection_before, &run.id)?;
            Ok(step)
        });
        // Redraw what the command has added so far (LINE adds a segment per point), not only when it ends.
        if let Ok(st) = self.state_mut()
            && !Arc::ptr_eq(&run.before, &st.doc)
        {
            st.revision += 1;
        }
        match step {
            Ok(Step::Continue) => {
                self.running = Some(run);
                Ok(())
            }
            Ok(Step::Done) => {
                self.finish(run, false);
                Ok(())
            }
            Ok(Step::Cancel) => {
                self.echo("*Cancel*");
                self.finish(run, true);
                Ok(())
            }
            Err(EngineError::Internal(id, m)) => {
                // Restore and end the command.
                if let Ok(st) = self.state_mut() {
                    st.doc = run.before.clone();
                }
                self.echo(format!("Internal error in {id}: {m}"));
                Err(EngineError::Internal(id, m))
            }
            Err(e) => {
                // Invalid input: report and keep prompting (AutoCAD re-prompts).
                self.echo(e.to_string());
                self.running = Some(run);
                Ok(())
            }
        }
    }

    fn finish(&mut self, run: Running, cancelled: bool) {
        if find_command(&run.id).is_some_and(|c| c.undoable)
            && let Err(e) = self.refuse_non_finite(None, &run.before, &run.selection_before, &run.id)
        {
            self.echo(e.to_string());
        }
        // A command outside undo that changed the drawing (UNDO, which swapped it for an earlier
        // one) records no undo step, as in `execute`.
        if find_command(&run.id).is_some_and(|c| !c.undoable) && self.state().is_ok_and(|st| !Arc::ptr_eq(&run.before, &st.doc)) {
            self.pending_window = None;
            return;
        }
        self.pending_window = None;
        let label = find_command(&run.id).map(|c| c.label).unwrap_or("Command");
        let _ = cancelled;
        cmd::groups::after_command(self, Some(&run.before));
        cmd::constraints::after_command(self, Some(&run.before), true);
        assoc::after_command(self, Some(&run.before), None);
        if let Err(error) = self.refuse_locked_changes(None, &run.before, &run.selection_before, &run.id) {
            self.echo(error.to_string());
            return;
        }
        if self.state().is_ok_and(|st| !Arc::ptr_eq(&run.before, &st.doc))
            && let Ok(d) = self.doc_mut()
        {
            d.ensure_record_ids();
        }
        let scope = self.undo_scope();
        if let Ok(st) = self.state_mut()
            && !Arc::ptr_eq(&run.before, &st.doc)
        {
            st.push_undo(Snapshot { label: label.to_string(), doc: run.before, selection: run.selection_before, steps: 0, scope });
            st.revision += 1;
        }
    }

    /// Cancel the running command (Esc). With no command, clears the selection.
    pub fn cancel(&mut self) {
        self.top(Session::cancel_inner);
    }

    fn cancel_inner(&mut self) {
        self.point_mods.clear();
        if self.running.is_some() {
            let _ = self.feed(Some(Input::Cancel));
        } else {
            self.pending_window = None;
            self.set_selection(Vec::new());
        }
    }

    pub fn current_prompt(&self) -> Option<Prompt> {
        self.running.as_ref().map(|r| r.machine.prompt(self))
    }

    /// Whether the cursor picks objects right now, so the UI shows the pick box (PICKBOX) at the
    /// crosshair: at prompts that select objects, and with no command running when PICKFIRST is on
    /// (noun-verb selection).
    pub fn picking_objects(&self) -> bool {
        match self.current_prompt() {
            Some(p) => p.picks_objects(),
            None => self.settings.pickfirst,
        }
    }

    /// The text shown on the command line: the active prompt, or "Command:".
    pub fn prompt_text(&self) -> String {
        match &self.running {
            Some(r) => format!("{} {}", r.id.to_ascii_uppercase(), r.machine.prompt(self).display()),
            None => "Command:".into(),
        }
    }

    /// Rubber-band preview entities for the cursor.
    pub fn preview(&self, cursor: Vec2) -> Vec<Entity> {
        let Some(r) = &self.running else { return Vec::new() };
        let common = self.current_common();
        r.machine.preview(self, cursor).into_iter().map(|k| Entity { handle: Handle(0), common: common.clone(), kind: k }).collect()
    }

    /// Process one line typed at the command line (Enter pressed).
    pub fn cmdline(&mut self, text: &str) -> Result<()> {
        self.top(|s| s.cmdline_inner(text))
    }

    fn cmdline_inner(&mut self, text: &str) -> Result<()> {
        let t = text.trim_end_matches(['\r', '\n']);
        match &self.running {
            None => {
                let t = t.trim();
                if t.is_empty() {
                    return self.input(Input::Enter);
                }
                // `cmd {json}`: programmatic call with parameters.
                if let Some((name, json)) = t.split_once(' ')
                    && json.trim_start().starts_with('{')
                {
                    let params: Value =
                        serde_json::from_str(json.trim()).map_err(|e| EngineError::BadParams { cmd: name.into(), msg: e.to_string() })?;
                    let id = cmd::resolve_alias(&name.to_ascii_lowercase());
                    self.may_run_typed(&id, Some(&params))?;
                    let r = self.execute(&id, &params)?;
                    if !r.is_null() {
                        self.echo(r.to_string());
                    }
                    return Ok(());
                }
                // Expressions like "LINE 0,0 5,5" in one line: first token is the command.
                let mut parts = t.split_whitespace();
                let name = parts.next().unwrap_or("");
                self.start_typed(name)?;
                for rest in parts {
                    if self.running.is_none() {
                        break;
                    }
                    self.typed(rest)?;
                }
                Ok(())
            }
            Some(_) => {
                // Space acts as Enter except where the prompt wants free text.
                let text_prompt = self.current_prompt().is_some_and(|p| p.accept.text && !p.accept.point && !p.accept.number);
                if text_prompt || !t.contains(' ') {
                    return self.typed(t);
                }
                for tok in t.split_whitespace() {
                    if self.running.is_none() {
                        break;
                    }
                    self.typed(tok)?;
                }
                Ok(())
            }
        }
    }

    /// One token typed while a command runs.
    fn typed(&mut self, t: &str) -> Result<()> {
        let Some(prompt) = self.current_prompt() else { return Ok(()) };
        let tt = t.trim();
        if tt.is_empty() {
            return self.input(Input::Enter);
        }
        // FROM, M2P, point filters, `<a` (before `'`: `'_from` is a modifier).
        if let Some(r) = self.typed_modifier(&prompt, tt) {
            return r;
        }
        // Transparent command.
        if tt.starts_with('\'') {
            return self.start_typed(tt);
        }
        // At "Select objects" prompts the selection modes (Previous, Last, ALL) win over a command
        // keyword sharing the letter (HATCH's "picK internal point" against `P`).
        let selection_mode = prompt.accept.select && matches!(tt.to_ascii_lowercase().as_str(), "all" | "l" | "last" | "p" | "previous");
        if !selection_mode
            && let Some(k) = prompt.match_keyword(tt)
            && !(prompt.accept.number && tt.parse::<f64>().is_ok())
        {
            return self.input(Input::Keyword(k));
        }
        if prompt.accept.point {
            if let Some(p) = prompt::parse_point_with(tt, self.last_point, &self.angle_settings()) {
                return self.input(Input::Point(p));
            }
            // Direct distance entry along the rubber band.
            if let (Some(base), Some(dist)) = (prompt.base, units::parse_distance(tt))
                && !prompt.accept.number
            {
                // Along the tracking path the cursor is on, from the point it comes from.
                if let Some((from, dir)) = self.tracking.last.as_ref().filter(|t| t.point.near(self.cursor, 1e-9)).and_then(|t| t.along()) {
                    return self.input(Input::Point(from + dir * dist));
                }
                let mut c = self.cursor;
                if self.settings.orthomode {
                    c = snap::ortho(base, c);
                }
                let dir = (c - base).normalized();
                let dir = if dir == Vec2::ZERO { Vec2::X } else { dir };
                return self.input(Input::Point(base + dir * dist));
            }
        }
        self.input(Input::Text(tt.to_string()))
    }

    /// Run a script: one input per line; within a line, spaces separate inputs except at text
    /// prompts (as AutoCAD scripts do).
    pub fn script(&mut self, text: &str) -> Result<()> {
        self.top(|s| s.script_inner(text))
    }

    fn script_inner(&mut self, text: &str) -> Result<()> {
        for line in text.lines() {
            let line = line.trim_end();
            if line.trim_start().starts_with(';') {
                continue;
            }
            if line.is_empty() {
                self.cmdline("")?;
                continue;
            }
            let mut rest = line;
            loop {
                // Free text only when the prompt has no keywords; otherwise split so `I 15` reaches the keyword and the value.
                let text_prompt = self.current_prompt().is_some_and(|p| p.accept.text && !p.accept.point && p.keywords.is_empty());
                if text_prompt || self.running.is_none() && rest.contains('{') {
                    self.cmdline(rest)?;
                    break;
                }
                match rest.split_once(' ') {
                    Some((tok, r)) => {
                        if self.running.is_none() {
                            self.start_typed(tok)?;
                        } else {
                            self.typed(tok)?;
                        }
                        rest = r;
                    }
                    None => {
                        if self.running.is_none() {
                            self.start_typed(rest)?;
                        } else {
                            self.typed(rest)?;
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    // ---------------- undo ----------------

    /// Undo the last step of [`Session::undo_scope`]. On a drawing with a replica, the replica
    /// steps back the transactions the step took and the drawing becomes its result, which keeps
    /// every collaborator's edit since, and the other scope's ([`kdm_bridge`]).
    pub fn undo(&mut self) -> Result<Option<String>> {
        self.undo_in(self.undo_scope())
    }
    pub fn redo(&mut self) -> Result<Option<String>> {
        self.redo_in(self.undo_scope())
    }
    /// [`Session::undo`] in `scope` (KitModel M12a, engine rule E14a): the person's and the
    /// agents' histories are separate.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<Option<String>> {
        self.top(|s| s.restore(false, scope))
    }
    pub fn redo_in(&mut self, scope: UndoScope) -> Result<Option<String>> {
        self.top(|s| s.restore(true, scope))
    }
    /// The undo scope of who runs the next command ([`Session::origin`]): `undo` and `redo` work
    /// in it unless their `scope` parameter names another (engine rule E14a).
    pub fn undo_scope(&self) -> UndoScope {
        UndoScope::of(self.origin().kind).unwrap_or(UndoScope::Person)
    }
    /// Whether the active drawing has a step to undo (or redo) in `scope`.
    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.state().is_ok_and(|st| st.undo.iter().any(|u| u.scope == scope))
    }
    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.state().is_ok_and(|st| st.redo.iter().any(|u| u.scope == scope))
    }

    fn restore(&mut self, redo: bool, scope: UndoScope) -> Result<Option<String>> {
        let origin = self.origin();
        let live = self.running.is_some();
        let st = self.state_mut()?;
        // Whatever is not written yet belongs to the step on top.
        st.sync_kdm(&origin, false, live);
        let from = if redo { &st.redo } else { &st.undo };
        let Some(i) = from.iter().rposition(|u| u.scope == scope) else { return Ok(None) };
        if i + 1 < from.len() {
            // The other scope has steps after this one: only the replica can take this one back
            // and keep theirs.
            return Ok(st.restore_under(redo, i, scope));
        }
        let (from, to) = if redo { (&mut st.redo, &mut st.undo) } else { (&mut st.undo, &mut st.redo) };
        let Some(snap) = from.pop() else { return Ok(None) };
        let label = snap.label.clone();
        to.push(Snapshot { label: label.clone(), doc: st.doc.clone(), selection: st.selection.clone(), steps: snap.steps, scope });
        let before = st.doc.clone();
        let reference = snap.doc.clone();
        st.doc = snap.doc;
        st.step_kdm(snap.steps, !redo, scope, before.handseed);
        // The replica's result is a new copy: when it holds just what the step's own snapshot did
        // (no collaborator's edit came in between), a drawing saved at that snapshot is clean again.
        if Arc::ptr_eq(&reference, &st.saved) && !Arc::ptr_eq(&st.doc, &reference) {
            let mut doc = (*st.doc).clone();
            doc.handseed = reference.handseed;
            if doc == *reference {
                st.saved = st.doc.clone();
            }
        }
        st.follow_layout(&before);
        st.selection = Vec::new();
        st.revision += 1;
        Ok(Some(label))
    }

    // ---------------- picking without a command ----------------

    /// Open a selection window with one corner at `corner`, for press-and-drag selection
    /// (AutoCAD's PICKDRAG = 2: a drag opens the window wherever it starts, even over an object;
    /// the next point, where the drag ends, closes it). Only while objects are being selected: no
    /// command running, or a command asking for objects. Returns whether a window was opened.
    pub fn begin_window(&mut self, corner: Vec2) -> bool {
        let during_command = self.running.is_some();
        if !corner.is_finite() || (during_command && !self.current_prompt().is_some_and(|p| p.accept.select)) {
            return false;
        }
        self.pending_window = Some(PendingWindow { corner, during_command });
        true
    }

    /// A click with no command running: pick/toggle objects or start a selection window.
    ///
    /// PICKADD on (the default): picks add to the selection and Shift removes. PICKADD off: each pick
    /// replaces the selection and Shift adds (Shift+pick on a selected object removes it).
    pub fn idle_click(&mut self, p: Vec2, shift: bool) -> Result<()> {
        let space = self.space();
        let pickadd = self.settings.pickadd;
        if let Some(pw) = self.pending_window.take() {
            let crossing = p.x < pw.corner.x;
            let hs = select::select_window(self.doc()?, &space, Bounds2::new(pw.corner, p), crossing);
            let mut sel = if pickadd || shift { self.selection() } else { Vec::new() };
            if shift && pickadd {
                sel.retain(|h| !hs.contains(h));
            } else {
                sel.extend(hs);
            }
            self.set_selection(sel);
            return Ok(());
        }
        let ap = self.pick_aperture();
        match select::pick(self.doc()?, &space, p, ap) {
            Some(h) => {
                let mut sel = if pickadd || shift { self.selection() } else { Vec::new() };
                let had = sel.contains(&h);
                if shift && (pickadd || had) {
                    sel.retain(|x| *x != h);
                } else if !had {
                    sel.push(h);
                }
                self.set_selection(sel);
            }
            None => self.pending_window = Some(PendingWindow { corner: p, during_command: false }),
        }
        Ok(())
    }

    /// Remember the selection used by a command as "Previous".
    pub fn remember_selection(&mut self, sel: &[Handle]) {
        if let Ok(st) = self.state_mut() {
            st.previous_selection = sel.to_vec();
        }
    }

    // ---------------- views ----------------

    /// The limits of the space being edited: the sheet in paper space, else LIMMIN/LIMMAX.
    pub fn zoom_limits(&self) -> Result<Bounds2> {
        let d = self.doc()?;
        if let Space::Paper(name) = self.space()
            && let Some(sheet) = kition_cad_render::sheet(d, &name)
        {
            return Ok(sheet.bounds());
        }
        let lo = d.header.point("LIMMIN").map(|p| p.xy()).unwrap_or(Vec2::ZERO);
        let hi = d.header.point("LIMMAX").map(|p| p.xy()).unwrap_or(Vec2::new(12.0, 9.0));
        Ok(Bounds2::new(lo, hi))
    }

    pub fn zoom_extents(&mut self) -> Result<()> {
        let space = self.space();
        let ext = self.doc()?.extents(&space);
        let (w, h) = self.viewport_px;
        let ext = if ext.is_empty() { self.zoom_limits()? } else { ext };
        let st = self.state_mut()?;
        let aspect = st.zoom_aspect(w / h.max(1.0));
        let height = ext.height().max(ext.width() / aspect.max(1e-6)).max(1e-6) * 1.05;
        st.set_zoom_frame(View { center: ext.center(), height });
        Ok(())
    }

    /// Zoom by `factor` (> 1 zooms in) keeping `about` fixed on screen.
    pub fn zoom_about(&mut self, factor: f64, about: Vec2) -> Result<()> {
        if !(factor.is_finite() && factor > 0.0) {
            return Ok(());
        }
        let st = self.state_mut()?;
        let v = st.view();
        let h = (v.height / factor).clamp(1e-9, 1e12);
        let k = h / v.height;
        let c = about + (v.center - about) * k;
        st.set_view_quiet(View { center: c, height: h });
        Ok(())
    }

    /// Serializable summary for inspect/automation.
    pub fn summary(&self) -> Value {
        let docs: Vec<Value> = self
            .docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                serde_json::json!({
                    "index": i, "title": d.title, "path": d.path, "dirty": d.is_dirty(),
                    "entities": d.doc.entity_count(), "active": i == self.active, "revision": d.revision,
                })
            })
            .collect();
        serde_json::json!({
            "documents": docs,
            "prompt": self.prompt_text(),
            "running": self.running.as_ref().map(|r| r.id.clone()),
            "selection": self.selection().iter().map(|h| h.hex()).collect::<Vec<_>>(),
            "settings": self.settings,
        })
    }
}

#[cfg(test)]
mod tests;
