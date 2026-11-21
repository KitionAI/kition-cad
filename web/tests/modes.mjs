// The editor in each display mode, through the development host: node tests/modes.mjs [chromium|webkit]
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/kit-dev-host/mode-smoke'

// Playwright's WebKit reports a Mac (navigator.platform MacIntel, from its Safari user agent) on
// every operating system, so on Linux the editor (kit-editor's isMac) waits for Cmd+\ while
// ControlOrMeta presses Ctrl. Without the override the page reports the platform it runs on.
process.env.PLAYWRIGHT_NO_UA_PLATFORM = '1'

await runModeSmoke({
  kit: 'kit.cad',
  config: new URL('../vite.config.ts', import.meta.url).pathname,
  browser: process.argv[2] === 'webkit' ? webkit : chromium,
  createServer,
  line: { tab: 'home', primary: ['cad-tool-line', 'cad-tool-circle', 'cad-tool-move', 'cad-tool-trim', 'cad-tool-dimlinear', 'cad-layer', 'cad-layer-properties', 'cad-zoom-extents'] },
})
