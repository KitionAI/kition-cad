// Panels and the shell by hand (stage 9): the insert dialog (the block list, a scale, a rotation,
// the insertion point clicked on the drawing with the block's ghost under the cursor; typed INSERT
// opens it) and a block reference picked by what it draws; more of the Properties panel writes
// (a polyline's vertices one by one, the positions of an ellipse, a point and a block reference,
// a block's scale and rotation) and its linetype list loads every library linetype; the status
// bar's Transparency toggle and the Drafting Settings dialog behind its gear; the layout tabs' "+"
// and their menu (rename, delete); clicked points on the command history and Copy on its menu.
// node tests/shell.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const output = new URL('../test-results/shell/', import.meta.url)
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
const history = async () => (await details()).drafting?.commandLine?.history?.lines ?? []
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
const text = async (testId) => (await kit.getByTestId(testId).innerText({ timeout: 1000 })).trim()
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
const toScreen = (x, y) => ({ x: scale.x0 + x * scale.sx, y: scale.y0 + y * scale.sy })
const perPixel = () => 1 / Math.abs(scale.sy)
const near = (a, b, tolerance) => Math.abs(a - b) <= tolerance
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
const click = async (x, y) => {
  const at = toScreen(x, y)
  await page.mouse.move(at.x, at.y, { steps: 3 })
  await page.mouse.click(at.x, at.y)
  await wait(300)
}
const rightClick = async (locator) => {
  const box = await locator.boundingBox()
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await page.mouse.down({ button: 'right' })
  await page.mouse.up({ button: 'right' })
  await wait(300)
}
// The canvas pixels of a window around page point (x, y) that are not the canvas's colour there
// before anything was drawn (`blank`): how much is drawn in it.
const drawn = async (x, y, half, blank) => {
  const bounds = await surface.boundingBox()
  return surface.evaluate((canvas, { x, y, half, blank }) => {
    const ratio = canvas.width / canvas.clientWidth
    const data = canvas.getContext('2d').getImageData(Math.round((x - half) * ratio), Math.round((y - half) * ratio), Math.max(1, Math.round(2 * half * ratio)), Math.max(1, Math.round(2 * half * ratio))).data
    let count = 0
    for (let i = 0; i < data.length; i += 4) if (Math.abs(data[i] - blank[0]) + Math.abs(data[i + 1] - blank[1]) + Math.abs(data[i + 2] - blank[2]) > 60) count += 1
    return count
  }, { x: x - bounds.x, y: y - bounds.y, half, blank })
}
const colourAt = async (x, y) => {
  const bounds = await surface.boundingBox()
  return surface.evaluate((canvas, { x, y }) => {
    const ratio = canvas.width / canvas.clientWidth
    const data = canvas.getContext('2d').getImageData(Math.round(x * ratio), Math.round(y * ratio), 1, 1).data
    return [data[0], data[1], data[2]]
  }, { x: x - bounds.x, y: y - bounds.y })
}
const open = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  // Only the pointer moves the cursor: no running snaps, no polar tracking (a section turns on
  // what it tests).
  await command('osnap {"on":false}', 'polar {"on":false}', 'ortho {"on":false}', 'grid {"on":false}')
  await wait(1000)
  await calibrate()
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

await mkdir(output, { recursive: true })
try {
  // 1. The insert dialog: Home › Insert lists the drawing's blocks; OK with "Specify on-screen"
  //    starts INSERT with the block, the scale and the rotation answered; the block's ghost follows
  //    the cursor and a click places it. Typed INSERT opens the dialog; a typed point inserts at
  //    once. The block reference is picked by what it draws, not by its box.
  await section('insert', async () => {
    const [circle] = await agent(['circle.add', { center: [450, 220], radius: 4 }])
    await agent(['block.add', { name: 'Chair', base: [450, 220], handles: [circle.handle] }], ['block.add', { name: 'Bed', base: [0, 0], handles: [(await agent(['rectangle.add', { p1: [0, 0], p2: [18, 14] }]))[0].handle] }])
    await calibrate()
    await kit.getByTestId('cad-insert-block').click()
    step('Home › Insert opens the insert dialog', await until(async () => (await kit.getByTestId('cad-insert-dialog').count()) === 1))
    const names = await kit.getByTestId('cad-insert-name').locator('option').allInnerTexts()
    step('it lists the drawing\'s blocks', names.map((name) => name.trim()).join(',') === 'Bed,Chair', names.join(','))
    await kit.getByTestId('cad-insert-name').selectOption('Chair')
    await kit.getByTestId('cad-insert-scale').fill('2')
    step('the insertion point is specified on the drawing by default', await kit.getByTestId('cad-insert-on-screen').isChecked() && await kit.getByTestId('cad-insert-x').isDisabled())
    await screenshot('insert-dialog')
    await kit.getByTestId('cad-insert-ok').click()
    step('OK starts INSERT at its insertion point', await until(async () => (await details()).running === 'block.insert' && /insertion point/.test((await prompt()).message ?? '')), JSON.stringify(await prompt()))
    const lines = await history()
    step('the history shows the block, the scale and the rotation answered', lines.some((line) => /Chair$/.test(line)) && lines.some((line) => /scale factor.* 2$/.test(line)), JSON.stringify(lines.slice(-6)))
    // The ghost: a circle of radius 8 around the cursor.
    const where = toScreen(480, 220)
    const ring = { x: where.x + 8 / perPixel(), y: where.y }
    const blank = await colourAt(ring.x, ring.y - 30)
    const before = await drawn(ring.x, ring.y, 4, blank)
    await page.mouse.move(where.x - 3, where.y, { steps: 2 })
    await page.mouse.move(where.x, where.y)
    step('the block\'s ghost follows the cursor', await until(async () => (await drawn(ring.x, ring.y, 4, blank)) > before + 3), `${before} pixels before`)
    await screenshot('insert-ghost')
    await page.mouse.click(where.x, where.y)
    await idle()
    const placed = (await query('insert')).filter((ref) => ref.geometry.block === 'Chair')
    const at = placed[0]?.geometry.insert
    step('the click places the block there, at the scale chosen', placed.length === 1 && near(at.x, 480, 2 * perPixel()) && near(at.y, 220, 2 * perPixel()) && near(placed[0].geometry.scale.x, 2, 1e-9), JSON.stringify(placed.map((ref) => ref.geometry)))
    // Typed INSERT: the dialog, with the last block; a point typed inserts at once.
    await command('insert')
    step('typed INSERT opens the dialog', await until(async () => (await kit.getByTestId('cad-insert-dialog').count()) === 1), JSON.stringify((await history()).slice(-2)))
    step('with the block inserted last', (await kit.getByTestId('cad-insert-name').inputValue()) === 'Chair')
    await kit.getByTestId('cad-insert-on-screen').uncheck()
    await kit.getByTestId('cad-insert-x').fill('520')
    await kit.getByTestId('cad-insert-y').fill('240')
    await kit.getByTestId('cad-insert-scale').fill('1')
    await kit.getByTestId('cad-insert-rotation').fill('30')
    await kit.getByTestId('cad-insert-ok').click()
    const typed = async () => (await query('insert')).find((ref) => ref.geometry.block === 'Chair' && near(ref.geometry.insert.x, 520, 1e-9))
    step('a typed insertion point inserts the block at once', await until(async () => (await typed()) != null))
    step('turned as asked', near((await typed())?.geometry.rotation ?? 0, Math.PI / 6, 1e-9), JSON.stringify((await typed())?.geometry))
    step('the dialog closed and nothing runs', (await kit.getByTestId('cad-insert-dialog').count()) === 0 && (await details()).running == null)
    // Picked by what it draws: the corner of the first chair's box (radius 8 at 480,220) is empty.
    await calibrate()
    await click(480 + 7.9, 220 + 7.9)
    step('a click by the corner of the block\'s box picks nothing', (await details()).selection === 0, `${(await details()).selection} selected`)
    // (The click on empty paper opened a selection window: Escape drops it.)
    await page.keyboard.press('Escape')
    await wait(300)
    await click(488, 220)
    step('a click on its circle picks it', await until(async () => (await details()).selection === 1 && (await details()).selectedObjects[0]?.type === 'Block Reference'), JSON.stringify((await details()).selectedObjects))
    await page.keyboard.press('Escape')
    await wait(300)
    // Escape closes the dialog and the keys go back to the drawing.
    await kit.getByTestId('cad-insert-block').click()
    await until(async () => (await kit.getByTestId('cad-insert-dialog').count()) === 1)
    await wait(300)
    await page.keyboard.press('Escape')
    step('Escape closes the dialog', await until(async () => (await kit.getByTestId('cad-insert-dialog').count()) === 0))
    await wait(300)
    await page.keyboard.type('line')
    await page.keyboard.press('Enter')
    step('and what is typed next goes to the drawing', await until(async () => (await details()).running === 'line.add'), JSON.stringify(await prompt()))
    await page.keyboard.press('Escape')
  })

  // 2. The Properties panel writes more: a polyline's vertices one by one, an ellipse's centre, a
  //    block reference's position and rotation; its linetype list has every library linetype, and
  //    choosing one not loaded loads it.
  await section('properties', async () => {
    const [poly] = await agent(['polyline.add', { points: [[400, 200], [440, 200], [440, 230]] }])
    await agent(['ellipse.add', { center: [470, 215], major: [8, 0], ratio: 0.5 }])
    const [circle] = await agent(['circle.add', { center: [0, 0], radius: 3 }])
    await agent(['block.add', { name: 'Stool', base: [0, 0], handles: [circle.handle] }], ['block.insert', { name: 'Stool', at: [500, 215] }])
    await calibrate()
    const field = (key) => kit.getByTestId(`cad-property-${key}`)
    const vertices = async () => (await query('lwpolyline')).find((item) => item.handle === poly.handle)?.geometry.vertices.map((v) => [v.p.x, v.p.y]) ?? []
    // A click on empty paper gives the keyboard to the drawing (and opens a window): Escape drops
    // the window, Escape again the selection.
    const deselect = async () => {
      await click(460, 260)
      await page.keyboard.press('Escape')
      await wait(300)
      await page.keyboard.press('Escape')
      await until(async () => (await details()).selection === 0)
    }
    await click(420, 200)
    step('a click selects the polyline', await until(async () => (await details()).selection === 1))
    step('the panel shows its first vertex', await until(async () => (await text('cad-property-currentVertex')) === '1' && (await field('vertex.x').inputValue()) === '400'), `${await text('cad-property-currentVertex').catch(() => '?')} at ${await field('vertex.x').inputValue().catch(() => '?')}`)
    await kit.getByTestId('cad-property-vertex-next').click()
    step('the next button steps to the second vertex', await until(async () => (await text('cad-property-currentVertex')) === '2' && (await field('vertex.x').inputValue()) === '440'))
    await field('vertex.y').fill('195')
    await field('vertex.y').press('Enter')
    step('its Y moves that vertex alone', await until(async () => JSON.stringify(await vertices()) === JSON.stringify([[400, 200], [440, 195], [440, 230]])), JSON.stringify(await vertices()))
    await screenshot('properties-vertex')
    // Every library linetype is listed; one not loaded loads when chosen.
    const library = (await agent(['object.describeProperties', { handles: [] }]))[0]?.library ?? []
    const offered = (await field('linetype').locator('option').allInnerTexts()).map((name) => name.trim())
    step('the linetype list has every library linetype', library.length > 5 && library.every((name) => offered.includes(name)), `${library.length} in the library, missing ${library.filter((name) => !offered.includes(name)).join(', ')}`)
    const choice = library.includes('DASHED') ? 'DASHED' : library[0]
    await field('linetype').selectOption(choice)
    const linetype = async () => (await agent(['object.describeProperties', { handles: [poly.handle] }]))[0]?.rows?.find((row) => row.key === 'linetype')?.value
    step('choosing one not loaded loads it and gives it to the polyline', await until(async () => (await linetype()) === choice), String(await linetype()))
    await deselect()
    // The ellipse's centre.
    await click(478, 215)
    step('a click selects the ellipse', await until(async () => (await details()).selectedObjects[0]?.type === 'Ellipse'), JSON.stringify((await details()).selectedObjects))
    await field('center.x').fill('480')
    await field('center.x').press('Enter')
    const ellipse = async () => (await query('ellipse'))[0]?.geometry.center
    step('its Center X moves it', await until(async () => (await ellipse())?.x === 480 && (await ellipse())?.y === 215), JSON.stringify(await ellipse()))
    await deselect()
    // The block reference's position and rotation.
    await click(503, 215)
    step('a click selects the block reference', await until(async () => (await details()).selectedObjects[0]?.type === 'Block Reference'), JSON.stringify((await details()).selectedObjects))
    await field('position.x').fill('510')
    await field('position.x').press('Enter')
    const stool = async () => (await query('insert')).find((ref) => ref.geometry.block === 'Stool')?.geometry
    step('its Position X moves it', await until(async () => (await stool())?.insert.x === 510), JSON.stringify(await stool()))
    await until(async () => (await field('rotation').inputValue()) === '0')
    await field('rotation').fill('30')
    await field('rotation').press('Enter')
    step('its Rotation turns it', await until(async () => near((await stool())?.rotation ?? 0, Math.PI / 6, 1e-9)), JSON.stringify(await stool()))
    await screenshot('properties-block')
  })

  // 3. The status bar: Transparency shows the objects' transparency; the gear opens Drafting
  //    Settings (Snap and Grid, Polar Tracking, Object Snap, Dynamic Input), which reads the
  //    drawing's settings and writes what changed, in the middle of a command too.
  await section('status bar', async () => {
    const drafting = async () => (await details()).drafting
    // A thick, 80% transparent line where nothing else is.
    const [line] = await agent(['line.add', { points: [[400, 230], [460, 230]] }])
    await agent(['object.setProperties', { handles: [line.handle ?? line.handles?.[0]], color: 'Red', lineweight: 1, transparency: 80 }])
    await calibrate()
    const on = toScreen(430, 230)
    const red = async () => surface.evaluate((canvas, { x, y }) => {
      const ratio = canvas.width / canvas.clientWidth
      const data = canvas.getContext('2d').getImageData(Math.round((x - 3) * ratio), Math.round((y - 3) * ratio), Math.round(6 * ratio), Math.round(6 * ratio)).data
      let best = 0
      for (let i = 0; i < data.length; i += 4) best = Math.max(best, data[i] - Math.max(data[i + 1], data[i + 2]))
      return best
    }, { x: on.x - (await surface.boundingBox()).x, y: on.y - (await surface.boundingBox()).y })
    await until(async () => (await red()) > 100)
    const solid = await red()
    await kit.getByTestId('cad-mode-transparency').click()
    step('Transparency turns on', await until(async () => (await drafting()).transparency === true))
    step('and the 80% transparent line is drawn faint', await until(async () => (await red()) < solid / 2), `red ${solid} → ${await red()}`)
    await screenshot('status-transparency')
    await kit.getByTestId('cad-mode-transparency').click()
    step('off again, it is drawn solid', await until(async () => (await drafting()).transparency === false && (await red()) > solid * 0.8), `red ${await red()}`)
    // Lineweight: the 1 mm line is drawn four hairlines wide. Measured as ink (how red the pixels
    // across the line are, summed), not as pixels touched: a hairline's antialiasing spreads about
    // one pixel of ink over two rows, and how soft a wide line's edges are differs by browser
    // (WebKit on Linux draws it as four solid rows, twice the rows a hairline touches).
    const ink = async () => surface.evaluate((canvas, { x, y }) => {
      const ratio = canvas.width / canvas.clientWidth
      const data = canvas.getContext('2d').getImageData(Math.round((x - 6) * ratio), Math.round((y - 6) * ratio), Math.round(12 * ratio), Math.round(12 * ratio)).data
      let sum = 0
      for (let i = 0; i < data.length; i += 4) sum += Math.max(0, data[i] - Math.max(data[i + 1], data[i + 2]))
      return sum / ratio / ratio
    }, { x: on.x - (await surface.boundingBox()).x, y: on.y - (await surface.boundingBox()).y })
    const thin = await ink()
    await kit.getByTestId('cad-mode-lwdisplay').click()
    step('Lineweight draws the 1 mm line wider', await until(async () => (await ink()) > thin * 2.5), `ink ${Math.round(thin)} → ${Math.round(await ink())}`)
    await kit.getByTestId('cad-mode-lwdisplay').click()
    await until(async () => (await drafting()).lwdisplay === false)
    step('and as a hairline again when it is off', await until(async () => (await ink()) < thin * 1.3), `ink ${Math.round(await ink())}, hairline ${Math.round(thin)}`)
    // The gear, while LINE runs.
    await command('line', '400,250')
    await kit.getByTestId('cad-status-settings').click()
    step('the gear opens Drafting Settings', await until(async () => (await kit.getByTestId('cad-drafting-settings').count()) === 1))
    step('Snap and Grid shows the snap spacing', await until(async () => (await kit.getByTestId('cad-settings-snapX').inputValue()) !== ''), await kit.getByTestId('cad-settings-snapX').inputValue())
    await kit.getByTestId('cad-settings-snapX').fill('5')
    await kit.getByTestId('cad-settings-snapY').fill('5')
    await kit.getByTestId('cad-settings-gridMajor').fill('4')
    await kit.getByTestId('cad-settings-snap').check()
    await screenshot('drafting-settings')
    await kit.getByTestId('cad-settings-page-polar').click()
    await kit.getByTestId('cad-settings-polarAngle').selectOption('45')
    await kit.getByTestId('cad-settings-polar').check()
    await kit.getByTestId('cad-settings-page-osnap').click()
    await kit.getByTestId('cad-settings-snap-none').click()
    await kit.getByTestId('cad-settings-snap-1').check()
    await kit.getByTestId('cad-settings-snap-2').check()
    await kit.getByTestId('cad-settings-osnap').check()
    await kit.getByTestId('cad-settings-page-dynamic').click()
    const dynamic = (await drafting()).dynamicInput
    await kit.getByTestId('cad-settings-dynamicInput').setChecked(!dynamic)
    await kit.getByTestId('cad-settings-ok').click()
    step('OK closes it', await until(async () => (await kit.getByTestId('cad-drafting-settings').count()) === 0))
    // (OK writes each change in turn; dynamic input is the last.)
    step('dynamic input changed', await until(async () => (await drafting()).dynamicInput === !dynamic))
    const after = await drafting()
    const snap = (await agent(['variable.get', { name: 'SNAPUNIT' }]))[0]?.value
    step('the snap spacing and Snap are set', await until(async () => JSON.stringify((await agent(['variable.get', { name: 'SNAPUNIT' }]))[0]?.value) === '[5,5]' && (await drafting()).snap === true), JSON.stringify(snap))
    step('the major grid lines', after.gridMajor === 4, String(after.gridMajor))
    step('polar tracking at 45°', after.polar === true && near(after.polarAngle, 45, 1e-9), `${after.polar} ${after.polarAngle}`)
    step('the running object snaps: Endpoint and Midpoint, on', after.osmode === 3 && after.osnap === true, `${after.osmode} ${after.osnap}`)
    step('and LINE is still running', (await details()).running === 'line.add', JSON.stringify(await prompt()))
    // Reopened, it shows what it set; Cancel changes nothing.
    await kit.getByTestId('cad-status-settings').click()
    await until(async () => (await kit.getByTestId('cad-drafting-settings').count()) === 1)
    step('reopened, it shows the settings', await until(async () => (await kit.getByTestId('cad-settings-snapX').inputValue()) === '5' && (await kit.getByTestId('cad-settings-snap').isChecked())))
    await kit.getByTestId('cad-settings-snap').uncheck()
    await page.keyboard.press('Escape')
    step('Escape closes it without a change', await until(async () => (await kit.getByTestId('cad-drafting-settings').count()) === 0) && (await drafting()).snap === true)
    await page.keyboard.press('Escape')
    // Tools › Drafting Settings... opens the same dialog.
    await idle()
    await kit.getByTestId('ribbon-tab-Tools').click()
    await wait(300)
    await kit.getByTestId('cad-cmd-drafting.settings').click()
    step('Tools › Drafting Settings... opens it too', await until(async () => (await kit.getByTestId('cad-drafting-settings').count()) === 1))
    await page.keyboard.press('Escape')
  })

  // 4. The layout tabs: "+" adds a layout; a tab's menu renames it in place and deletes it; the
  //    Model tab's menu has New Layout only.
  await section('layout tabs', async () => {
    const tabs = async () => kit.locator('[data-testid=cad-layout-tabs] [role=tab]').evaluateAll((items) => items.map((tab) => tab.textContent.trim()))
    const before = await tabs()
    await kit.getByTestId('cad-layout-new').click()
    step('"+" adds a layout tab', await until(async () => (await tabs()).length === before.length + 1), JSON.stringify(await tabs()))
    const added = (await tabs()).find((name) => !before.includes(name))
    step('and Model stays the tab shown', (await kit.getByTestId('cad-layout-Model').getAttribute('aria-selected')) === 'true', added)
    await rightClick(kit.getByTestId(`cad-layout-${added}`))
    step('a right-click on a layout tab opens its menu', await until(async () => (await kit.getByTestId('cad-layout-menu').count()) === 1))
    await screenshot('layout-menu')
    await kit.getByTestId('cad-layout-menu-rename').click()
    step('Rename puts its name in a field on the tab', await until(async () => (await kit.getByTestId('cad-layout-rename').count()) === 1))
    await kit.getByTestId('cad-layout-rename').fill('Plan A')
    await kit.getByTestId('cad-layout-rename').press('Enter')
    step('Enter renames the layout', await until(async () => (await tabs()).includes('Plan A') && !(await tabs()).includes(added)), JSON.stringify(await tabs()))
    step('the drawing has the new name', ((await details()).layouts ?? []).some((layout) => (layout.name ?? layout) === 'Plan A'))
    await rightClick(kit.getByTestId('cad-layout-Plan A'))
    await until(async () => (await kit.getByTestId('cad-layout-menu').count()) === 1)
    await kit.getByTestId('cad-layout-menu-delete').click()
    step('Delete removes it', await until(async () => JSON.stringify(await tabs()) === JSON.stringify(before)), JSON.stringify(await tabs()))
    await rightClick(kit.getByTestId('cad-layout-Model'))
    await until(async () => (await kit.getByTestId('cad-layout-menu').count()) === 1)
    step('the Model tab\'s menu offers New Layout only', (await kit.getByTestId('cad-layout-menu-new').count()) === 1 && (await kit.getByTestId('cad-layout-menu-rename').count()) === 0)
    await page.keyboard.press('Escape')
  })

  // 5. The command history: clicked points are written beside their prompts, a grip's too; the
  //    history window's menu copies the history.
  await section('history', async () => {
    const frame = page.frames().find((item) => item !== page.mainFrame())
    // The clipboard the page writes, kept where the test can read it.
    await frame.evaluate(() => {
      window.__copied = null
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (text) => { window.__copied = text } } })
    })
    await kit.getByTestId('cad-tool-line.add').click()
    await wait(300)
    await click(400, 250)
    await click(440, 250)
    await page.keyboard.press('Escape')
    await idle()
    const point = /: -?\d+(\.\d+)?,-?\d+(\.\d+)?$/
    const lines = await history()
    step('the first point clicked is on the history', lines.some((line) => line.startsWith('LINE Specify first point') && point.test(line)), JSON.stringify(lines.slice(-4)))
    step('and the next point', lines.some((line) => line.startsWith('LINE Specify next point') && point.test(line)))
    const written = lines.find((line) => line.startsWith('LINE Specify next point'))?.split(': ').pop()?.split(',').map(Number) ?? []
    step('in drawing units, where the click was', near(written[0], 440, 2 * perPixel()) && near(written[1], 250, 2 * perPixel()), JSON.stringify(written))
    // A grip placed by a click.
    await click(420, 250)
    await until(async () => (await details()).selection === 1)
    await click(440, 250)
    await until(async () => (await prompt()).grip != null)
    await click(450, 260)
    step('a grip placed by a click is on the history too', await until(async () => /^STRETCH .*: -?[\d.]+,-?[\d.]+$/.test((await history()).at(-1) ?? '')), (await history()).at(-1))
    await page.keyboard.press('Escape')
    await wait(300)
    await page.keyboard.press('Escape')
    // The history window's menu.
    await kit.getByTestId('cad-history-toggle').click()
    await until(async () => (await kit.locator('[data-testid=cad-history][data-open=true]').count()) === 1)
    await rightClick(kit.locator('[data-testid=cad-history][data-open=true]'))
    step('a right-click on the history window opens its menu', await until(async () => (await kit.getByTestId('cad-history-menu').count()) === 1))
    await screenshot('history-menu')
    await kit.getByTestId('cad-history-menu-copy-all').click()
    const copied = async () => frame.evaluate(() => window.__copied)
    step('Copy History puts the history on the clipboard', await until(async () => /Command: LINE/.test((await copied()) ?? '') && /LINE Specify next point/.test((await copied()) ?? '')), String(await copied()).slice(0, 120))
    // Copy with a line selected copies that line.
    await kit.locator('[data-testid=cad-history][data-open=true] .cad-history-line', { hasText: 'Specify first point' }).first().evaluate((line) => {
      const range = document.createRange()
      range.selectNodeContents(line)
      const selection = window.getSelection()
      selection.removeAllRanges()
      selection.addRange(range)
    })
    await rightClick(kit.locator('[data-testid=cad-history][data-open=true] .cad-history-line', { hasText: 'Specify first point' }).first())
    await until(async () => (await kit.getByTestId('cad-history-menu').count()) === 1)
    await kit.getByTestId('cad-history-menu-copy').click()
    step('Copy copies the lines selected', await until(async () => /^LINE Specify first point.*\d$/.test(((await copied()) ?? '').trim())), String(await copied()))
  })
} finally {
  if (errors.length) step('no page errors', false, errors.join(' | '))
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} step(s) failed` : 'all steps passed')
process.exit(failures ? 1 : 0)
