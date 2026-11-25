# Kition CAD architecture

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Autodesk AutoCAD 2027

How Kition CAD is built today. This describes the code on `main`, not plans; plans live in
[roadmap.md](roadmap.md). Agents: the rules that keep this shape (layering, never-crash,
everything-is-a-command, clean-room) are in [AGENTS.md](../AGENTS.md).

## Workspace

About 77,000 lines of Rust in 11 library crates, 1 app (`kition-cad`) and `xtask`.

| Layer | Crate | Lines | What it does |
|---|---|---:|---|
| L0 | `geom` | 2,909 | f64 2D/3D vectors, `Mat3`/`Mat4`, bounds, lines, arcs and bulges, circles, ellipses, (rational) B-splines with fit-point interpolation, line/arc intersections, tessellation, planar region tracing for HATCH/BOUNDARY (`region.rs`) |
| L0 | `dxf` | 571 | Standalone DXF tag reader/writer: ASCII and binary, tag-count limits for hostile input |
| L1 | `color` | 268 | AutoCAD Color Index table, true colour, ByLayer/ByBlock |
| L1 | `doc` | 4,291 | The drawing database: header, symbol tables (layers, linetypes, text/dimension/table/multileader styles), blocks, model space, layouts, constraints; copy-on-write `EntityStore`; our own hatch pattern and linetype library (`library.rs`) |
| L2 | `fonts` | 2,106 | Our single-stroke drafting font, TrueType/OpenType/`.ttc` outlines through `skrifa`, TEXT and MTEXT layout and formatting codes, per-character fallback and FONTALT |
| L2 | `render` | 4,286 | Display lists (polylines, triangles, points tagged with entity handles): linetypes, hatches, gradients, text, block expansion, dimension geometry, paper-space clipping; CPU rasteriser (`tiny-skia`) for PNG, plot preview and tests |
| L2 | `constraints` | 2,586 | Own parametric solver: damped Gauss–Newton / Levenberg–Marquardt (`lm.rs`), 12 geometric and the dimensional constraints, expressions and parameters, AutoConstrain inference |
| L3 | `io` | 7,649 | DXF ⇄ document mapping (`dxf_read.rs`, `dxf_write.rs`, Kition CAD xdata in `dxf_ext.rs`), PDF plotting, SVG and PNG export |
| L3 | `dwg` | 195 | DWG through a DXF bridge: the `acadrust` crate (MPL-2.0, unmodified) converts DWG ⇄ DXF bytes; nothing else sees its types. Native only |
| L4 | `kdm-model` | 3,351 | Kition's KDM bridge (Apache-2.0): the drawing as a `kit.cad` replica (entities as objects with minted ids, draw order per store, named tables in maps keyed by name, table cell texts as `TableCell` objects, issued record identities as a growing map; what each person draws with stays out of it: the current layer and styles, the dimension settings but `DIMSCALE`, the command defaults, drawing aids, LAYERP and LAYUNISO snapshots and the current UCS), the diff writer, the reader, the post-merge repair, `CadSession`; below `engine`, which writes every command through it |
| L4 | `engine` | 39,881 | `Session`, the command registry (`cmd/*`), command-line parser and prompt machines, object snaps and polar/ortho (`snap.rs`), selection (`select.rs`), grips (`grips.rs`), associativity (`assoc.rs`), system variables, units, undo, R-tree spatial index (`spatial.rs`), the command bridge to the drawing's replica (`kdm_bridge.rs`) |
| L5 | `kition-cad-kit` | 8,181 | Kition's in-page editor engine (Apache-2.0): the drawing as a `KitDocument`, the command prompts as `KitTools` |
| app | `kition-cad` | 12 | The headless kit executable for Kition's agent, on kition-sdk's `kition-headless` |
| tool | `xtask` | 1,002 | `ci`, `layers`, `assets`, `wasm`, `parity`, `version`, `stats` |

## Data model

- A `Drawing` is header variables, symbol tables, block definitions, model space and paper-space
  layouts. Entities carry a handle, layer, colour, linetype, lineweight, transparency and an
  `EntityKind`: Line, Point, Circle, Arc, Ellipse, LwPolyline, Polyline3d, Spline, Ray, XLine,
  Text, MText, AttDef, Insert (with attributes), Dimension (linear, aligned, angular, diameter,
  radius, ordinate, arc length), Leader, MLeader, Hatch (pattern, solid, gradient), Solid,
  Trace, Face3d, Viewport, Image, Wipeout, Table and Unknown (raw DXF tags).
- There are no 3D solids, surfaces, meshes, regions (ACIS), multilines, tolerance frames or
  underlays in the model yet ([gaps.md](gaps.md)).
- Entity collections are copy-on-write, so undo is a whole-drawing snapshot that costs little.
- Coordinates are f64 world units. 2D work happens in the WCS; there is no UCS yet.

## Commands

Every user-visible action is a `CommandSpec` (`crates/engine/engine/src/cmd/mod.rs`): an id that follows
the Kition command naming standard (`line.add`, `object.fillet`), the classic CAD name a person
types at the command line (`line`, `fillet`; `cmd/names.rs`), label, menu path, shortcut, aliases, a
JSON `run`, an `enabled` check and an optional interactive prompt machine. More than 300 engine commands are registered, many
with interactive prompts; [writing-format.md](writing-format.md) lists every one. The kit's editor (`web/`) and the headless
kit executable (`bindings/headless`) both reach the same commands through `KitTools`. JSON calls never
open dialogs.

## Rendering

`render::build` turns a space into a display list once per change; the editor's engine
(`bindings/wasm`) keeps it per revision and rasterises each area the editor asks for with the CPU
rasteriser (`render::raster`, `tiny-skia`). Tessellation tolerance follows the view. The R-tree
(`spatial.rs`) is patched per changed store chunk after an edit, so picking stays near 0.001 ms
at 200k entities. PNG, PDF, SVG and plot preview reuse the same display list through the CPU rasteriser or vector writers.

## Files

DXF is the native path: `dxf` parses tags, `io::dxf_read` maps them to the document, and
`io::dxf_write` writes an R2000 (AC1015) DXF. DWG is converted to DXF in memory by `acadrust` and
read through the same path; saving DWG writes our DXF and converts it back. Kition CAD-only data
(constraints, associativity details, table flags) travels in xdata under our own application
name. Detail and gaps: [file-format-parity.md](parity/file-format-parity.md).

## Agent control

Kition's agent drives the engine through `bindings/headless`, a headless executable built on
kition-sdk's `kition-headless`, over the same `KitTools` contract as the in-page editor
(`bindings/wasm`). There is no separate control channel, MCP server or CLI in this
repository.

## Safety

Production crates deny `unwrap`/`expect`/`panic!` and forbid `unsafe`. `Session::execute` and
interactive input run under `catch_unwind`, restoring the drawing if a panic escapes. Input-sized
allocations are capped (DXF tag counts, DWG size limits, hatch segment and island limits,
`MAX_BLOCK_DEPTH`).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
