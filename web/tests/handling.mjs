// Drafting by hand, round two (stage 8): a narrow rectangle drawn by two clicks keeps its height
// with Ortho and polar tracking on; a click just inside a rectangle that lies over a dimension's
// (undrawn) bounding box picks the rectangle, so BLOCK takes it; one undo step per grip placement
// (a STRETCH copy, several hot grips); the grip menu's Radius and Convert to Line; the dynamic input
// boxes of a hot grip; Select Similar on the drawing menu, the "Select objects" options, a group
// picked as one.
// node tests/handling.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const output = new URL('../test-results/handling/', import.meta.url)
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
const wait = (ms) => page.waitForTimeout(ms)
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details
const prompt = async () => (await details()).drafting?.commandLine?.prompt ?? {}
const gripping = async () => (await prompt()).grip != null
const command = async (...entries) => {
  for (const text of entries) { await input.fill(text); await input.press('Enter'); await wait(150) }
}
const idle = () => until(async () => (await details()).running == null)
/** An agent's commands: the setup a step does not test, and the test's eyes (never while a grip is hot). */
const agent = async (...steps) => {
  const run = await page.evaluate(async (steps) => window.devHost.connection().ai.run(steps.map(([command, params]) => ({ command, params })), false), steps)
  return (run.completed ?? []).map((done) => done?.result ?? done?.value ?? done)
}
const query = async (type) => (await agent(['object.query', type ? { type } : {}]))[0]?.entities ?? []
// Drawing units to page pixels: measured with the coordinate readout (the point under the pointer
// between commands) at two whole pixels, after every change of view.
let scale = null
const calibrate = async () => {
  const box = await surface.boundingBox()
  const probes = [[Math.round(box.x + box.width * 0.3), Math.round(box.y + box.height * 0.3)], [Math.round(box.x + box.width * 0.6), Math.round(box.y + box.height * 0.7)]]
  const world = []
  for (const [x, y] of probes) {
    await page.mouse.move(x + 1, y)
    await page.mouse.move(x, y)
    await wait(200)
    world.push((await text('cad-coordinates')).split(',').map((part) => Number(part.trim())))
  }
  const sx = (probes[1][0] - probes[0][0]) / (world[1][0] - world[0][0])
  const sy = (probes[1][1] - probes[0][1]) / (world[1][1] - world[0][1])
  scale = { sx, sy, x0: probes[0][0] - world[0][0] * sx, y0: probes[0][1] - world[0][1] * sy }
}
const toScreen = async (x, y) => ({ x: scale.x0 + x * scale.sx, y: scale.y0 + y * scale.sy })
const pixel = async () => 1 / Math.abs(scale.sy)
const near = (a, b, tolerance) => Math.abs(a - b) <= tolerance
const text = async (testId) => (await kit.getByTestId(testId).innerText({ timeout: 1000 })).trim()
const shown = () => tooltip.isVisible().then(async (visible) => visible && (await tooltip.evaluate((element) => getComputedStyle(element).visibility)) === 'visible')
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
const click = async (x, y) => {
  const at = await toScreen(x, y)
  await page.mouse.move(at.x, at.y, { steps: 3 })
  await page.mouse.click(at.x, at.y)
  await wait(300)
}
const rightClick = async (at) => {
  await page.mouse.move(at.x, at.y)
  await page.mouse.down({ button: 'right' })
  await page.mouse.up({ button: 'right' })
}
const open = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  // Only the pointer moves the cursor: no running snaps, no polar tracking (a section turns on
  // what it tests).
  await command('osnap {"on":false}', 'polar {"on":false}', 'ortho {"on":false}', 'grid {"on":false}')
  // The sections draw right of the sample (x > 390), in the view the drawing opens with.
  await wait(1000)
  await calibrate()
}
/**
 * Shows `height` drawing units around `center` (between commands), and measures the view: until two
 * measures agree and `center` is in the middle of the canvas (the view is asked again otherwise).
 */
const viewAt = async (center, height) => {
  for (let ask = 0; ask < 4; ask += 1) {
    await agent(['view.set', { center, height }])
    let last = null
    for (let i = 0; i < 10; i += 1) {
      await wait(300)
      await calibrate()
      if (last && near(last.x0, scale.x0, 0.5) && near(last.y0, scale.y0, 0.5) && near(last.sx, scale.sx, 0.01)) break
      last = scale
    }
    const box = await surface.boundingBox()
    const middle = await toScreen(center[0], center[1])
    if (near(middle.x, box.x + box.width / 2, 20) && near(middle.y, box.y + box.height / 2, 20)) return
  }
}
const section = async (name, body) => {
  try {
    await open()
    await body()
  } catch (error) {
    step(`${name} (threw)`, false, error instanceof Error ? error.message.split('\n')[0] : String(error))
    await screenshot(`failure-${name.replace(/\W+/g, '-')}`)
  }
}
const lines = async () => (await query('line')).map((line) => [[line.geometry.a.x, line.geometry.a.y], [line.geometry.b.x, line.geometry.b.y]])
const mine = (found) => found.filter(([a]) => a[0] >= 390)

await mkdir(output, { recursive: true })
try {
  // 1. The acceptance drawing's window: RECTANG's other corner with Ortho and polar tracking on, a
  //    few pixels above the first corner's level.
  await section('rectangle', async () => {
    await command('ortho {"on":true}', 'polar {"on":true}')
    await calibrate()
    await kit.getByTestId('cad-tool-rectangle.add').click()
    await wait(300)
    const perPixel = await pixel()
    const first = await toScreen(400, 200)
    await page.mouse.move(first.x, first.y, { steps: 3 })
    await page.mouse.click(first.x, first.y)
    await wait(300)
    // 40 units across, 4 pixels up: polar tracking's 0° path holds a point this close.
    const other = { x: first.x + Math.round(40 / perPixel), y: first.y - 4 }
    await page.mouse.move(other.x, other.y, { steps: 4 })
    await wait(200)
    await page.mouse.click(other.x, other.y)
    await idle()
    const made = (await query('lwpolyline')).filter((poly) => poly.geometry.vertices.some((v) => near(v.p.x, 400, 2 * perPixel)))
    const ys = made[0]?.geometry.vertices.map((v) => v.p.y) ?? []
    const height = ys.length ? Math.max(...ys) - Math.min(...ys) : 0
    step('with Ortho and polar on, two clicks draw the narrow rectangle', made.length === 1, `${made.length} rectangles; history ${JSON.stringify((await details()).drafting.commandLine.history.lines.slice(-2))}`)
    step('it keeps the height the pointer gave (not squared, not pulled flat)', near(height, 4 * perPixel, 1.5 * perPixel), `height ${height}, 4 pixels = ${4 * perPixel}`)
    await screenshot('rectangle')
  })

  // 2. The acceptance drawing in WebKit: a bed over the 120 dimension's bounding box. The pointer
  //    a little inside the bed, nearer the box's (undrawn) top edge than the bed's edge: BLOCK takes
  //    the bed.
  await section('pick', async () => {
    await command('dimlinear', '400,300', '520,300', '460,288.1')
    await idle()
    // ISO-25 (2.5 high text): the box keeps 6.25 units above the definition points.
    await command('rectang', '495,306', '513,320')
    await idle()
    await viewAt([470, 306], 80)
    await command('block', 'Bed', '495,306')
    step('BLOCK asks for the objects', await until(async () => (await prompt()).kind === 'select'), JSON.stringify(await prompt()))
    const at = await toScreen(504, 306.3)
    await page.mouse.move(at.x, at.y, { steps: 3 })
    await wait(300)
    const pointer = await text('cad-coordinates')
    await page.mouse.click(at.x, at.y)
    await wait(300)
    const history = (await details()).drafting.commandLine.history.lines.slice(-2)
    await page.keyboard.press('Enter')
    await idle()
    const beds = (await query('lwpolyline')).filter((poly) => poly.geometry.vertices.some((v) => near(v.p.x, 495, 1e-6) && near(v.p.y, 306, 1e-6)))
    const dimensions = await query('dimension')
    step('the click inside the bed picks the bed, not the dimension under it', beds.length === 0 && dimensions.some((dimension) => near(dimension.geometry.p13.x, 400, 1e-6)), `pointer at ${pointer}; ${JSON.stringify(history)}; beds left ${beds.length}`)
    step('the bed became the block', (await query('insert')).length === 1 || (await details()).blocks?.some((block) => (block.name ?? block) === 'Bed'))
    await screenshot('pick')
  })

  // 3. One undo step per grip placement: a STRETCH copy, and two hot grips stretched together.
  await section('undo', async () => {
    await command('line', '400,200', '440,200', '')
    await idle()
    // (The canvas settles after the first edits: measure the view again.)
    await calibrate()
    await click(410, 200)
    await until(async () => (await details()).selection === 1)
    await click(440, 200)
    step('the end grip is hot', await until(gripping))
    await command('c')
    await click(450, 220)
    step('the edit goes on placing copies', await until(async () => (await gripping()) && (await prompt()).grip?.copy === true))
    await page.keyboard.press(' ')
    await until(async () => !(await gripping()))
    const copied = mine(await lines())
    step('the copy is there', copied.length === 2, JSON.stringify(copied))
    await command('u')
    await wait(300)
    const after = mine(await lines())
    step('one Undo takes the whole copy back', after.length === 1 && near(after[0][1][0], 440, 1e-6) && near(after[0][1][1], 200, 1e-6), JSON.stringify(after))
    // Two lines, both right ends hot, stretched together.
    await command('line', '400,240', '440,240', '')
    await idle()
    await page.keyboard.press('Escape')
    await wait(300)
    await calibrate()
    await click(410, 200)
    await click(410, 240)
    await until(async () => (await details()).selection === 2)
    for (const y of [200, 240]) {
      const end = await toScreen(440, y)
      await page.keyboard.down('Shift')
      await page.mouse.click(end.x, end.y)
      await page.keyboard.up('Shift')
      await wait(300)
    }
    await click(440, 200)
    await until(gripping)
    await click(460, 200)
    await until(async () => !(await gripping()))
    const stretched = mine(await lines())
    const tolerance = 1.5 * await pixel()
    step('two hot grips stretch together', stretched.filter(([, b]) => near(b[0], 460, tolerance) && (near(b[1], 200, tolerance) || near(b[1], 240, tolerance))).length === 2, JSON.stringify(stretched))
    await command('u')
    await wait(300)
    const back = mine(await lines())
    step('one Undo takes both back', back.length === 2 && back.every(([, b]) => near(b[0], 440, 1e-6)), JSON.stringify(back))
  })

  // 4. The grip menu: Radius at an arc's midpoint, Convert to Line at a polyline arc segment's.
  await section('grip-menu', async () => {
    await command('arc', 'c', '400,200', '420,200', '400,220')
    await idle()
    await calibrate()
    const r = 20
    await click(400 + r * Math.cos(Math.PI / 12), 200 + r * Math.sin(Math.PI / 12))
    await until(async () => (await details()).selection === 1)
    const middle = await toScreen(400 + r * Math.SQRT1_2, 200 + r * Math.SQRT1_2)
    await page.mouse.move(middle.x, middle.y, { steps: 3 })
    await wait(300)
    await rightClick(middle)
    step('the arc midpoint\'s menu offers Radius', await until(async () => (await kit.getByTestId('cad-grip-menu-Radius').count()) === 1))
    await screenshot('radius-menu')
    await kit.getByTestId('cad-grip-menu-Radius').click()
    step('Radius asks for the radius', await until(async () => (await prompt()).command === '** RADIUS **'), JSON.stringify(await prompt()))
    await page.waitForTimeout(300)
    await command('30')
    await until(async () => !(await gripping()))
    const [arc] = (await query('arc')).filter((found) => near(found.geometry.center.x, 400, 1e-6))
    step('30 Enter: the radius is 30 about the same centre', arc && near(arc.geometry.radius, 30, 1e-6), JSON.stringify(arc?.geometry))
    await page.keyboard.press('Escape')
    await wait(300)
    // A polyline whose first segment is a half circle below its chord (bulge 1).
    await agent(['polyline.add', { points: [{ p: [400, 260], bulge: 1 }, [440, 260], [440, 290]] }])
    await calibrate()
    await click(440, 275)
    await until(async () => (await details()).selection === 1)
    const bottom = await toScreen(420, 240)
    await page.mouse.move(bottom.x, bottom.y, { steps: 3 })
    await wait(300)
    await rightClick(bottom)
    step('the arc segment\'s midpoint offers Convert to Line', await until(async () => (await kit.getByTestId('cad-grip-menu-convert-to-Line').count()) === 1), `selection ${(await details()).selection}; ${JSON.stringify((await details()).selectedObjects)}`)
    await kit.getByTestId('cad-grip-menu-convert-to-Line').click()
    await until(async () => !(await gripping()))
    await wait(300)
    const [poly] = (await query('lwpolyline')).filter((found) => near(found.geometry.vertices[0].p.x, 400, 1e-6) && near(found.geometry.vertices[0].p.y, 260, 1e-6))
    step('Convert to Line straightens the segment', poly && poly.geometry.vertices.every((v) => v.bulge === 0), JSON.stringify(poly?.geometry.vertices))
  })

  // 5. A hot grip's dynamic input boxes: distance and angle in STRETCH (a typed number is direct
  //    distance toward the cursor), one angle box in ROTATE.
  await section('grip-boxes', async () => {
    await command('dynmode {"on":true}')
    await command('circle', '400,200', '20')
    await idle()
    await calibrate()
    await click(400, 220)
    await until(async () => (await details()).selection === 1)
    await click(420, 200)
    step('the quadrant grip is hot', await until(gripping))
    const right = await toScreen(430, 200)
    await page.mouse.move(right.x, right.y, { steps: 3 })
    step('the hot grip shows the dynamic input boxes', await until(shown))
    step('distance and angle from the grip', (await tooltip.getAttribute('data-layout')) === 'pair' && (await tooltip.innerText()).includes('@'), (await tooltip.innerText()).replace(/\s+/g, ' '))
    const perPixel = await pixel()
    step('the live distance follows the cursor', await until(async () => near(Number(await text('cad-dynamic-first')), 10, 1.5 * perPixel)), await text('cad-dynamic-first'))
    await screenshot('grip-boxes')
    await page.keyboard.type('15')
    step('typed on the canvas, the number is in the box', await until(async () => (await text('cad-dynamic-first')) === '15'))
    await page.keyboard.press('Enter')
    await until(async () => !(await gripping()))
    const [circle] = (await query('circle')).filter((found) => near(found.geometry.center.x, 400, 1e-6))
    step('Enter: the grip goes 15 toward the cursor (radius 35; the pointer a pixel off level tilts it a little)', circle && near(circle.geometry.radius, 35, 0.05), JSON.stringify(circle?.geometry))
    await click(435, 200)
    await until(gripping)
    await page.keyboard.press(' ')
    await until(async () => (await prompt()).command === '** MOVE **')
    await page.keyboard.press(' ')
    await until(async () => (await prompt()).command === '** ROTATE **')
    const above = await toScreen(435, 220)
    await page.mouse.move(above.x, above.y, { steps: 3 })
    // (About 90°: the pointer is straight above the grip, to a pixel.)
    step('ROTATE shows one angle box', await until(async () => (await shown()) && (await tooltip.getAttribute('data-layout')) === 'single' && near(parseFloat(await text('cad-dynamic-value')), 90, 5) && (await text('cad-dynamic-value')).endsWith('°')), `${await tooltip.getAttribute('data-layout')} ${(await tooltip.innerText().catch(() => '')).replace(/\s+/g, ' ')} ${JSON.stringify(await prompt())}`)
    await screenshot('grip-rotate-box')
    await page.keyboard.press('Escape')
  })

  // 6. Selecting: Select Similar, the "Select objects" options, a group picked as one.
  await section('selecting', async () => {
    await command('line', '400,200', '440,200', '')
    await command('line', '400,220', '440,220', '')
    await command('circle', '470,210', '8')
    await idle()
    await calibrate()
    await click(410, 200)
    await until(async () => (await details()).selection === 1)
    const layer = (await details()).selectedObjects[0]?.layer
    const expected = (await query('line')).filter((line) => line.layer === layer).length
    await rightClick(await toScreen(380, 260))
    step('the drawing menu offers Select Similar', await until(async () => (await kit.getByTestId('cad-context-select.similar').count()) === 1))
    await screenshot('select-similar')
    await kit.getByTestId('cad-context-select.similar').click()
    step('Select Similar selects every line on the layer', await until(async () => (await details()).selection === expected), `${(await details()).selection} of ${expected}`)
    step('and not the circle', (await details()).selectedObjects.every((object) => object.type === 'Line'))
    await page.keyboard.press('Escape')
    await wait(300)
    await page.keyboard.press('Escape')
    await until(async () => (await details()).selection === 0)
    await command('move')
    step('"Select objects" offers ALL, Last, Previous, Add and Remove', await until(async () => JSON.stringify((await prompt()).keywords) === JSON.stringify(['ALL', 'Last', 'Previous', 'Add', 'Remove'])), JSON.stringify(await prompt()))
    await kit.getByTestId('cad-prompt-option-Last').click()
    step('Last, clicked, picks the last object drawn', await until(async () => (await details()).selection === 1 && (await details()).selectedObjects[0]?.type === 'Circle'))
    await page.keyboard.press('Escape')
    await wait(300)
    await page.keyboard.press('Escape')
    await idle()
    // A group of the two lines (made by an agent: the setup).
    const pair = mine(await lines()).length
    const handles = (await query('line')).filter((line) => line.geometry.a.x >= 390).map((line) => line.handle)
    await agent(['arrange.group', { name: 'Pair', handles }])
    await agent(['edit.deselectAll', {}])
    await click(412, 220)
    step('a click on one member selects the group', await until(async () => (await details()).selection === pair), `${(await details()).selection} of ${pair}`)
    await screenshot('group')
  })
} finally {
  if (errors.length) step('no page errors', false, errors.join(' | '))
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} step(s) failed` : 'all steps passed')
process.exit(failures ? 1 : 0)
