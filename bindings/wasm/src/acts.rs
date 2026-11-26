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
//! with `layer.setCurrent` in an earlier step, or a drawing command takes an explicit `layer.open`
//! ([`takes_place`]).
use kition_kit::EditorState::{self, Active, Part, Selection};

const ALL: &[EditorState] = &[Active, Part, Selection];
const ACTIVE: &[EditorState] = &[Active];
const ACTIVE_AND_PART: &[EditorState] = &[Active, Part];
const ACTIVE_AND_SELECTION: &[EditorState] = &[Active, Selection];
const PART: &[EditorState] = &[Part];
const SELECTION: &[EditorState] = &[Selection];
const SELECTION_AND_PART: &[EditorState] = &[Part, Selection];

/// The editor state engine command `measure.point` reads besides its parameters. Derived from the code
/// (`tests/acts.rs`).
pub fn reads(id: &str) -> &'static [EditorState] {
    match id {
        "block.add"
        | "block.addDialog"
        | "centerMark.add"
        | "dimension.addArcLength"
        | "dimension.addDiameter"
        | "dimension.addRadius"
        | "point.divide"
        | "file.inspect"
        | "hatch.addGradient"
        | "hatch.add"
        | "block.insert"
        | "point.measure"
        | "object.offset"
        | "dimension.addQuick"
        | "spline.add" => ALL,
        "polyline.add3d"
        | "arc.add"
        | "arc.addContinue"
        | "arc.addCenterStartAngle"
        | "arc.addCenterStartEnd"
        | "arc.addCenterStartLength"
        | "arc.addStartCenterAngle"
        | "arc.addStartCenterEnd"
        | "arc.addStartCenterLength"
        | "arc.addStartEndAngle"
        | "arc.addStartEndDirection"
        | "arc.addStartEndRadius"
        | "attribute.define"
        | "object.blend"
        | "boundary.add"
        | "centerLine.add"
        | "circle.add"
        | "circle.addTwoPoints"
        | "circle.addThreePoints"
        | "circle.addCenterDiameter"
        | "circle.addTanTanRadius"
        | "circle.addTanTanTan"
        | "dimension.add"
        | "dimension.addAligned"
        | "dimension.addAngular"
        | "dimension.addBaseline"
        | "dimension.addContinue"
        | "dimension.addLinear"
        | "dimension.addOrdinate"
        | "donut.add"
        | "ellipse.add"
        | "ellipse.addArc"
        | "ellipse.addAxisEnd"
        | "variable.get"
        | "helix.add"
        | "leader.add"
        | "line.add"
        | "multileader.add"
        | "multiline.add"
        | "multilineText.add"
        | "viewport.add"
        | "edit.pasteAsBlock"
        | "polyline.add"
        | "point.add"
        | "point.addMultiple"
        | "polygon.add"
        | "ray.add"
        | "rectangle.add"
        | "revisionCloud.add"
        | "revisionCloud.addFreehand"
        | "revisionCloud.addPolygonal"
        | "variable.set"
        | "spline.addControlVertices"
        | "variable.list"
        | "table.add"
        | "text.add"
        | "layout.manage"
        | "viewport.new"
        | "viewport.one"
        | "viewport.two"
        | "viewport.three"
        | "viewport.four"
        | "wipeout.add"
        | "constructionLine.add" => ACTIVE_AND_PART,
        "dimension.updateStyle" | "layer.changeToCurrent" | "layer.freeze" | "layer.lock" | "layer.off" | "layer.unlock" | "object.properties" => {
            ACTIVE_AND_SELECTION
        }
        "dimensionStyle.open"
        | "dimensionStyle.setCurrent"
        | "dimensionStyle.delete"
        | "dimensionStyle.openManager"
        | "dimensionStyle.list"
        | "dimensionStyle.rename"
        | "layer.deleteWithObjects"
        | "layer.open"
        | "layer.delete"
        | "layer.set"
        | "layer.merge"
        | "layout.add"
        | "multileaderStyle.open"
        | "file.purge"
        | "file.status"
        | "textStyle.open"
        | "textStyle.delete"
        | "textStyle.list"
        | "textStyle.rename"
        | "namedObject.rename"
        | "tableStyle.open" => ACTIVE,
        "attribute.editGlobal"
        | "object.break"
        | "object.breakAtPoint"
        | "object.changeSpace"
        | "object.explode"
        | "object.extend"
        | "object.join"
        | "object.copyNested"
        | "object.deleteDuplicates"
        | "select.quick"
        | "select.quickInfo"
        | "select.set"
        | "object.trim"
        | "viewport.set"
        | "viewport.freezeLayer"
        | "view.zoomRealtime"
        | "view.zoomAll"
        | "view.zoomToFit"
        | "view.zoomIn"
        | "view.zoomObject"
        | "view.zoomOut"
        | "view.zoomPrevious"
        | "view.zoomWindow" => SELECTION_AND_PART,
        "object.chamfer"
        | "object.query"
        | "file.exportPdf"
        | "object.fillet"
        | "edit.find"
        | "layout.delete"
        | "layout.list"
        | "layout.rename"
        | "viewport.modelSpace"
        | "plot.pageSetup"
        | "edit.paste"
        | "edit.pasteToOriginalCoordinates"
        | "file.print"
        | "viewport.paperSpace"
        | "edit.selectAll"
        | "object.stretch" => PART,
        "dimension.reassociate"
        | "arrange.sendToBack"
        | "arrange.bringToFront"
        | "object.align"
        | "measure.area"
        | "object.array"
        | "object.arrayPath"
        | "object.arrayPolar"
        | "object.arrayRectangular"
        | "attribute.edit"
        | "attribute.sync"
        | "constraint.auto"
        | "constraint.bars"
        | "object.copy"
        | "edit.copyWithBasePoint"
        | "edit.copy"
        | "edit.cut"
        | "constraint.dynamicDimensions"
        | "constraint.delete"
        | "dimension.disassociate"
        | "dimension.override"
        | "dimension.space"
        | "dimensionStyle.override"
        | "dimension.editText"
        | "dimension.textAngle"
        | "dimension.textCenter"
        | "dimension.textHome"
        | "dimension.textLeft"
        | "dimension.textRight"
        | "object.delete"
        | "edit.delete"
        | "object.flatten"
        | "grip.mirror"
        | "grip.move"
        | "grip.rotate"
        | "grip.scale"
        | "grip.stretch"
        | "arrange.group"
        | "hatch.edit"
        | "layer.isolate"
        | "layer.match"
        | "layer.makeObjectCurrent"
        | "object.lengthen"
        | "object.list"
        | "measure.massProperties"
        | "measure.geometry"
        | "object.mirror"
        | "object.move"
        | "polyline.edit"
        | "plot.style"
        | "object.setProperties"
        | "measure.radius"
        | "region.add"
        | "object.reverse"
        | "object.rotate"
        | "object.scale"
        | "spline.edit"
        | "table.deleteColumn"
        | "table.deleteRow"
        | "table.insertColumn"
        | "table.insertRow"
        | "table.merge"
        | "table.set"
        | "table.unmerge"
        | "text.edit"
        | "arrange.ungroup"
        | "block.write" => SELECTION,
        // These setters replace the current property from their parameters; they never read its
        // previous value (props.rs and layer.rs). Selection fallback remains a separate read.
        "property.color" | "layer.setCurrent" | "layer.add" | "property.linetype" | "property.lineweight" | "textStyle.setCurrent" => &[],
        _ => &[],
    }
}

/// The parameter an agent's `on` fills for engine command `measure.point`: `handles` where the command reads
/// it (through `targets`, which falls back to the selection), `handle` where that is the one
/// handle it takes. Derived from the code (`tests/acts.rs`).
pub fn targets(id: &str) -> Option<&'static str> {
    match id {
        "measure.area"
        | "attribute.edit"
        | "attribute.sync"
        | "object.break"
        | "object.breakAtPoint"
        | "dimension.addArcLength"
        | "dimension.addDiameter"
        | "dimension.addRadius"
        | "point.divide"
        | "object.extend"
        | "object.lengthen"
        | "point.measure"
        | "object.copyNested"
        | "object.offset"
        | "measure.radius"
        | "revisionCloud.add"
        | "revisionCloud.addFreehand"
        | "revisionCloud.addPolygonal"
        | "spline.edit"
        | "text.edit"
        | "object.trim"
        | "wipeout.add"
        | "constructionLine.add" => Some("handle"),
        "attribute.editGlobal"
        | "block.add"
        | "object.align"
        | "object.array"
        | "object.arrayPath"
        | "object.arrayPolar"
        | "object.arrayRectangular"
        | "constraint.auto"
        | "block.addDialog"
        | "centerMark.add"
        | "object.changeSpace"
        | "object.copy"
        | "edit.copyWithBasePoint"
        | "edit.copy"
        | "edit.cut"
        | "dimension.disassociate"
        | "dimension.override"
        | "dimension.reassociate"
        | "dimension.space"
        | "dimensionStyle.override"
        | "dimension.updateStyle"
        | "dimension.editText"
        | "dimension.textAngle"
        | "dimension.textCenter"
        | "dimension.textHome"
        | "dimension.textLeft"
        | "dimension.textRight"
        | "arrange.sendToBack"
        | "arrange.bringToFront"
        | "object.delete"
        | "object.explode"
        | "object.flatten"
        | "hatch.addGradient"
        | "grip.mirror"
        | "grip.move"
        | "grip.rotate"
        | "grip.scale"
        | "grip.stretch"
        | "arrange.group"
        | "hatch.add"
        | "hatch.edit"
        | "object.join"
        | "layer.changeToCurrent"
        | "layer.freeze"
        | "layer.isolate"
        | "layer.lock"
        | "layer.match"
        | "layer.makeObjectCurrent"
        | "layer.off"
        | "layer.unlock"
        | "object.list"
        | "measure.massProperties"
        | "measure.geometry"
        | "object.mirror"
        | "object.move"
        | "object.deleteDuplicates"
        | "polyline.edit"
        | "plot.style"
        | "object.properties"
        | "object.setProperties"
        | "dimension.addQuick"
        | "region.add"
        | "object.reverse"
        | "object.rotate"
        | "object.scale"
        | "select.set"
        | "spline.add"
        | "arrange.ungroup"
        | "viewport.set"
        | "viewport.freezeLayer"
        | "block.write"
        | "view.zoomObject" => Some("handles"),
        _ => None,
    }
}

/// Whether engine command `measure.point` draws new objects with the current properties and takes an
/// explicit `layer.open` (and `layout.manage`) for them: the objects go on that layer and into that space,
/// and the drawing's current layer and the space shown stay as they were. Derived from the code
/// (`tests/acts.rs`): the commands that reach `current_common` or `add_entity` and have no `layer.open`
/// or `layout.manage` parameter of their own.
pub fn takes_place(id: &str) -> bool {
    matches!(
        id,
        "block.add"
            | "polyline.add3d"
            | "arc.add"
            | "arc.addContinue"
            | "arc.addCenterStartAngle"
            | "arc.addCenterStartEnd"
            | "arc.addCenterStartLength"
            | "arc.addStartCenterAngle"
            | "arc.addStartCenterEnd"
            | "arc.addStartCenterLength"
            | "arc.addStartEndAngle"
            | "arc.addStartEndDirection"
            | "arc.addStartEndRadius"
            | "attribute.define"
            | "object.blend"
            | "block.addDialog"
            | "boundary.add"
            | "centerLine.add"
            | "centerMark.add"
            | "circle.add"
            | "circle.addTwoPoints"
            | "circle.addThreePoints"
            | "circle.addCenterDiameter"
            | "circle.addTanTanRadius"
            | "circle.addTanTanTan"
            | "dimension.add"
            | "dimension.addAligned"
            | "dimension.addAngular"
            | "dimension.addArcLength"
            | "dimension.addBaseline"
            | "dimension.addContinue"
            | "dimension.addDiameter"
            | "dimension.addLinear"
            | "dimension.addOrdinate"
            | "dimension.addRadius"
            | "point.divide"
            | "donut.add"
            | "ellipse.add"
            | "ellipse.addArc"
            | "ellipse.addAxisEnd"
            | "hatch.addGradient"
            | "hatch.add"
            | "helix.add"
            | "block.insert"
            | "layout.manage"
            | "layout.add"
            | "leader.add"
            | "line.add"
            | "point.measure"
            | "multileader.add"
            | "multiline.add"
            | "multilineText.add"
            | "edit.pasteAsBlock"
            | "polyline.add"
            | "point.add"
            | "point.addMultiple"
            | "polygon.add"
            | "dimension.addQuick"
            | "ray.add"
            | "rectangle.add"
            | "revisionCloud.add"
            | "revisionCloud.addFreehand"
            | "revisionCloud.addPolygonal"
            | "spline.add"
            | "spline.addControlVertices"
            | "table.add"
            | "text.add"
            | "wipeout.add"
            | "constructionLine.add"
    )
}

/// The commands the binding adds (not engine commands): id, the editor state each reads, and the
/// parameter `on` fills. `command.runLine` runs any typed command, so it reads all of it.
pub const BINDING_COMMANDS: &[(&str, &[EditorState], Option<&str>)] = &[
    ("edit.undo", &[], None),
    ("edit.redo", &[], None),
    ("command.runLine", ALL, None),
    ("file.export", &[], None),
    ("file.render", &[], None),
    ("viewport.sync", &[], None),
    ("command.list", &[], None),
    ("drafting.setDynamicEntry", &[], None),
    ("grip.edit", SELECTION, None),
];

/// [`reads`] for any listed command: the binding's own from [`BINDING_COMMANDS`].
pub fn reads_any(id: &str) -> &'static [EditorState] {
    BINDING_COMMANDS.iter().find(|(command, ..)| *command == id).map_or_else(|| reads(id), |(_, state, _)| *state)
}

/// [`targets`] for any listed command.
pub fn targets_any(id: &str) -> Option<&'static str> {
    BINDING_COMMANDS.iter().find(|(command, ..)| *command == id).map_or_else(|| targets(id), |(_, _, target)| *target)
}

// Command families. Their ids share no prefix, so each family is listed or told by its last part.

/// The twelve geometric-constraint commands (`constraint.coincident` … `constraint.fix`).
pub fn geometric_constraint(id: &str) -> bool {
    matches!(
        id,
        "constraint.coincident"
            | "constraint.perpendicular"
            | "constraint.parallel"
            | "constraint.tangent"
            | "constraint.horizontal"
            | "constraint.vertical"
            | "constraint.collinear"
            | "constraint.concentric"
            | "constraint.smooth"
            | "constraint.symmetric"
            | "constraint.equal"
            | "constraint.fix"
    )
}

/// The dimensional-constraint commands (`constraint.dimensionLinear`, `constraint.dimensionAligned`, …).
pub fn dimensional_constraint(id: &str) -> bool {
    id.starts_with("constraint.dimension")
}

/// The commands that show or hide constraint bars.
pub fn constraint_bars(id: &str) -> bool {
    matches!(id, "constraint.bars" | "constraint.showAllBars" | "constraint.hideAllBars")
}

/// The commands that show or hide dynamic dimensional constraints.
pub fn dynamic_dimensions(id: &str) -> bool {
    matches!(id, "constraint.dynamicDimensions" | "constraint.showAllDynamicDimensions" | "constraint.hideAllDynamicDimensions")
}

/// The bar and dynamic-dimension commands that act on every object without being given any.
pub fn shows_or_hides_all(id: &str) -> bool {
    matches!(id, "constraint.showAllBars" | "constraint.hideAllBars" | "constraint.showAllDynamicDimensions" | "constraint.hideAllDynamicDimensions")
}

/// The mode of a dimension-text command (`dimension.textHome` gives `Home`), or `None` for any other command.
pub fn dimension_text_mode(id: &str) -> Option<&str> {
    id.strip_prefix("dimension.text").filter(|mode| !mode.is_empty())
}

/// The commands that edit an existing table (`table.add` draws a new one).
pub fn edits_a_table(id: &str) -> bool {
    id.starts_with("table.") && id != "table.add"
}

/// The zoom and pan commands.
pub fn zooms_or_pans(id: &str) -> bool {
    id.starts_with("view.zoom") || id.starts_with("view.pan")
}
