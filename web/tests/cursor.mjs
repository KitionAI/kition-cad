// The cursor and the canvas: the crosshair and pick box in their three states, the rollover
// highlight, the coordinate readout, the pick box as the pick range, the canvas look (the canvas
// colour everywhere, colour 7 against it, an adaptive grid over the whole canvas, the UCS icon),
// the selection window's fill, MOVE's guide line, and the status bar's Zoom Extents. It also
// measures how long a pointer move takes to reach the screen (pointer move to readout update).
// node tests/cursor.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const output = new URL('../test-results/cursor/', import.meta.url)
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
// Drawing units to page pixels, from the view the editor shows.
const toScreen = async (x, y) => {
  const { view } = await details()
  const box = await surface.boundingBox()
  const perPixel = view.height / box.height
  return { x: box.x + box.width / 2 + (x - view.center[0]) / perPixel, y: box.y + box.height / 2 - (y - view.center[1]) / perPixel }
}
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  // Only what a section turns on moves the cursor: no running snaps, no polar tracking. New
  // objects go on layer 0 (colour 7), not on the sample's current layer (Walls, blue).
  await command('osnap {"on":false}', 'polar {"on":false}', 'grid {"on":true}', 'layer.current {"name":"0"}')
}
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
// The canvas pixels of a window around page point (x, y), as [r, g, b] triples (CSS pixels).
const pixels = async (x, y, halfX, halfY = halfX) => {
  const box = await surface.boundingBox()
  return surface.evaluate((canvas, { x, y, halfX, halfY }) => {
    const scale = canvas.width / canvas.clientWidth
    const left = Math.round((x - halfX) * scale)
    const top = Math.round((y - halfY) * scale)
    const data = canvas.getContext('2d').getImageData(left, top, Math.max(1, Math.round(2 * halfX * scale)), Math.max(1, Math.round(2 * halfY * scale))).data
    const out = []
    for (let i = 0; i < data.length; i += 4) out.push([data[i], data[i + 1], data[i + 2]])
    return out
  }, { x: x - box.x, y: y - box.y, halfX, halfY })
}
const any = async (x, y, half, test) => (await pixels(x, y, half)).some(test)
// The colour most pixels of a window have.
const mode = async (x, y, half = 5) => {
  const counts = new Map()
  for (const p of await pixels(x, y, half)) counts.set(p.join(','), (counts.get(p.join(',')) ?? 0) + 1)
  return [...counts.entries()].sort((a, b) => b[1] - a[1])[0][0].split(',').map(Number)
}
const close = (a, b, tolerance = 3) => a.every((v, i) => Math.abs(v - b[i]) <= tolerance)
const dark = ([r, g, b]) => r + g + b < 300
const blue = ([r, g, b]) => b - r > 60 && b - g > 40
const readout = () => kit.getByTestId('cad-coordinates').innerText({ timeout: 1000 })
const section = async (name, body, query = '') => {
  try {
    await open(query)
    await body()
  } catch (error) {
    step(`${name} (threw)`, false, error instanceof Error ? error.message.split('\n')[0] : String(error))
    await screenshot(`failure-${name.replace(/\W+/g, '-')}`)
  }
}
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.floor(sorted.length / 2)]
}
// Pointer move to screen: the median time from a pointer move on the canvas to the coordinate
// readout (written as the canvas draws the engine's answer) showing the new place.
const latency = async (from, to, moves = 31) => kit.locator('body').evaluate(async (body, { from, to, moves }) => {
  const canvas = body.querySelector('[data-testid="kit-canvas-surface"]')
  const shown = body.querySelector('[data-testid="cad-coordinates"]')
  const box = canvas.getBoundingClientRect()
  const times = []
  for (let i = 0; i < moves; i += 1) {
    const at = i % 2 ? to : from
    const before = shown.textContent
    const start = performance.now()
    const changed = new Promise((resolve) => {
      const observer = new MutationObserver(() => { if (shown.textContent !== before) { observer.disconnect(); resolve(performance.now()) } })
      observer.observe(shown, { childList: true, characterData: true, subtree: true })
      setTimeout(() => { observer.disconnect(); resolve(null) }, 3000)
    })
    canvas.dispatchEvent(new PointerEvent('pointermove', { clientX: box.left + at.x + (i % 7), clientY: box.top + at.y, pointerId: 1, pointerType: 'mouse', bubbles: true, cancelable: true }))
    const end = await changed
    if (end != null) times.push(end - start)
    await new Promise((resolve) => setTimeout(resolve, 30))
  }
  return times
}, { from, to, moves })

await mkdir(output, { recursive: true })
try {
  // 1. The crosshair and pick box (N1): idle = crosshair and box, a point prompt = crosshair, a
  //    "Select objects" prompt = the box alone; the system cursor is hidden over the canvas.
  await section('crosshair', async () => {
    const at = await toScreen(380, 270)
    const arms = () => any(at.x + 20, at.y, 3, dark)
    const box = () => any(at.x + 4.5, at.y + 4.5, 1.5, dark)
    await page.mouse.move(at.x - 30, at.y)
    await page.mouse.move(at.x, at.y, { steps: 3 })
    step('idle: crosshair arms at the pointer', await until(arms))
    step('idle: the pick box at the pointer', await until(box))
    step('the system cursor is hidden over the canvas', await until(async () => (await surface.evaluate((canvas) => getComputedStyle(canvas).cursor)) === 'none'), await surface.evaluate((canvas) => getComputedStyle(canvas).cursor))
    await screenshot('idle')
    await command('line')
    await page.mouse.move(at.x - 5, at.y)
    await page.mouse.move(at.x, at.y)
    step('point prompt: crosshair arms', await until(arms))
    step('point prompt: no pick box', await until(async () => !(await box())))
    await page.keyboard.press('Escape')
    await idle()
    await command('erase')
    await page.mouse.move(at.x - 5, at.y)
    await page.mouse.move(at.x, at.y)
    step('"Select objects": the pick box', await until(box))
    step('"Select objects": no crosshair arms', await until(async () => !(await arms())))
    await screenshot('select-objects')
    await page.keyboard.press('Escape')
    await idle()
    // Off the canvas (over the layers panel) the crosshair goes and the system cursor is back.
    await page.mouse.move(at.x, at.y)
    await until(arms)
    const panel = await kit.locator('.cad-side').boundingBox()
    await page.mouse.move(panel.x + panel.width / 2, panel.y + panel.height / 2, { steps: 4 })
    step('off the canvas the crosshair is not drawn', await until(async () => !(await arms())))
    step('and the panel keeps the system cursor', (await kit.locator('.cad-side').evaluate((el) => getComputedStyle(el).cursor)) !== 'none')
  })

  // 2. The rollover highlight (N4): the object under the pick box, idle and at "Select objects",
  //    not at a point prompt; gone when the pointer moves off.
  await section('rollover', async () => {
    await command('circle', '360,255', '15')
    await idle()
    const edge = await toScreen(375, 255)
    const far = await toScreen(345, 255)
    const highlighted = () => any(far.x, far.y, 4, blue)
    await page.mouse.move(edge.x + 30, edge.y)
    step('no highlight away from the circle', await until(async () => !(await highlighted())))
    await page.mouse.move(edge.x, edge.y, { steps: 3 })
    step('idle: hovering the circle highlights it', await until(highlighted))
    await screenshot('rollover')
    await page.mouse.move(edge.x + 30, edge.y, { steps: 3 })
    step('moving off clears the highlight', await until(async () => !(await highlighted())))
    await command('erase')
    await page.mouse.move(edge.x, edge.y, { steps: 3 })
    step('"Select objects": hovering highlights', await until(highlighted))
    await page.keyboard.press('Escape')
    await idle()
    await command('line')
    await page.mouse.move(edge.x + 1, edge.y, { steps: 3 })
    await page.waitForTimeout(300)
    step('a point prompt highlights nothing', !(await highlighted()))
    await page.keyboard.press('Escape')
  })

  // 3. The coordinate readout (N3) follows the pointer and, snapped, is the snap point; the
  //    pick box is the pick range (a click 3 pixels off a line picks it, 9 off does not).
  await section('readout', async () => {
    const at = await toScreen(380, 270)
    await page.mouse.move(at.x, at.y, { steps: 3 })
    const parse = async () => (await readout()).split(',').map((v) => Number.parseFloat(v))
    step('the readout shows the pointer\'s drawing coordinates', await until(async () => { const [x, y] = await parse(); return Math.abs(x - 380) < 1 && Math.abs(y - 270) < 1 }), await readout().catch(() => 'none'))
    step('with LUPREC places', /^-?\d+\.\d{4}, -?\d+\.\d{4}/.test(await readout().catch(() => '')), await readout().catch(() => 'none'))
    const there = await toScreen(320, 250)
    await page.mouse.move(there.x, there.y, { steps: 3 })
    step('the readout follows the pointer', await until(async () => { const [x, y] = await parse(); return Math.abs(x - 320) < 1 && Math.abs(y - 250) < 1 }), await readout().catch(() => 'none'))
    await command('osnap {"on":true}', 'line')
    const corner = await toScreen(300, 220)
    await page.mouse.move(corner.x + 3, corner.y - 2, { steps: 3 })
    step('snapped, the readout is the endpoint', await until(async () => (await readout()).startsWith('300.0000, 220.0000')), await readout().catch(() => 'none'))
    const arms = (x, y) => any(x + 20, y, 3, dark)
    step('snapped, the crosshair is on the endpoint', await until(() => arms(corner.x, corner.y)) && !(await arms(corner.x + 3, corner.y - 8)))
    await page.keyboard.press('Escape')
    await idle()
    await command('osnap {"on":false}')
    // A wall at y = 220 from x = 40 to 170: 3 pixels above it picks it, 9 pixels above does not.
    const wall = await toScreen(100, 220)
    // Escape goes to the engine once it has answered the click (it is busy then: a window is open).
    await page.mouse.click(wall.x, wall.y - 9)
    await page.waitForTimeout(300)
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
    await until(async () => (await details()).selection === 0)
    await page.mouse.click(wall.x, wall.y - 3)
    step('a click within the pick box picks', await until(async () => (await details()).selection === 1), String((await details()).selection))
    await page.keyboard.press('Escape')
    await until(async () => (await details()).selection === 0)
    await page.mouse.click(wall.x, wall.y - 9)
    await page.waitForTimeout(300)
    step('a click outside the pick box does not', (await details()).selection === 0, String((await details()).selection))
    await page.keyboard.press('Escape')
  })

  // 4. The canvas look (N17): the canvas colour inside and outside the drawing's area, an
  //    adaptive grid over the whole canvas that stays visible and light when zoomed far out, the
  //    UCS icon; the status bar's Zoom Extents (and the selection window's fill, MOVE's guide).
  await section('canvas', async () => {
    const box = await surface.boundingBox()
    const inside = await toScreen(390, 290)
    const outside = { x: box.x + 6, y: box.y + box.height / 2 }
    const background = await mode(inside.x, inside.y)
    step('the drawing area has the canvas colour all around', close(await mode(outside.x, outside.y), background), `${background} vs ${await mode(outside.x, outside.y)}`)
    // Far out, the grid still covers the canvas: some pixels are grid, most are background.
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    for (let i = 0; i < 14; i += 1) { await page.mouse.wheel(0, 100); await page.waitForTimeout(40) }
    await page.waitForTimeout(500)
    await screenshot('zoomed-out')
    const corner = await pixels(box.x + box.width - 80, box.y + 80, 40)
    const plain = corner.filter((p) => close(p, background, 2)).length / corner.length
    step('zoomed far out, the grid is still drawn', plain < 1, `${(plain * 100).toFixed(1)}% background`)
    step('and does not turn the canvas grey', plain > 0.6, `${(plain * 100).toFixed(1)}% background`)
    await command('grid {"on":false}')
    await page.waitForTimeout(300)
    const off = await pixels(box.x + box.width - 80, box.y + 80, 40)
    step('F7 off removes it', off.every((p) => close(p, background, 2)))
    await command('grid {"on":true}')
    step('the UCS icon is in the lower left corner', await until(() => any(box.x + 34, box.y + box.height - 34, 26, dark)))
    const zoomedOut = (await details()).view.height
    await kit.getByTestId('cad-status-zoom-extents').click()
    await until(async () => (await details()).view.height < zoomedOut / 2)
    await page.waitForTimeout(500)
    const fit = await toScreen(300, 220)
    step('the status bar\'s Zoom Extents brings the drawing back', fit.x > box.x && fit.x < box.x + box.width && fit.y > box.y && fit.y < box.y + box.height, JSON.stringify(fit))
    // The selection window: blue see-through fill left to right, green right to left.
    // Inside the room, where nothing is: the drawing's extents fill the view now.
    const a = await toScreen(185, 208)
    const b = await toScreen(290, 168)
    await page.mouse.click(a.x, a.y)
    await page.mouse.move(b.x, b.y, { steps: 4 })
    const middle = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }
    step('a window (left to right) is filled blue', await until(async () => { const [r, , bl] = await mode(middle.x, middle.y, 3); return bl - r > 6 }), String(await mode(middle.x, middle.y, 3)))
    await screenshot('window')
    await page.keyboard.press('Escape')
    await page.mouse.click(b.x, b.y)
    await page.mouse.move(a.x, a.y, { steps: 4 })
    step('a crossing (right to left) is filled green', await until(async () => { const [r, g, bl] = await mode(middle.x, middle.y, 3); return g - r > 4 && g - bl > 2 }), String(await mode(middle.x, middle.y, 3)))
    await page.keyboard.press('Escape')
    // MOVE: the line from the base point to the cursor is a guide in its own colour.
    await command('circle', '240,195', '8', 'move', 'last', '', '240,195')
    const from = await toScreen(240, 195)
    const to = await toScreen(280, 175)
    await page.mouse.move(to.x, to.y, { steps: 4 })
    const mid = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 }
    step('MOVE\'s base-to-cursor line is drawn as a guide, not as drawing', await until(() => any(mid.x, mid.y, 3, ([r, g, bl]) => bl - r > 80 && bl > 200)))
    await screenshot('move-guide')
    await page.keyboard.press('Escape')
  })

  // 5. A dark host theme: the canvas is dark, and colour 7 draws white on it.
  await section('dark theme', async () => {
    await command('line', '330,280', '410,280', '')
    await idle()
    const empty = await toScreen(380, 255)
    const [r, g, b] = await mode(empty.x, empty.y)
    step('the canvas follows the dark theme', r + g + b < 200, `${r},${g},${b}`)
    const on = await toScreen(370, 280)
    const lightAndDark = async () => { const around = await pixels(on.x, on.y, 3); return around.some(([r, g, b]) => r > 180 && g > 180 && b > 180) && around.some(([r, g, b]) => r + g + b < 200) }
    step('colour 7 draws light on it', await until(lightAndDark))
    await screenshot('dark')
  }, '&scheme=dark')

  // 6. Pointer move to screen, on the sample and on 5000 objects (reported, not a gate).
  await section('latency', async () => {
    const box = await surface.boundingBox()
    const from = { x: box.width * 0.4, y: box.height * 0.5 }
    const to = { x: box.width * 0.6, y: box.height * 0.45 }
    let times = await latency(from, to)
    step('every move reached the readout (sample)', times.length > 25, String(times.length))
    console.log(`INFO ${which} pointer move to screen, sample (7 objects): median ${median(times).toFixed(1)} ms over ${times.length} moves`)
    await command('line', '0,0', '2,1.5', '')
    await idle()
    const made = await page.evaluate(async () => {
      const ai = window.devHost.connection().ai
      const query = await ai.run([{ command: 'object.query', params: { type: 'line' } }], false)
      const found = query.completed?.[0]?.result ?? query.completed?.[0]?.value ?? query.completed?.[0]
      const last = (found?.entities ?? []).at(-1)
      const handle = last?.handle ?? last?.id
      return ai.run([{ command: 'object.arrayRectangular', params: { handles: [handle], rows: 50, cols: 100, rowSpacing: 3, colSpacing: 4 } }], false)
    })
    step('5000 more objects', await until(async () => (await details()).entities >= 5000, 100), `${(await details()).entities} ${JSON.stringify(made).slice(0, 200)}`)
    await kit.getByTestId('cad-status-zoom-extents').click()
    await page.waitForTimeout(500)
    times = await latency(from, to)
    step('every move reached the readout (5000 objects)', times.length > 25, String(times.length))
    console.log(`INFO ${which} pointer move to screen, ${(await details()).entities} objects: median ${median(times).toFixed(1)} ms over ${times.length} moves`)
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
