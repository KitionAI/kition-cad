# Hardware parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Autodesk AutoCAD 2027 (Mac and Windows)

The hardware AutoCAD uses and how Kition CAD compares, per platform. Work items:
[gaps.md](../gaps.md).

**Headline (estimated): hardware ≈ 45%.** The editor draws 2D on the CPU (`tiny-skia`, in
WebAssembly) and takes wheel and trackpad input from the browser. Missing: 3D mice,
tablets/digitizers, GPU-accelerated 3D (there is no 3D), and any deliberate multi-monitor handling. Remaining: **≈ 15–25 Opus 5.5 hours**, plus a human with
the devices to test 3D mice and tablets.

Kition CAD's column is read from the source of this kit's editor (`web/`), `bindings/wasm` and
`crates/render`. AutoCAD's side is from Autodesk's system requirements and
graphics-performance documentation. Nothing here is benchmarked.

| Feature | AutoCAD 2027 | Kition CAD (`web/`) |
|---|---|---|
| GPU canvas for 2D | yes (hardware acceleration, GPU smoothing, high-quality geometry) | no: the engine rasterises the display list on the CPU (`tiny-skia`, in WebAssembly) |
| CPU fallback | software mode | CPU only (see above) |
| GPU for 3D (shaded visual styles, ray-traced rendering) | yes | no |
| Low-latency presentation | yes | browser-controlled |
| HiDPI / Retina, display scaling | yes | yes |
| Multi-monitor (palettes on other screens, per-monitor DPI) | yes | — |
| Trackpad pan, pinch zoom | yes (Mac) | browser events |
| Magic Mouse | yes | — |
| Wheel mouse, middle-button pan | yes | yes |
| 3D mouse (3Dconnexion SpaceMouse) | yes | no |
| Pen tablet / digitizer (TABLET command, calibration) | yes (Windows) | no |
| Screen magnifier following the cursor | yes | — |
| Plotters and printers (PC3 devices, system printers) | yes | **no**: PDF, PNG and SVG export only |

## Performance on hardware (internal numbers only)

200,000 entities pick in 0.001 ms, about 4 ms after an edit (incremental R-tree). Nothing has
been compared with AutoCAD on the same machine, and 1M-entity drawings are untested. A 10 MB DWG
can take about two minutes to fail.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
