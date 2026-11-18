//! What each command acts on and the editor state it reads besides its parameters
//! (`KitCommand::targets`, `KitCommand::reads`), for kition-sdk's explicit call context: a host
//! sets the current space and the selection a command reads from a run step's `at.part` and `on`,
//! and puts them back after the call (task card C4/C5/C6-cad).
//!
//! Both are what the engine's code does, not what descriptions say: `tests/acts.rs` follows each
//! command's `run` and `enabled` code through the engine to the accessors of the selection (and
//! `targets`, which falls back to it when a command is given no handles), of the current space,
//! and of the current properties new objects take (`Active`: the current layer and styles, the
//! drawing's header variables), and to the helpers that read a parameter as a handle, and fails
//! when these tables say otherwise. No call context sets `Active`: a run sets the current layer
//! with `layer.current` in an earlier step, or a drawing command takes an explicit `layer`
//! ([`takes_place`]).
use kition_kit::EditorState::{self, Active, Part, Selection};

const ALL: &[EditorState] = &[Active, Part, Selection];
const ACTIVE: &[EditorState] = &[Active];
const ACTIVE_AND_PART: &[EditorState] = &[Active, Part];
const ACTIVE_AND_SELECTION: &[EditorState] = &[Active, Selection];
const PART: &[EditorState] = &[Part];
const SELECTION: &[EditorState] = &[Selection];
const SELECTION_AND_PART: &[EditorState] = &[Part, Selection];

/// The editor state engine command `id` reads besides its parameters. Derived from the code
/// (`tests/acts.rs`).
pub fn reads(id: &str) -> &'static [EditorState] {
    match id {
        "-block" | "block" | "centermark" | "dimarc" | "dimdiameter" | "dimradius" | "divide" | "drawing.inspect" | "gradient" | "hatch"
        | "insert" | "measure" | "offset" | "qdim" | "spline" => ALL,
        "3dpoly" | "arc" | "arc.continue" | "arc.csa" | "arc.cse" | "arc.csl" | "arc.sca" | "arc.sce" | "arc.scl" | "arc.sea" | "arc.sed"
        | "arc.ser" | "attdef" | "blend" | "boundary" | "centerline" | "circle" | "circle.2p" | "circle.3p" | "circle.cd" | "circle.ttr"
        | "circle.ttt" | "dim" | "dimaligned" | "dimangular" | "dimbaseline" | "dimcontinue" | "dimlinear" | "dimordinate" | "donut" | "ellipse"
        | "ellipse.arc" | "ellipse.axis" | "getvar" | "helix" | "leader" | "line" | "mleader" | "mline" | "mtext" | "mview" | "pasteblock"
        | "pline" | "point" | "point.multiple" | "polygon" | "ray" | "rectang" | "revcloud" | "revcloud.freehand" | "revcloud.polygonal"
        | "setvar" | "spline.cv" | "sysvars" | "table" | "text" | "layout" | "vports" | "vports.1" | "vports.2" | "vports.3" | "vports.4"
        | "wipeout" | "xline" => ACTIVE_AND_PART,
        "dimstyle.update" | "laycur" | "layfrz" | "laylck" | "layoff" | "layulk" | "properties" => ACTIVE_AND_SELECTION,
        "dimstyle" | "dimstyle.current" | "dimstyle.delete" | "dimstyle.dimension" | "dimstyle.list" | "dimstyle.rename" | "laydel" | "layer"
        | "layer.delete" | "layer.set" | "laymrg" | "layout.new" | "mleaderstyle" | "purge" | "status" | "style" | "style.delete" | "style.list"
        | "style.rename" | "rename" | "tablestyle" => ACTIVE,
        "-attedit" | "break" | "breakatpoint" | "chspace" | "explode" | "extend" | "join" | "ncopy" | "overkill" | "qselect" | "qselect.info"
        | "select" | "trim" | "viewport.set" | "vplayer" | "zoom" | "zoom.all" | "zoom.extents" | "zoom.in" | "zoom.object" | "zoom.out"
        | "zoom.previous" | "zoom.window" => SELECTION_AND_PART,
        "chamfer" | "entities" | "exportpdf" | "fillet" | "find" | "layout.delete" | "layout.list" | "layout.rename" | "mspace" | "pagesetup"
        | "pasteclip" | "pasteorig" | "plot" | "pspace" | "selectall" | "stretch" => PART,
        "dimreassociate" | "draworder.back" | "draworder.front" | "align" | "area" | "array" | "arraypath" | "arraypolar" | "arrayrect"
        | "attedit" | "attsync" | "autoconstrain" | "constraintbar" | "copy" | "copybase" | "copyclip" | "cutclip" | "dcdisplay"
        | "delconstraint" | "dimdisassociate" | "dimoverride" | "dimspace" | "dimstyle.override" | "dimtedit" | "dimtedit.angle"
        | "dimtedit.center" | "dimtedit.home" | "dimtedit.left" | "dimtedit.right" | "erase" | "erase.selection" | "flatten" | "grip.mirror"
        | "grip.move" | "grip.rotate" | "grip.scale" | "grip.stretch" | "group" | "hatchedit" | "layiso" | "laymch" | "laymcur" | "lengthen"
        | "list" | "massprop" | "measuregeom" | "mirror" | "move" | "pedit" | "plotstyle" | "properties.set" | "radius.measure" | "region"
        | "reverse" | "rotate" | "scale" | "splinedit" | "table.deletecol" | "table.deleterow" | "table.insertcol" | "table.insertrow"
        | "table.merge" | "table.set" | "table.unmerge" | "textedit" | "ungroup" | "wblock" => SELECTION,
        // These setters replace the current property from their parameters; they never read its
        // previous value (props.rs and layer.rs). Selection fallback remains a separate read.
        "color" | "layer.current" | "layer.new" | "linetype" | "lweight" | "style.current" => &[],
        _ => &[],
    }
}

/// The parameter an agent's `on` fills for engine command `id`: `handles` where the command reads
/// it (through `targets`, which falls back to the selection), `handle` where that is the one
/// handle it takes. Derived from the code (`tests/acts.rs`).
pub fn targets(id: &str) -> Option<&'static str> {
    match id {
        "area" | "attedit" | "attsync" | "break" | "breakatpoint" | "dimarc" | "dimdiameter" | "dimradius" | "divide" | "extend" | "lengthen"
        | "measure" | "ncopy" | "offset" | "radius.measure" | "revcloud" | "revcloud.freehand" | "revcloud.polygonal" | "splinedit" | "textedit"
        | "trim" | "wipeout" | "xline" => Some("handle"),
        "-attedit" | "-block" | "align" | "array" | "arraypath" | "arraypolar" | "arrayrect" | "autoconstrain" | "block" | "centermark"
        | "chspace" | "copy" | "copybase" | "copyclip" | "cutclip" | "dimdisassociate" | "dimoverride" | "dimreassociate" | "dimspace"
        | "dimstyle.override" | "dimstyle.update" | "dimtedit" | "dimtedit.angle" | "dimtedit.center" | "dimtedit.home" | "dimtedit.left"
        | "dimtedit.right" | "draworder.back" | "draworder.front" | "erase" | "explode" | "flatten" | "gradient" | "grip.mirror" | "grip.move"
        | "grip.rotate" | "grip.scale" | "grip.stretch" | "group" | "hatch" | "hatchedit" | "join" | "laycur" | "layfrz" | "layiso" | "laylck"
        | "laymch" | "laymcur" | "layoff" | "layulk" | "list" | "massprop" | "measuregeom" | "mirror" | "move" | "overkill" | "pedit"
        | "plotstyle" | "properties" | "properties.set" | "qdim" | "region" | "reverse" | "rotate" | "scale" | "select" | "spline" | "ungroup"
        | "viewport.set" | "vplayer" | "wblock" | "zoom.object" => Some("handles"),
        _ => None,
    }
}

/// Whether engine command `id` draws new objects with the current properties and takes an
/// explicit `layer` (and `layout`) for them: the objects go on that layer and into that space,
/// and the drawing's current layer and the space shown stay as they were. Derived from the code
/// (`tests/acts.rs`): the commands that reach `current_common` or `add_entity` and have no `layer`
/// or `layout` parameter of their own.
pub fn takes_place(id: &str) -> bool {
    matches!(
        id,
        "-block"
            | "3dpoly"
            | "arc"
            | "arc.continue"
            | "arc.csa"
            | "arc.cse"
            | "arc.csl"
            | "arc.sca"
            | "arc.sce"
            | "arc.scl"
            | "arc.sea"
            | "arc.sed"
            | "arc.ser"
            | "attdef"
            | "blend"
            | "block"
            | "boundary"
            | "centerline"
            | "centermark"
            | "circle"
            | "circle.2p"
            | "circle.3p"
            | "circle.cd"
            | "circle.ttr"
            | "circle.ttt"
            | "dim"
            | "dimaligned"
            | "dimangular"
            | "dimarc"
            | "dimbaseline"
            | "dimcontinue"
            | "dimdiameter"
            | "dimlinear"
            | "dimordinate"
            | "dimradius"
            | "divide"
            | "donut"
            | "ellipse"
            | "ellipse.arc"
            | "ellipse.axis"
            | "gradient"
            | "hatch"
            | "helix"
            | "insert"
            | "layout"
            | "layout.new"
            | "leader"
            | "line"
            | "measure"
            | "mleader"
            | "mline"
            | "mtext"
            | "pasteblock"
            | "pline"
            | "point"
            | "point.multiple"
            | "polygon"
            | "qdim"
            | "ray"
            | "rectang"
            | "revcloud"
            | "revcloud.freehand"
            | "revcloud.polygonal"
            | "spline"
            | "spline.cv"
            | "table"
            | "text"
            | "wipeout"
            | "xline"
    )
}

/// The commands the binding adds (not engine commands): id, the editor state each reads, and the
/// parameter `on` fills. `commandLine` runs any typed command, so it reads all of it.
pub const BINDING_COMMANDS: &[(&str, &[EditorState], Option<&str>)] = &[
    ("edit.undo", &[], None),
    ("edit.redo", &[], None),
    ("commandLine", ALL, None),
    ("file.export", &[], None),
    ("file.render", &[], None),
    ("viewport.sync", &[], None),
];

/// [`reads`] for any listed command: the binding's own from [`BINDING_COMMANDS`].
pub fn reads_any(id: &str) -> &'static [EditorState] {
    BINDING_COMMANDS.iter().find(|(command, ..)| *command == id).map_or_else(|| reads(id), |(_, state, _)| *state)
}

/// [`targets`] for any listed command.
pub fn targets_any(id: &str) -> Option<&'static str> {
    BINDING_COMMANDS.iter().find(|(command, ..)| *command == id).map_or_else(|| targets(id), |(_, _, target)| *target)
}
