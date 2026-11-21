// Two-stage Escape, cancelled pointer release and subsequent edit history.
import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
const kit = page.frameLocator('#kit')
const input = kit.getByTestId('cad-command-line')
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const output = new URL('../test-results/cancellation/', import.meta.url)
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
const open = async () => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until('sample ready', async () => (await details()).entities === 7)
}
const grip = async () => {
  await page.mouse.click(737, 528)
  await until('circle selected', async () => (await circle())?.radius === 30)
  await page.mouse.click(737, 592)
  await until('grip active', async () => (await details()).prompt.startsWith('STRETCH '))
}
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname })
const idleWithSelection = async (count) => {
  const state = await details()
  return !state.prompt.startsWith('STRETCH ') && state.selection === count && !(await inspect()).dirty
}

try {
  await mkdir(output, { recursive: true })
  for (const focus of ['input', 'canvas']) {
    await open()
    await grip()
    await page.mouse.move(820, 594)
    if (focus === 'input') await input.press('Escape')
    else await page.keyboard.press('Escape')
    await until(`${focus}: first Escape retains the circle selection without an edit`, () => idleWithSelection(1))
    assert.deepEqual((await circle()).center, [235, 130])
    await screenshot(`${focus}-first-escape`)
    if (focus === 'input') await input.press('Escape')
    else await page.keyboard.press('Escape')
    await until(`${focus}: second Escape clears selection`, () => idleWithSelection(0))
  }

  await open()
  await grip()
  await page.mouse.move(737, 592)
  await page.mouse.down()
  await page.keyboard.press('Escape')
  await page.mouse.up()
  await until('Escape while held cancels without losing selection', () => idleWithSelection(1))
  await page.mouse.click(737, 592)
  await until('release after Escape did not reactivate the grip or open a window', async () => (await details()).prompt.startsWith('STRETCH '))
  await command('@45,12')
  await until('fresh grip input commits after the cancelled gesture', async () => JSON.stringify((await circle())?.center) === '[280,142]')
  await kit.getByTestId('cad-undo').click()
  await until('cancelled gesture adds no undo step', async () => !(await inspect()).dirty)
  await kit.getByTestId('cad-redo').click()
  await command('selectall')
  await until('Redo restores the subsequent edit', async () => JSON.stringify((await circle())?.center) === '[280,142]')
  await kit.getByTestId('cad-save').click()
  await until('subsequent edit saves through the host', async () => !(await inspect()).dirty)
  await page.evaluate(async () => { const file = window.devHost.files.get('Sample.dxf'); await window.devHost.open('Sample.dxf', file.bytes) })
  await command('selectall')
  await until('subsequent edit survives reopening', async () => JSON.stringify((await circle())?.center) === '[280,142]')

  await open()
  await page.mouse.move(650, 480)
  await page.mouse.down()
  await page.mouse.move(820, 700, { steps: 4 })
  await page.keyboard.press('Escape')
  await page.mouse.up()
  await until('cancelled selection drag leaves no selection or edit', () => idleWithSelection(0))
  await page.mouse.click(737, 528)
  await until('next click selects the circle after the cancelled drag', async () => (await circle())?.radius === 30)
  await page.mouse.click(737, 592)
  await until('next grip works after the cancelled selection drag', async () => (await details()).prompt.startsWith('STRETCH '))
  await input.press('Escape')
  await until('next grip also cancels while retaining selection', () => idleWithSelection(1))

  await open()
  await command('selectall')
  await until('mixed selection includes all seven objects', async () => (await details()).selection === 7)
  const selected = (await details()).selectedObjects
  await page.mouse.click(737, 592)
  await until('grip starts in mixed selection', async () => (await details()).prompt.startsWith('STRETCH '))
  await page.mouse.move(820, 594)
  await input.press('Escape')
  await until('first Escape retains the entire mixed selection', () => idleWithSelection(7))
  assert.deepEqual((await details()).selectedObjects, selected)
  await screenshot('mixed-first-escape')
  await input.press('Escape')
  await until('second Escape clears the entire mixed selection', () => idleWithSelection(0))
  assert.deepEqual(errors, [])
} catch (error) {
  await screenshot('failure').catch(() => {})
  throw error
} finally {
  await browser.close()
  await server.close()
}
