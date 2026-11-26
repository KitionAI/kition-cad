# Roadmap

> **Last updated:** 2026-11-26 · **Source:** `features.yaml` (updated 2026-11-26) and the engine's command list

Open work on Kition CAD, in order. Each item names where its detail lives.

1. **Prove the P0 features end to end:** `cad.agent.environment`, `cad.agent.ids`, `cad.agent.premises`: each needs a case that shows it working in the editor and surviving save and reopen before it is `has`. Detail in [gaps.md](gaps.md).
2. **Then the P1 features:** `cad.block.manage`, `cad.collab.drafts`, `cad.constraint.parametric`, `cad.dimension.create`, `cad.dimension.edit`, `cad.dimension.leaders`, `cad.dimension.styles`, `cad.draw.centers`, `cad.draw.curves`, `cad.draw.hatch`, `cad.draw.lines`, `cad.draw.points`, `cad.draw.shapes`, `cad.draw.text`, `cad.drawing.settings`, `cad.edit.clipboard`, `cad.editor.command-line`, `cad.editor.cursor`, `cad.editor.drafting-input`, `cad.editor.dynamic-input`, `cad.editor.grips`, `cad.editor.properties`, `cad.editor.status-bar`, `cad.file.drawings`, `cad.layer.manage`, `cad.layout.paper`, `cad.modify.array`, `cad.modify.order-groups`, `cad.modify.shape`, `cad.modify.transform`, `cad.output.plot`, `cad.properties.edit`, `cad.select.objects`, `cad.table.edit`, `cad.tools.inquiry`, `cad.view.navigation`.
3. **Then the P2 features:** `cad.app.command-line`, `cad.editor.layout-tabs`.
4. **Keep the catalogue complete:** every new engine command joins a feature in `features.yaml` with a case.
