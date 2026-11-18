// Hot-grip polar tracking: actual canvas guides, mode changes, coordinates and history.
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
const output = new URL('../../temp/polar/', import.meta.url)
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
const guide = async (x = 774, y = 591) => {
  const canvas = kit.getByTestId('kit-canvas-surface')
  const box = await canvas.boundingBox()
  return canvas.evaluate((surface, point) => {
    const scale = surface.width / surface.clientWidth
    const pixels = surface.getContext('2d').getImageData(Math.round(point.x * scale) - 8, Math.round(point.y * scale) - 6, 16, 12).data
    for (let offset = 0; offset < pixels.length; offset += 4) {
      // One-pixel guides blend with the white canvas; distinguish orange from neutral grid lines and purple previews.
      const [red, green, blue] = pixels.slice(offset, offset + 3)
      if (red > 200 && red - green > 20 && green - blue > 30 && blue < 190) return true
    }
    return false
  }, { x: x - box.x, y: y - box.y })
}
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname })

try {
  await mkdir(output, { recursive: true })
  await open()
  await grip()
  await page.mouse.move(820, 594)
  await until('polar guide appears near the horizontal ray', () => guide())
  assert.equal((await inspect()).dirty, false)
  await screenshot('horizontal-preview')
  await input.press('F10')
  await until('F10 removes the guide without cancelling the grip', async () => !(await details()).drafting.polar && (await details()).prompt.startsWith('STRETCH ') && !await guide())
  await screenshot('polar-off')
  await input.press('F10')
  await until('F10 restores the guide at the same cursor', () => guide())
  await input.press('F8')
  await until('Ortho suppresses the polar guide', async () => (await details()).drafting.ortho && !await guide())
  await input.press('F8')
  await until('disabling Ortho restores polar tracking', () => guide())
  await page.mouse.click(820, 594)
  await until('pointer commits the horizontal destination', async () => (await circle())?.center[0] > 260 && (await circle())?.center[1] === 130)
  await until('commit clears the guide', async () => !await guide())
  await kit.getByTestId('cad-undo').click()
  await until('one Undo restores clean geometry after all mode changes', async () => !(await inspect()).dirty)
  await kit.getByTestId('cad-redo').click()
  await command('selectall')
  const center = (await circle()).center
  await kit.getByTestId('cad-save').click()
  await until('polar edit saves through the host', async () => !(await inspect()).dirty)
  await page.evaluate(async () => { const file = window.devHost.files.get('Sample.dxf'); await window.devHost.open('Sample.dxf', file.bytes) })
  await command('selectall')
  await until('reopened circle preserves the constrained destination', async () => JSON.stringify((await circle())?.center) === JSON.stringify(center))

  await open()
  await grip()
  await input.press('F10')
  await until('polar is disabled before free placement', async () => !(await details()).drafting.polar)
  await page.mouse.click(820, 594)
  await until('polar-off pointer keeps its unconstrained Y', async () => (await circle())?.center[0] > 260 && Math.abs((await circle()).center[1] - 130) > 0.5)

  await open()
  await command('setvar')
  await until('SETVAR requests the variable', async () => (await details()).prompt.includes('variable name'))
  await command('POLARANG')
  await until('SETVAR requests POLARANG value', async () => (await details()).prompt.includes('new value for POLARANG'))
  await command('45')
  await until('45-degree setting completes', async () => !(await details()).running)
  await grip()
  await page.mouse.move(820, 510)
  await until('45-degree polar guide follows the configured increment', () => guide(774, 553))
  await screenshot('diagonal-preview')
  await page.mouse.click(820, 510)
  await until('diagonal commit preserves equal X and Y displacement', async () => {
    const point = (await circle())?.center
    return point?.[0] > 260 && Math.abs((point[0] - 235) - (point[1] - 130)) < 1e-8
  })

  await open()
  await grip()
  await page.mouse.move(820, 594)
  await until('guide active before exact entry', () => guide())
  await command('@45,1')
  await until('typed coordinates remain exact despite the active polar ray', async () => JSON.stringify((await circle())?.center) === '[280,131]')
  await kit.getByTestId('cad-undo').click()
  await until('typed grip undo restores the sample', async () => !(await inspect()).dirty)
  await input.press('Escape')
  await grip()
  await page.mouse.move(820, 594)
  await until('guide active before cancellation', () => guide())
  await input.press('Escape')
  await until('Escape clears the guide without editing', async () => !(await details()).prompt.startsWith('STRETCH ') && !await guide() && !(await inspect()).dirty)
  assert.deepEqual(errors, [])
} catch (error) {
  await screenshot('failure').catch(() => {})
  throw error
} finally {
  await browser.close()
  await server.close()
}
