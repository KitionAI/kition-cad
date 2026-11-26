// The status bar (walkthrough N19, 别扭 9): each drafting mode's tooltip names it with its key (F3,
// F7, F8, F9, F10, F11, F12); the small arrow beside Osnap lists the running object snap modes with
// the ones on checked, and choosing one changes OSMODE (and turns object snaps on), Clear All and
// Select All too; the arrow beside Polar sets polar tracking's increment angle and turns polar on;
// both work in the middle of LINE without ending it; the modes' buttons keep their test ids.
// node tests/status-bar.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const output = new URL('../test-results/status-bar/', import.meta.url)
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
const drafting = async () => (await details()).drafting
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})

await mkdir(output, { recursive: true })
try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  await page.waitForTimeout(300)

  // 1. Each mode keeps its test id, and its tooltip names its key.
  const keys = { grid: 'F7', snap: 'F9', ortho: 'F8', polar: 'F10', osnap: 'F3', otrack: 'F11', dynamicInput: 'F12' }
  for (const [mode, key] of Object.entries(keys)) {
    const title = await kit.getByTestId(`cad-mode-${mode}`).getAttribute('title')
    step(`the ${mode} button's tooltip names ${key}`, title?.endsWith(`(${key})`) === true, title ?? 'no title')
  }
  step('Lineweight keeps its button', (await kit.getByTestId('cad-mode-lwdisplay').count()) === 1)

  // 2. The Osnap menu: the modes on are checked; choosing one changes OSMODE.
  const before = (await drafting()).osmode
  await kit.getByTestId('cad-mode-osnap-menu').click()
  await until(async () => (await kit.getByTestId('cad-osnap-mode-1').count()) > 0)
  await screenshot('osnap-menu')
  const checked = async (bit) => (await kit.getByTestId(`cad-osnap-mode-${bit}`).getAttribute('aria-checked')) === 'true'
  step('the menu checks the modes that are on', (await checked(1)) === ((before & 1) !== 0) && (await checked(2)) === ((before & 2) !== 0) && (await checked(512)) === ((before & 512) !== 0), `OSMODE ${before}`)
  const nearestOn = (before & 512) !== 0
  await kit.getByTestId('cad-osnap-mode-512').click()
  await until(async () => (await drafting()).osmode !== before)
  const after = (await drafting()).osmode
  step('choosing Nearest switches its bit', ((after & 512) !== 0) === !nearestOn && (after & ~512) === (before & ~512), `${before} -> ${after}`)

  await kit.getByTestId('cad-mode-osnap-menu').click()
  await kit.getByTestId('cad-osnap-mode-none').click()
  await until(async () => (await drafting()).osmode === 0)
  step('Clear All turns every mode off', (await drafting()).osmode === 0)
  await until(async () => (await kit.getByTestId('cad-mode-osnap').getAttribute('aria-pressed')) === 'false')
  step('and the Osnap button shows it off', (await kit.getByTestId('cad-mode-osnap').getAttribute('aria-pressed')) === 'false')
  await kit.getByTestId('cad-mode-osnap-menu').click()
  await kit.getByTestId('cad-osnap-mode-all').click()
  await until(async () => (await drafting()).osmode === 0x3fff)
  step('Select All turns every mode on', (await drafting()).osmode === 0x3fff, String((await drafting()).osmode))

  // 3. In the middle of LINE: Osnap and Polar menus change the settings and LINE keeps running.
  await kit.getByTestId('cad-mode-osnap').click()
  await until(async () => (await drafting()).osnap === false)
  await input.fill('line')
  await input.press('Enter')
  await input.fill('10,10')
  await input.press('Enter')
  await until(async () => (await details()).running === 'line.add')
  await kit.getByTestId('cad-mode-osnap-menu').click()
  await kit.getByTestId('cad-osnap-mode-none').click()
  await kit.getByTestId('cad-mode-osnap-menu').click()
  await kit.getByTestId('cad-osnap-mode-1').click()
  await until(async () => (await drafting()).osmode === 1)
  step('choosing Endpoint during LINE sets it and turns object snaps on', (await drafting()).osmode === 1 && (await drafting()).osnap === true)
  if ((await drafting()).polar) {
    await kit.getByTestId('cad-mode-polar').click()
    await until(async () => (await drafting()).polar === false)
  }
  await kit.getByTestId('cad-mode-polar-menu').click()
  await until(async () => (await kit.getByTestId('cad-polar-angle-45').count()) > 0)
  await screenshot('polar-menu')
  await kit.getByTestId('cad-polar-angle-45').click()
  await until(async () => Math.abs((await drafting()).polarAngle - 45) < 1e-9 && (await drafting()).polar === true)
  const set = await drafting()
  step('choosing 45° sets the increment and turns polar on', Math.abs(set.polarAngle - 45) < 1e-9 && set.polar === true, `${set.polarAngle} ${set.polar}`)
  await kit.getByTestId('cad-mode-polar-menu').click()
  step('the menu checks the angle', (await kit.getByTestId('cad-polar-angle-45').getAttribute('aria-checked')) === 'true')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  step('LINE keeps running', (await details()).running === 'line.add', String((await details()).running))
  step('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '))
} catch (error) {
  step('threw', false, error instanceof Error ? error.message.split('\n')[0] : String(error))
  await screenshot('failure')
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
