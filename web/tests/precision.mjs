// Precision entry, mode toggles, mixed properties and history in the actual CAD editor.
import assert from 'node:assert/strict'
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
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details
const circle = async () => (await details()).selectedObjects.find((object) => object.radius !== undefined)
const until = async (name, check) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await check()) { console.log(`PASS ${name}`); return }
    await page.waitForTimeout(100)
  }
  assert.fail(name)
}
const command = async (text) => { await input.fill(text); await input.press('Enter') }
const open = async (viewer = false) => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${viewer ? '&role=viewer' : ''}`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await kit.getByTestId('cad-status').waitFor()
  await until('sample opened', async () => (await details()).entities === 7)
}
const grip = async () => {
  await page.mouse.click(737, 528)
  await until('circle selected', async () => (await circle())?.radius === 30)
  await page.mouse.click(737, 592)
  await until('grip ready', async () => (await details()).prompt.startsWith('STRETCH '))
}
const evidence = async (name) => {
  await mkdir(new URL('../test-results/precision/', import.meta.url), { recursive: true })
  await page.screenshot({ path: new URL(`../test-results/precision/${which}-${name}.png`, import.meta.url).pathname })
}
const previewAt = async (x, y) => {
  const surface = kit.getByTestId('kit-canvas-surface')
  const box = await surface.boundingBox()
  return surface.evaluate((canvas, point) => {
    // The editor draws previews in the primary colour (the kit's brand, kit.json ui.brand).
    const primary = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim()
    const [red, green, blue] = [1, 3, 5].map((at) => parseInt(primary.slice(at, at + 2), 16))
    const scale = canvas.width / canvas.clientWidth
    const pixels = canvas.getContext('2d').getImageData(Math.round(point.x * scale) - 8, Math.round(point.y * scale) - 8, 16, 16).data
    for (let offset = 0; offset < pixels.length; offset += 4) {
      if (Math.abs(pixels[offset] - red) < 10 && Math.abs(pixels[offset + 1] - green) < 10 && Math.abs(pixels[offset + 2] - blue) < 10) return true
    }
    return false
  }, { x: x - box.x, y: y - box.y })
}

try {
  await open()
  await grip()
  await input.press('F8')
  await until('F8 preserves the grip and enables Ortho', async () => (await details()).drafting.ortho && (await details()).prompt.startsWith('STRETCH '))
  await kit.getByTestId('cad-mode-grid').click()
  await until('Grid preserves the grip', async () => (await details()).prompt.startsWith('STRETCH '))
  await page.mouse.move(820, 560)
  await until('Ortho preview appears at the constrained destination', () => previewAt(884, 591))
  await evidence('ortho-preview')
  await page.mouse.click(820, 560)
  await until('Ortho fixes the original Y coordinate', async () => (await circle())?.center[1] === 130 && (await circle()).center[0] > 260)
  const moved = (await circle()).center
  await kit.getByTestId('cad-undo').click()
  await until('one Undo restores geometry despite mode toggles', async () => !(await inspect()).dirty)
  await kit.getByTestId('cad-redo').click()
  await kit.getByTestId('cad-save').click()
  await until('redo saves through the host', async () => !(await inspect()).dirty)
  await page.evaluate(async () => { const file = window.devHost.files.get('Sample.dxf'); await window.devHost.open('Sample.dxf', file.bytes) })
  await command('edit.selectAll')
  await until('saved precision edit survives reopening', async () => JSON.stringify((await circle())?.center) === JSON.stringify(moved))

  await open()
  await grip()
  await input.press('F8')
  await until('Ortho enabled for snap priority check', async () => (await details()).drafting.ortho)
  await page.mouse.move(601, 547)
  await until('Endpoint preview appears at the snapped destination', () => previewAt(662, 548))
  await evidence('endpoint-preview')
  await page.mouse.click(601, 547)
  await until('Endpoint snap overrides Ortho with an exact destination', async () => JSON.stringify((await circle())?.center) === '[170,150]')

  await open()
  await grip()
  await input.press('F3')
  await until('F3 disables object snaps without cancelling the grip', async () => !(await details()).drafting.osnap && (await details()).prompt.startsWith('STRETCH '))
  await kit.getByTestId('cad-mode-snap').click()
  await until('Snap stays active during the grip', async () => (await details()).drafting.snap && (await details()).prompt.startsWith('STRETCH '))
  await page.mouse.click(820, 560)
  await until('grid snapping commits exact half-unit coordinates', async () => {
    const center = (await circle())?.center
    return center?.[0] > 260 && center.every((value) => Number.isInteger(value * 2))
  })

  await open()
  for (const [index, end] of ['280,170', '280,90'].entries()) {
    await command('line')
    await until(`connected line ${index + 1} requests its first point`, async () => (await details()).prompt.includes('first point'))
    await command('235,130')
    await until(`connected line ${index + 1} requests its endpoint`, async () => (await details()).prompt.includes('next point'))
    await command(end)
    await until(`connected line ${index + 1} reaches its endpoint`, async () => (await details()).entities === 8 + index)
    await command('')
    await until(`connected line ${index + 1} finishes`, async () => !(await details()).running)
  }
  await until('two connected lines are created through the command input', async () => (await details()).entities === 9)
  await kit.getByTestId('cad-save').click()
  await until('connected drawing starts clean', async () => !(await inspect()).dirty)
  await command('edit.selectAll')
  await until('connected drawing is selected', async () => (await details()).selectedObjects.length === 9)
  await page.mouse.click(737, 592)
  await until('shared circle and line grip is active', async () => (await details()).prompt.startsWith('STRETCH '))
  await page.mouse.move(820, 560)
  await until('both connected lines appear in the grip preview', async () => await previewAt(827, 533) && await previewAt(830, 650))
  assert.equal((await inspect()).dirty, false)
  await evidence('connected-preview')
  await input.press('Escape')
  await until('input Escape cancels the shared grip without editing', async () => !(await details()).prompt.startsWith('STRETCH ') && !(await inspect()).dirty)
  await command('edit.selectAll')
  await page.mouse.click(737, 592)
  await until('shared grip can be restarted after cancellation', async () => (await details()).prompt.startsWith('STRETCH '))
  await page.mouse.click(820, 560)
  await until('shared grip commits from the canvas', async () => (await circle())?.center[0] > 260 && (await inspect()).dirty)
  await kit.getByTestId('cad-undo').click()
  await until('one Undo restores the complete shared grip edit', async () => !(await inspect()).dirty)
  await command('edit.selectAll')
  await until('all connected objects are selected before locking', async () => (await details()).selectedObjects.length === 9)
  await kit.getByTestId('cad-layer-Walls-lock').click()
  await kit.getByTestId('cad-save').click()
  await until('locked connected members remain selected after saving', async () => !(await inspect()).dirty && (await details()).selectedObjects.length === 9)
  await page.mouse.click(737, 592)
  await until('unlocked circle grip remains available beside locked lines', async () => (await details()).prompt.startsWith('STRETCH '))
  await page.mouse.move(820, 560)
  await until('unlocked circle still previews its destination', () => previewAt(884, 560))
  assert.equal(await previewAt(827, 533), false, 'locked upper line must not preview a move')
  assert.equal(await previewAt(830, 650), false, 'locked lower line must not preview a move')
  await evidence('connected-locked-preview')
  await page.keyboard.press('Escape')
  await until('canvas Escape cancels the locked-member preview cleanly', async () => !(await details()).prompt.startsWith('STRETCH ') && !(await inspect()).dirty)

  await open()
  await command('edit.selectAll')
  await until('mixed selection exposes all seven objects', async () => (await details()).selectedObjects.length === 7)
  const layer = kit.getByTestId('cad-selection-layer')
  assert.equal(await layer.inputValue(), '')
  assert.equal(await kit.getByRole('spinbutton', { name: 'Radius', exact: true }).count(), 0)
  await layer.selectOption('0')
  await until('layer edit applies to the whole mixed selection', async () => (await details()).selectedObjects.length === 7 && (await details()).selectedObjects.every((object) => object.layer === '0'))
  assert.equal((await details()).currentLayer, 'Walls')
  await evidence('mixed-properties')
  await kit.getByTestId('cad-undo').click()
  await command('edit.selectAll')
  await until('Undo restores all original object layers', async () => new Set((await details()).selectedObjects.map((object) => object.layer)).size === 3)
  await kit.getByTestId('cad-redo').click()
  await command('edit.selectAll')
  await until('Redo restores the mixed-selection edit', async () => (await details()).selectedObjects.length === 7 && (await details()).selectedObjects.every((object) => object.layer === '0'))
  await kit.getByTestId('cad-undo').click()
  await command('edit.selectAll')
  await kit.getByTestId('cad-layer-Furniture-lock').click()
  await until('a locked member disables mixed property editing', async () => await layer.isDisabled())

  await open()
  await page.mouse.click(737, 528)
  const radius = kit.getByRole('spinbutton', { name: 'Radius', exact: true })
  await radius.fill('-1')
  await radius.press('Enter')
  await until('invalid radius restores its field without an edit', async () => await radius.inputValue() === '30' && !(await inspect()).dirty)
  await radius.fill('55')
  await radius.press('Escape')
  await radius.press('Tab')
  await until('Escape cancels a property draft without an edit', async () => (await circle())?.radius === 30 && !(await inspect()).dirty)

  await open()
  await grip()
  await page.keyboard.press('Delete')
  await until('Delete removes the object and ends the grip', async () => (await details()).entities === 6 && !(await details()).prompt.startsWith('STRETCH '))
  await kit.getByTestId('cad-undo').click()
  await until('Undo restores a deleted gripped object cleanly', async () => (await details()).entities === 7 && !(await inspect()).dirty)

  await open(true)
  await page.mouse.click(737, 528)
  await page.mouse.click(737, 592)
  await page.mouse.click(820, 560)
  assert.equal((await details()).entities, 7)
  assert.equal((await inspect()).dirty, false)
  assert.equal(await input.isDisabled(), true)
  console.log('PASS grip gestures cannot modify a viewer document')
  assert.deepEqual(errors, [])
} catch (error) {
  await evidence('failure').catch(() => {})
  throw error
} finally {
  await browser.close()
  await server.close()
}
