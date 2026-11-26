// Properties, layers, text in place and hatch (walkthrough N12, N13, N14, B8, 别扭 7 and 8): the
// Properties panel describes a line by group with rounded values and writes its end X in one undo
// step, takes arithmetic, marks what several objects disagree on and sets their colour at once, and
// with nothing selected shows the current properties; the ribbon's Layer Properties no longer hides
// the panel and the layer list stays put when objects are selected; a layer's colour recolours its
// objects, its linetype loads from the library, a double-click renames it, and a layer with objects
// says why it cannot be deleted; a double-click on a text edits it where it is, MTEXT types two lines
// that are stored with \P, TEXT types in place (Space a space), a composed Chinese entry commits
// only after its composition ends; Hatch… asks the pattern, then picks points with the area under
// the cursor previewed, and a double-click on the hatch opens it again; when the box closes the
// keyboard is the drawing's again (Escape clears the selection, Delete erases).
// node tests/properties.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
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
const box = kit.getByTestId('cad-in-place')
const output = new URL('../test-results/properties/', import.meta.url)
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
const command = async (...entries) => {
  for (const text of entries) { await input.fill(text); await input.press('Enter'); await page.waitForTimeout(150) }
}
const idle = () => until(async () => (await details()).running == null)
/** Runs commands as an agent (setup and reading back), answering each one's result. */
const agent = async (...calls) => {
  const run = await page.evaluate(async (calls) => window.devHost.connection().ai.run(calls.map(([command, params]) => ({ command, params })), false), calls)
  return (run.completed ?? []).map((item) => item?.result ?? item?.value ?? item)
}
const query = async (type) => (await agent(['object.query', { type }]))[0]?.entities ?? []
const describe = async (handles) => (await agent(['object.describeProperties', handles ? { handles } : {}]))[0]
const entity = async (type, handle) => (await query(type)).find((item) => item.handle === handle)
// Drawing units to page pixels, from the view the editor shows.
const toScreen = async (x, y) => {
  const { view } = await details()
  const bounds = await surface.boundingBox()
  const perPixel = view.height / bounds.height
  return { x: bounds.x + bounds.width / 2 + (x - view.center[0]) / perPixel, y: bounds.y + bounds.height / 2 - (y - view.center[1]) / perPixel }
}
const click = async (x, y) => {
  const at = await toScreen(x, y)
  await page.mouse.click(at.x, at.y)
  await page.waitForTimeout(300)
}
const doubleClick = async (x, y) => {
  const at = await toScreen(x, y)
  await page.mouse.dblclick(at.x, at.y)
  await page.waitForTimeout(300)
}
// The canvas pixels of a window around page point (x, y), as [r, g, b] triples.
const pixels = async (x, y, half) => {
  const bounds = await surface.boundingBox()
  return surface.evaluate((canvas, { x, y, half }) => {
    const scale = canvas.width / canvas.clientWidth
    const data = canvas.getContext('2d').getImageData(Math.round((x - half) * scale), Math.round((y - half) * scale), Math.max(1, Math.round(2 * half * scale)), Math.max(1, Math.round(2 * half * scale))).data
    const out = []
    for (let i = 0; i < data.length; i += 4) out.push([data[i], data[i + 1], data[i + 2]])
    return out
  }, { x: x - bounds.x, y: y - bounds.y, half })
}
const red = ([r, g, b]) => r > 180 && g < 90 && b < 90
const field = (key) => kit.getByTestId(`cad-property-${key}`)
const open = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  await command('osnap {"on":false}', 'polar {"on":false}', 'ortho {"on":false}', 'grid {"on":false}')
}
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

await mkdir(output, { recursive: true })
try {
  // 1. The panel describes a line by group, rounded, and writes its end X in one undo step (N13, 别扭 8).
  await section('properties', async () => {
    const [made] = await agent(['line.add', { points: [[320, 200], [400.123456789, 240]] }])
    const line = made.handles?.[0] ?? made.handle
    await click(360, 220)
    await until(async () => (await details()).selection === 1)
    step('the panel describes the line by group', await until(async () => (await kit.getByTestId('cad-property-group-General').count()) === 1 && (await kit.getByTestId('cad-property-group-Geometry').count()) === 1))
    for (const key of ['color', 'cad-selection-layer', 'linetype', 'ltscale', 'lineweight', 'transparency']) {
      step(`General has ${key}`, (await (key.startsWith('cad-') ? kit.getByTestId(key) : field(key)).count()) === 1)
    }
    step('numbers are rounded to the drawing\'s precision', await until(async () => (await field('end.x').inputValue()) === '400.1235'), await field('end.x').inputValue())
    step('Start X shows 320', (await field('start.x').inputValue()) === '320')
    await screenshot('line-properties')
    await field('end.x').fill('380')
    await field('end.x').press('Enter')
    step('End X moves the line\'s end', await until(async () => (await entity('line', line))?.geometry.b.x === 380), JSON.stringify((await entity('line', line))?.geometry))
    step('and keeps its Y', (await entity('line', line))?.geometry.b.y === 240)
    await kit.getByTestId('cad-undo').click()
    step('one Undo gives the old end back', await until(async () => Math.abs((await entity('line', line))?.geometry.b.x - 400.123456789) < 1e-9))
    // Arithmetic, and Tab writes too.
    await click(360, 220)
    await until(async () => (await details()).selection === 1)
    await field('length').fill('2*30+5')
    await field('length').press('Tab')
    step('a typed expression sets the length (Tab writes it)', await until(async () => {
      const g = (await entity('line', line))?.geometry
      return g && Math.abs(Math.hypot(g.b.x - g.a.x, g.b.y - g.a.y) - 65) < 1e-6
    }))
    // Escape gives the shown value back.
    await field('start.y').fill('999')
    await field('start.y').press('Escape')
    step('Escape gives the shown value back', (await field('start.y').inputValue()) === '200')
  })

  // 2. Several objects: what differs is marked, a colour goes to all; nothing selected: the current properties.
  await section('several', async () => {
    const [a] = await agent(['circle.add', { center: [340, 200], radius: 10 }])
    const [b] = await agent(['circle.add', { center: [400, 200], radius: 15 }])
    await click(350, 200)
    await click(415, 200)
    await until(async () => (await details()).selection === 2)
    step('two circles selected', (await details()).selection === 2, String((await details()).selection))
    step('the radius they disagree on is marked', await until(async () => (await field('radius').getAttribute('placeholder')) === '*Varies*' && (await field('radius').inputValue()) === ''))
    await field('color').selectOption('Red')
    step('a colour goes to both', await until(async () => (await entity('circle', a.handle))?.color === 'Red' && (await entity('circle', b.handle))?.color === 'Red'), JSON.stringify(await entity('circle', a.handle)))
    await kit.getByTestId('cad-undo').click()
    step('in one undo step', await until(async () => (await entity('circle', a.handle))?.color !== 'Red' && (await entity('circle', b.handle))?.color !== 'Red'))
    await page.keyboard.press('Escape')
    await page.waitForTimeout(300)
    await page.keyboard.press('Escape')
    await until(async () => (await details()).selection === 0)
    step('nothing selected: the current properties', await until(async () => /Current properties/.test(await kit.getByTestId('cad-selection-properties').innerText())))
    await field('color').selectOption('Blue')
    step('the current colour is set', await until(async () => (await describe())?.rows.find((row) => row.key === 'color')?.value === 'Blue'))
    const [made] = await agent(['circle.add', { center: [370, 260], radius: 5 }])
    step('new objects take it', (await entity('circle', made.handle))?.color === 'Blue', JSON.stringify(await entity('circle', made.handle)))
    await screenshot('current-properties')
  })

  // 3. The panel and the layers (B8, 别扭 7, N14).
  await section('layers', async () => {
    await kit.getByTestId('cad-layer-properties').click()
    await page.waitForTimeout(300)
    step('Layer Properties keeps the panel open', (await kit.getByTestId('cad-layers').count()) === 1)
    step('and shows its layers', (await kit.getByTestId('cad-layers').getAttribute('data-part')) === 'layers')
    await kit.getByTestId('cad-panel-close').click()
    step('the panel\'s own button closes it', await until(async () => (await kit.getByTestId('cad-layers').count()) === 0))
    await kit.getByTestId('cad-layer-properties').click()
    step('Layer Properties opens it again', await until(async () => (await kit.getByTestId('cad-layers').count()) === 1))
    await agent(['layer.add', { name: 'Notes' }], ['layer.add', { name: 'Spare' }])
    const [made] = await agent(['line.add', { points: [[320, 280], [440, 280]], layer: 'Notes' }])
    const handle = made.handles?.[0] ?? made.handle
    await until(async () => (await kit.getByTestId('cad-layer-Notes-color').count()) === 1)
    await page.waitForTimeout(300)
    const before = await kit.getByTestId('cad-layer-0').boundingBox()
    await click(380, 280)
    await until(async () => (await details()).selection === 1)
    await until(async () => (await kit.getByTestId('cad-property-group-Geometry').count()) === 1)
    const after = await kit.getByTestId('cad-layer-0').boundingBox()
    step('the layer list stays put when an object is selected', before && after && Math.abs(before.y - after.y) < 1, `${before?.y} → ${after?.y}`)
    await page.keyboard.press('Escape')
    await page.waitForTimeout(300)
    await page.keyboard.press('Escape')
    await until(async () => (await details()).selection === 0)
    await kit.getByTestId('cad-layer-Notes-color').selectOption('Red')
    step('the layer takes the colour', await until(async () => (await describe())?.layers.find((layer) => layer.name === 'Notes')?.color === 'Red'))
    const mid = await toScreen(380, 280)
    step('its objects are drawn in it', await until(async () => (await pixels(mid.x, mid.y, 3)).some(red)))
    await kit.getByTestId('cad-layer-Notes-linetype').selectOption('DASHED')
    step('a library linetype loads and goes to the layer', await until(async () => (await describe())?.layers.find((layer) => layer.name === 'Notes')?.linetype === 'DASHED'))
    await kit.getByTestId('cad-layer-Notes-lineweight').selectOption('0.5')
    step('the layer takes a lineweight', await until(async () => (await describe())?.layers.find((layer) => layer.name === 'Notes')?.lineweight === 0.5))
    await kit.getByTestId('cad-layer-Notes-name').dblclick()
    const rename = kit.getByTestId('cad-layer-Notes-rename')
    await rename.fill('Labels')
    await rename.press('Enter')
    step('a double-click on the name renames the layer', await until(async () => (await details()).layers.some((layer) => layer.name === 'Labels')))
    step('and its objects follow', (await query('line')).find((line) => line.handle === handle)?.layer === 'Labels')
    await kit.getByTestId('cad-layer-Labels-delete').click()
    step('a layer with objects says why it stays', await until(async () => /has objects/.test(await kit.getByTestId('cad-layer-Labels-message').innerText())))
    step('and stays', (await details()).layers.some((layer) => layer.name === 'Labels'))
    await kit.getByTestId('cad-layer-Spare-delete').click()
    step('an empty layer is deleted', await until(async () => !(await details()).layers.some((layer) => layer.name === 'Spare')))
    await kit.getByTestId('cad-layer-Labels-row').dblclick()
    step('a double-click on the row makes it current', await until(async () => (await details()).currentLayer === 'Labels'))
    await screenshot('layers')
  })

  // 4. Text in place (N12).
  await section('text', async () => {
    const [made] = await agent(['text.add', { at: [330, 200], height: 8, text: 'Kitchen' }])
    await doubleClick(340, 203)
    step('a double-click on a text opens the box there', await until(async () => (await box.count()) === 1))
    step('with the text in it', (await box.inputValue()) === 'Kitchen', await box.inputValue())
    const where = await box.boundingBox()
    const at = await toScreen(330, 200)
    step('over the text', where && Math.abs(where.x - at.x) < 40 && Math.abs(where.y + where.height - at.y) < 40, `${JSON.stringify(where)} ${JSON.stringify(at)}`)
    step('all of it selected', await box.evaluate((element) => element.selectionStart === 0 && element.selectionEnd === element.value.length))
    await screenshot('text-in-place')
    await page.keyboard.type('Dining room')
    await page.keyboard.press('Enter')
    step('Enter writes it', await until(async () => (await entity('text', made.handle))?.geometry.value === 'Dining room'), JSON.stringify((await entity('text', made.handle))?.geometry))
    step('and closes the box', (await box.count()) === 0)
    await kit.getByTestId('cad-undo').click()
    step('in one undo step', await until(async () => (await entity('text', made.handle))?.geometry.value === 'Kitchen'))
    // Escape leaves it; emptied it keeps its text.
    await doubleClick(340, 203)
    await until(async () => (await box.count()) === 1)
    await page.keyboard.type('Nothing')
    await page.keyboard.press('Escape')
    step('Escape leaves the text as it was', await until(async () => (await box.count()) === 0) && (await entity('text', made.handle))?.geometry.value === 'Kitchen')
    // A composed (input method) entry: Enter while composing is the input method's.
    await page.keyboard.press('Escape')
    await doubleClick(340, 203)
    await until(async () => (await box.count()) === 1)
    await box.evaluate((element) => element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' })))
    await page.keyboard.insertText('厨房')
    await box.evaluate((element) => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, isComposing: true })))
    await page.waitForTimeout(300)
    step('Enter while composing writes nothing', (await box.count()) === 1 && (await entity('text', made.handle))?.geometry.value === 'Kitchen')
    await box.evaluate((element) => element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '厨房' })))
    await box.press('Enter')
    step('a composed Chinese text is written once composed', await until(async () => (await entity('text', made.handle))?.geometry.value === '厨房'), JSON.stringify((await entity('text', made.handle))?.geometry))
    // The drawing menu's Edit Text opens the same box.
    await click(340, 203)
    await until(async () => (await details()).selection === 1)
    const at2 = await toScreen(345, 260)
    await page.mouse.click(at2.x, at2.y, { button: 'right' })
    await until(async () => (await kit.getByTestId('cad-context-menu').count()) === 1)
    await kit.getByTestId('cad-context-text.edit').click()
    step('Edit Text in the drawing menu opens it in place', await until(async () => (await box.count()) === 1 && (await box.inputValue()) === '厨房'))
    await page.keyboard.press('Escape')

    // MTEXT: two lines stored with \P (Ctrl or Cmd+Enter writes).
    await until(async () => (await box.count()) === 0)
    await command('mtext', '320,150', '420,120')
    step('MTEXT asks for its text in place', await until(async () => (await box.count()) === 1 && (await box.getAttribute('data-kind')) === 'paragraphs'))
    await box.click()
    await page.keyboard.type('Line one')
    await page.keyboard.press('Enter')
    await page.keyboard.type('Line two')
    await screenshot('mtext-in-place')
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+Enter' : 'Control+Enter')
    step('the two lines are stored with \\P', await until(async () => (await query('mtext')).some((item) => item.geometry.contents === 'Line one\\PLine two')), JSON.stringify((await query('mtext')).map((item) => item.geometry.contents)))
    await idle()

    // TEXT types in place: Space is a space, Enter the next line, Enter on an empty box ends it.
    await command('text', '320,100', '6', '0')
    step('TEXT asks for its text in place', await until(async () => (await box.count()) === 1 && (await box.getAttribute('data-kind')) === 'text'))
    const textAt = await toScreen(320, 100)
    const placed = await box.boundingBox()
    step('where the text goes', placed && Math.abs(placed.x - textAt.x) < 20 && Math.abs(placed.y + placed.height - textAt.y) < 30, `${JSON.stringify(placed)} ${JSON.stringify(textAt)}`)
    await page.keyboard.type('Hall way')
    step('Space types a space there', (await box.inputValue()) === 'Hall way' && (await details()).running === 'text.add', await box.inputValue())
    await page.keyboard.press('Enter')
    step('Enter writes the line and asks for the next', await until(async () => (await query('text')).some((item) => item.geometry.value === 'Hall way') && (await box.count()) === 1 && (await box.inputValue()) === ''))
    await page.keyboard.press('Enter')
    step('Enter on an empty box ends TEXT', await idle())

    // A dimension's text: emptied, the measurement comes back.
    const [dimension] = await agent(['dimension.addLinear', { p1: [320, 60], p2: [400, 60], at: [360, 40], text: 'Wall' }])
    const mid = (await describe([dimension.handle]))?.inPlace?.box?.at
    if (mid) await doubleClick(mid[0], mid[1])
    step('a double-click on a dimension opens its text', await until(async () => (await box.count()) === 1 && (await box.inputValue()) === 'Wall'), mid ? JSON.stringify(mid) : 'no text box')
    await box.fill('')
    await box.press('Enter')
    step('emptied, it shows its measurement again', await until(async () => (await describe([dimension.handle]))?.rows.find((row) => row.key === 'textOverride')?.value === ''))
  })

  // 5. Hatch: the dialog, then points with a preview; a double-click opens it again.
  await section('hatch', async () => {
    await agent(['rectangle.add', { p1: [320, 180], p2: [420, 260] }])
    await kit.getByTestId('ribbon-tab-Draw').click()
    await kit.getByTestId('cad-cmd-hatch.add').click()
    step('Hatch… asks the pattern first', await until(async () => (await kit.getByTestId('cad-hatch-dialog').count()) === 1))
    await kit.getByTestId('cad-hatch-pattern').selectOption('ANSI37')
    await kit.getByTestId('cad-hatch-scale').fill('3')
    await screenshot('hatch-dialog')
    await kit.getByTestId('cad-hatch-dialog-ok').click()
    step('then picks internal points', await until(async () => (await details()).running === 'hatch.add' && /Pick internal point/.test((await prompt()).message ?? '')), JSON.stringify(await prompt()))
    const inside = await toScreen(370, 220)
    const accent = ([r, g, b]) => Math.abs(r - 79) < 30 && Math.abs(g - 109) < 30 && Math.abs(b - 138) < 30
    const before = (await pixels(inside.x + 25, inside.y + 15, 12)).filter(accent).length
    await page.mouse.move(inside.x, inside.y)
    await page.mouse.move(inside.x + 2, inside.y + 1)
    step('the area under the cursor is previewed', await until(async () => (await pixels(inside.x + 25, inside.y + 15, 12)).filter(accent).length > before + 5), String(before))
    await screenshot('hatch-preview')
    await page.mouse.click(inside.x, inside.y)
    await page.waitForTimeout(300)
    await page.keyboard.press('Enter')
    await idle()
    const hatches = await query('hatch')
    const hatch = hatches.at(-1)
    step('the hatch has the pattern and scale', hatch?.geometry.pattern === 'ANSI37' && hatch?.geometry.scale === 3, JSON.stringify(hatch?.geometry ?? null).slice(0, 200))
    // The hatch alone: a patterned hatch is picked by its boundary, which the rectangle shares.
    const rectangle = (await query('polyline')).at(-1)
    if (rectangle) await agent(['object.delete', { handles: [rectangle.handle] }])
    await doubleClick(320, 220)
    step('a double-click on the hatch opens its dialog', await until(async () => (await kit.getByTestId('cad-hatch-dialog').count()) === 1 && (await kit.getByTestId('cad-hatch-scale').inputValue()) === '3'))
    await kit.getByTestId('cad-hatch-scale').fill('5')
    await kit.getByTestId('cad-hatch-dialog-ok').click()
    step('OK changes the hatch', await until(async () => (await query('hatch')).find((item) => item.handle === hatch?.handle)?.geometry.scale === 5))
  })

  // 6. Regression issue-in-place-focus: when the box closes the keyboard is the drawing's again, so
  //    Escape then clears the selection and Delete erases it (the stage 7 walkthrough found both
  //    keys lost: the focus stayed on the page after the box was gone).
  await section('focus', async () => {
    const [made] = await agent(['text.add', { at: [330, 200], height: 8, text: 'Hall' }])
    await doubleClick(340, 203)
    step('the box opens on the text', await until(async () => (await box.count()) === 1))
    await page.keyboard.press('Escape')
    await until(async () => (await box.count()) === 0)
    step('Escape closes it and the text stays selected', (await details()).selection === 1)
    await page.keyboard.press('Escape')
    step('Escape after the box closes clears the selection', await until(async () => (await details()).selection === 0), `${(await details()).selection} selected`)
    await doubleClick(340, 203)
    await until(async () => (await box.count()) === 1)
    await page.keyboard.type('Hallway')
    await page.keyboard.press('Enter')
    await until(async () => (await entity('text', made.handle))?.geometry.value === 'Hallway')
    await page.waitForTimeout(300)
    await page.keyboard.press('Delete')
    step('Delete after Enter erases the selected text', await until(async () => (await entity('text', made.handle)) == null))
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
