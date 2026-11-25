// Regression coverage for the CAD editor's keyboard, grips, properties and viewport commands.
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
const until = async (message, check) => {
  for (let i = 0; i < 100; i += 1) {
    if (await check()) { console.log(`PASS ${message}`); return }
    await page.waitForTimeout(100)
  }
  assert.fail(message)
}
const command = async (...entries) => {
  for (const text of entries) { await input.fill(text); await input.press('Enter'); await page.waitForTimeout(100) }
}
const reopen = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await kit.getByTestId('cad-status').waitFor()
  await until('sample is ready', async () => (await details()).entities === 7)
}
const pixels = () => kit.getByTestId('kit-canvas-surface').evaluate((canvas) => canvas.toDataURL())
const circle = async () => (await details()).selectedObjects[0]
const evidence = async (name) => {
  await mkdir(new URL('../test-results/interactions/', import.meta.url), { recursive: true })
  await page.screenshot({ path: new URL(`../test-results/interactions/${which}-${name}.png`, import.meta.url).pathname })
}

try {
  await reopen()
  await page.mouse.click(737, 528)
  await until('selects the circle on its actual layer', async () => (await circle())?.layer === 'Furniture')
  assert.equal(await kit.getByTestId('cad-selection-layer').inputValue(), 'Furniture')
  assert.equal((await details()).currentLayer, 'Walls')
  await evidence('properties')
  await kit.getByRole('spinbutton', { name: 'Radius', exact: true }).fill('35')
  await kit.getByRole('spinbutton', { name: 'Radius', exact: true }).press('Enter')
  await until('edits the selected radius', async () => (await circle())?.radius === 35)
  await kit.getByTestId('cad-undo').click()
  await until('property undo returns to the saved drawing', async () => !(await inspect()).dirty)
  await page.mouse.click(737, 528)
  await until('undo restores the radius', async () => (await circle())?.radius === 30)

  await page.mouse.click(737, 592)
  await until('center grip starts a stretch', async () => (await kit.getByTestId('cad-prompt').innerText()).includes('STRETCH'))
  await input.press('Escape')
  await until('Escape cancels a grip without editing', async () => !(await inspect()).dirty)
  await page.mouse.click(737, 592)
  await command('255,140')
  await until('grip accepts precise coordinates', async () => JSON.stringify((await circle())?.center) === '[255,140]')
  await kit.getByTestId('cad-undo').click()
  await until('one undo restores the grip edit', async () => !(await inspect()).dirty)

  await page.mouse.click(737, 528)
  await until('circle can be selected again after undo', async () => (await circle())?.radius === 30)
  await page.mouse.click(737, 592)
  await page.mouse.move(780, 570)
  await evidence('grip-preview')
  await page.mouse.click(780, 570)
  await until('second click commits the grip destination', async () => Math.abs(((await circle())?.center[0] ?? 0) - 255) < 1)
  await kit.getByTestId('cad-undo').click()
  await until('mouse grip edit is a single undo step', async () => !(await inspect()).dirty)

  await input.press('Escape')
  await page.mouse.click(1100, 300)
  await page.keyboard.press('Escape')
  await page.keyboard.type('line')
  assert.equal(await input.inputValue(), 'line', 'the first character must not be lost when focus moves')
  await page.keyboard.press('Enter')
  await until('canvas typing starts LINE', async () => (await details()).running === 'line.add')
  await command('10,10', '100,10', '')
  await until('keyboard coordinates add one line', async () => (await details()).entities === 8)
  await input.press('ArrowUp')
  assert.equal(await input.inputValue(), 'line')
  await input.press('ArrowDown')
  assert.equal(await input.inputValue(), '')
  console.log('PASS command history excludes coordinate replies and restores the draft')
  const ortho = (await details()).drafting.ortho
  await input.press('F8')
  await until('F8 works with command input focus', async () => (await details()).drafting.ortho !== ortho)
  await input.fill('draft')
  await input.press('Escape')
  assert.equal(await input.inputValue(), '')
  await input.fill('typing')
  await input.press('Meta+z')
  assert.equal((await details()).entities, 8, 'text undo must not undo the drawing')
  await input.fill('')

  await page.mouse.move(700, 500)
  const initialZoom = await kit.getByTestId('kit-zoom').innerText()
  await page.mouse.wheel(0, -250)
  await until('plain wheel zooms the CAD canvas', async () => (await kit.getByTestId('kit-zoom').innerText()) !== initialZoom)
  const zoomed = await kit.getByTestId('kit-zoom').innerText()
  await command('zoom.extents')
  await until('typed Zoom Extents changes the viewport', async () => (await kit.getByTestId('kit-zoom').innerText()) !== zoomed)
  const fitZoom = await kit.getByTestId('kit-zoom').innerText()
  await page.mouse.move(700, 500)
  await page.mouse.wheel(0, -250)
  await kit.getByTestId('cad-zoom-extents').click()
  await until('toolbar and command Zoom Extents agree', async () => (await kit.getByTestId('kit-zoom').innerText()) === fitZoom)

  await command('grid {"on":false}')
  await page.mouse.move(1400, 250)
  await page.waitForTimeout(500)
  const noGrid = await pixels()
  await kit.getByTestId('cad-mode-grid').click()
  await until('Grid changes the rendered drawing', async () => (await pixels()) !== noGrid)
  await page.mouse.click(700, 450, { button: 'right' })
  await kit.getByTestId('cad-context-menu').waitFor()
  await evidence('context-menu')
  await page.keyboard.press('Escape')
  await until('Escape dismisses the context menu', async () => await kit.getByTestId('cad-context-menu').count() === 0)
  await page.mouse.click(700, 450, { button: 'right' })
  await kit.getByTestId('cad-context-edit.undo').click()
  await until('context-menu Undo uses the drawing history', async () => (await details()).entities === 7)

  await command('line', '10,10', '100,10', '')
  await kit.getByTestId('cad-save').click()
  await until('saves through the host', async () => !(await inspect()).dirty)
  await page.evaluate(async () => { const file = window.devHost.files.get('Sample.dxf'); await window.devHost.open('Sample.dxf', file.bytes) })
  await until('saved edit survives reopening', async () => (await details()).entities === 8 && !(await inspect()).dirty)

  // No artificial pause between finishing a command and choosing the next tool.
  // A late engine switch-to-Select reply must not cancel the new Circle choice.
  for (let round = 0; round < 10; round += 1) {
    const start = (await details()).entities
    for (const text of ['line', '10,10', '100,10', '']) {
      await input.fill(text)
      await input.press('Enter')
    }
    await kit.getByTestId('cad-tool-circle.add').click()
    await until(`rapid transition ${round + 1}: Circle stays active`, async () =>
      (await details()).running === 'circle.add' && await kit.getByTestId('cad-tool-circle.add').getAttribute('aria-pressed') === 'true')
    await command('250,180', '10')
    await until(`rapid transition ${round + 1}: both entities survive`, async () =>
      (await details()).entities === start + 2 && (await details()).running === null)
  }

  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US&role=viewer`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  assert.equal(await input.isDisabled(), true)
  await page.mouse.click(737, 528)
  await page.keyboard.type('line')
  await page.keyboard.press('Enter')
  assert.equal((await details()).entities, 7)
  assert.equal((await inspect()).dirty, false)
  console.log('PASS new input routes cannot edit a view-only drawing')
  assert.deepEqual(errors, [], 'no browser errors')
} catch (error) {
  await mkdir(new URL('../test-results/interactions/', import.meta.url), { recursive: true })
  await page.screenshot({ path: new URL(`../test-results/interactions/${which}.png`, import.meta.url).pathname }).catch(() => {})
  throw error
} finally {
  await browser.close()
  await server.close()
}
