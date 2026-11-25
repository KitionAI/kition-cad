# Target-app parity

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

Each feature in `features.yaml` records its status against what the feature promises and, where it
has one, the target application it is compared with (AutoCAD 2027).
The difference column is the feature's recorded gap.

| Feature | Priority | Status | Target | Difference |
| --- | --- | --- | --- | --- |
| `cad.agent.act` An agent organizes objects on layers | P0 | has | — | — |
| `cad.agent.context` Explicit call context: steps name their space and entities (at.part, on), nothing carries over between calls, a failing command changes nothing | P0 | has | — | — |
| `cad.agent.environment` Hosted repairs precede observation; typed owning-map snapshots and deterministic field, text and creation replay | P0 | partial | — | — |
| `cad.agent.ids` Typed entity handles, named identities preserved on rename and saved constraint ids; full resolver audit pending | P0 | partial | — | — |
| `cad.agent.observe` An agent reads the geometry and dimensions it | P0 | has | — | — |
| `cad.agent.premises` Public SDK premise planning, refreshed boundary facts and typed name gaps; conservative declarations remain | P0 | partial | — | Some declarations remain conservative; nested union name/type roles remain unsupported. |
| `cad.agent.preview` A preview by explicit view (file.render, agent.render): a space, a window, an entity or a named view, pure and deterministic, with the window and units per pixel to read coordinates off it | P0 | has | — | — |
| `cad.agent.recover` An agent fixes what the check reports | P0 | has | — | — |
| `cad.block.define-insert` Define a block from objects and insert it | P0 | has | AutoCAD 2027 | — |
| `cad.dimension.basic` Linear, aligned and radius dimensions | P0 | has | AutoCAD 2027 | — |
| `cad.draw.basic-geometry` Lines, circles, arcs, closed polylines and rectangles | P0 | has | AutoCAD 2027 | — |
| `cad.draw.text-labels` Single-line and multiline text | P0 | has | AutoCAD 2027 | — |
| `cad.layer.basics` New layers with a colour, the current layer and frozen layers | P0 | has | AutoCAD 2027 | — |
| `cad.modify.transform-hatch` Move, rotate, scale, copy, mirror, polar array and hatch | P0 | has | AutoCAD 2027 | — |
| `cad.agent.draft` An agent works in a draft over a conversation | P1 | has | — | — |
| `cad.agent.measure` Measurements in one read-only command (entities.measure): bounds, minimum distance, intersections, duplicate objects | P1 | has | — | — |
| `cad.block.manage` Blocks, attributes, insert, write block and purge | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.collab.drafts` Drafts merge in and earlier versions restore as one undo step | P1 | partial | Kit UI protocol section 7 (replica/restore); engine contract K3a | Engine tests merge a draft (also one whose entity and the drawing's later one both append to model space), compare it first, restore a version, answer receipts, name reply ids and serve the object model; no case drives a draft or a restore in the editor yet. |
| `cad.constraint.parametric` Geometric and dimensional constraints, parameters and constraint display | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.dimension.create` Linear, aligned, radius, diameter, angular, arc, ordinate, continue, baseline and quick dimensions | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.dimension.edit` Dimension text, spacing, overrides and association | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.dimension.leaders` Leaders, multileaders and multileader styles | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.dimension.styles` Dimension styles | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.document.text` The drawing's words as plain text for search (document.text): TEXT, MTEXT, attribute values and dimension text overrides, model space then each layout | P1 | has | — | — |
| `cad.draw.centers` Center marks and center lines | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.draw.curves` Circles, arcs, ellipses, splines, donuts and helices | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.draw.hatch` Hatch, gradient fill, boundary and region | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.draw.lines` Lines, polylines, construction lines, rays, multilines and 3D polylines | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.draw.points` Points, divide and measure | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.draw.shapes` Rectangles, polygons, revision clouds and wipeouts | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.draw.text` Single-line and multiline text, and text styles | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.drawing.settings` Units, limits, drafting settings, object snaps and status-bar modes | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.edit.clipboard` Undo, redo, clipboard, paste variants and erase | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.editor.display-modes` The editor in each display mode: Classic gives way before stepping down, Focus shows only the drawing, letters pick tools and Escape cancels first in Focus | P1 | has | — | — |
| `cad.file.drawings` New, open, save, close drawings, and audit or recover them | P1 | partial | AutoCAD 2027 | Drawings made with the core drawing, text, layer, block, dimension and modify commands save to DXF and reopen unchanged (the cad.*/dxf-save-reopen cases); other entities and tables are not yet proved, and the DWG path (native app) is proved only for the data its regression cases cover. |
| `cad.file.export` Export the drawing as PDF, SVG or PNG, framed to its extents (file.export; the agent's cad_export) | P1 | has | — | Model space only (a layout prints with plot or exportpdf in the editor); the PNG is 2400 x 1600 whatever the drawing's shape; DWG is not an export, since the web build cannot write it. |
| `cad.layer.manage` Layers, layer tools and layer states | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.layout.paper` Layouts, viewports, model and paper space | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.modify.array` Rectangular, polar and path arrays | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.modify.order-groups` Draw order and groups | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.modify.shape` Offset, trim, extend, fillet, chamfer, break, join, lengthen, blend and reverse | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.modify.transform` Move, copy, rotate, scale, mirror, stretch and align | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.output.plot` Page setup, plot, export to PDF and plot styles | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.properties.edit` Properties, match properties, colour, linetype, lineweight and linetype scale | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.select.objects` Select, select all, quick select, find, and grips | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.table.edit` Tables and table styles | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.tools.inquiry` Inquiry: distance, ID point, list, area, mass properties, time, status, count, calculator and system variables | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.view.navigation` Zoom, pan, regenerate, redraw and saved views | P1 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `cad.app.command-line` Command line | P2 | partial | AutoCAD 2027 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |

| Priority | has | partial | missing |
| --- | ---: | ---: | ---: |
| P0 | 11 | 3 | 0 |
| P1 | 5 | 28 | 0 |
| P2 | 0 | 1 | 0 |
