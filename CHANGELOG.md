# Changelog

Changes to Kition CAD, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## 0.1.1

- `kit.json` declares the engine's editor state (`engine.editorState`), so a host that does not run the engine's Rust code reads `inspect()` without it.
- README: an agent connects with `npm install -g @kition/cli` and `kition agent add`.

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: a 2D drawing database with layers, linetypes, text and dimension styles,
  blocks with attributes, and model and paper space; lines, polylines, circles, arcs, ellipses,
  splines, text, dimensions, leaders, hatches and tables; move, copy, rotate, scale, mirror,
  arrays, object snaps and polar tracking; a parametric constraint solver; DXF read and write;
  and PDF, SVG and PNG export.
- The kit engine (`kition-cad-kit`), native and WebAssembly: the drawing is a KDM replica, every
  command writes through it, and replicas merge concurrent edits; undo is scoped to who made the
  change. A check catalogue reports what is wrong with a drawing.
- The headless kit (`kition-cad`) for agents over KHP: it inspects a drawing's
  layers and blocks, draws and edits with the editor's own commands, previews a space or a
  window, and works in drafts. A write names the revision it read and is refused when someone
  else changed what it rests on. It reads nothing outside its workspace.
- The kit's editor (`web/`): the Home ribbon, the drawing canvas with the engine's tools, the
  layers panel, the command line and the drafting modes, in kit-editor's frame.
- NOTICE and docs/attribution.md list the upstream work, every asset, and Loro (MIT), bundled by
  the editor and linked by the engine.
