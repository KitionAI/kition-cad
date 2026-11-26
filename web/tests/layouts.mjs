// The Model and layout tabs (walkthrough N16): the status bar shows Model, selected; a layout the
// drawing gets shows as a tab after it; choosing it shows that layout (the engine's space follows,
// the view changes to the sheet), a line drawn there goes into the layout, and choosing Model comes
// back to the model with its objects; in Chinese the Model tab reads 模型.
// node tests/layouts.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
import { mkdir } from 'node:fs/promises'
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
const input = kit.getByTestId('cad-command-line')
const output = new URL('../test-results/layouts/', import.meta.url)
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 50) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const agent = (command, params) => page.evaluate(async ([command, params]) => window.devHost.connection().ai.run([{ command, params }], false), [command, params])
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
const open = async (locale) => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=${locale}`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  await page.waitForTimeout(300)
}

await mkdir(output, { recursive: true })
try {
  await open('en-US')
  const model = kit.getByTestId('cad-layout-Model')
  step('the status bar shows the Model tab', (await model.count()) === 1 && (await model.textContent()) === 'Model')
  step('Model is the selected tab', (await model.getAttribute('aria-selected')) === 'true')

  await agent('layout.add', { name: 'Plan' })
  const plan = kit.getByTestId('cad-layout-Plan')
  step('a new layout shows as a tab', await until(async () => (await plan.count()) === 1))
  const order = await kit.locator('[data-testid=cad-layout-tabs] [role=tab]').evaluateAll((tabs) => tabs.map((tab) => tab.textContent))
  step('after Model', order[0] === 'Model' && order.at(-1) === 'Plan', order.join(', '))

  const before = (await details()).view
  await plan.click()
  step('choosing it shows the layout', await until(async () => (await details()).space === 'Plan'), String((await details()).space))
  step('its tab is selected', await until(async () => (await plan.getAttribute('aria-selected')) === 'true' && (await model.getAttribute('aria-selected')) === 'false'))
  const sheet = (await details()).view
  step('the view moves to the sheet', JSON.stringify(sheet) !== JSON.stringify(before), JSON.stringify(sheet))
  await screenshot('layout')

  const inLayout = async () => (await details()).layouts.find((layout) => layout.name === 'Plan')?.entities ?? -1
  const start = await inLayout()
  for (const text of ['line', '1,1', '4,3', '']) {
    await input.fill(text)
    await input.press('Enter')
    await page.waitForTimeout(150)
  }
  step('a line drawn there goes into the layout', await until(async () => (await inLayout()) === start + 1), `${start} -> ${await inLayout()}`)
  step('and not into the model', (await details()).entities === 7)

  await model.click()
  step('choosing Model comes back to the model', await until(async () => (await details()).space === 'Model'))
  step('with its objects', (await details()).entities === 7)

  await open('zh-CN')
  step('in Chinese the Model tab reads 模型', (await kit.getByTestId('cad-layout-Model').textContent()) === '模型')
  step('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '))
} catch (error) {
  step('threw', false, error instanceof Error ? error.message.split('\n')[0] : String(error))
  await screenshot('failure')
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
