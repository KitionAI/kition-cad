# UI parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Autodesk AutoCAD 2027 for Mac (26.0)

How Kition CAD's interaction compares with AutoCAD's: point entry, object snaps, tracking, dynamic
input, grips, selection, the command line, chrome and navigation. A drafter's speed in AutoCAD
comes from these, so "the command exists" isn't enough: it has to accept the same input, snap to
the same points and respond to the same keys. Work items: [gaps.md](../gaps.md).

**Headline (estimated): UI/UX fidelity ≈ 44%.** The command line, point syntax, the common
running snaps, polar/ortho and object snap tracking are real, and so are the Extension, Parallel
and Apparent Intersection snaps; the editor has no dynamic input; TK tracking doesn't exist; hot-grip options are advertised but ignored. Remaining: **≈ 110–170 Opus
5.5 hours**.

## How this is measured

- Kition CAD: read from the source, `crates/engine/engine/src/{prompt, snap, select, grips, lib,
  units}.rs`, `cmd/*`, this kit's editor (`web/`) and `bindings/wasm`, without running them.
- AutoCAD: behaviour observed black-box in the running AutoCAD 2027 for Mac
  (`plan/autocad/01-observed-ui.md`, local only, with reference screenshots that are never
  committed), its menu tree (`tools/xtask/data/menu-catalog.txt`) and Autodesk's documentation.
- Weights (how often a drafter hits each area): point entry 15%, object snaps 15%, tracking and
  polar 10%, dynamic input 10%, grips 10%, selection 10%, command line 10%, chrome and palettes
  10%, navigation 5%, keyboard and context menus 5%.

| Area | Weight | Kition CAD | Weighted |
|---|---:|---:|---:|
| Point entry | 15% | 45% | 6.8 |
| Object snaps | 15% | 45% | 6.8 |
| Tracking, polar, ortho, grid/snap | 10% | 70% | 7.0 |
| Dynamic input | 10% | 0% | 0.0 |
| Grips | 10% | 40% | 4.0 |
| Selection | 10% | 40% | 4.0 |
| Command line | 10% | 60% | 6.0 |
| Chrome and palettes | 10% | 45% | 4.5 |
| Navigation | 5% | 55% | 2.8 |
| Keyboard shortcuts, context menus | 5% | 25% | 1.3 |
| **Total** | 100% | | **≈ 44%** |

## Point entry (≈ 45%)

| Input | AutoCAD | Kition CAD | Evidence |
|---|---|---|---|
| `x,y[,z]`, `@dx,dy`, `@d<a`, `d<a`, `#x,y`, bare `@` | yes | yes; `#` equals absolute because there is no UCS; Z parsed then discarded | `prompt.rs:147-180` |
| `*x,y` (WCS override) | yes | yes (`*x,y`, `*@dx,dy`, `@*dx,dy`); the same as the plain forms because there is no UCS | `prompt.rs` |
| Direct distance entry | yes, honours ortho, polar and tracking paths | yes, ortho; polar only through the UI's snapped cursor | `lib.rs:782-793` |
| Angle forms: `45d30'`, radians, grads, surveyor `N45dE` | yes | yes | `units.rs:141-197` |
| Architectural/engineering distances `5'6-1/2"` | yes | yes | `units.rs:93-140` |
| `FROM` (base point + offset) | yes | yes: "Base point:", "<Offset>:" with `@` measured from the base; nests with M2P and filters | `pointmod.rs` |
| `M2P` / `MTP` (midpoint between two points) | yes | yes | `pointmod.rs` |
| `TK` (tracking) | yes | **no** | |
| Point filters `.x`, `.y`, `.xy`, `.z` | yes | yes (`.x` … `.yz`, "(need YZ):"); Z is asked for and dropped (no 3D) | `pointmod.rs` |
| Angle override `<45` | yes | yes: `<a`, `<<a`, `<<<a`; a typed distance or a pick follows the angle, typed coordinates win | `pointmod.rs` |
| Object snap overrides (`END`, `MID`, `INT` … `NON`) | yes | yes, every mode and `NON`, with long names and `_`/`'` prefixes, for the next pick only; a typed point is snapped within the aperture; a command's own keyword wins (`_cen` always snaps) | `pointmod.rs` |
| Typed calculator `'CAL` | yes | no | |

## Object snaps (≈ 45%)

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
| AutoSnap marker, tooltip, magnet, aperture box | yes | one marker and the snap's name (`bindings/wasm`); aperture from APERTURE; no magnet, no aperture box |
| OSNAPZ, 3D object snaps | yes | no |

## Tracking, polar, ortho, grid and snap (≈ 70%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Ortho (F8) | yes | yes (`snap.rs:503`) |
| Polar tracking increment | yes | yes, measured from ANGBASE (`snap/tracking.rs`) |
| Additional polar angles, relative to last segment | yes | additional angles yes (POLARADDANG with POLARMODE 4); **relative to the last segment no** |
| Polar distance snap (PolarSnap) | yes | **no** |
| **Object snap tracking** (acquire points, alignment paths, intersections of paths) | yes, F11 | yes: resting on a snap point acquires it (`+`), resting again releases it, seven at most; horizontal/vertical paths, or every polar angle with POLARMODE 2; Shift to acquire with POLARMODE 8; crossings of two paths, a path and an object, and a path and a polar path snap; tooltip names each path; a typed distance (direct distance entry) runs along the path the cursor is on, measured from the point the path comes from. Engine: `Session::snap_cursor`, `snap/tracking.rs`; AUTOSNAP bits 8 and 16 |
| Grid (rectangular, major lines, adaptive, limits) | yes | rectangular, coarser as the view zooms out; no major lines (`bindings/wasm`) |
| Snap (rectangular, isometric, polar snap) | yes | rectangular from 0,0 only (`snap.rs:531`) |
| Isometric drafting (ISODRAFT, isoplanes F5) | yes | **no** |
| Drafting Settings dialog | yes | no dialog; the `dsettings` command sets the same values |

## Dynamic input (0%)

AutoCAD's heads-up input (F12) has editable pointer-input fields (x, y), dimensional input
fields (length and angle) with Tab between them and lock icons, relative-by-default second
points, dimensional input on grips, a prompt with clickable options and a down-arrow menu.
The editor (`web/`) has no dynamic input: points are picked on the canvas or typed in the
command line.

## Grips (≈ 40%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Stretch grips per object type | yes | lines (midpoint moves), arcs (start/mid/end), ellipses (axes), polylines (vertices; segment midpoints move or reshape the bulge), dimensions (text and definition points), MLEADER, viewport, wipeout, spline (`grips.rs:79-270`) |
| Text/MText grips | position, width, column grips | move the whole object |
| Hot-grip modes with Space/Enter cycling | Stretch, Move, Rotate, Scale, Mirror | the engine has the modes (`grips.rs`); the editor's hot grip only stretches (`bindings/wasm`) |
| Hot-grip options: Base point, Copy, Undo, Reference, eXit | yes | **advertised in the prompt but not parsed**; the engine's JSON grip commands accept `copy`, the editor never passes it |
| Several hot grips at once (Shift-click) | yes | **no** (one handle) |
| Multifunctional grip hover menus (Add Vertex, Remove Vertex, Convert to Arc, Lengthen …) | yes | **no** |
| Grip dimensional input (length/angle fields) | yes | no |
| Dynamic block grips | yes | no (no dynamic blocks) |
| Grip appearance (GRIPSIZE, colours, hover) | yes | fixed size (`bindings/wasm`) |

## Selection (≈ 40%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Pick, implied window (left→right) and crossing (right→left) | yes | yes, coloured and dashed rectangles (`lib.rs:576-589`, `bindings/wasm`) |
| Keywords W, C, WP, CP, F, BOX, AU, SI, M, G (group), CL (class) | yes | **All, Last, Previous, Add, Remove only** (`lib.rs:595-599`, `machines.rs:77-80`); fence only as a JSON parameter |
| Press-drag selection windows (PICKDRAG) | yes | yes (PICKDRAG = 2) |
| Lasso | yes | **no** |
| Selection cycling (SELECTIONCYCLING) | yes | toggle nothing reads |
| Selection preview, hover highlight | yes | selection highlight only |
| PICKADD, PICKFIRST, PICKAUTO | yes | PICKADD (Shift), PICKFIRST |
| QSELECT, SELECTSIMILAR, FILTER | yes | QSELECT command; no SELECTSIMILAR or FILTER |
| Object isolate/hide (ISOLATEOBJECTS, HIDEOBJECTS) | yes | **no** (only layer isolate) |
| Groups selectable as one (PICKSTYLE) | yes | no |

## Command line (≈ 60%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| Aliases | about 250 in `acad.pgp` | 144 aliases across the commands (counted from source) |
| History (F2), Up/Down recall | yes | Up/Down recall (`web/src/CommandLine.tsx`); no F2 history |
| Transparent commands (`'zoom`, `'pan`, `'osnap`) | yes | yes (`lib.rs:496-503`) |
| Enter/Space repeats last command | yes | yes |
| UNDO with Mark, Back, BEgin, End, Auto, Control | yes | count only, whole-document snapshots capped at 2,000 (`lib.rs:477`, `edit.rs:56`) |
| SETVAR for every system variable | ~1,000 sysvars | about 60 session/header variables by name (`sysvars.rs`) |
| Scripts (SCRIPT, `.scr`) | yes | no: agents send JSON command steps through the headless kit's `cad_run`; no `.scr` file command |
| Floating/docked command window, transparency, lines of history | yes | one line with the engine's prompt (`web/src/CommandLine.tsx`); no history lines |

## Chrome and palettes (≈ 45%)

| Feature | AutoCAD for Mac | Kition CAD |
|---|---|---|
| Menu bar | 11 menus, 491 leaf items | ribbon tabs for 9 of the engine's menus (`MENU_TABS` in `web/src/cad.ts`); 233/491 items have an engine command (`cargo xtask parity`: see [parity-checklist.md](parity-checklist.md)) |
| Tool Sets palette (Drafting, Modeling, … with flyouts) | yes | the Home ribbon tab's Draw, Modify and Annotation groups (`web/src/ribbon/HomeTab.tsx`) |
| Properties Inspector, Quick Properties | yes | a Properties section: the selection's type and layer, and one object's centre and radius where it has them (`web/src/SelectionProperties.tsx`) |
| Layer Properties Manager (filters, states, VP overrides) | yes | a Layers list: new layer, make current, on/off, freeze, lock (`web/src/LayersPanel.tsx`) |
| Status bar toggles | ~25 | Grid, Snap, Ortho, Polar, Osnap, Otrack, Lineweight (`DRAFTING` in `web/src/cad.ts`) |
| Navigation bar, SteeringWheels | yes | no |
| Workspaces | Drafting & Annotation, 3D Modeling | no |
| Options / Preferences dialog | many tabs | **no** |
| Customisation (CUI, aliases editor, tool palettes) | yes | no |
| Themes | Dark, Light | the host's theme (`@kition/kit-editor` theme tokens) |
| Accessibility | VoiceOver basics | ARIA labels on the editor's controls; screen-reader coverage untested |

## Navigation (≈ 55%)

| Feature | AutoCAD | Kition CAD |
|---|---|---|
| ZOOM Extents, All, Window, Previous, In/Out, Center, Scale, Object | yes | yes, 50-entry history (`cmd/view.rs:107-149`) |
| ZOOM Realtime (drag), Dynamic | yes | **no** |
| Wheel zoom about cursor, ZOOMFACTOR, ZOOMWHEEL | yes | fixed factor (no ZOOMFACTOR) |
| Middle-drag pan, double-middle-click extents | yes | middle-drag pan (`@kition/kit-editor`'s canvas) |
| Trackpad two-finger pan, pinch zoom (macOS) | yes | a two-finger scroll zooms about the cursor like a wheel (`web/src/CadCanvas.tsx`) |
| Named views (VIEW) | yes | **no** (VIEW table not read or written) |
| Tiled model-space viewports (VPORTS) | yes | not confirmed: VPORTS maps to paper-space MVIEW |
| 3D orbit, ViewCube drag, SteeringWheels | yes | no |

## Keyboard and context menus (≈ 25%)

- Function keys: the engine's commands have F3 and F7–F12 (`cmd/settings.rs`); pressed in the
  command line, they run their command (`web/src/CommandLine.tsx`). F4 (3D osnap), F5
  (isoplane), F6 (dynamic UCS) are missing.
- Delete or Backspace erases the selection when no command runs; Escape cancels
  (`bindings/wasm`).
- Right-click on the canvas opens a shortcut menu: Repeat last command, the last five commands,
  Undo/Redo, Erase, Select All, Pan, Zoom Extents/In/Out (`web/src/CadCanvas.tsx`). Still
  missing: AutoCAD's per-command menus and the menus of the command line, palettes and tabs.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
