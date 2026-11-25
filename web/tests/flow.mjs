// The editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html in Chinese, and prints PASS or FAIL per step.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await page.waitForTimeout(1000)
  step('Chinese tab names', (await kit.getByTestId('ribbon-tab-home').textContent()) === '默认')
  const start = (await details()).entities
  step('status counts the objects', (await kit.getByTestId('cad-status').textContent()) === `${start} 个对象`)
  const line = kit.getByTestId('cad-command-line')
  for (const text of ['line', '10,250', '120,260', '']) {
    await line.fill(text)
    await line.press('Enter')
    await page.waitForTimeout(150)
  }
  await until(async () => (await details()).entities === start + 1)
  step('draws a line from the command line', (await details()).entities === start + 1)

  await kit.getByTestId('cad-tool-circle.add').click()
  await until(async () => (await kit.getByTestId('cad-tool-circle.add').getAttribute('aria-pressed')) === 'true')
  step('chooses the circle tool', (await kit.getByTestId('cad-tool-circle.add').getAttribute('aria-pressed')) === 'true')
  const region = await kit.getByTestId('cad-canvas-region').boundingBox()
  await page.mouse.click(region.x + region.width * 0.75, region.y + region.height * 0.3)
  await page.waitForTimeout(200)
  step('the prompt follows the command', /radius|半径|Specify/i.test(await kit.getByTestId('cad-prompt').textContent()))
  await page.mouse.click(region.x + region.width * 0.75 + 40, region.y + region.height * 0.3)
  await until(async () => (await details()).entities === start + 2)
  step('draws a circle with two clicks', (await details()).entities === start + 2)
  await page.keyboard.press('Escape')

  await kit.getByTestId('cad-new-layer').click()
  await kit.locator('#param-name').fill('Doors')
  await kit.getByTestId('command-dialog-run').click()
  await until(async () => (await details()).currentLayer === 'Doors')
  step('a new layer becomes current', (await details()).currentLayer === 'Doors')
  await kit.getByTestId('cad-layer-Furniture-on').click()
  await until(async () => (await details()).layers.find((layer) => layer.name === 'Furniture')?.on === false)
  step('turns a layer off', (await details()).layers.find((layer) => layer.name === 'Furniture')?.on === false)
  await kit.getByTestId('cad-layer').selectOption('Walls')
  await until(async () => (await details()).currentLayer === 'Walls')
  step('the ribbon makes a layer current', (await details()).currentLayer === 'Walls')
  await kit.getByTestId('cad-mode-ortho').click()
  await until(async () => (await details()).drafting.ortho === true)
  step('turns Ortho on', (await details()).drafting.ortho === true)

  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'circle.add', params: { center: [60, 250], radius: 10 } }], false))
  await page.waitForTimeout(300)
  step('the agent edits the live drawing', run.completed[0].changed && (await page.evaluate(() => window.devHost.changes.at(-1).by)) === 'agent')
  await kit.getByTestId('cad-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.dxf').version)) > 1)
  const text = await page.evaluate(() => new TextDecoder().decode(new Uint8Array(window.devHost.files.get('Sample.dxf').bytes).slice(0, 40)))
  step('saves a DXF through the host', /SECTION/.test(text))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
