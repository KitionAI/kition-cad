// Grips and the right-click menus by state (walkthrough N9, N11, 别扭 4 and 5): the grip under the
// pointer turns warm and a clicked grip hot; a drag from a grip opens no window and the release
// makes it hot; Space twice goes to ROTATE and a typed 90 turns the circle about the grip; Ctrl (Cmd
// on a Mac) held while placing places a copy; the grip menu (right-click on a grip) offers the
// modes and the grip's options (Lengthen); the menu of a hot grip switches its mode; with objects
// selected the right-click menu edits them (Move asks for the base point at once) and opens the
// properties; with nothing selected it offers Repeat, Recent, Paste, Zoom.
// node tests/grips.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const output = new URL('../test-results/grips/', import.meta.url)
// Ctrl copies a grip (the engine hears Ctrl and Cmd alike); on a Mac, Ctrl+click is the system's
// right-click, so Cmd is the key there.
const COPY_KEY = process.platform === 'darwin' ? 'Meta' : 'Control'
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
const prompt = async () => (await details()).drafting?.commandLine?.prompt ?? {}
const gripping = async () => (await details()).prompt.startsWith('STRETCH ')
const command = async (...entries) => {
  for (const text of entries) { await input.fill(text); await input.press('Enter'); await page.waitForTimeout(150) }
}
const idle = () => until(async () => (await details()).running == null)
const query = async (type) => {
  const run = await page.evaluate(async (type) => window.devHost.connection().ai.run([{ command: 'object.query', params: { type } }], false), type)
  const result = run.completed?.[0]?.result ?? run.completed?.[0]?.value ?? run.completed?.[0]
  return result?.entities ?? []
}
// The circles this test draws (the sample's are left of x = 300).
// (A command an agent runs, object.query included, ends a hot grip's edit: read it after the edit.)
const circles = async () => (await query('circle')).map((circle) => ({ center: [circle.geometry.center.x, circle.geometry.center.y], radius: circle.geometry.radius })).filter((circle) => circle.center[0] > 300)
const lines = async () => (await query('line')).map((line) => [[line.geometry.a.x, line.geometry.a.y], [line.geometry.b.x, line.geometry.b.y]])
// Drawing units to page pixels, from the view the editor shows.
const toScreen = async (x, y) => {
  const { view } = await details()
  const box = await surface.boundingBox()
  const perPixel = view.height / box.height
  return { x: box.x + box.width / 2 + (x - view.center[0]) / perPixel, y: box.y + box.height / 2 - (y - view.center[1]) / perPixel }
}
const pixel = async () => (await details()).view.height / (await surface.boundingBox()).height
const near = (a, b, tolerance) => Math.abs(a - b) <= tolerance
const open = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  // Only the pointer moves the cursor: no running snaps, no polar tracking.
  await command('osnap {"on":false}', 'polar {"on":false}', 'ortho {"on":false}', 'grid {"on":false}')
}
// The canvas pixels of a window around page point (x, y), as [r, g, b] triples (CSS pixels).
const pixels = async (x, y, half) => {
  const box = await surface.boundingBox()
  return surface.evaluate((canvas, { x, y, half }) => {
    const scale = canvas.width / canvas.clientWidth
    const data = canvas.getContext('2d').getImageData(Math.round((x - half) * scale), Math.round((y - half) * scale), Math.max(1, Math.round(2 * half * scale)), Math.max(1, Math.round(2 * half * scale))).data
    const out = []
    for (let i = 0; i < data.length; i += 4) out.push([data[i], data[i + 1], data[i + 2]])
    return out
  }, { x: x - box.x, y: y - box.y, half })
}
const count = async (x, y, half, test) => (await pixels(x, y, half)).filter(test).length
const warm = ([r, g, b]) => r > 220 && g > 90 && g < 175 && b > 120 && b < 205
const hot = ([r, g, b]) => r > 170 && g < 90 && b < 90
const cold = ([r, g, b]) => b - r > 60 && b - g > 40
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
const section = async (name, body) => {
  try {
    await open()
    await body()
  } catch (error) {
    step(`${name} (threw)`, false, error instanceof Error ? error.message.split('\n')[0] : String(error))
    await screenshot(`failure-${name.replace(/\W+/g, '-')}`)
  }
}
// A circle at (360, 255), radius 15, selected (clicked on its top).
const selectedCircle = async () => {
  await command('circle', '360,255', '15')
  await idle()
  const top = await toScreen(360, 270)
  await page.mouse.click(top.x + 3, top.y)
  await until(async () => (await details()).selection === 1)
  await page.waitForTimeout(300)
}
const rightClick = async (at, hold = 0) => {
  await page.mouse.move(at.x, at.y)
  await page.mouse.down({ button: 'right' })
  if (hold) await page.waitForTimeout(hold)
  await page.mouse.up({ button: 'right' })
}

await mkdir(output, { recursive: true })
try {
  // 1. 别扭 5 and 4: the grip under the pointer is warm, a clicked one hot; a drag from a grip opens
  //    no window, and its release makes the grip hot.
  await section('looks', async () => {
    await selectedCircle()
    const center = await toScreen(360, 255)
    const off = 2
    await page.mouse.move(center.x + 60, center.y + 60)
    step('a grip away from the pointer is the selection\'s colour', await until(async () => (await count(center.x, center.y, 2, cold)) > 4 && (await count(center.x, center.y, 3, warm)) === 0))
    await page.mouse.move(center.x + off, center.y + off, { steps: 3 })
    step('the grip under the pointer turns warm', await until(async () => (await count(center.x, center.y, 3, warm)) > 4), JSON.stringify((await pixels(center.x, center.y, 1))[0]))
    await screenshot('warm')
    await page.mouse.move(center.x + 60, center.y + 60, { steps: 3 })
    step('moving off cools it again', await until(async () => (await count(center.x, center.y, 3, warm)) === 0))
    await page.mouse.click(center.x, center.y)
    await page.mouse.move(center.x + 40, center.y + 20, { steps: 3 })
    step('a clicked grip is hot', await until(async () => (await count(center.x, center.y, 3, hot)) > 4))
    await screenshot('hot')
    await page.waitForTimeout(300)
    await page.keyboard.press('Escape')
    await until(async () => !(await gripping()))
    // A drag from the quadrant grip: no selection window, the release makes it hot.
    const quadrant = await toScreen(375, 255)
    await page.mouse.move(quadrant.x, quadrant.y)
    await page.mouse.down()
    await page.mouse.move(quadrant.x + 60, quadrant.y - 40, { steps: 6 })
    await page.mouse.up()
    step('a drag from a grip makes it hot instead of opening a window', await until(async () => await gripping()), (await details()).prompt)
    await page.waitForTimeout(300)
    const unchanged = (await details()).selectedObjects
    step('the selection stays and nothing changed', unchanged.length === 1 && near(unchanged[0].radius, 15, 1e-9), JSON.stringify(unchanged))
    const to = await toScreen(385, 255)
    await page.mouse.click(to.x, to.y)
    await until(async () => !(await gripping()))
    const [circle] = await circles()
    const tolerance = 1.5 * await pixel()
    step('the next click places it (radius 25)', circle && near(circle.radius, 25, tolerance), JSON.stringify(circle))
  })

  // 2. N11: Space twice goes to ROTATE; 90 Enter turns the circle 90° about the grip.
  await section('modes', async () => {
    await selectedCircle()
    const quadrant = await toScreen(375, 255)
    await page.mouse.click(quadrant.x, quadrant.y)
    step('a click on a grip starts STRETCH', await until(async () => (await prompt()).command === '** STRETCH **'), JSON.stringify(await prompt()))
    step('the command line shows the mode and its options', await until(async () => /STRETCH/.test(await kit.getByTestId('cad-prompt').innerText()) && await kit.getByTestId('cad-prompt-option-Copy').isVisible()))
    await page.waitForTimeout(300)
    await page.keyboard.press(' ')
    step('Space: MOVE', await until(async () => (await prompt()).command === '** MOVE **'))
    await page.keyboard.press(' ')
    step('Space again: ROTATE', await until(async () => (await prompt()).command === '** ROTATE **'))
    step('ROTATE offers Reference', ((await prompt()).keywords ?? []).includes('Reference'))
    step('the history shows the modes', await until(async () => (await kit.getByTestId('cad-history').innerText()).includes('** ROTATE **')))
    await screenshot('rotate')
    await page.keyboard.type('90')
    await page.keyboard.press('Enter')
    await until(async () => (await prompt()).command !== '** ROTATE **')
    const [circle] = await circles()
    step('90 Enter turns the circle about the grip', circle && near(circle.center[0], 375, 1e-6) && near(circle.center[1], 240, 1e-6) && near(circle.radius, 15, 1e-6), JSON.stringify(circle))
    step('the edit ends and the selection stays', !(await gripping()) && (await details()).selection === 1)
  })

  // 3. Ctrl (Cmd on a Mac) held while placing: a copy, and the edit goes on placing copies.
  await section('copy', async () => {
    await selectedCircle()
    const before = (await details()).entities
    const center = await toScreen(360, 255)
    await page.mouse.click(center.x, center.y)
    await until(gripping)
    const to = await toScreen(400, 220)
    await page.mouse.move(to.x, to.y, { steps: 3 })
    await page.keyboard.down(COPY_KEY)
    await page.mouse.click(to.x, to.y)
    await page.keyboard.up(COPY_KEY)
    step(`${COPY_KEY} held while placing adds a copy`, await until(async () => (await details()).entities === before + 1), `${(await details()).entities} (${before})`)
    step('the edit goes on placing copies', await until(async () => await gripping() && (await prompt()).grip?.copy === true), `${(await details()).prompt} ${JSON.stringify((await prompt()).grip)}`)
    await page.waitForTimeout(300)
    await page.keyboard.press(' ')
    step('Space after copies ends the edit', await until(async () => !(await gripping())))
    const found = await circles()
    step('the original stays where it was and the copy is where it was placed', found.length === 2 && found.some((circle) => near(circle.center[0], 360, 1e-6) && near(circle.center[1], 255, 1e-6)), JSON.stringify(found))
  })

  // 4. The grip menu: right-click on a grip offers its modes and options (a line's end: Lengthen);
  //    the menu of a hot grip switches its mode.
  await section('grip menu', async () => {
    await command('line', '300,200', '340,200', '')
    await idle()
    const middle = await toScreen(325, 200)
    await page.mouse.click(middle.x, middle.y)
    await until(async () => (await details()).selection === 1)
    const end = await toScreen(340, 200)
    await page.mouse.move(end.x + 1, end.y + 1, { steps: 3 })
    await page.waitForTimeout(300)
    await rightClick(end)
    const menu = kit.getByTestId('cad-grip-menu')
    step('right-click on a grip opens the grip menu', await until(async () => (await menu.count()) === 1))
    step('it offers Stretch, the modes and Lengthen', await until(async () => (await kit.getByTestId('cad-grip-menu-stretch').count()) === 1 && (await kit.getByTestId('cad-grip-menu-rotate').count()) === 1 && (await kit.getByTestId('cad-grip-menu-Lengthen').count()) === 1))
    step('no drawing menu with it', (await kit.getByTestId('cad-context-menu').count()) === 0)
    await screenshot('grip-menu')
    await kit.getByTestId('cad-grip-menu-Lengthen').click()
    step('Lengthen asks for the new end', await until(async () => (await prompt()).command === '** LENGTHEN **'), JSON.stringify(await prompt()))
    const further = await toScreen(360, 210)
    await page.mouse.move(further.x, further.y, { steps: 3 })
    await page.mouse.click(further.x, further.y)
    await until(async () => !(await gripping()) && (await prompt()).command !== '** LENGTHEN **')
    const line = (await lines()).find((item) => near(item[0][0], 300, 1e-6) && near(item[0][1], 200, 1e-6))
    const tolerance = 1.5 * await pixel()
    step('the line is lengthened along itself', line && near(line[1][0], 360, tolerance) && near(line[1][1], 200, 1e-6), JSON.stringify(line))
    // A hot grip's menu (held right-click): ROTATE.
    const tip = await toScreen(line ? line[1][0] : 360, 200)
    await page.mouse.click(tip.x, tip.y)
    await until(gripping)
    await page.waitForTimeout(300)
    await rightClick({ x: tip.x + 40, y: tip.y - 30 }, 400)
    step('a held right-click on a hot grip opens its menu', await until(async () => (await menu.count()) === 1))
    step('it offers Copy and Base point', (await kit.getByTestId('cad-grip-menu-Copy').count()) === 1 && (await kit.getByTestId('cad-grip-menu-Base-point').count()) === 1)
    await kit.getByTestId('cad-grip-menu-rotate').click()
    step('choosing Rotate switches the mode', await until(async () => (await prompt()).command === '** ROTATE **'))
    await page.keyboard.press('Escape')
    step('Escape cools the grip', await until(async () => (await prompt()).command !== '** ROTATE **'))
  })

  // 5. N9: the right-click menu by state. With objects selected it edits them (Move asks for the base
  //    point at once) and opens the properties; with nothing selected, Repeat, Recent, Paste, Zoom.
  await section('menus', async () => {
    await selectedCircle()
    const empty = await toScreen(250, 120)
    const menu = kit.getByTestId('cad-context-menu')
    await rightClick(empty)
    step('a right-click with a selection opens the drawing menu', await until(async () => (await menu.count()) === 1))
    for (const id of ['edit.delete', 'object.move', 'object.copy', 'object.scale', 'object.rotate', 'object.mirror', 'edit.deselectAll', 'properties', 'draworder', 'edit.undo']) {
      step(`it offers ${id}`, (await kit.getByTestId(`cad-context-${id}`).count()) === 1)
    }
    step('no Paste or Select All with a selection', (await kit.getByTestId('cad-context-edit.paste').count()) === 0)
    await screenshot('selection-menu')
    await kit.getByTestId('cad-context-object.move').click()
    step('Move starts with the selection: it asks for the base point', await until(async () => (await details()).running === 'object.move' && /base point/i.test((await prompt()).message ?? '')), JSON.stringify(await prompt()))
    await page.waitForTimeout(300)
    await page.keyboard.press('Escape')
    await idle()
    // Properties opens the right panel (hidden first with the ribbon's Layer Properties).
    await kit.getByTestId('cad-layer-properties').click()
    await until(async () => (await kit.getByTestId('cad-layers').count()) === 0)
    if ((await details()).selection !== 1) {
      const top = await toScreen(360, 270)
      await page.mouse.click(top.x + 3, top.y)
      await until(async () => (await details()).selection === 1)
      await page.waitForTimeout(300)
    }
    await rightClick(empty)
    await until(async () => (await menu.count()) === 1)
    await kit.getByTestId('cad-context-properties').click()
    step('Properties opens the properties panel', await until(async () => (await kit.getByTestId('cad-selection-properties').count()) === 1))
    // Deselect All, then the menu without a selection.
    await rightClick(empty)
    await until(async () => (await menu.count()) === 1)
    await kit.getByTestId('cad-context-edit.deselectAll').click()
    step('Deselect All clears the selection', await until(async () => (await details()).selection === 0))
    await rightClick(empty)
    step('without a selection the drawing menu opens too', await until(async () => (await menu.count()) === 1))
    for (const id of ['repeat', 'recent', 'edit.undo', 'edit.redo', 'edit.paste', 'edit.selectAll', 'pan', 'zoom']) {
      step(`it offers ${id}`, (await kit.getByTestId(`cad-context-${id}`).count()) === 1)
    }
    step('no Move without a selection', (await kit.getByTestId('cad-context-object.move').count()) === 0)
    step('Repeat names the last command (MOVE, from the menu)', /move/i.test(await kit.getByTestId('cad-context-repeat').innerText()), await kit.getByTestId('cad-context-repeat').innerText())
    await screenshot('idle-menu')
    await kit.getByTestId('cad-context-zoom').click()
    step('Zoom ▸ opens the zoom menu', await until(async () => (await kit.getByTestId('cad-view-zoom-menu').count()) === 1))
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
