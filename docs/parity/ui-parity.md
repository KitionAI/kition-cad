# UI parity

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-26 · **Target:** Autodesk AutoCAD 2027 for Mac (26.0)

How Kition CAD's interaction compares with AutoCAD's: point entry, object snaps, tracking, dynamic
input, grips, selection, the command line, chrome and navigation. A drafter's speed in AutoCAD
comes from these, so "the command exists" isn't enough: it has to accept the same input, snap to
the same points and respond to the same keys. Work items: [gaps.md](../gaps.md).

**Headline (estimated): UI/UX fidelity ≈ 66%,** scored on what works end to end in the web editor
(`web/` with `bindings/wasm`), not on what the engine could do. The 2026-11-17 score (44%) counted
engine abilities the editor never reached (ortho, polar and tracking during commands, direct
distance, Space repeating); the hand-drafting work of 2026-11-26 wired them and added the
crosshair, hover highlight, coordinate readout, dynamic input, the command line's history,
options and completion, hot-grip modes and menus, menus by state, the Properties panel, in-place
text, the status bar's menus, layout tabs, an icon per command and the Chinese interface; the
second round (stages 8 and 9) added one undo step per grip placement, the Radius and Convert to
Line grip options, dynamic input at a hot grip, Select Similar, groups picked as one, an Insert
dialog with the block's ghost, more Properties writes (vertices, positions), the Drafting Settings
dialog, lineweight and transparency drawn, layout tab menus and clicked points in the history.
Still missing: TK tracking, selection cycling and the selection keywords beyond
All/Last/Previous, realtime zoom by dragging, grip dimensional input beside the object, isometric
drafting, palettes that dock, and the Options and named-view dialogs.

## How this is measured

- Kition CAD: read from the source (`crates/engine/engine/src/{prompt, snap, select, grips, lib,
  units}.rs`, `cmd/*`, this kit's editor `web/` and `bindings/wasm`) and, since 2026-11-26, run:
  each row the editor reaches has a browser test in `web/tests/` (named below), and a hand
  walkthrough of 61 findings was repeated against the built editor.
- AutoCAD: behaviour observed black-box in the running AutoCAD 2027 for Mac
  (`plan/autocad/01-observed-ui.md`, local only, with reference screenshots that are never
  committed), its menu tree (`tools/xtask/data/menu-catalog.txt`) and Autodesk's documentation.
- Weights (how often a drafter hits each area): point entry 15%, object snaps 15%, tracking and
  polar 10%, dynamic input 10%, grips 10%, selection 10%, command line 10%, chrome and palettes
  10%, navigation 5%, keyboard and context menus 5%.

| Area | Weight | Kition CAD | Weighted |
|---|---:|---:|---:|
| Point entry | 15% | 65% | 9.8 |
| Object snaps | 15% | 55% | 8.3 |
| Tracking, polar, ortho, grid/snap | 10% | 72% | 7.2 |
| Dynamic input | 10% | 64% | 6.4 |
| Grips | 10% | 72% | 7.2 |
| Selection | 10% | 56% | 5.6 |
| Command line | 10% | 80% | 8.0 |
| Chrome and palettes | 10% | 70% | 7.0 |
| Navigation | 5% | 65% | 3.3 |
| Keyboard shortcuts, context menus | 5% | 65% | 3.3 |
| **Total** | 100% | | **≈ 66%** |

## Point entry (≈ 65%)

| Input | AutoCAD | Kition CAD | Evidence |
|---|---|---|---|
| `x,y[,z]`, `@dx,dy`, `@d<a`, `d<a`, `#x,y`, bare `@` | yes | yes; `#` equals absolute because there is no UCS; Z parsed then discarded | `prompt.rs:147-180` |
| `*x,y` (WCS override) | yes | yes (`*x,y`, `*@dx,dy`, `@*dx,dy`); the same as the plain forms because there is no UCS | `prompt.rs` |
| Direct distance entry | yes, honours ortho, polar and tracking paths | yes: toward the cursor as the pipeline resolves it (ortho, polar, a tracking path); the editor writes every pointer's point into `Session.cursor` | `bindings/wasm/src/interaction.rs`; `web/tests/drafting.mjs` |
| Angle forms: `45d30'`, radians, grads, surveyor `N45dE` | yes | yes | `units.rs:141-197` |
| Architectural/engineering distances `5'6-1/2"` | yes | yes | `units.rs:93-140` |
| `FROM` (base point + offset) | yes | yes: "Base point:", "<Offset>:" with `@` measured from the base; nests with M2P and filters | `pointmod.rs` |
| `M2P` / `MTP` (midpoint between two points) | yes | yes | `pointmod.rs` |
| `TK` (tracking) | yes | **no** | |
| Point filters `.x`, `.y`, `.xy`, `.z` | yes | yes (`.x` … `.yz`, "(need YZ):"); Z is asked for and dropped (no 3D) | `pointmod.rs` |
| Angle override `<45` | yes | yes: `<a`, `<<a`, `<<<a`; a typed distance or a pick follows the angle, typed coordinates win | `pointmod.rs` |
| Object snap overrides (`END`, `MID`, `INT` … `NON`) | yes | yes, every mode and `NON`, with long names and `_`/`'` prefixes, for the next pick only; a typed point is snapped within the aperture; a command's own keyword wins (`_cen` always snaps) | `pointmod.rs` |
| Typed calculator `'CAL` | yes | no | |

## Object snaps (≈ 55%)

| Mode | AutoCAD | Kition CAD (`snap.rs:135-334`) |
|---|---|---|
| Endpoint | every curve, solid/3D vertices, dimension ends | lines, arcs, polyline segments, open ellipses and splines, ray base, solid/trace corners |
| Midpoint | every curve incl. ellipse/spline arcs, polyline segments | segments only; **none on circles, ellipses, splines** |
| Center | arcs, circles, ellipses, polyarcs | arcs, circles, ellipses |
| Geometric center | any closed polyline, spline | closed LWPOLYLINE only |
| Node | points, dimension definition points, text alignment points | POINT only |
| Quadrant | circles, arcs, ellipses | yes |
| Intersection | every pair, extended intersection | among up to 200 nearby primitives; ellipse/spline pairs approximate ([geometry-parity.md](geometry-parity.md)) |
| Extension | yes, with dashed extension path | yes: rest on a line or arc end, then the line runs on (a ray) and the arc round its circle; dotted path and "Extension: d < a°" (`snap/tracking.rs`); not from ellipse, spline or polyline-arc ends |
| Insertion | text, mtext, blocks, attributes, attdefs | TEXT, MTEXT, INSERT; no ATTDEF or MLEADER |
| Perpendicular | every curve, deferred | lines, arcs, circles, xlines; deferred for the first point |
| Tangent | circles, arcs, ellipses, splines, deferred | arcs and circles; deferred for the first point; **no ellipse or spline** |
| Nearest | every curve | yes |
| Apparent intersection | projected intersection | yes in 2D: where objects near the cursor would meet if extended; an object rested on sends its whole extension, so its crossing with another object snaps too (`snap.rs` `osnap_with`, `snap/tracking.rs`) |
| Parallel | yes, with parallel path | yes: rest on a line, then a path parallel to it runs through the base point (`snap/tracking.rs`) |
| Temporary overrides typed at a prompt (`END`, `MID`, `INT` …) | yes | yes (see Point entry) |
| AutoSnap marker, tooltip, magnet, aperture box | yes | one marker and the snap's name (`bindings/wasm`); aperture from APERTURE; no magnet, no aperture box; the crosshair jumps to the snapped point |
| Snap menu (Shift+right-click), running modes from the status bar | yes | yes: Shift+right-click lists END … NEA and None (types the override); the arrow beside Osnap checks the running modes (OSMODE) (`web/src/CadCanvas.tsx`, `web/src/StatusBar.tsx`; `drafting.mjs`, `status-bar.mjs`) |
| OSNAPZ, 3D object snaps | yes | no |

## Tracking, polar, ortho, grid and snap (≈ 70%)

Before 2026-11-26 these worked in the engine and on a hot grip only; the editor's commands now run
the same cursor pipeline (object snap, grid snap, tracking paths, ortho) for every point prompt
(`bindings/wasm/src/interaction.rs`; `web/tests/drafting.mjs`, `polar.mjs`).

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Ortho (F8) | yes | yes, in commands and on hot grips (`snap/tracking.rs`); holding Shift does not invert it (no modifier event reaches the engine without a pointer move) |
| Polar tracking increment | yes | yes, measured from ANGBASE (`snap/tracking.rs`); the arrow beside Polar sets 90, 45, 30, 22.5, 18, 15, 10 or 5° (`status-bar.mjs`) |
| Additional polar angles, relative to last segment | yes | additional angles yes (POLARADDANG with POLARMODE 4); **relative to the last segment no** |
| Polar distance snap (PolarSnap) | yes | **no** |
| **Object snap tracking** (acquire points, alignment paths, intersections of paths) | yes, F11 | yes: resting on a snap point acquires it (`+`), resting again releases it, seven at most; horizontal/vertical paths, or every polar angle with POLARMODE 2; Shift to acquire with POLARMODE 8; crossings of two paths, a path and an object, and a path and a polar path snap; tooltip names each path; a typed distance (direct distance entry) runs along the path the cursor is on, measured from the point the path comes from. Engine: `Session::snap_cursor`, `snap/tracking.rs`; AUTOSNAP bits 8 and 16. In the editor: dashed paths, a `+` per acquired point, an `×` where paths cross, "Polar: d < a°"; the canvas re-sends a pointer that rests 400 ms (`drafting.mjs`) |
| Grid (rectangular, major lines, adaptive, limits) | yes | rectangular over the whole canvas, five times coarser until lines are 8 px apart, a major line every GRIDMAJOR (`web/src/cursor.ts`; `cursor.mjs`); not limited to LIMITS |
| Snap (rectangular, isometric, polar snap) | yes | rectangular from 0,0 only (`grid_snap` in `snap.rs`), in commands since 2026-11-26 |
| Isometric drafting (ISODRAFT, isoplanes F5) | yes | **no** |
| Drafting Settings dialog | yes | yes: the status bar's gear and Tools › Drafting Settings... open Snap and Grid, Polar Tracking, Object Snap and Dynamic Input pages that write through the same transparent commands, in a command too (`web/src/DraftingSettings.tsx`; `shell.mjs`); no additional polar angles, tracking settings or Dynamic Input formats |

## Dynamic input (≈ 60%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Pointer input (X, Y fields at the first point) | yes | yes, live grey values beside the cursor (`web/src/DynamicTooltip.tsx`, `web/src/dynamicInput.ts`) |
| Dimensional input (distance and angle at later points), relative by default | yes | yes: `@d<a` from the last point; typing one number and Enter is direct distance |
| Tab, `,` and `<` lock a field (lock icon); a locked value holds the rubber band | yes | yes (`drafting.setDynamicEntry`, held in the cursor pipeline) |
| A prompt's options under Down Arrow | yes | yes |
| One field for a radius, distance or angle prompt | yes | yes |
| F12 and the status bar toggle | yes | yes (`DynInput`) |
| Typed in the command line after clicking into it | goes to the fields | sent as typed (absolute coordinates), as existing tests type them |
| Grip dimensional input, DYNPIFORMAT/DYNPICOORDS/DYNPROMPT, angle field on an arc | yes | no |

Tests: `web/tests/dynamic-input.mjs`.

## Grips (≈ 65%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Stretch grips per object type | yes | lines (midpoint moves), arcs (start/mid/end), ellipses (axes), polylines (vertices; segment midpoints move or reshape the bulge), dimensions (text and definition points), MLEADER, viewport, wipeout, spline (`grips.rs:79-270`) |
| Text/MText grips | position, width, column grips | move the whole object |
| Hot-grip modes with Space/Enter cycling | Stretch, Move, Rotate, Scale, Mirror | yes: Space, Enter or a quick right-click cycles them, a mode's name typed picks it, each previews (`bindings/wasm/src/grip_edit.rs`; `grips.mjs`) |
| Hot-grip options: Base point, Copy, Undo, Reference, eXit | yes | yes, clickable on the command line; Ctrl (or Cmd) held while placing places a copy; one undo step per placement, several hot grips together too (`handling.mjs`) |
| Several hot grips at once (Shift-click) | yes | yes, stretched by one offset |
| Multifunctional grip hover menus (Add Vertex, Remove Vertex, Convert to Arc, Lengthen …) | yes | right-click a grip: Stretch, Lengthen (line and arc ends), Add Vertex, Remove Vertex (polylines), Radius (an arc's midpoint), Convert to Line (a polyline arc segment's midpoint), Move, Rotate, Scale, Mirror; no Convert to Arc and no Lengthen at a polyline's end (an engine test pins those grips' options) |
| Grip dimensional input (length/angle fields) | yes | a hot grip has the dynamic input boxes beside the cursor (distance and angle, an angle in ROTATE, a factor in SCALE; `handling.mjs`); no fields beside the object |
| Dynamic block grips | yes | no (no dynamic blocks) |
| Grip appearance (GRIPSIZE, colours, hover) | yes | fixed size; hover turns a grip warm and a hot grip red; grips on locked layers are drawn but take no click; dragging from a grip no longer opens a selection window |

## Selection (≈ 50%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Pick, implied window (left→right) and crossing (right→left) | yes | yes: a blue solid window and a green dashed crossing, both lightly filled; the pick box is PICKBOX (`bindings/wasm`; `cursor.mjs`) |
| Keywords W, C, WP, CP, F, BOX, AU, SI, M, G (group), CL (class) | yes | **All, Last, Previous, Add, Remove only** (`machines.rs`); fence only as a JSON parameter |
| Press-drag selection windows (PICKDRAG) | yes | yes (PICKDRAG = 2) |
| Lasso | yes | **no** |
| Selection cycling (SELECTIONCYCLING) | yes | toggle nothing reads |
| Selection preview, hover highlight | yes | yes: the object under the pick box is drawn dashed when idle and at Select objects (`Session::hover`); not TRIM/EXTEND's part to be cut |
| PICKADD, PICKFIRST, PICKAUTO | yes | PICKADD (Shift), PICKFIRST |
| QSELECT, SELECTSIMILAR, FILTER | yes | QSELECT command; Select Similar on the drawing menu (same type and layer; `handling.mjs`); no FILTER |
| Object isolate/hide (ISOLATEOBJECTS, HIDEOBJECTS) | yes | **no** (only layer isolate) |
| Groups selectable as one (PICKSTYLE) | yes | yes: PICKSTYLE 1 by default, a click on a member selects the group and the highlight shows it (`handling.mjs`) |

## Command line (≈ 75%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Aliases | about 250 in `acad.pgp` | 144 aliases across the commands (counted from source) |
| History (F2), Up/Down recall | yes | three lines above the input (drag its top edge for up to twelve), F2 opens the scrollable history; Up/Down recall; DIST, LIST, ID and AREA results and *Cancel* show there; points clicked on the drawing are written beside their prompts (`command-line.mjs`, `shell.mjs`) |
| Transparent commands (`'zoom`, `'pan`, `'osnap`) | yes | yes (`lib.rs`); the command menu's Pan ▸ and Zoom ▸ and the status bar's toggles and menus keep a command running |
| Enter/Space repeats last command | yes | yes: Space is Enter in commands and between them, Enter repeats with objects selected, a quick right-click is Enter (`drafting.mjs`) |
| UNDO with Mark, Back, BEgin, End, Auto, Control | yes | count only, whole-document snapshots capped at 2,000 (`lib.rs`, `edit.rs`) |
| SETVAR for every system variable | ~1,000 sysvars | about 60 session/header variables by name (`sysvars.rs`) |
| Scripts (SCRIPT, `.scr`) | yes | no: agents send JSON command steps through the headless kit's `cad_run`; no `.scr` file command |
| Floating/docked command window, transparency, lines of history | yes | a docked input with the prompt on the line above it: options `[Close/Undo]` and the default `<…>` are buttons, the placeholder says what the prompt waits for; command-line errors go to the history and shake the input; up to eight completions by name and alias (Tab completes, Enter runs the first); no transparency setting; the history window's menu has Copy and Copy History only |

## Chrome and palettes (≈ 60%)

| Feature | AutoCAD for Mac | Kition CAD |
|---|---|---|
| Menu bar | 11 menus, 491 leaf items | ribbon tabs for 9 of the engine's menus (`MENU_TABS` in `web/src/cad.ts`); 233/491 items have an engine command (`cargo xtask parity`: see [parity-checklist.md](parity-checklist.md)); a button of a command with prompts starts them on the command line; each command has its own icon and keeps its label when the ribbon is short of room (a group's More, `ribbon.mjs`) |
| Tool Sets palette (Drafting, Modeling, … with flyouts) | yes | the Home ribbon tab's Draw, Modify, Annotation, Layers and Utilities groups, Classic (always open) by default (`web/src/ribbon/HomeTab.tsx`) |
| Insert dialog, Blocks palette | yes | an Insert dialog (Home › Insert, typed INSERT): the block list, one scale, a rotation, the insertion point on-screen with the block's ghost or typed (`web/src/InsertDialog.tsx`; `shell.mjs`); no preview image, no Blocks palette; Insert › Block... still opens the shared parameter dialog |
| Properties Inspector, Quick Properties | yes | a Properties panel from `object.describeProperties`: General (colour, layer, linetype, linetype scale, lineweight, transparency) and each kind's geometry, text or pattern, rounded to LUPREC, *Varies* for mixed values, arithmetic and units typed, the current properties when nothing is selected (`web/src/PropertiesPanel.tsx`; `properties.mjs`); a polyline's vertices one by one, the positions of an ellipse, a point, a paragraph and a block reference (`shell.mjs`); no Quick Properties |
| Layer Properties Manager (filters, states, VP overrides) | yes | a Layers list under the Properties panel: new, make current, on/off, freeze, lock, colour, linetype (loaded from the library on choosing), lineweight, rename by double-click, delete with the reason when a layer cannot go (`web/src/LayersPanel.tsx`); no filters, states or viewport overrides in the panel |
| Status bar toggles | ~25 | Grid, Snap, Ortho, Polar, Osnap, Otrack, Lineweight, Transparency, DynInput, each with its key in the tooltip (Lineweight and Transparency drawn on the canvas); Osnap's and Polar's settings menus; the gear (Drafting Settings); the coordinate readout; Zoom Extents (`web/src/StatusBar.tsx`; `status-bar.mjs`, `shell.mjs`); no isometric, quick properties or annotation scale (no CANNOSCALE) |
| Model and layout tabs | yes | Model and each layout at the status bar's left, choosing one switches (`layout.set`; `layouts.mjs`); "+" adds a layout, a tab's menu has New, Rename (in place), Copy and Delete (`shell.mjs`); switching ends a running command; no overflow list |
| Navigation bar, SteeringWheels | yes | no |
| Workspaces | Drafting & Annotation, 3D Modeling | no |
| Options / Preferences dialog | many tabs | **no** |
| Customisation (CUI, aliases editor, tool palettes) | yes | no |
| Themes | Dark, Light | the host's theme (`@kition/sdk/editor` theme tokens); model space is drawn on the host's canvas colour with colour 7 black or white to match |
| Accessibility | VoiceOver basics | ARIA labels on the editor's controls; screen-reader coverage untested |

## Navigation (≈ 65%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| ZOOM Extents, All, Window, Previous, In/Out, Center, Scale, Object | yes | yes, 50-entry history (`cmd/view.rs:107-149`) |
| ZOOM Realtime (drag), Dynamic | yes | **no** |
| Wheel zoom about cursor, ZOOMFACTOR, ZOOMWHEEL | yes | about the cursor, about 1.22 times a notch (no ZOOMFACTOR variable) |
| Middle-drag pan, double-middle-click extents | yes | yes (`web/src/CadCanvas.tsx`; `drafting.mjs`) |
| Trackpad two-finger pan, pinch zoom (macOS) | yes | yes: a two-finger scroll pans, a pinch zooms (`web/src/CadCanvas.tsx`) |
| Named views (VIEW) | yes | the VIEW table is read (`inspect()` lists `views`; `file.render` renders a named view); no Named Views dialog in the editor |
| Tiled model-space viewports (VPORTS) | yes | not confirmed: VPORTS maps to paper-space MVIEW |
| 3D orbit, ViewCube drag, SteeringWheels | yes | no |

## Keyboard and context menus (≈ 65%)

- Function keys: the engine's commands have F3 and F7–F12 (`cmd/settings.rs`); pressed in the
  command line, they run their command (`web/src/CommandLine.tsx`). F4 (3D osnap), F5
  (isoplane), F6 (dynamic UCS) are missing.
- Delete or Backspace erases the selection when no command runs; Escape cancels
  (`bindings/wasm`).
- Space is Enter (it does not pan); Enter with objects selected repeats the last command.
- Right-click by state (`web/src/CadCanvas.tsx`; `drafting.mjs`, `grips.mjs`): a quick click in a
  command is Enter; held, the command menu (Enter, Cancel, the prompt's options, Snap Overrides ▸,
  Pan ▸, Zoom ▸); Shift+right-click the object snap menu; with objects selected Erase, Move, Copy
  Selection, Scale, Rotate, Mirror, Draw Order ▸, Group, Ungroup, the edit of their kind (Edit
  Text, Edit Hatch, Edit Polyline, Edit Dimension Text), Deselect All, Properties; with nothing
  selected Repeat, Recent Commands ▸, Undo, Redo, Paste, Select All, Pan, Zoom ▸; on a grip its
  menu; Select Similar with objects selected; the layout tabs' menu and the history window's Copy. Still missing: the command line's own menu, palettes, Quick Select.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
| 2026-11-26 | major | Re-scored on what works end to end in the web editor (≈ 62%) after the hand-drafting work: the cursor pipeline in commands, crosshair and hover, command line, dynamic input, hot-grip modes and menus, menus by state, Properties and Layers panels, in-place text, status bar menus, layout tabs, ribbon icons and labels; stale rows and line references fixed |
| 2026-11-26 | minor | Second round, stages 8 and 9 (≈ 66%): one undo step per grip placement, Radius and Convert to Line on the grip menu, dynamic input at a hot grip, Select Similar, groups picked as one, a block reference and a dimension picked by what they draw; the Insert dialog, more Properties writes, Transparency and Lineweight drawn, Drafting Settings, layout tab menus, clicked points and Copy in the history |
