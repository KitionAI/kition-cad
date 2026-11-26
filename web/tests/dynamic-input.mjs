// Dynamic input (F12): the prompt and value boxes beside the cursor. X and Y at a first point,
// distance and angle after it with their live values; 50 Tab 30 Enter draws 50 at 30°; a locked
// distance keeps the preview's length; Down Arrow lists the options and Enter picks Undo; a value
// prompt has one box; typing in the command line itself stays absolute; the tooltip keeps clear of
// the command line at the bottom; F12 and the status bar turn it off.
// node tests/dynamic-input.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const surface = kit.getByTestId('kit-canvas-surface')
const tooltip = kit.getByTestId('cad-dynamic-input')
const output = new URL('../test-results/dynamic-input/', import.meta.url)
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
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details
const command = async (...entries) => {
  for (const text of entries) { await input.fill(text); await input.press('Enter'); await page.waitForTimeout(150) }
}
const idle = () => until(async () => (await details()).running == null)
const query = async (type) => {
  const run = await page.evaluate(async (type) => window.devHost.connection().ai.run([{ command: 'object.query', params: { type } }], false), type)
  const result = run.completed?.[0]?.result ?? run.completed?.[0]?.value ?? run.completed?.[0]
  return result?.entities ?? []
}
// The lines drawn since the section opened the sample.
let baseline = 0
const lines = async () => (await query('line')).slice(baseline).map((line) => [[line.geometry.a.x, line.geometry.a.y], [line.geometry.b.x, line.geometry.b.y]])
const lastLine = async () => (await lines()).at(-1) ?? null
// Drawing units to page pixels, from the view the editor shows.
const toScreen = async (x, y) => {
  const { view } = await details()
  const box = await surface.boundingBox()
  const perPixel = view.height / box.height
  return { x: box.x + box.width / 2 + (x - view.center[0]) / perPixel, y: box.y + box.height / 2 - (y - view.center[1]) / perPixel }
}
const open = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  // Only the boxes move the cursor: no running snaps, no polar tracking.
  await command('osnap {"on":false}', 'polar {"on":false}', 'ortho {"on":false}')
  baseline = (await query('line')).length
}
// Page pixels to drawing units.
const toWorld = async (x, y) => {
  const { view } = await details()
  const box = await surface.boundingBox()
  const perPixel = view.height / box.height
  return [view.center[0] + (x - box.x - box.width / 2) * perPixel, view.center[1] - (y - box.y - box.height / 2) * perPixel]
}
const pixel = async () => (await details()).view.height / (await surface.boundingBox()).height
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
const shown = () => tooltip.isVisible().then(async (visible) => visible && (await tooltip.evaluate((element) => getComputedStyle(element).visibility)) === 'visible')
const text = async (testId) => (await kit.getByTestId(testId).innerText({ timeout: 1000 })).trim()
// The coordinate readout as numbers (the point a click there gives).
const readout = async () => (await text('cad-coordinates')).split(',').map((part) => Number(part.trim()))
const near = (a, b, tolerance = 1e-6) => Math.abs(a - b) < tolerance
const section = async (name, body) => {
  try {
    await open()
    await body()
  } catch (error) {
    step(`${name} (threw)`, false, error instanceof Error ? error.message.split('\n')[0] : String(error))
    await screenshot(`failure-${name.replace(/\W+/g, '-')}`)
  }
}

await mkdir(output, { recursive: true })
try {
  // 1. The boxes: X and Y at a first point, @ distance < angle after it, live values dimmed, beside
  //    the cursor (right of it and below), and the pointer passes through them.
  await section('boxes', async () => {
    const start = await toScreen(60, 260)
    await command('line')
    await page.mouse.move(start.x, start.y)
    step('the tooltip shows at a first point', await until(shown))
    step('it says what the prompt asks', /first point/i.test(await text('cad-dynamic-message')), await text('cad-dynamic-message'))
    const first = (await tooltip.innerText()).replace(/\s+/g, ' ')
    step('a first point has X and Y boxes', /\bX\b/.test(first) && /\bY\b/.test(first) && !first.includes('@'), first)
    const [wx, wy] = await toWorld(start.x, start.y)
    const tolerance = 1.5 * await pixel()
    step('the X and Y boxes show the cursor\'s X and Y, live', await until(async () => near(Number(await text('cad-dynamic-first')), wx, tolerance) && near(Number(await text('cad-dynamic-second')), wy, tolerance)), `${await text('cad-dynamic-first')} ${await text('cad-dynamic-second')} (${wx} ${wy})`)
    const box = await tooltip.boundingBox()
    step('right of the cursor and below it', box && box.x > start.x && box.y > start.y, JSON.stringify(box))
    const through = await tooltip.evaluate((element) => {
      const r = element.getBoundingClientRect()
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)
      return hit != null && !element.contains(hit)
    })
    step('the pointer passes through it', through)
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 100, start.y)
    await until(async () => (await text('cad-dynamic-second')) === '0°')
    const later = (await tooltip.innerText()).replace(/\s+/g, ' ')
    step('a later point has @ distance < angle', later.includes('@') && later.includes('<') && !/\bX\b/.test(later), later)
    const perPixel = await pixel()
    step('the live distance and angle follow the cursor', near(Number(await text('cad-dynamic-first')), 100 * perPixel, 1.5 * perPixel) && (await text('cad-dynamic-second')) === '0°', `${await text('cad-dynamic-first')} ${await text('cad-dynamic-second')}`)
    step('the prompt has options: ▾', await kit.getByTestId('cad-dynamic-more').isVisible())
    await screenshot('boxes')
    await page.mouse.move(start.x, start.y - 100)
    step('the angle follows the cursor', await until(async () => (await text('cad-dynamic-second')) === '90°'), await text('cad-dynamic-second'))
  })

  // 2. 50 Tab 30 Enter at LINE's second point: 50 units at 30°, sent as @50<30.
  await section('typing', async () => {
    const start = await toScreen(60, 260)
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 80, start.y + 40)
    await page.waitForTimeout(300)
    await page.keyboard.type('50')
    step('typing on the canvas fills the first box', await until(async () => (await text('cad-dynamic-first')) === '50'), await text('cad-dynamic-first'))
    await page.keyboard.press('Tab')
    step('Tab locks the distance and moves to the angle box', await until(async () => (await kit.getByTestId('cad-dynamic-first-lock').count()) === 1 && (await kit.getByTestId('cad-dynamic-second').getAttribute('data-active')) === 'true'))
    await page.keyboard.type('30')
    step('the angle goes in the second box', await until(async () => (await text('cad-dynamic-second')) === '30'))
    await screenshot('typed')
    await page.keyboard.press('Enter')
    step('Enter draws the line', await until(async () => (await lines()).length === 1))
    const line = await lastLine()
    const end = [line[0][0] + 50 * Math.cos(Math.PI / 6), line[0][1] + 50 * Math.sin(Math.PI / 6)]
    step('the end is 50 at 30° from the start', line && near(line[1][0], end[0]) && near(line[1][1], end[1]), JSON.stringify(line))
    const history = await kit.getByTestId('cad-history').innerText()
    step('the command line got @50<30', history.includes('@50<30'), history.split('\n').slice(-2).join(' | '))
    step('the boxes are empty again', (await input.inputValue()) === '' && (await until(async () => (await kit.getByTestId('cad-dynamic-first').getAttribute('data-typed')) == null)))
    // A plain number and Enter is still direct distance: 20 toward the cursor (straight down);
    // typed while the command line keeps the keyboard, it is still the boxes'.
    const at = await toScreen(line[1][0], line[1][1])
    await page.mouse.move(at.x, at.y + 120)
    await page.waitForTimeout(300)
    await page.keyboard.type('20')
    await page.keyboard.press('Enter')
    await until(async () => (await lines()).length === 2)
    const down = await lastLine()
    step('a plain number is direct distance', down && near(Math.hypot(down[1][0] - down[0][0], down[1][1] - down[0][1]), 20) && down[1][1] - down[0][1] < -19.9, JSON.stringify(down))
    // A comma reads X and Y from the last point: 15,-5 Enter.
    await page.mouse.move(at.x + 50, at.y + 50)
    await page.waitForTimeout(300)
    await page.keyboard.type('15,-5')
    await page.keyboard.press('Enter')
    await until(async () => (await lines()).length === 3)
    const relative = await lastLine()
    step('15,-5 is relative to the last point', relative && near(relative[1][0] - relative[0][0], 15) && near(relative[1][1] - relative[0][1], -5), JSON.stringify(relative))
    await page.keyboard.press('Escape')
    await idle()
  })

  // 3. A locked distance holds the cursor: the preview keeps its length wherever the pointer goes.
  await section('lock', async () => {
    const start = await toScreen(100, 200)
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 60, start.y - 30)
    await page.waitForTimeout(300)
    // The base point, where the click put it (WebKit's pointer is on whole pixels).
    const [bx, by] = (await details()).drafting.commandLine.prompt.base
    await page.keyboard.type('40')
    await page.keyboard.press('Tab')
    await page.waitForTimeout(300)
    const lengths = []
    for (const [dx, dy] of [[150, 0], [-20, -170], [90, 90]]) {
      await page.mouse.move(start.x + dx, start.y + dy)
      await page.waitForTimeout(300)
      const [x, y] = await readout()
      lengths.push(Math.hypot(x - bx, y - by))
    }
    step('the preview keeps the locked length 40', lengths.every((length) => near(length, 40, 1e-3)), lengths.map((l) => l.toFixed(4)).join(' '))
    step('the angle box follows the cursor, live', /°$/.test(await text('cad-dynamic-second')), await text('cad-dynamic-second'))
    await screenshot('locked')
    await page.mouse.click(start.x + 90, start.y + 90)
    await until(async () => (await lines()).length === 1)
    const line = await lastLine()
    step('a click places the locked length in its direction', line && near(Math.hypot(line[1][0] - bx, line[1][1] - by), 40, 1e-6) && line[1][0] > bx && line[1][1] < by, JSON.stringify(line))
    step('the click used the entry up', (await input.inputValue()) === '', await input.inputValue())
    await page.mouse.move(start.x + 10, start.y - 70)
    await page.waitForTimeout(300)
    const [x, y] = await readout()
    const [px, py] = await toWorld(start.x + 10, start.y - 70)
    step('the next point is free again: the cursor is where the pointer is', near(x, px, 1.5 * await pixel()) && near(y, py, 1.5 * await pixel()), `${x} ${y} (${px} ${py})`)
    await page.keyboard.press('Escape')
    await idle()
  })

  // 4. Down Arrow lists the prompt's options under the tooltip; Down and Up move, Enter picks.
  await section('options', async () => {
    const start = await toScreen(60, 260)
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.click(start.x + 120, start.y)
    await until(async () => (await lines()).length === 1)
    await page.mouse.move(start.x + 160, start.y + 60)
    await page.waitForTimeout(300)
    await page.keyboard.press('ArrowDown')
    const list = kit.getByTestId('cad-dynamic-options')
    step('Down Arrow opens the options', await until(() => list.isVisible()))
    step('the options are the prompt\'s', (await list.innerText()).includes('Undo'), await list.innerText())
    await screenshot('options')
    await page.keyboard.press('Escape')
    step('Escape closes the list and keeps the command', await until(async () => !(await list.isVisible())) && (await details()).running === 'line.add')
    await page.keyboard.press('ArrowDown')
    await until(() => list.isVisible())
    const rows = await list.locator('button').count()
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowUp')
    step('Down and Up move the highlight', (await kit.getByTestId('cad-dynamic-option-Undo').getAttribute('data-current')) === 'true', String(rows))
    await page.keyboard.press('Enter')
    step('Enter picks Undo: the segment is gone, LINE goes on', await until(async () => (await lines()).length === 0) && (await details()).running === 'line.add')
    // The list from the command line too, and a click picks.
    await page.mouse.click(start.x + 120, start.y)
    await page.mouse.click(start.x + 120, start.y - 80)
    await until(async () => (await lines()).length === 2)
    await page.mouse.move(start.x + 160, start.y + 60)
    await input.focus()
    await input.press('ArrowDown')
    step('Down Arrow in the command line opens them too', await until(() => list.isVisible()))
    await kit.getByTestId('cad-dynamic-option-Close').click()
    step('a click picks Close', await until(async () => (await lines()).length === 3) && await idle())
  })

  // 5. A value prompt has one box: CIRCLE's radius, live from the centre, 12 Enter.
  await section('single', async () => {
    const center = await toScreen(200, 150)
    await command('circle')
    await page.mouse.click(center.x, center.y)
    await page.mouse.move(center.x + 50, center.y)
    await page.waitForTimeout(300)
    step('one box for the radius', await until(async () => (await tooltip.getAttribute('data-layout')) === 'single') && (await kit.getByTestId('cad-dynamic-second').count()) === 0)
    const perPixel = await pixel()
    step('it shows the distance from the centre, live', near(Number(await text('cad-dynamic-value')), 50 * perPixel, 1.5 * perPixel), await text('cad-dynamic-value'))
    await page.keyboard.type('12')
    step('the typed radius goes in it', await until(async () => (await text('cad-dynamic-value')) === '12'))
    await screenshot('single')
    await page.keyboard.press('Enter')
    await idle()
    const circle = (await query('circle')).at(-1)
    step('12 Enter: radius 12', circle && near(circle.geometry.radius, 12), JSON.stringify(circle?.geometry))
  })

  // 6. Typed in the command line itself, coordinates stay absolute and the tooltip steps aside.
  await section('command line', async () => {
    const start = await toScreen(60, 260)
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 50, start.y + 50)
    await input.fill('100,10')
    step('the tooltip steps aside while the command line is typed in', await until(async () => !(await shown())))
    await input.press('Enter')
    await until(async () => (await lines()).length === 1)
    const line = await lastLine()
    step('100,10 typed in the command line is absolute', line && near(line[1][0], 100) && near(line[1][1], 10), JSON.stringify(line))
    await page.keyboard.press('Escape')
    await idle()
  })

  // 7. Clear of the command line at the bottom; F12 and the status bar turn it off and on.
  await section('placement and F12', async () => {
    const bounds = await surface.boundingBox()
    await command('line')
    const low = { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height - 30 }
    await page.mouse.move(low.x, low.y)
    await until(shown)
    const box = await tooltip.boundingBox()
    const prompt = await kit.getByTestId('cad-prompt').boundingBox()
    step('near the bottom it goes above the cursor', box && box.y + box.height < low.y, JSON.stringify(box))
    step('it keeps clear of the command line\'s prompt and history', box && prompt && box.y + box.height <= prompt.y && box.y + box.height <= bounds.y + bounds.height - 70, `${JSON.stringify(box)} ${JSON.stringify(prompt)}`)
    await screenshot('bottom')
    await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
    await page.waitForTimeout(300)
    await page.keyboard.press('F12')
    step('F12 turns dynamic input off', await until(async () => (await details()).drafting.dynamicInput === false))
    step('the status bar shows it off', (await kit.getByTestId('cad-mode-dynamicInput').getAttribute('aria-pressed')) === 'false')
    step('F12 off: no tooltip', await until(async () => !(await shown())))
    step('LINE keeps running', (await details()).running === 'line.add')
    const start = await toScreen(60, 260)
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 50, start.y)
    await page.waitForTimeout(300)
    await page.keyboard.type('30,40')
    await page.keyboard.press('Enter')
    await until(async () => (await lines()).length === 1)
    const line = await lastLine()
    step('off: typed points are absolute, as on the command line', line && near(line[1][0], 30) && near(line[1][1], 40), JSON.stringify(line))
    await page.keyboard.press('Escape')
    await idle()
    await kit.getByTestId('cad-mode-dynamicInput').click()
    step('the status bar button turns it on', await until(async () => (await details()).drafting.dynamicInput === true))
    await command('line')
    await page.mouse.move(start.x, start.y)
    step('on again: the tooltip is back', await until(shown))
  })

  step('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which} ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
  await screenshot('failure')
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
