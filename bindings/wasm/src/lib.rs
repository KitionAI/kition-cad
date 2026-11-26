//! # kition-cad-kit
//!
//! The CAD engine for Kition's in-page editor, on the Kition SDK. [`CadDocument`] implements
//! [`KitDocument`] for a Kition CAD drawing (DXF) and [`KitTools`] for Kition CAD's way of working:
//! a drawing tool starts its command, a click answers the command's prompt with a point (object
//! snaps applied), and with no command running a click or drag selects. The prompt shows as a
//! status line. The command line is the `command.runLine` command (`{"text": "line"}`), for the host's
//! command-line bar and for agents; the rail follows the command it starts.
//!
//! Document pixels: the drawing's limits and extents, with a margin, y down, at a power-of-two
//! scale that puts the longer side between half of [`DOC_SIDE`] and [`DOC_SIDE`] pixels. The scale
//! follows the drawing alone, so a saved drawing reopens to the same pixels, and it changes only
//! when the drawing doubles or halves in size. Model space is drawn on
//! white paper, colour 7 black, as on a plot; the on-screen editor gives its canvas colour instead
//! (`setBackdrop`), colour 7 then black or white against it, and draws the grid itself.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod acts;
pub mod agent_text;
pub mod check;
mod collab;
mod command_line;
#[cfg(test)]
mod command_line_tests;
#[cfg(test)]
mod conformance;
mod context;
#[cfg(test)]
mod cursor_tests;
pub mod document_text;
#[cfg(test)]
mod drafting_tests;
mod files;
mod interaction;
pub mod links;
mod params;
mod place;
mod query;
mod read_fields;
mod render;
mod scan;
mod tools;

use std::cell::RefCell;

use kition_cad_color::Rgb;
use kition_cad_engine::prompt::Input;
use kition_cad_engine::{Session, View};
use kition_cad_geom::{Bounds2, Vec2};
use kition_cad_render::raster::{self as cad_raster, RasterOptions};
use kition_cad_render::{DisplayList, Kind};
use kition_kit::raster::{self, Frame};
use kition_kit::tools::{KitTools, PointerAction, ToolInfo, ToolMods, ToolOverlay, ToolPointer, ToolRequest, ToolResponse};
use kition_kit::{KitCommand, KitDocument, MAX_RENDER_SIDE};
use serde_json::{Value, json};

pub use context::AGENT_TOLERANCE;

/// The editor's state in [`KitDocument::inspect`], as JSON Pointers: the space shown and its
/// MSPACE viewport, the view, the selection and the selected objects, the view the editor is asked
/// to show, the command line's prompt and running command, the drafting modes, and the canvas's
/// drawing units per pixel (which follow the space shown). It describes the editing session, not
/// the drawing, so a headless host leaves it out of what an agent reads
/// (`kition_headless::KitInfo::volatile`) and `testing::check_context` does not compare it; every
/// space's own facts are in `/layouts`, and `entities {layout}` lists any space's entities.
pub const EDITOR_STATE: &[&str] =
    &["/space", "/viewport", "/view", "/selection", "/selectedObjects", "/viewRequest", "/prompt", "/running", "/drafting", "/units"];

/// The most document pixels on a drawing's longer side.
pub const DOC_SIDE: f64 = 2000.0;
/// Margin around the drawing's limits and extents, as a fraction of its size.
const MARGIN: f64 = 0.05;
/// The viewport height (pixels) the engine's pick and snap apertures are measured against.
const VIEWPORT_PX: f64 = 1000.0;
/// The width of zero-lineweight lines in document pixels at 1:1.
const HAIRLINE: f64 = 2.0;
/// A drag shorter than this (screen pixels) is a click.
const DRAG_PX: f64 = 4.0;

const PAPER: Rgb = Rgb(255, 255, 255);
/// Kition CAD's brand colour (`ui.brand.light` in `kit.json`) for the tool previews.
const ACCENT: [u8; 4] = [79, 109, 138, 255];
const SELECTED: [u8; 4] = [37, 99, 235, 255];
const SNAP: [u8; 4] = [234, 140, 0, 255];
const WINDOW: [u8; 4] = [37, 99, 235, 255];
const CROSSING: [u8; 4] = [22, 163, 74, 255];
/// The selection window's see-through fill, blue for a window and green for a crossing.
const WINDOW_FILL: [u8; 4] = [37, 99, 235, 38];
const CROSSING_FILL: [u8; 4] = [22, 163, 74, 38];
/// A preview's guide that is not part of the result (MOVE's line from the base point to the
/// cursor), so it does not read as a line being drawn.
const GUIDE: [u8; 4] = [47, 107, 255, 255];
/// The cursor's own overlays (the crosshair's anchor and the coordinate readout's label) are fully
/// transparent: the editor draws the crosshair and the readout itself (`web/src/cursor.ts`), and a
/// host that draws overlays plainly shows nothing for them.
pub const CURSOR: [u8; 4] = [0, 0, 0, 0];

/// The drawing tools on the rail, in AutoCAD's order: (command, group, icon). Icon names are
/// `tool-<kind>`, which Kition's editor maps to its own icons.
const TOOLS: &[(&str, usize, &str)] = &[
    ("line.add", 1, "tool-line"),
    ("polyline.add", 1, "tool-pen"),
    ("circle.add", 2, "tool-ellipse"),
    ("arc.add", 2, "tool-arc"),
    ("ellipse.add", 2, "tool-ellipse"),
    ("rectangle.add", 3, "tool-rect"),
    ("polygon.add", 3, "tool-polygon"),
    ("spline.add", 4, "tool-curvature"),
    ("hatch.add", 4, "tool-bucket"),
    ("multilineText.add", 5, "tool-type"),
    ("text.add", 5, "tool-type"),
    ("dimension.addLinear", 6, "tool-measure"),
    ("dimension.addAligned", 6, "tool-measure"),
    ("dimension.addRadius", 6, "tool-measure"),
    ("object.move", 7, "tool-move"),
    ("object.copy", 7, "tool-copy"),
    ("object.rotate", 8, "tool-rotate"),
    ("object.scale", 8, "tool-scale"),
    ("object.mirror", 8, "tool-reflect"),
    ("object.offset", 9, "tool-offset"),
    ("object.trim", 9, "tool-scissors"),
    ("object.extend", 9, "tool-extend"),
    ("object.fillet", 9, "tool-fillet"),
    ("object.delete", 10, "tool-eraser"),
];

/// Document pixels of the drawing: `px = (x - x0) * k`, `py = (y1 - y) * k`.
#[derive(Clone, Copy, Debug)]
struct Placement {
    x0: f64,
    y1: f64,
    k: f64,
    width: f64,
    height: f64,
}

impl Placement {
    fn of(bounds: Bounds2, k: f64) -> Self {
        Self { x0: bounds.min.x, y1: bounds.max.y, k, width: (bounds.width() * k).max(1.0), height: (bounds.height() * k).max(1.0) }
    }

    fn to_world(self, x: f64, y: f64) -> Vec2 {
        Vec2::new(self.x0 + x / self.k, self.y1 - y / self.k)
    }

    fn to_doc(self, p: Vec2) -> (f64, f64) {
        ((p.x - self.x0) * self.k, (self.y1 - p.y) * self.k)
    }
}

/// The display list of the drawing, built once per revision.
struct Built {
    revision: u64,
    space: kition_cad_doc::Space,
    list: DisplayList,
}

/// One open Kition CAD drawing.
pub struct CadDocument {
    session: Session,
    built: RefCell<Option<Built>>,
    /// Where the pointer is, in drawing units, and what it snapped to.
    cursor: Option<Vec2>,
    snapped: Option<(Vec2, &'static str)>,
    /// What the cursor pipeline made of the pointer ([`interaction`]): the point a click there
    /// gives, the object snap behind it and the tracking path it lies on.
    aim: Option<Vec2>,
    hit: Option<kition_cad_engine::snap::SnapHit>,
    track: Option<kition_cad_engine::snap::tracking::Track>,
    /// The object snap under the press not yet released (a deferred tangent goes in as one).
    press_hit: Option<kition_cad_engine::snap::SnapHit>,
    /// Whether Shift was held at the last pointer event.
    shift: bool,
    /// The object under the pick box, for the rollover highlight: picked again only when the
    /// pointer moves ([`interaction`]).
    hover: Option<kition_cad_doc::Handle>,
    /// The editor's canvas colour (`setBackdrop`): drawn behind the drawing instead of white paper,
    /// and the editor then draws the grid itself over the whole canvas.
    backdrop: Option<Rgb>,
    /// The tracking clock, in seconds ([`interaction`]): only a pointer move repeated where the
    /// last one was (the page sends one when the pointer rests) moves it on.
    clock: f64,
    rest: Option<(f64, f64)>,
    /// What the acquired tracking points belong to (the running command, or a hot grip).
    tracking_for: Option<String>,
    /// A press not yet released: where it went down (drawing units, document pixels).
    press: Option<(Vec2, (f64, f64))>,
    dragging: bool,
    hot_grip: Option<interaction::HotGrip>,
    viewport_synced: bool,
    view_request: Option<Value>,
    view_serial: u64,
    /// The tool the host was last told is active; the next answer switches the host's rail when
    /// the running command differs (one a menu or the command line started, or one that ended).
    reported: RefCell<String>,
    /// The editor states [`KitDocument::editor_state`] answered, for
    /// [`KitDocument::restore_editor_state`] ([`context`]).
    kept: std::sync::Mutex<context::Kept>,
    /// Refreshed by model_export before every exchange transaction, including after commands.
    batch_entities: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<kition_kit::kdm::ObjectId>>>,
    /// Imported names without persisted IDs or DXF handles cannot prove reread continuity.
    unproven_record_ids: std::collections::BTreeSet<String>,
    /// The command line's history ([`command_line`]): what the person's entries and the commands
    /// they ran wrote, the last [`command_line::HISTORY_LINES`] lines, and how many lines it has
    /// ever had.
    history: std::collections::VecDeque<String>,
    history_serial: u64,
}

/// The parameter names of commands that read or write files outside the document, as
/// `kition-headless` refuses them for agents. The command line refuses these commands too, so it
/// is no way around that.
const FILE_PARAMS: &[&str] = &["path", "paths", "folder", "dir", "directory", "output", "outputDir", "file", "files", "target"];

/// Whether a command's parameter notation (`{path, format?: dxf|dwg}`) names a file parameter.
fn names_a_file(params: &str) -> bool {
    let mut rest = params;
    while let Some(start) = rest.find(|c: char| c.is_ascii_alphabetic()) {
        let word_end = rest[start..].find(|c: char| !c.is_ascii_alphanumeric()).map_or(rest.len(), |end| start + end);
        let word = &rest[start..word_end];
        let next = rest[word_end..].trim_start().chars().next();
        if FILE_PARAMS.contains(&word) && matches!(next, Some('?' | ':' | ',' | '}')) {
            return true;
        }
        rest = &rest[word_end..];
    }
    false
}

/// The command line's gate ([`kition_cad_engine::Session::step_gate`]): what typed text may start or
/// run, however it is written (`_RECOVER`, `.open`, `-insert`, a `'recover` typed while another
/// command runs, `name {json}`, a script line). A command that reads or writes files never runs
/// from the command line: the host opens, saves and exports drawings (KHP A28, A29). A command
/// typed with its parameters then asks the host's gate, as a direct call does
/// (`kition_kit::dispatch::may_run`; without a host it allows everything), so the command line runs
/// nothing its caller could not run directly. The commands whose top level takes any DIM* key
/// ([`OPEN_PARAMETERS`]) check their keys themselves.
fn command_line_gate(command: &str, params: Option<&Value>) -> Result<(), String> {
    if kition_cad_engine::cmd::find_command(command).is_some_and(|spec| names_a_file(spec.params)) {
        return Err(format!(
            "{} reads or writes files, which the command line cannot do here",
            kition_cad_engine::cmd::typed_name(command).to_ascii_uppercase()
        ));
    }
    match params {
        Some(params) if !OPEN_PARAMETERS.contains(&command) => kition_kit::dispatch::may_run(command, params),
        _ => Ok(()),
    }
}

/// Commands that change the drawing although the engine does not record them as undoable steps:
/// the history itself, and the commands that make, open, close, recover, save, or switch drawings.
const DOCUMENT_COMMANDS: &[&str] = &[
    "edit.undoStep",
    "edit.undoMany",
    "edit.redoStep",
    "file.new",
    "file.open",
    "file.close",
    "file.closeAll",
    "file.recover",
    "file.save",
    "file.saveAs",
    "file.switch",
];

/// Whether running a command changes the drawing (the catalog's `mutates`): its edits (the
/// engine's undoable commands) and [`DOCUMENT_COMMANDS`]. Selection, zoom and pan, drafting
/// toggles, queries, and outputs (plot, PDF, the drawing's bytes) do not.
fn changes_document(spec: &kition_cad_engine::cmd::CommandSpec) -> bool {
    spec.undoable || DOCUMENT_COMMANDS.contains(&spec.id)
}

fn engine_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn union(a: Bounds2, b: Bounds2) -> Bounds2 {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    Bounds2::new(Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)), Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)))
}

/// The formats the drawing exports to (`kit.json` `exports`, KHP A41): model space framed to its
/// extents, as `kition_cad_io` writes images (PDF on its page setup's paper, fitted; SVG and PNG
/// with a small margin). Text is drawn with the kit's own stroke font or, natively, the TrueType
/// font a style names. DWG is left out: the web build cannot write it.
const EXPORTS: &[&str] = &["pdf", "svg", "png"];

/// Commands whose top level takes any dimension style field or DIM* variable as a key (as
/// DIMSTYLE and DIMOVERRIDE do), so a schema cannot list them; every other command refuses a key
/// its schema does not name (platform rule E7a).
const OPEN_PARAMETERS: &[&str] = &["dimensionStyle.open", "dimensionStyle.openManager", "dimensionStyle.override", "dimension.override"];

impl CadDocument {
    /// Each command's parameter schema, as the catalogue offers it to agents; read once, since a
    /// command's parameters do not change with the drawing.
    fn parameter_schemas(&self) -> &'static std::collections::HashMap<String, Value> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Value>> = std::sync::OnceLock::new();
        SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect())
    }

    /// The drawing exported as `format`, one of [`EXPORTS`]; the drawing is left as it is.
    fn export_bytes(&self, format: &str) -> Result<Vec<u8>, String> {
        let format = format.trim().trim_start_matches('.').to_ascii_lowercase();
        if !EXPORTS.contains(&format.as_str()) {
            return Err(format!(
                "a drawing exports to {}, not {}; to save the drawing itself, save it as .dxf",
                EXPORTS.join(", "),
                if format.is_empty() { "an empty format".to_owned() } else { format!(".{format}") }
            ));
        }
        kition_cad_io::write(self.session.doc().map_err(engine_error)?, &format!("drawing.{format}")).map_err(engine_error)
    }

    /// `file.export {format}`: the export as base64 `data`, the command `cad_export` runs
    /// (`kit.json` `agent.export`, Kition platform K21). It names no file: the host writes it.
    fn export_command(&self, params: &Value) -> Result<Value, String> {
        let format =
            params.get("format").and_then(Value::as_str).ok_or_else(|| format!("file.export needs `format`: one of {}", EXPORTS.join(", ")))?;
        let bytes = self.export_bytes(format)?;
        let format = format.trim().trim_start_matches('.').to_ascii_lowercase();
        Ok(json!({ "format": format, "data": kition_cad_engine::cmd::file::base64_encode(&bytes), "bytes": bytes.len() }))
    }

    /// The document of `session`, which reaches no file by path when a host owns every file
    /// (`kition_kit::host::hosting`, KHP A29: kition-headless says so before it opens or creates a
    /// drawing, and [`KitDocument::hosted`] after), and whose command line passes
    /// [`command_line_gate`].
    fn from_session(mut session: Session) -> Self {
        // DXF regenerates these caches on every save. Keep them out of the agent model;
        // the raw Drawing/KDM bridge still preserves imported headers exactly.
        let has_replica = session.kdm().is_some();
        if let Ok(state) = session.state_mut() {
            let clean = !state.is_dirty();
            let header = &mut std::sync::Arc::make_mut(&mut state.doc).header.vars;
            for key in ["EXTMIN", "EXTMAX", "HANDSEED"] {
                header.remove(key);
            }
            if clean {
                state.saved = state.doc.clone();
            }
        }
        if has_replica {
            session.sync_kdm();
        }
        session.host_files_only |= kition_kit::host::hosting().files_only;
        let gate: kition_cad_engine::StepGate = std::sync::Arc::new(command_line_gate);
        session.step_gate = Some(gate);
        Self {
            session,
            built: RefCell::new(None),
            cursor: None,
            snapped: None,
            aim: None,
            hit: None,
            track: None,
            press_hit: None,
            shift: false,
            hover: None,
            backdrop: None,
            clock: 0.0,
            rest: None,
            tracking_for: None,
            press: None,
            dragging: false,
            hot_grip: None,
            viewport_synced: false,
            view_request: None,
            view_serial: 0,
            reported: RefCell::new("select".into()),
            kept: std::sync::Mutex::default(),
            unproven_record_ids: std::collections::BTreeSet::new(),
            batch_entities: Default::default(),
            history: std::collections::VecDeque::new(),
            history_serial: 0,
        }
    }

    /// The display list of the space being edited, rebuilt when the drawing changes.
    fn list(&self) -> Result<std::cell::Ref<'_, Built>, String> {
        let state = self.session.state().map_err(engine_error)?;
        let (revision, space) = (state.revision, state.edit_space());
        // The space too: a call context changes it without a new revision.
        let stale = self.built.borrow().as_ref().is_none_or(|built| built.revision != revision || built.space != space);
        if stale {
            let options = kition_cad_render::Options { tolerance: self.tolerance(), ..Default::default() };
            let list = kition_cad_render::build(&state.doc, &space, &options);
            *self.built.borrow_mut() = Some(Built { revision, space, list });
        }
        std::cell::Ref::filter_map(self.built.borrow(), Option::as_ref).map_err(|_| "no display list".to_owned())
    }

    /// Curve flattening tolerance in drawing units, about a quarter of a document pixel; from the
    /// stored extents, since it is needed to build the display list.
    fn tolerance(&self) -> f64 {
        let Ok(state) = self.session.state() else { return 0.01 };
        let extents =
            union(state.doc.extents(&state.edit_space()), self.session.zoom_limits().unwrap_or(Bounds2::new(Vec2::ZERO, Vec2::new(420.0, 297.0))));
        let side = extents.width().max(extents.height()).max(1e-9);
        0.25 * side / DOC_SIDE
    }

    /// The drawing's limits and extents with a margin, in drawing units.
    fn world_bounds(&self) -> Bounds2 {
        let limits = self.session.zoom_limits().unwrap_or(Bounds2::new(Vec2::ZERO, Vec2::new(420.0, 297.0)));
        let extents = self.list().map(|built| built.list.bounds).unwrap_or(limits);
        let all = union(limits, extents);
        let (mx, my) = (all.width().max(1.0) * MARGIN, all.height().max(1.0) * MARGIN);
        Bounds2::new(Vec2::new(all.min.x - mx, all.min.y - my), Vec2::new(all.max.x + mx, all.max.y + my))
    }

    fn placement(&self) -> Placement {
        let bounds = self.world_bounds();
        let k = (DOC_SIDE / bounds.width().max(bounds.height()).max(1e-9)).log2().floor().exp2();
        Placement::of(bounds, k)
    }

    /// `width × height` straight-alpha pixels of the area at document pixel (`x`, `y`) at `scale`
    /// screen pixels per document pixel: the vector display list rasterised at that scale.
    fn draw(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let place = self.placement();
        let built = self.list()?;
        let center = place.to_world(x + f64::from(width) / (2.0 * scale), y + f64::from(height) / (2.0 * scale));
        let view = cad_raster::View { center, scale: place.k * scale, width, height };
        // Hairlines are two pixels wide at 1:1, so they stay visible in the exact reductions Kition
        // shows below 1:1, and thin to one screen pixel as the view zooms in.
        let options = RasterOptions { background: self.background(), hairline: (HAIRLINE / scale).max(1.0) as f32, ..RasterOptions::default() };
        let pixmap = cad_raster::render(&built.list, &view, &options).ok_or("the drawing could not be drawn")?;
        // The background is opaque, so premultiplied and straight alpha are the same.
        Ok(pixmap.take())
    }

    /// What the drawing is drawn on: the editor's canvas colour, or white paper.
    fn background(&self) -> Rgb {
        self.backdrop.unwrap_or(PAPER)
    }

    /// Draws the drawing on `color` (`#rrggbb`, the editor's canvas colour) from now on, colour 7
    /// black or white against it, and leaves the grid to the editor ([`CadDocument::draw_grid`]).
    pub fn set_backdrop(&mut self, color: &str) -> Result<(), String> {
        let hex = color.trim().trim_start_matches('#');
        let channel = |at: usize| hex.get(at..at + 2).and_then(|part| u8::from_str_radix(part, 16).ok());
        match (hex.len(), channel(0), channel(2), channel(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                self.backdrop = Some(Rgb(r, g, b));
                Ok(())
            }
            _ => Err(format!("{color} is not a #rrggbb colour")),
        }
    }

    /// Tells the engine how many drawing units a screen pixel covers, so pick boxes and snap
    /// apertures are the size they are on screen.
    fn sync_view(&mut self, zoom: f64) {
        if self.viewport_synced {
            return;
        }
        let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
        self.session.viewport_px = (VIEWPORT_PX, VIEWPORT_PX);
        let center = self.cursor.unwrap_or_else(|| self.world_bounds().center());
        let k = self.placement().k;
        if let Ok(state) = self.session.state_mut() {
            state.set_view_quiet(View { center, height: VIEWPORT_PX / (k * zoom) });
        }
    }

    /// Runs a typed command-line entry (`line`, `l`, a coordinate such as `10,20`, a keyword,
    /// or an empty line for Enter), as the command line does.
    ///
    /// The entry goes on the history beside the prompt it answers; the answer's `rejected` says
    /// the running command refused it and asks again (the reason is on the history).
    pub fn command_line(&mut self, text: &str) -> Result<Value, String> {
        self.echo_entry(text);
        if let Some(grip) = self.hot_grip {
            let point = kition_cad_engine::prompt::parse_point(text, grip.base)
                .filter(|p| p.is_finite())
                .ok_or("Specify a point, or press Escape to cancel")?;
            self.commit_grip(point)?;
            return Ok(json!({ "prompt": self.prompt(), "running": null, "rejected": false }));
        }
        let before = self.current_view();
        let refused = self.session.rejected_inputs;
        // What the text starts or runs passes the session's step gate (command_line_gate).
        self.session.cmdline(text).map_err(engine_error)?;
        self.record_view(before);
        Ok(json!({
            "prompt": self.session.prompt_text(),
            "running": self.session.running.as_ref().map(|running| running.id.clone()),
            "rejected": self.session.rejected_inputs != refused,
        }))
    }

    /// The display list's polylines for `object.query`, as paths in document pixels.
    fn paths(&self, list: &DisplayList, place: &Placement, color: [u8; 4], width: f32, dashed: bool) -> Vec<ToolOverlay> {
        let mut out = Vec::new();
        for prim in &list.prims {
            if !matches!(prim.kind, Kind::Polyline) {
                continue;
            }
            let (start, len) = (prim.start as usize, prim.len as usize);
            let Some(points) = list.verts.get(start..start.saturating_add(len)) else { continue };
            let mut d = String::new();
            for (index, point) in points.iter().enumerate() {
                let (x, y) = place.to_doc(*point);
                d.push_str(&format!("{}{x:.2},{y:.2}", if index == 0 { "M" } else { " L" }));
            }
            if !d.is_empty() {
                out.push(ToolOverlay::Path { d, color, width, dashed });
            }
        }
        out
    }

    fn overlays(&self) -> Vec<ToolOverlay> {
        let mut out = Vec::new();
        let place = self.placement();
        let Ok(state) = self.session.state() else { return out };
        // The selection, highlighted, with its grips.
        let selection = self.session.selection();
        if !selection.is_empty() {
            let entities: Vec<&kition_cad_doc::Entity> =
                selection.iter().filter_map(|handle| state.doc.entity(*handle)).map(|entity| &**entity).collect();
            let list = kition_cad_render::build_entities(
                &state.doc,
                entities,
                &kition_cad_render::Options { tolerance: self.tolerance(), ..Default::default() },
            );
            out.extend(self.paths(&list, &place, SELECTED, 2.0, true));
            if self.session.running.is_none() {
                for handle in &selection {
                    for grip in self.session.grips_of(*handle).unwrap_or_default() {
                        let (x, y) = place.to_doc(grip);
                        out.push(ToolOverlay::Anchor { x, y, color: SELECTED, filled: true, size: 7.0 });
                    }
                }
            }
        }
        // The object under the pick box (the rollover highlight), thinner than the selection.
        if let Some(entity) = self.hovered().and_then(|handle| state.doc.entity(handle)).filter(|entity| !selection.contains(&entity.handle)) {
            let list = kition_cad_render::build_entities(
                &state.doc,
                std::iter::once(&**entity),
                &kition_cad_render::Options { tolerance: self.tolerance(), ..Default::default() },
            );
            out.extend(self.paths(&list, &place, SELECTED, 1.0, true));
        }
        // What the running command would draw at the cursor, and its guide (MOVE's line from the
        // base point to the cursor) in a colour of its own: it is not drawn.
        if let Some(cursor) = self.aim.or(self.cursor) {
            let mut preview = self.session.preview(cursor);
            let guide = self.preview_guide(&preview, cursor).then(|| preview.pop()).flatten();
            let options = kition_cad_render::Options { tolerance: self.tolerance(), ..Default::default() };
            if !preview.is_empty() {
                let list = kition_cad_render::build_entities(&state.doc, preview.iter(), &options);
                out.extend(self.paths(&list, &place, ACCENT, 1.5, false));
            }
            if let Some(guide) = guide {
                let list = kition_cad_render::build_entities(&state.doc, std::iter::once(&guide), &options);
                out.extend(self.paths(&list, &place, GUIDE, 1.5, false));
            }
        }
        // The selection window being dragged: a see-through blue window left to right with a solid
        // edge, a green crossing right to left with a dashed one.
        if let (Some(window), Some(cursor)) = (self.session.pending_window.as_ref(), self.cursor) {
            let (ax, ay) = place.to_doc(window.corner);
            let (bx, by) = place.to_doc(cursor);
            let crossing = cursor.x < window.corner.x;
            let (color, fill) = if crossing { (CROSSING, CROSSING_FILL) } else { (WINDOW, WINDOW_FILL) };
            out.push(ToolOverlay::Highlight { quad: [[ax, ay], [bx, ay], [bx, by], [ax, by]], color: fill });
            let d = format!("M{ax:.2},{ay:.2} L{bx:.2},{ay:.2} L{bx:.2},{by:.2} L{ax:.2},{by:.2} Z");
            out.push(ToolOverlay::Path { d, color, width: 1.0, dashed: crossing });
        }
        // The object snap the next click will use.
        if let Some((point, name)) = self.snapped {
            let (x, y) = place.to_doc(point);
            out.push(ToolOverlay::Anchor { x, y, color: SNAP, filled: false, size: 11.0 });
            out.push(ToolOverlay::Label { x: x + 8.0, y: y - 8.0, text: name.to_owned(), color: SNAP });
        }
        out.extend(self.tracking_overlays());
        out.extend(self.grip_overlays());
        out.extend(self.cursor_overlays());
        out
    }

    /// Whether the preview's last object is only a guide: a line from the prompt's base point to
    /// the cursor after the objects the command moves, turns or scales (MOVE, COPY, ROTATE, SCALE,
    /// MIRROR, STRETCH), where the result is those objects. A line alone (LINE's rubber band) is
    /// what the command draws.
    fn preview_guide(&self, preview: &[kition_cad_doc::Entity], cursor: Vec2) -> bool {
        let [_, .., last] = preview else { return false };
        let kition_cad_doc::EntityKind::Line(line) = &last.kind else { return false };
        let base = self.session.current_prompt().and_then(|prompt| prompt.base);
        line.b.xy().near(cursor, 1e-9) && base.is_none_or(|base| line.a.xy().near(base, 1e-9))
    }

    fn respond(&self) -> ToolResponse {
        let running = self.session.running.is_some() || self.hot_grip.is_some();
        let prompt = self.prompt();
        let mut requests = Vec::new();
        let tool = self.tool();
        if *self.reported.borrow() != tool {
            requests.push(ToolRequest::SwitchTool { id: tool.clone() });
            *self.reported.borrow_mut() = tool;
        }
        if !prompt.is_empty() {
            requests.push(ToolRequest::Status { text: prompt });
        }
        ToolResponse {
            requests,
            overlays: self.overlays(),
            // What the cursor is now ([`CadDocument::cursor_state`]), which the editor draws.
            cursor: self.cursor_state().to_owned(),
            // Editing keys come here while a command runs, a window is open, or objects are
            // selected (Escape clears the selection, Delete erases it).
            busy: running || self.session.pending_window.is_some() || !self.session.selection().is_empty(),
            wants_text: false,
        }
    }

    /// Whether the running command's prompt takes free text and no point or number (TEXT's
    /// "Enter text", a name): the prompts where the command line keeps a typed space, as
    /// [`Session::cmdline`] does.
    fn free_text_prompt(&self) -> bool {
        self.hot_grip.is_none() && self.session.current_prompt().is_some_and(|p| p.accept.text && !p.accept.point && !p.accept.number)
    }

    fn feed(&mut self, input: Input) -> Result<(), String> {
        let before = self.current_view();
        self.session.input(input).map_err(engine_error)?;
        self.record_view(before);
        Ok(())
    }
}

impl KitDocument for CadDocument {
    fn batch_settler(&self) -> Option<kition_kit::BatchSettler> {
        let existing = self.batch_entities.clone();
        Some(Box::new(move |tx| {
            let root = kition_kit::kdm::ObjectId::from_string("drawing");
            let header = tx.map_entries(&root, "header").map_err(|e| e.to_string())?;
            if let Some((key, _)) = header.iter().find(|(key, _)| matches!(key.as_str(), "EXTMIN" | "EXTMAX" | "HANDSEED")) {
                return Err(format!("read-only: header.{key} is generated by the DXF writer"));
            }
            let known = existing.lock().map_err(|_| "entity inventory unavailable")?;
            for object in tx.objects_of_type("Entity") {
                if !known.contains(&object) && !tx.get(&object, "handle").map_err(|e| e.to_string())?.is_null() {
                    return Err("read-only: Entity.handle is allocated by the kit, including at creation".into());
                }
            }
            Ok(())
        }))
    }

    /// Opens a DXF drawing (DWG needs the native app).
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        let mut drawing = kition_cad_io::read(bytes, name).map_err(engine_error)?;
        let mut occurrences = std::collections::BTreeMap::new();
        let mut unproven = Vec::new();
        for (ty, record) in drawing.record_names() {
            let nth = occurrences.entry((ty.clone(), record.clone())).or_insert(0usize);
            if drawing.record_id(&ty, &record, *nth).is_none() {
                unproven.push((ty, record, *nth));
            }
            *nth = nth.saturating_add(1);
        }
        drawing.ensure_record_ids();
        let unproven_record_ids =
            unproven.into_iter().filter_map(|(ty, record, nth)| drawing.record_id(&ty, &record, nth).map(|id| format!("{ty}:{id}"))).collect();
        for space in std::iter::once(&mut drawing.model)
            .chain(drawing.layouts.iter_mut().map(|l| &mut l.entities))
            .chain(drawing.blocks.values_mut().map(|b| &mut std::sync::Arc::make_mut(b).entities))
        {
            let tables: Vec<_> = space
                .iter()
                .filter_map(|e| if let kition_cad_doc::EntityKind::Table(t) = &e.kind { Some((e.handle, t.clone())) } else { None })
                .collect();
            for (h, mut t) in tables {
                t.identify_cells().map_err(str::to_owned)?;
                space.modify(h, |e| e.kind = kition_cad_doc::EntityKind::Table(t));
            }
        }
        let mut session = Session::empty();
        let title = std::path::Path::new(name).file_stem().map_or_else(|| "Drawing".to_owned(), |stem| stem.to_string_lossy().into_owned());
        session.open_drawing(drawing, &title, Some(name.to_owned()));
        let mut document = Self::from_session(session);
        document.unproven_record_ids = unproven_record_ids;
        Ok(document)
    }

    /// A new metric drawing (A3 limits). The size is the host's page hint; drawings are measured
    /// in their own units.
    fn create(_name: &str, _width: u32, _height: u32) -> Result<Self, String> {
        let mut session = Session::empty();
        session.new_drawing(true);
        if let Ok(state) = session.state_mut() {
            state.saved = state.doc.clone();
        }
        Ok(Self::from_session(session))
    }

    fn width(&self) -> u32 {
        self.placement().width.round().max(1.0) as u32
    }

    fn height(&self) -> u32 {
        self.placement().height.round().max(1.0) as u32
    }

    fn revision(&self) -> u64 {
        self.session.state().map_or(0, |state| state.revision)
    }

    fn dirty(&self) -> bool {
        self.session.state().is_ok_and(kition_cad_engine::DocState::is_dirty)
    }

    /// Vectors: drawn at the zoom above 1:1, so lines stay sharp; below, at 1:1 and reduced
    /// exactly.
    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if scale >= 1.0 {
            let mut pixels = self.draw(x, y, scale, width, height)?;
            self.draw_grid(&mut pixels, x, y, scale, scale, width, height);
            return Ok(pixels);
        }
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let span_x = ((f64::from(width) / scale).ceil() as i32).max(1);
        let span_y = ((f64::from(height) / scale).ceil() as i32).max(1);
        self.render_area(x0, y0, x0.saturating_add(span_x), y0.saturating_add(span_y), width, height)
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let w = u32::try_from(x1.saturating_sub(x0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let h = u32::try_from(y1.saturating_sub(y0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        // The reduction is the box mean of the 1:1 pixels (the kit contract), grid included; the
        // editor, which gives its canvas colour, draws the grid itself at the size shown.
        let mut full = self.draw(f64::from(x0), f64::from(y0), 1.0, w, h)?;
        self.draw_grid(&mut full, f64::from(x0), f64::from(y0), 1.0, 1.0, w, h);
        Ok(raster::render_area(Frame { rgba: &full, width: w, height: h }, 0, 0, w as i32, h as i32, width, height))
    }

    /// Runs `command`. An agent's command leaves the space shown, MSPACE, the views and the
    /// selection as they were, also when it fails, and picks by a point with a tolerance no zoom
    /// changes ([`context`]); only the commands whose work is moving the editor
    /// ([`context::moves_the_editor`]: `zoom*`, `pan*`, `view.set`, `layout.set`, `viewport.modelSpace`,
    /// `viewport.paperSpace`, `select`, `select.quick`, `edit.selectAll`, `edit.deselectAll`) keep their effect, and inside an
    /// MSPACE viewport an agent's zoom or pan, which the drawing would save, is refused. A drawing
    /// command ([`acts::takes_place`]) given `layer` or `layout` draws there and leaves the current
    /// layer and the space shown as they were, for anyone. A person's commands run as they always did.
    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        let params = place::resolve(&self.session, command, params)?;
        let params = params::resolve(self.session.doc().map_err(engine_error)?, command, params)?;
        if self.session.host_files_only && command == "block.insert" && params.get("file").is_some() {
            return Err("command disabled: the host opens files; insert takes an existing block name".into());
        }
        if !OPEN_PARAMETERS.contains(&command)
            && let Some(schema) = self.parameter_schemas().get(command)
        {
            kition_kit::schema::refuse_unknown(command, schema, &params)?;
        }
        if !self.is_agent() {
            return self.run_placed(command, params);
        }
        if context::moves_the_editor(command) {
            self.refuse_saved_navigation(command)?;
            let previous = self.session.pick_tolerance.replace(self.agent_tolerance());
            let result = self.run_command(command, params);
            self.session.pick_tolerance = previous;
            return result;
        }
        self.bracketed(|doc| doc.run_placed(command, params))
    }

    /// The editor's state besides the drawing (the space shown, MSPACE, the views, the
    /// selection), kept by the document and answered as `{snapshot, layout, selection}`
    /// ([`context`]).
    fn editor_state(&self) -> Value {
        self.take_editor_state()
    }

    /// Puts back what [`KitDocument::editor_state`] answered; no edit. A layout or entity the run
    /// deleted is dropped.
    fn restore_editor_state(&mut self, saved: &Value) -> Result<(), String> {
        self.put_back(saved)
    }

    fn fork_for_premises(&self) -> Result<Self, String> {
        let mut session = Session::empty();
        if let Some(replica) = self.session.kdm() {
            session.open_snapshot(&replica.snapshot().map_err(engine_error)?).map_err(engine_error)?;
            // The current properties are this person's own, not in the replica.
            session.carry_current(self.session.doc().map_err(engine_error)?).map_err(engine_error)?;
        } else {
            session.open_drawing(self.session.doc().map_err(engine_error)?.clone(), "premise", None);
        }
        session.host_files_only = self.session.host_files_only;
        session.last_used = self.session.last_used.clone();
        session.last_dim = self.session.last_dim;
        session.settings = self.session.settings.clone();
        session.clipboard = self.session.clipboard.clone();
        session.clipboard_base = self.session.clipboard_base;
        session.clipboard_source = self.session.clipboard_source.clone();
        session.last_point = self.session.last_point;
        session.last_command = self.session.last_command.clone();
        if let Some(state) = self.session.editor_state() {
            session.restore_editor_state(&state).map_err(engine_error)?;
        }
        let mut document = Self::from_session(session);
        document.unproven_record_ids = self.unproven_record_ids.clone();
        Ok(document)
    }

    fn unproven_identities(&self, objects: &[Value]) -> Result<Vec<Value>, String> {
        Ok(objects.iter().filter(|id| id.as_str().is_some_and(|id| self.unproven_record_ids.contains(id))).cloned().collect())
    }

    fn carry_replica_objects(&mut self, previous: &Self) -> Result<(), String> {
        self.unproven_record_ids.extend(previous.unproven_record_ids.iter().cloned());
        Ok(())
    }

    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        let bytes = self.save(Some("drawing.dxf"))?;
        let baseline = kdm_model::decode("drawing.dxf", &bytes).map_err(engine_error)?;
        self.session.reconcile_reread(&baseline, fresh.session.doc().map_err(engine_error)?).map_err(engine_error)?;
        self.unproven_record_ids = fresh.unproven_record_ids.clone();
        Ok(true)
    }

    /// Sets the space a call acts in (`part`: `{"layout": "<name>"}`, `Model` for model space) and
    /// the entities it acts on there (`on`: handles), as [`context`] describes; refuses what the
    /// drawing lacks and `time`. No edit.
    fn set_context(&mut self, context: &kition_kit::CallContext) -> Result<(), String> {
        self.set_call_context(context)
    }

    /// The drawing: every space and layer with its id, the current layer, and the editor's state
    /// ([`EDITOR_STATE`]).
    fn inspect(&self) -> Result<Value, String> {
        self.inspected()
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands: Vec<kition_kit::KitCommand> = {
            let mut commands: Vec<KitCommand> = kition_cad_engine::cmd::command_specs()
                .iter()
                .map(|spec| {
                    KitCommand {
                        id: spec.id.to_owned(),
                        title: spec.label.trim_end_matches('…').to_owned(),
                        menu: (!spec.menu.is_empty()).then(|| spec.menu.iter().map(|part| (*part).to_owned()).collect()),
                        shortcut: spec.shortcut.map(str::to_owned),
                        params: Some(spec.params.to_owned()),
                        mutates: changes_document(spec),
                        enabled: (spec.enabled)(&self.session).is_ok(),
                        ..Default::default()
                    }
                    .with_kit_schema()
                })
                .collect();
            let scope = self.session.undo_scope();
            for (id, title, shortcut, enabled) in [
                ("edit.undo", "Undo", "Cmd+Z", self.session.can_undo_in(scope)),
                ("edit.redo", "Redo", "Cmd+Shift+Z", self.session.can_redo_in(scope)),
            ] {
                commands.push(
                    KitCommand {
                        id: id.into(),
                        title: title.into(),
                        menu: Some(vec!["Edit".into()]),
                        shortcut: Some(shortcut.into()),
                        mutates: true,
                        enabled,
                        // The undo scope (KitModel M12a); without it, that of who runs the command.
                        params: Some(r#"{scope?: "person"|"agent"}"#.into()),
                        ..Default::default()
                    }
                    .with_kit_schema(),
                );
            }
            commands.push(
                KitCommand {
                    id: "command.runLine".into(),
                    title: "Command line".into(),
                    params: Some("{text: string}".into()),
                    mutates: true,
                    enabled: true,
                    ..Default::default()
                }
                .with_kit_schema(),
            );
            commands.push(
                KitCommand {
                    id: "file.export".into(),
                    title: "Export".into(),
                    params: Some(r#"{format: "pdf"|"svg"|"png"}"#.into()),
                    mutates: false,
                    enabled: true,
                    ..Default::default()
                }
                .with_kit_schema(),
            );
            commands.push(render::command());
            commands.push(command_line::command());
            commands.push(
                KitCommand {
                    id: "viewport.sync".into(),
                    title: "Synchronize viewport".into(),
                    params: Some("{x: number, y: number, zoom: number, width: number, height: number}".into()),
                    mutates: false,
                    enabled: true,
                    ..Default::default()
                }
                .with_kit_schema(),
            );
            commands
        };
        agent_text::apply(&mut commands);
        kition_kit::files::declare(&mut commands, files::FILES);
        if self.session.host_files_only {
            for c in commands.iter_mut().filter(|c| c.id == "block.insert") {
                if let Some(schema) = c.schema.as_mut().and_then(Value::as_object_mut) {
                    schema.remove("file");
                }
            }
        }
        for command in &mut commands {
            command.reads = acts::reads_any(&command.id);
            command.targets = acts::targets_any(&command.id);
            if acts::takes_place(&command.id)
                && let Some(Value::Object(schema)) = command.schema.as_mut()
            {
                schema.insert(
                    "layer".into(),
                    json!({ "type": "string", "description": "the layer the new objects go on, by name (default: the current layer); the drawing's current layer stays as it is" }),
                );
                schema.insert(
                    "layout".into(),
                    json!({ "type": "string", "description": "the space the new objects go in: \"Model\" or a layout's name (default: the space the call's part or the editor names); the space shown stays as it is" }),
                );
            }
        }
        params::declare(&mut commands);
        for command in &mut commands {
            place::declare(command);
            read_fields::declare(command);
        }
        commands
    }

    /// What is wrong with the drawing (KHP A21): see [`check`].
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        self.session.state().map(|state| check::check(&state.doc)).unwrap_or_default()
    }

    fn under(&self, command: &str, params: &Value) -> Result<Option<kition_kit::Under>, String> {
        place::under(&self.session, command, params, &self.commands())
    }

    fn covered(&self, command: &str, params: &Value, objects: &[kition_kit::CoveredAsk]) -> Result<Option<kition_kit::CoveredAt>, String> {
        place::covered(&self.session, command, params, objects)
    }

    fn content_at(&self, ask: &kition_kit::ContentAsk, _blobs: &dyn kition_kit::HistoricalBlobs) -> Result<Option<bool>, String> {
        if ask.premise.field != "kind.attribs" {
            return Ok(None);
        }
        let Some(h) = ask.premise.id.as_str().and_then(kition_cad_doc::Handle::parse_hex) else { return Ok(None) };
        let Some(kition_cad_doc::EntityKind::Insert(insert)) = self.session.doc().map_err(engine_error)?.entity(h).map(|e| &e.kind) else {
            return Ok(None);
        };
        let current = serde_json::to_value(&insert.attribs).map_err(engine_error)?;
        Ok(read_fields::attribute_projection(&ask.before, &ask.premise.region)
            .zip(read_fields::attribute_projection(&current, &ask.premise.region))
            .map(|(before, current)| before != current))
    }

    fn derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        place::derived(self.session.doc().map_err(engine_error)?, ids, names)
    }

    fn carried(&self, ids: &[Value], changes: &[Value]) -> Result<Vec<kition_kit::Carried>, String> {
        Ok(place::carried(self.session.doc().map_err(engine_error)?, ids, changes))
    }

    fn disabled_commands(&self) -> Vec<String> {
        let mut disabled: Vec<String> = kition_cad_engine::cmd::command_specs()
            .iter()
            .filter(|spec| (spec.enabled)(&self.session).is_err())
            .map(|spec| spec.id.to_owned())
            .collect();
        let scope = self.session.undo_scope();
        if !self.session.can_undo_in(scope) {
            disabled.push("edit.undo".into());
        }
        if !self.session.can_redo_in(scope) {
            disabled.push("edit.redo".into());
        }
        disabled
    }

    /// The drawing as DXF.
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        let state = self.session.state().map_err(engine_error)?;
        let target = name.unwrap_or("drawing.dxf");
        let extension = std::path::Path::new(target).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if extension != "dxf" {
            return Err(format!("saving as .{extension} needs the native app; save the drawing as .dxf"));
        }
        kition_cad_io::write(&state.doc, target).map_err(engine_error)
    }

    /// The drawing as PDF, SVG or PNG ([`EXPORTS`]), as `file.export` answers it.
    fn export(&self, format: &str, _options: &Value) -> Result<Vec<u8>, String> {
        self.export_bytes(format)
    }

    fn exports() -> Vec<&'static str> {
        EXPORTS.to_vec()
    }

    /// `document.text` (KitText 1.0): the drawing's words, a part for model space and each
    /// layout (see [`document_text`]). It changes nothing.
    fn text(&self, limit: usize) -> Result<kition_kit::text::DocumentText, String> {
        Ok(document_text::document_text(self.session.doc().map_err(engine_error)?, limit))
    }

    /// A host that owns every file (kition-headless) serves the drawing: no command reaches a
    /// file by path (KHP A29, engine contract E22).
    fn hosted(&mut self, hosting: &kition_kit::host::Hosting) {
        self.session.host_files_only |= hosting.files_only;
        // Hosted agents must observe repairs before taking a basis, rather than discover
        // unrelated repairs in their next write. Standalone opens retain audit diagnostics.
        if hosting.files_only {
            if let Ok(state) = self.session.state_mut() {
                let doc = std::sync::Arc::make_mut(&mut state.doc);
                kdm_model::repair_drawing(doc);
                doc.ensure_record_ids();
                state.saved = state.doc.clone();
            }
            self.session.sync_kdm();
        }
    }

    fn mark_saved(&mut self) {
        if self.dirty() {
            // The host just persisted our complete identity metadata. A clean reread
            // is marked saved too, but cannot vouch for an unproven outside rewrite.
            self.unproven_record_ids.clear();
        }
        if let Ok(state) = self.session.state_mut() {
            state.saved = state.doc.clone();
        }
    }
}

impl CadDocument {
    /// Runs `command` with the explicit `layer` and `layout` of a drawing command
    /// ([`acts::takes_place`], [`CadDocument::explicit`]), else as it is.
    fn run_placed(&mut self, command: &str, params: Value) -> Result<Value, String> {
        if acts::takes_place(command)
            && let Value::Object(mut fields) = params
        {
            let (layer, layout) = (fields.remove("layer"), fields.remove("layout"));
            let params = Value::Object(fields);
            if layer.is_none() && layout.is_none() {
                return self.run_command(command, params);
            }
            return self.explicit(layer.as_ref(), layout.as_ref(), |doc| doc.run_command(command, params));
        }
        self.run_command(command, params)
    }

    /// Runs `command` as the editor does.
    fn run_command(&mut self, command: &str, params: Value) -> Result<Value, String> {
        if command == "viewport.sync" {
            return self.sync_viewport(&params);
        }
        // A read of the engine's catalogue: nothing the person holds (a hot grip) ends.
        if command == command_line::LIST {
            return Ok(command_line::list(&params));
        }
        let before = self.current_view();
        let document_uid = self.session.state().ok().map(|state| state.uid);
        let transparent = kition_cad_engine::cmd::find_command(command).is_some_and(|spec| spec.transparent);
        if command != "command.runLine" && !transparent {
            self.hot_grip = None;
            self.forget_aim();
        }
        let result = match command {
            // The history as the engine keeps it; a running command ends first, as UNDO ends it.
            "edit.undo" => {
                if self.session.running.is_some() {
                    self.session.cancel();
                }
                let scope = kition_kit::session::history_scope(&params, &self.session.origin()).map_err(|e| e.to_string())?;
                self.session.undo_in(scope).map(|label| json!({ "undone": label })).map_err(engine_error)
            }
            "edit.redo" => {
                if self.session.running.is_some() {
                    self.session.cancel();
                }
                let scope = kition_kit::session::history_scope(&params, &self.session.origin()).map_err(|e| e.to_string())?;
                self.session.redo_in(scope).map(|label| json!({ "redone": label })).map_err(engine_error)
            }
            // The person's entry: what it and the commands it runs write goes on the history.
            "command.runLine" => self.echoed(|doc| doc.command_line(params.get("text").and_then(Value::as_str).unwrap_or_default())),
            "file.export" => self.export_command(&params),
            render::RENDER => self.render_view(&params),
            _ => self.session.execute(command, &params).map_err(engine_error),
        };
        // A successful close ends this kit document. The public SDK must not
        // save, inspect or restore editor state after this terminal result.
        if result.is_ok() && document_uid.is_some_and(|uid| !self.session.docs.iter().any(|state| state.uid == uid)) {
            return Ok(json!({"closed":true}));
        }
        if command != "command.runLine" && result.is_ok() {
            self.record_view(before);
            // A drafting mode or the view changed: the pointer resolves anew where it is.
            if self.hot_grip.is_some() || self.session.running.is_some() {
                self.resolve_cursor();
            }
        }
        result
    }

    /// The drawing as [`KitDocument::inspect`] answers it: every space (`layouts`, Model first,
    /// then the layouts in tab order, each with its entity count and the part that names it),
    /// every layer, block and named view with its object-model id, the current layer, the files it
    /// links when it links any (`links`, [`links::report`]), and the editor's state
    /// ([`EDITOR_STATE`]).
    fn inspected(&self) -> Result<Value, String> {
        let state = self.session.state().map_err(engine_error)?;
        let doc = &state.doc;
        let mut per_layer: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for entity in doc.model.iter().chain(doc.layouts.iter().flat_map(|l| l.entities.iter())) {
            *per_layer.entry(entity.common.layer.to_ascii_lowercase()).or_default() += 1;
        }
        let layers: Vec<Value> = doc
            .layers
            .iter()
            .map(|layer| {
                json!({
                    "id": format!("layer:{}", doc.record_id("layer", &layer.name, 0).unwrap_or(&layer.name)),
                    "name": layer.name,
                    "on": layer.on,
                    "frozen": layer.frozen,
                    "locked": layer.locked,
                    "entities": per_layer.get(&layer.name.to_ascii_lowercase()).copied().unwrap_or(0),
                })
            })
            .collect();
        let mut layouts: Vec<&kition_cad_doc::Layout> = doc.layouts.iter().collect();
        layouts.sort_by_key(|l| l.tab_order);
        let spaces: Vec<Value> = std::iter::once(json!({ "name": "Model", "part": { "layout": "Model" }, "entities": doc.model.len() }))
            .chain(layouts.iter().map(|l| {
                let viewports = l.entities.iter().filter(|e| matches!(e.kind, kition_cad_doc::EntityKind::Viewport(_))).count();
                json!({
                    "name": l.name,
                    "id": format!("layout:{}", doc.record_id("layout", &l.name, 0).unwrap_or(&l.name)),
                    "part": { "layout": l.name },
                    "entities": l.entities.len(),
                    "viewports": viewports,
                })
            }))
            .collect();
        let blocks: Vec<Value> = doc
            .blocks
            .iter()
            .filter(|(name, _)| !name.starts_with('*'))
            .map(|(name, block)| json!({ "id": format!("block:{}", doc.record_id("block", name, 0).unwrap_or(name)), "name": name, "entities": block.entities.len() }))
            .collect();
        let views: Vec<Value> = doc
            .views
            .iter()
            .map(|v| json!({ "id": format!("view:{}", doc.record_id("view", &v.name, 0).unwrap_or(&v.name)), "name": v.name, "center": [v.center.x, v.center.y], "width": v.width, "height": v.height }))
            .collect();
        let place = self.placement();
        let settings = &self.session.settings;
        let view = state.view();
        // The files the drawing links and whether the host lends them (KHP A42), when it links any.
        let links = links::report(doc);
        let mut inspected = json!({
            "title": state.title,
            "entities": doc.model.len(),
            "layouts": spaces,
            "layers": layers,
            "blocks": blocks,
            "views": views,
            "currentLayer": doc.header.str("CLAYER", "0"),
            // The editor's (EDITOR_STATE).
            "space": context::space_name(&state.space),
            "viewport": state.mspace.map(|h| h.hex()),
            "view": { "center": [view.center.x, view.center.y], "height": view.height },
            // The status bar's drafting modes; OSMODE's 16384 bit turns running object snaps off.
            "drafting": {
                "ortho": settings.orthomode,
                "polar": settings.polarmode,
                "osnap": settings.osmode != 0 && settings.osmode & 16384 == 0,
                "otrack": settings.otrack,
                "grid": settings.gridmode,
                "snap": settings.snapmode,
                "lwdisplay": settings.lwdisplay,
                // The grid the editor draws: GRIDUNIT (SNAPUNIT where it is 0) and GRIDMAJOR.
                "gridUnit": [
                    if settings.gridunit.x == 0.0 { settings.snapunit.x } else { settings.gridunit.x },
                    if settings.gridunit.y == 0.0 { settings.snapunit.y } else { settings.gridunit.y },
                ],
                "gridMajor": settings.gridmajor.max(1),
                // CURSORSIZE: the crosshair's arms, as a percentage of the canvas height.
                "cursorSize": settings.cursorsize,
                // The command line: the prompt taken apart and the history (command_line.rs).
                "commandLine": self.command_line_state(),
            },
            "selection": self.session.selection().len(),
            "selectedObjects": self.selected_objects(),
            "viewRequest": self.view_request,
            "prompt": self.prompt(),
            "running": self.session.running.as_ref().map(|running| running.id.clone()),
            // The canvas's drawing units per document pixel, and the document pixel of the
            // drawing's origin (where the editor's grid lines meet).
            "units": { "perPixel": 1.0 / place.k, "origin": [place.to_doc(Vec2::ZERO).0, place.to_doc(Vec2::ZERO).1] },
            // The person's (engine rule E14a).
            "canUndo": self.session.can_undo_in(kition_cad_engine::UndoScope::Person),
            "canRedo": self.session.can_redo_in(kition_cad_engine::UndoScope::Person),
        });
        if !links.is_empty() {
            inspected["links"] = Value::Array(links);
        }
        Ok(inspected)
    }
}

kition_kit::export_kit_document!(CadDocument as "CadDocument");
kition_kit::export_kit_tools!(CadDocument as "CadDocument");

/// The collaboration members (engine contract 2, [`collab`]) on the exported class. Shapes mirror
/// `kition-sdk`'s `session::web` (`export_kit_session!` does not apply here; see `collab`).
#[cfg(target_arch = "wasm32")]
pub mod web {
    use kition_kit::wasm_bindgen::prelude::*;
    use serde_json::json;

    use crate::__kition_kit_export::WebDocument;

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    /// An origin in the KDM's commit form, the JSON array `[kind, principal, label]`.
    fn origin_of(origin: &str) -> Result<kition_cad_engine::Origin, JsError> {
        kition_cad_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))
    }

    #[wasm_bindgen(js_class = "CadDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        pub fn version(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.version().map_err(error)
        }

        #[wasm_bindgen(js_name = exportChanges)]
        pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.export_changes(since).map_err(error)
        }

        #[wasm_bindgen(js_name = importChanges)]
        pub fn import_changes(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.0.import_changes(bytes).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        pub fn snapshot(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.snapshot().map_err(error)
        }

        pub fn undo(&mut self) -> Result<String, JsError> {
            self.0.undo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        pub fn redo(&mut self) -> Result<String, JsError> {
            self.0.redo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        #[wasm_bindgen(js_name = canUndo)]
        pub fn can_undo(&self) -> bool {
            self.0.can_undo()
        }

        #[wasm_bindgen(js_name = canRedo)]
        pub fn can_redo(&self) -> bool {
            self.0.can_redo()
        }

        /// `origin` is the KDM's commit form, the JSON array `[kind, principal, label]`.
        #[wasm_bindgen(js_name = executeAs)]
        pub fn execute_as(&mut self, command: &str, params: &str, origin: &str) -> Result<String, JsError> {
            let params = kition_kit::parse_params(params).map_err(error)?;
            let origin =
                kition_cad_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))?;
            self.0.execute_as(command, params, &origin).map(|value| value.to_string()).map_err(error)
        }

        /// Draws the drawing on the editor's canvas colour (`#rrggbb`) from now on, and leaves the
        /// grid to the editor (`kit.json` `extras`; kit-editor's `useEngineBackdrop` calls it).
        #[wasm_bindgen(js_name = setBackdrop)]
        pub fn set_backdrop(&mut self, color: &str) -> Result<(), JsError> {
            self.0.set_backdrop(color).map_err(error)
        }

        /// Every blob the replica references (a drawing references none), for a peer's `addBlobs`.
        pub fn blobs(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.blobs().map_err(error)
        }

        /// Takes a peer's blobs (from their `blobs()`).
        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }

        /// The blobs newly needed since `since`, encoded like `blobs()`.
        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = kdmResets)]
        pub fn kdm_resets(&self) -> u32 {
            self.0.kdm_resets()
        }

        /// A draft's changes taken in as one undo step, recorded with `origin` (`[kind, principal,
        /// label]`): `{"damage", "summary", "scope"}`. A refusal fails with the KDM's message.
        #[wasm_bindgen(js_name = mergeChanges)]
        pub fn merge_changes(&mut self, changes: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.merge_changes(changes, &origin).map(|answer| answer.to_string()).map_err(error)
        }

        /// What merging a draft's changes would change now, as `StateDiff` JSON; changes nothing.
        #[wasm_bindgen(js_name = compareChanges)]
        pub fn compare_changes(&self, changes: &[u8]) -> Result<String, JsError> {
            self.0.compare_changes(changes).map(|answer| answer.to_string()).map_err(error)
        }

        /// Writes the state a snapshot holds over the drawing as one change of `origin`; answers
        /// the damage JSON.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, snapshot: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.restore_state(snapshot, &origin).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        /// An agent's reply (JSON text) with its object ids named as the tool interface names them.
        #[wasm_bindgen(js_name = modelReply)]
        pub fn model_reply(&self, value: &str) -> Result<String, JsError> {
            self.0.model_reply(value).map_err(error)
        }

        /// What changed since the version `since` (a receipt): `{"changes": StateDiff}`, or
        /// `{"changes": null, "unavailable"}` when the replica's log does not hold it.
        #[wasm_bindgen(js_name = changesSince)]
        pub fn changes_since(&self, since: &[u8]) -> String {
            self.0.changes_since(since).to_string()
        }

        /// `model/describe` (KitObjectModel 1.0) as JSON text; a failure's message is the error's
        /// JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelDescribe)]
        pub fn model_describe(&mut self) -> Result<String, JsError> {
            self.0.model_describe().map(|answer| answer.to_string()).map_err(error)
        }

        /// `model/batch`: the request and the answer as JSON text, one undo step; a failure's
        /// message is the error's JSON.
        #[wasm_bindgen(js_name = modelBatch)]
        pub fn model_batch(&mut self, request: &str) -> Result<String, JsError> {
            self.0.model_batch(request).map(|answer| answer.to_string()).map_err(error)
        }

        /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
        /// capability); a replica that does not open is ignored and the file opens plainly.
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            crate::CadDocument::open_with_replica(name, bytes, snapshot, blobs).map(Self).map_err(error)
        }

        /// Whether the drawing has made its replica yet (it makes one on the first shared edit).
        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            self.0.has_replica()
        }

        /// The replica's own document id, for the Kit UI protocol's `replica/info` message.
        /// `undefined` before the drawing has made a replica.
        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> Option<String> {
            self.0.doc_id()
        }
    }
}

#[cfg(test)]
mod tests;
