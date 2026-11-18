# Gaps

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

Every known shortfall of Kition CAD, from the feature catalogue: features that are not yet `has`,
then the differences recorded on features that work. Update the feature in `features.yaml` when you
land work, then this list. The comparison with the target application is in
[parity/target-app-parity.md](parity/target-app-parity.md); the order of work is in [roadmap.md](roadmap.md).

## Features not done

- `cad.agent.environment` (P0, partial): Hosted repairs precede observation; typed owning-map snapshots and deterministic field, text and creation replay.
- `cad.agent.ids` (P0, partial): Typed entity handles, named identities preserved on rename and saved constraint ids; full resolver audit pending.
- `cad.agent.premises` (P0, partial): Public SDK premise planning, refreshed boundary facts and typed name gaps; conservative declarations remain. Some declarations remain conservative; nested union name/type roles remain unsupported.
- `cad.block.manage` (P1, partial): Blocks, attributes, insert, write block and purge. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.constraint.parametric` (P1, partial): Geometric and dimensional constraints, parameters and constraint display. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.dimension.create` (P1, partial): Linear, aligned, radius, diameter, angular, arc, ordinate, continue, baseline and quick dimensions. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.dimension.edit` (P1, partial): Dimension text, spacing, overrides and association. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.dimension.leaders` (P1, partial): Leaders, multileaders and multileader styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.dimension.styles` (P1, partial): Dimension styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.centers` (P1, partial): Center marks and center lines. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.curves` (P1, partial): Circles, arcs, ellipses, splines, donuts and helices. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.hatch` (P1, partial): Hatch, gradient fill, boundary and region. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.lines` (P1, partial): Lines, polylines, construction lines, rays, multilines and 3D polylines. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.points` (P1, partial): Points, divide and measure. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.shapes` (P1, partial): Rectangles, polygons, revision clouds and wipeouts. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.draw.text` (P1, partial): Single-line and multiline text, and text styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.drawing.settings` (P1, partial): Units, limits, drafting settings, object snaps and status-bar modes. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.edit.clipboard` (P1, partial): Undo, redo, clipboard, paste variants and erase. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.file.drawings` (P1, partial): New, open, save, close drawings, and audit or recover them. Drawings made with the core drawing, text, layer, block, dimension and modify commands save to DXF and reopen unchanged (the cad.*/dxf-save-reopen cases); other entities and tables are not yet proved, and the DWG path (native app) is proved only for the data its regression cases cover.
- `cad.layer.manage` (P1, partial): Layers, layer tools and layer states. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.layout.paper` (P1, partial): Layouts, viewports, model and paper space. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.modify.array` (P1, partial): Rectangular, polar and path arrays. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.modify.order-groups` (P1, partial): Draw order and groups. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.modify.shape` (P1, partial): Offset, trim, extend, fillet, chamfer, break, join, lengthen, blend and reverse. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.modify.transform` (P1, partial): Move, copy, rotate, scale, mirror, stretch and align. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.output.plot` (P1, partial): Page setup, plot, export to PDF and plot styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.properties.edit` (P1, partial): Properties, match properties, colour, linetype, lineweight and linetype scale. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.select.objects` (P1, partial): Select, select all, quick select, find, and grips. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.table.edit` (P1, partial): Tables and table styles. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.tools.inquiry` (P1, partial): Inquiry: distance, ID point, list, area, mass properties, time, status, count, calculator and system variables. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.view.navigation` (P1, partial): Zoom, pan, regenerate, redraw and saved views. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `cad.app.command-line` (P2, partial): Command line. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.

## Differences on working features

- `cad.file.export`: Model space only (a layout prints with plot or exportpdf in the editor); the PNG is 2400 x 1600 whatever the drawing's shape; DWG is not an export, since the web build cannot write it.

## Catalogue

The catalogue has 48 features: 16 `has`, 32 not yet.
