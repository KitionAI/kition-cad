// Regression issue-keys-after-ribbon (the stage 7 acceptance drawing): what a person types after
// clicking a ribbon button, a status bar toggle or a dialog's button goes to the drawing, as it does
// after a click on the canvas: Polyline clicked on the ribbon, then "100,60" and Enter typed with
// the pointer over the canvas start the polyline there; F8 after the status bar's Grid toggles
// Ortho; F8 after the New Layer dialog closes toggles Ortho; Escape after the ribbon's Line cancels
// it. Before, the keys went to the clicked button or to the page and were lost (Enter clicked the
// button again). A ribbon button pressed with the keyboard keeps the focus (keyboard navigation).
// node tests/keys.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
page.setDefaultTimeout(10_000)
const kit = page.frameLocator('#kit')
const surface = kit.getByTestId('kit-canvas-surface')
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 40) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const prompt = async () => (await details()).drafting.commandLine.prompt
const toScreen = async (x, y) => {
  const { view } = await details()
  const b = await surface.boundingBox()
  const k = view.height / b.height
  return { x: b.x + b.width / 2 + (x - view.center[0]) / k, y: b.y + b.height / 2 - (y - view.center[1]) / k }
}

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  await kit.getByTestId('cad-tool-polyline.add').waitFor()
  await page.waitForTimeout(500)

  // 1. Polyline on the ribbon, then typed coordinates with the pointer over the canvas.
  await kit.getByTestId('cad-tool-polyline.add').click()
  await until(async () => (await details()).running === 'polyline.add')
  const over = await toScreen(110, 70)
  await page.mouse.move(over.x, over.y, { steps: 4 })
  await page.waitForTimeout(200)
  await page.keyboard.type('100,60', { delay: 30 })
  await page.keyboard.press('Enter')
  step('typing after the ribbon\'s Polyline gives its start point', await until(async () => JSON.stringify((await prompt()).base) === '[100,60]'), JSON.stringify(await prompt()))
  step('and Enter did not start it again', (await details()).running === 'polyline.add')
  await page.keyboard.press('Escape')
  step('Escape then cancels it', await until(async () => (await details()).running == null))

  // 2. F8 after a status bar toggle.
  const ortho = (await details()).drafting.ortho
  await kit.getByTestId('cad-mode-grid').click()
  await page.waitForTimeout(300)
  await page.keyboard.press('F8')
  step('F8 after the status bar\'s Grid toggles Ortho', await until(async () => (await details()).drafting.ortho === !ortho))

  // 3. F8 after the New Layer dialog closes.
  await kit.getByTestId('cad-new-layer').click()
  await kit.locator('#param-name').fill('Keys')
  await kit.getByTestId('command-dialog-run').click()
  await until(async () => (await details()).currentLayer === 'Keys')
  await page.waitForTimeout(300)
  await page.keyboard.press('F8')
  step('F8 after a dialog closes toggles Ortho', await until(async () => (await details()).drafting.ortho === ortho))

  // 4. Escape after the ribbon's Line.
  await kit.getByTestId('cad-tool-line.add').click()
  await until(async () => (await details()).running === 'line.add')
  await page.keyboard.press('Escape')
  step('Escape after the ribbon\'s Line cancels it', await until(async () => (await details()).running == null))

  // 5. A ribbon button pressed with the keyboard keeps the focus.
  await kit.getByTestId('cad-tool-circle.add').focus()
  await page.keyboard.press('Enter')
  await until(async () => (await details()).running === 'circle.add')
  step('a button pressed with the keyboard keeps the focus', await kit.getByTestId('cad-tool-circle.add').evaluate((button) => document.activeElement === button))
  await page.keyboard.press('Escape')
  step('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '))
} catch (error) {
  step('threw', false, error instanceof Error ? error.message.split('\n')[0] : String(error))
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
