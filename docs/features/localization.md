# Interface languages

The kit's editor (`web/`) has no language setting of its own: the host (Kition) tells it which
locale to use, at `connectHost`'s `host.init.locale` and over the live `ui/locale` event
(`@kition/sdk/editor`'s `connectHost`).

## Translation coverage

- `web/src/strings.ts`'s `CATALOGS` holds the editor's own interface strings (the layers
  palette, the command line, selection status) in `en-US` and `zh-CN`.
- `web/src/strings.ts`'s `PHRASES` holds `zh-CN` translations of the ribbon, menu and tool
  labels the upstream editor's English text uses, for locales its own catalogs don't
  cover.
- `web/src/i18n/upstream/*.tsv` carries translations of that same English text, loaded with
  `@kition/sdk/editor`'s `loadPhrases` (`main.tsx`) and layered under `PHRASES` with
  `withPhrases`: `uk.tsv` (Ukrainian).
- Drawing titles, layer/layout/block/style names, parameter names and expressions, text in a
  drawing, and command-line input are user data and keep their original spelling. Engine
  diagnostics stay English. Untranslated text falls back to English.

## Adding translations

- `*.tsv` files are UTF-8, `English source<TAB>translation` per line (a leading empty column is
  harmless and ignored; `#` starts a comment). Add a file under `web/src/i18n/upstream/` for a
  new locale's upstream-style catalog, or add entries to `web/src/strings.ts`'s `PHRASES` for a
  locale already using `CATALOGS`.
- `cd web && pnpm test:flow` opens the editor at a given locale (see `tests/flow.mjs`, which
  checks `zh-CN`) and asserts on the rendered text.

Translations are original work under Apache-2.0, written from the meaning of the upstream
editor's English strings. Do not copy proprietary localization resources.
