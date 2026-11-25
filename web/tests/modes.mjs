// The editor in each display mode, through the development host: node tests/modes.mjs [chromium|webkit]
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/sdk/dev-host/mode-smoke'

await runModeSmoke({
  kit: 'kit.cad',
  config: new URL('../vite.config.ts', import.meta.url).pathname,
  browser: process.argv[2] === 'webkit' ? webkit : chromium,
  createServer,
  line: { tab: 'home', primary: ['cad-tool-line.add', 'cad-tool-circle.add', 'cad-tool-object.move', 'cad-tool-object.trim', 'cad-tool-dimension.addLinear', 'cad-layer', 'cad-layer-properties', 'cad-zoom-extents'] },
})
