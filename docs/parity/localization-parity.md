# Localization parity

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-26 · **Target:** Autodesk AutoCAD 2027 for Mac (26.0)

How far Kition CAD's interface and drawing text are from AutoCAD's languages. Work items are in
[gaps.md](../gaps.md).

## What the target ships

Measured from the installed AutoCAD 2027 for Mac bundle by listing only (folder names, nothing
read): `Contents/Resources` has `Base.lproj` (English), `de`, `es`, `fr`, `it`, `ja`, `ko` and
`zh_CN` localizations, and `Resources/Support` has matching `@de@` … `@zh_CN@` folders. That is
**8 interface languages** on the Mac. AutoCAD for Windows also has language packs for Traditional
Chinese, Brazilian Portuguese, Russian, Polish, Czech and Hungarian (Autodesk's published
language-pack list, not measured here). AutoCAD's spell checker ships about 20 dictionaries
(`*.dct` names in `Resources/Support`).

Drawing text in AutoCAD: TrueType and SHX big fonts (CJK), Unicode `\U+` escapes, complex-script
shaping and right-to-left text through TrueType, vertical SHX text styles.

## What Kition CAD has

- **A display-time string catalog** in this kit's editor (`web/src/i18n/upstream/`, loaded by
  `@kition/sdk/editor`'s `loadPhrases`/`withPhrases`; the editor has no settings menu of its own,
  so the host (Kition) sets the locale; [localization.md](../features/localization.md)). The Ukrainian catalog has
  **973 rows**, covering menus, command labels, tool groups and tooltips, status controls,
  object snaps, drafting prompts and property labels. Engine diagnostics and command history
  stay English, and uncatalogued text falls back to English.
- **Simplified Chinese** (`PHRASES['zh-CN']` and the zh-CN catalog in `web/src/strings.ts`): 492
  phrases and 31 editor strings, every label the editor shows: the engine's command titles and
  menu groups (ribbon tabs, menus, the command line's completions), the editor's menus, the status
  bar and its menus, the Properties rows and object types, the grip menu. `web/tests/phrases.mjs`
  fails on any label left in English. Command prompts and their options stay English by the
  owner's decision (2026-11-25), so do engine diagnostics and the command history.
- **Drawing text:** TrueType/OpenType/`.ttc` through `skrifa` with per-character font fallback
  and FONTALT, so CJK characters render. DXF text is written with `\U+` escapes so
  non-ASCII survives in R2000 files. Glyphs are placed one per code point: there is **no
  shaping** (Arabic joining, Devanagari conjuncts), **no bidi** (right-to-left order) and **no
  vertical text**. SHX fonts are not read; our stroke font stands in.
- **Text input:** the browser's IME, in the editor's HTML fields; the command line ignores keys
  while a composition is in progress (`web/src/CommandLine.tsx`).

## Languages

Order and columns follow Kition's progress-docs convention. Every UI percentage is **measured**
from the catalogs (0 rows for every language without a catalog file). Hours are **estimated**:
about 3–5 h per language for an Opus 5.5 agent to translate the ~940 catalog rows and fix layout,
plus native review by a human; extending the catalog to the remaining English-only strings
(engine diagnostics, uncatalogued text) is about 4–6 h, counted once in the first row.

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Estimate to `full` |
|---|---|---|---|---|---|---|---|
| English | en | source (100%) | 100% (help is a command reference, not AutoCAD-sized help) | Latin: yes | n/a | full | — |
| Simplified Chinese | zh-Hans | 491 phrases + 31 editor strings: every label the editor shows (measured by `web/tests/phrases.mjs`); command prompts, diagnostics and history English | labels and tooltips yes; the hatch and colour dialogs yes; no help | CJK glyphs render via fallback; the IME types into the command line and in-place text; no vertical text | no | partial | 6–10 h (a prompt translation table in the engine for about 150 interactive commands, keeping the option letters) |
| Spanish | es | 0 (0%) | 0% | Latin: yes | no | none | 3–5 h |
| Hindi | hi | 0 (0%) | 0% | Devanagari: no shaping | no | none | 3–5 h + 15–25 h shaping (shared with Arabic) |
| Arabic | ar | 0 (0%) | 0% | No shaping, no RTL layout or bidi text | no | none | 3–5 h + shaping + 15–25 h RTL UI |
| French | fr | 0 (0%) | 0% | Latin: yes | no | none | 3–5 h |
| Portuguese | pt | 0 (0%) | 0% | Latin: yes | no | none | 3–5 h |
| Indonesian | id | 0 (0%) | 0% | Latin: yes | no | none | 3–5 h |
| Japanese | ja | 0 (0%) | 0% | CJK glyphs render; no vertical text | no | none | 4–6 h |
| German | de | 0 (0%) | 0% | Latin: yes (long strings will need layout checks) | no | none | 3–5 h |
| Korean | ko | 0 (0%) | 0% | Hangul renders | no | none | 4–6 h |
| Vietnamese | vi | 0 (0%) | 0% | Latin with stacked diacritics: renders per code point, untested | no | none | 3–5 h |
| Ukrainian | uk | 973 catalog rows (≈ 100% of the catalog; ≈ 80% of all UI strings, estimated: engine diagnostics and uncatalogued text stay English) | tooltips yes (no dialogs or command reference in this kit's editor) | Cyrillic: yes (the host's own fonts, via the Kit UI protocol's theme tokens) | translated by a contributor; no second review | partial | 2–4 h |

Other languages shipped: Simplified Chinese (partial) and Ukrainian (partial). AutoCAD for Mac ships 7
non-English languages (zh-Hans, es, fr, ja, de, ko, it); Kition CAD ships one of those (zh-Hans,
labels only), and Ukrainian, which AutoCAD for Mac doesn't ship.

**Total to put all twelve at `full`:** about 60–100 Opus 5.5 hours (catalog extension 4–6 h,
eleven translations 35–55 h, complex-script shaping and RTL 30–45 h), plus native-speaker review for
each language, which needs humans. Translations parallelise fully once the catalog exists.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
| 2026-11-26 | minor | Simplified Chinese: every label the editor shows, checked by a test; prompts stay English |
| 2026-11-26 | minor | Simplified Chinese: the labels of the second round's dialogs and menus (Insert, Drafting Settings, the layout tabs' and the history window's menus, the polyline vertex row) |
