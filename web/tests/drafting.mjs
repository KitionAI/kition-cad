// Drafting by hand: the cursor pipeline in a command (Ortho, grid snap, polar, tracking), direct
// distance entry, Space and right-click as Enter, the command and snap menus, Enter with a
// selection, and navigation (double middle click, trackpad pan, wheel zoom).
// node tests/drafting.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
const kit = page.frameLocator('#kit')
const input = kit.getByTestId('cad-command-line')
const surface = kit.getByTestId('kit-canvas-surface')
const output = new URL('../test-results/drafting/', import.meta.url)
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
// The last line drawn, as [[x0, y0], [x1, y1]].
const lastLine = async () => {
  const last = (await query('line')).at(-1)?.geometry
  return last ? [[last.a.x, last.a.y], [last.b.x, last.b.y]] : null
}
const lineCount = async () => (await query('line')).length
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
  // Running object snaps off unless a section turns them on: only the mode under test moves the cursor.
  await command('osnap {"on":false}')
}
const canvasFocus = () => kit.locator('.kui-app').focus()
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
// Orange (the snap and tracking colour) pixels in the canvas around page point (x, y).
const orange = async (x, y, half = 6) => {
  const box = await surface.boundingBox()
  return surface.evaluate((canvas, { x, y, half }) => {
    const scale = canvas.width / canvas.clientWidth
    const data = canvas.getContext('2d').getImageData(Math.round((x - half) * scale), Math.round((y - half) * scale), Math.round(2 * half * scale), Math.round(2 * half * scale)).data
    for (let i = 0; i < data.length; i += 4) {
      const [r, g, b] = [data[i], data[i + 1], data[i + 2]]
      if (r > 200 && r - g > 20 && g - b > 30 && b < 190) return true
    }
    return false
  }, { x: x - box.x, y: y - box.y, half })
}
const near = (a, b) => Math.abs(a - b) < 1e-9
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
  // 1. The cursor pipeline in a command (B2): Ortho, grid snap, polar 45°.
  await section('pipeline', async () => {
    const start = await toScreen(60, 260)
    await command('ortho {"on":true}', 'line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.click(start.x + 160, start.y - 25)
    await canvasFocus()
    await page.keyboard.press('Enter')
    await idle()
    let line = await lastLine()
    step('Ortho (F8) keeps the second point level with the first', line && line[0][1] === line[1][1] && line[1][0] > line[0][0], JSON.stringify(line))
    await command('ortho {"on":false}', 'snap {"on":true}', 'setvar {"name":"SNAPUNIT","value":"10,10"}', 'line')
    await page.mouse.click(start.x + 3, start.y + 4)
    await page.mouse.click(start.x + 130, start.y - 70)
    await canvasFocus()
    await page.keyboard.press('Enter')
    await idle()
    line = await lastLine()
    step('grid snap (F9) puts both points on the grid', line && line.flat().every((v) => near(v / 10, Math.round(v / 10))), JSON.stringify(line))
    await command('snap {"on":false}', 'setvar {"name":"POLARANG","value":45}', 'line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 100, start.y - 103)
    await page.waitForTimeout(200)
    // Past the cursor: up to it the guide lies over the rubber band.
    step('the polar path is drawn as a guide', await until(() => orange(start.x + 125, start.y - 125, 8)))
    await screenshot('polar-45')
    await page.mouse.click(start.x + 100, start.y - 103)
    await canvasFocus()
    await page.keyboard.press('Enter')
    await idle()
    line = await lastLine()
    step('polar tracking at 45° draws exactly 45°', line && Math.abs((line[1][0] - line[0][0]) - (line[1][1] - line[0][1])) < 1e-9 && line[1][0] > line[0][0], JSON.stringify(line))
  })

  // 2. Direct distance entry (B1): the typed distance goes toward the cursor.
  await section('direct distance', async () => {
    const start = await toScreen(60, 260)
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 140, start.y)
    await page.waitForTimeout(150)
    await page.keyboard.type('60')
    await page.keyboard.press('Enter')
    await page.waitForTimeout(200)
    await command('')
    await idle()
    let line = await lastLine()
    step('LINE, pointer to the right, 60 Enter: 60 to the right', line && near(line[1][0] - line[0][0], 60) && near(line[1][1], line[0][1]), JSON.stringify(line))
    await command('ortho {"on":true}', 'line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(start.x + 140, start.y - 140 * Math.tan(Math.PI / 18))
    await page.waitForTimeout(150)
    await page.keyboard.type('60')
    await page.keyboard.press('Enter')
    await page.waitForTimeout(200)
    await command('')
    await idle()
    line = await lastLine()
    step('with Ortho and the pointer 10° up: still 60 to the right', line && near(line[1][0] - line[0][0], 60) && near(line[1][1], line[0][1]), JSON.stringify(line))
  })

  // 3. Object snap tracking (N10): rest on an endpoint, then follow its alignment path.
  await section('tracking', async () => {
    await command('osnap {"on":true}', 'polar {"on":false}', 'line', '360,60', '360,120', '')
    await idle()
    const end = await toScreen(360, 120)
    const start = await toScreen(330, 250)
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.move(end.x + 1, end.y + 1)
    await page.waitForTimeout(900)
    await page.mouse.move(end.x - 60, end.y + 2, { steps: 5 })
    step('a rested endpoint sends a horizontal alignment guide', await until(() => orange(end.x - 30, end.y, 6)))
    await screenshot('tracking')
    await page.mouse.click(end.x - 60, end.y + 2)
    await canvasFocus()
    await page.keyboard.press('Enter')
    await idle()
    const line = await lastLine()
    step('the click lands on the alignment path, level with the endpoint', line && near(line[1][1], 120), JSON.stringify(line))
  })

  // 4. Space = Enter (B3) on the canvas and in the command line, but not at a free-text prompt.
  await section('space', async () => {
    const start = await toScreen(60, 260)
    const before = await lineCount()
    await input.fill('line')
    await input.press(' ')
    step('Space in the command line enters LINE', await until(async () => (await details()).running === 'line.add'))
    await input.fill('10,250')
    await input.press(' ')
    await input.fill('80,250')
    await input.press(' ')
    step('Space enters typed points', await until(async () => (await lineCount()) === before + 1))
    await page.mouse.click(start.x, start.y + 40)
    await page.keyboard.press(' ')
    step('Space on the canvas ends LINE', await until(async () => (await details()).running == null))
    await page.keyboard.press(' ')
    step('Space with nothing running repeats LINE', await until(async () => (await details()).running === 'line.add'))
    await page.keyboard.press('Escape')
    await idle()
    await command('text', '20,200', '', '')
    step('TEXT reaches its free-text prompt', await until(async () => (await details()).prompt.includes('Enter text')), (await details()).prompt)
    await input.fill('two words')
    await input.press(' ')
    step('Space at a free-text prompt types a space', (await input.inputValue()) === 'two words ' && (await details()).running === 'text.add', await input.inputValue())
    await input.fill('two words')
    await input.press('Enter')
    await until(async () => (await details()).running == null)
    const texts = await query('text')
    step('the text keeps its space', texts.some((t) => JSON.stringify(t).includes('two words')), JSON.stringify(texts.at(-1)))
  })

  // 5. Enter with a selection repeats the last command (B5).
  await section('enter with a selection', async () => {
    await command('circle', '200,200', '10')
    await idle()
    const edge = await toScreen(210, 200)
    await page.mouse.click(edge.x, edge.y)
    await until(async () => (await details()).selection > 0)
    await page.keyboard.press('Enter')
    step('Enter with objects selected repeats CIRCLE', await until(async () => (await details()).running === 'circle.add'), String((await details()).running))
    await page.keyboard.press('Escape')
  })

  // 6. Right-click (B4, N8): quick = Enter, held = the command menu, Shift = the snap menu.
  await section('right click', async () => {
    const start = await toScreen(60, 260)
    const before = await lineCount()
    await command('line')
    await page.mouse.click(start.x, start.y)
    await page.mouse.click(start.x + 120, start.y)
    await page.mouse.click(start.x + 120, start.y + 10, { button: 'right' })
    step('a quick right-click ends LINE', await until(async () => (await details()).running == null) && (await lineCount()) === before + 1)
    const first = await lastLine()
    step('a quick right-click in a command opens no menu', (await kit.getByTestId('cad-context-menu').count()) === 0 && (await kit.getByTestId('cad-command-menu').count()) === 0)
    await command('line')
    await page.mouse.click(start.x + 120, start.y)
    await input.fill('@40,0')
    await page.mouse.click(start.x + 120, start.y + 10, { button: 'right' })
    step('a quick right-click enters what was typed first', await until(async () => (await lineCount()) === before + 2 && (await details()).running === 'line.add'))
    await page.mouse.move(start.x + 150, start.y + 30)
    await page.mouse.down({ button: 'right' })
    await page.waitForTimeout(400)
    await page.mouse.up({ button: 'right' })
    const menu = kit.getByTestId('cad-command-menu')
    step('a held right-click opens the command menu', await until(async () => (await menu.count()) === 1))
    const labels = (await menu.innerText().catch(() => '')).split('\n').map((s) => s.trim())
    step('the command menu offers Enter, Cancel, the prompt options and snaps', ['Enter', 'Cancel', 'Undo'].every((label) => labels.includes(label)) && labels.some((l) => l.startsWith('Snap Overrides')), labels.join('|'))
    await screenshot('command-menu')
    await kit.getByTestId('cad-command-menu-Undo').click()
    step('a prompt option from the menu runs (Undo takes the segment back)', await until(async () => (await lineCount()) === before + 1 && (await details()).running === 'line.add'))
    await page.keyboard.down('Shift')
    await page.mouse.click(start.x + 150, start.y + 30, { button: 'right' })
    await page.keyboard.up('Shift')
    const snaps = kit.getByTestId('cad-snap-menu')
    step('Shift+right-click opens the object snap menu at a point prompt', await until(async () => (await snaps.count()) === 1))
    await screenshot('snap-menu')
    await kit.getByTestId('cad-snap-menu-_end').click()
    step('choosing Endpoint makes it the next pick\'s snap', await until(async () => /Endpoint of/i.test((await details()).prompt)), (await details()).prompt)
    await page.mouse.click(start.x + 2, start.y + 3)
    await page.mouse.down({ button: 'right' })
    await page.waitForTimeout(400)
    await page.mouse.up({ button: 'right' })
    await until(async () => (await menu.count()) === 1)
    await kit.getByTestId('cad-command-menu-cancel').click()
    step('Cancel from the menu ends LINE', await until(async () => (await details()).running == null))
    const line = await lastLine()
    step('the Endpoint override snapped the pick onto the first line\'s start', line && first && line[1][0] === first[0][0] && line[1][1] === first[0][1], JSON.stringify([first, line]))
    await page.mouse.click(start.x + 300, start.y + 60, { button: 'right' })
    step('a right-click with nothing running still opens the drawing menu', await until(async () => (await kit.getByTestId('cad-context-menu').count()) === 1))
    await page.keyboard.press('Escape')
  })

  // 7. Navigation (N15): double middle click = Zoom Extents, trackpad scroll pans, the wheel zooms.
  await section('navigation', async () => {
    const zoom = () => kit.getByTestId('kit-zoom').innerText()
    await kit.getByTestId('cad-zoom-extents').click()
    await page.waitForTimeout(300)
    const fit = await zoom()
    const center = await toScreen(200, 150)
    await page.mouse.move(center.x, center.y)
    await page.mouse.wheel(0, -100)
    step('a wheel notch zooms', await until(async () => (await zoom()) !== fit))
    const zoomed = await zoom()
    const before = (await details()).view.center
    await page.mouse.wheel(30, 0.5)
    await page.waitForTimeout(400)
    const after = (await details()).view.center
    step('a trackpad scroll pans without zooming', (await zoom()) === zoomed && (Math.abs(after[0] - before[0]) > 1e-6 || Math.abs(after[1] - before[1]) > 1e-6), JSON.stringify([before, after]))
    await command('line')
    await page.mouse.click(center.x, center.y, { button: 'middle' })
    await page.mouse.click(center.x, center.y, { button: 'middle' })
    step('a double middle click zooms to the extents', await until(async () => (await zoom()) === fit), `${await zoom()} vs ${fit}`)
    step('and leaves the command running', (await details()).running === 'line.add')
    await page.keyboard.press('Escape')
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
