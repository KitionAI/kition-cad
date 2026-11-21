// Submit successive CAD commands without waiting for prompts between replies.
import assert from 'node:assert/strict'
import { mkdir, writeFile } from 'node:fs/promises'
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
const output = new URL('../test-results/rapid-commands/', import.meta.url)
const errors = []
const results = []
page.on('pageerror', (error) => errors.push(error.message))
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const until = async (name, check) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await check()) return
    await page.waitForTimeout(100)
  }
  assert.fail(name)
}

// Read only the LINE records from the saved ASCII DXF; test the actual saved coordinates.
function savedLines(bytes) {
  const lines = new TextDecoder().decode(new Uint8Array(bytes)).trimEnd().split(/\r?\n/)
  const records = []
  let record = null
  for (let index = 0; index + 1 < lines.length; index += 2) {
    const code = lines[index].trim()
    const value = lines[index + 1].trim()
    if (code === '0') {
      if (record) records.push(record)
      record = value === 'LINE' ? {} : null
    } else if (record) record[code] = value
  }
  if (record) records.push(record)
  return records.map((record) => ['10', '20', '11', '21'].map((code) => Number(record[code])))
}

try {
  await mkdir(output, { recursive: true })
  for (let trial = 1; trial <= 10; trial += 1) {
    await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
    await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
    await until('sample is ready', async () => (await inspect()).details.entities === 7)
    // Deliberately no prompt or worker wait between the eight submissions.
    for (const text of ['line', '235,130', '280,170', '', 'line', '235,130', '280,90', '']) {
      await input.fill(text)
      await input.press('Enter')
    }
    await until('both consecutive lines finish', async () => {
      const state = (await inspect()).details
      return state.entities === 9 && state.running === null
    })
    await kit.getByTestId('cad-undo').click()
    await until('first Undo removes only the second line', async () => (await inspect()).details.entities === 8)
    await kit.getByTestId('cad-undo').click()
    await until('second Undo restores the clean sample', async () => {
      const state = await inspect()
      return state.details.entities === 7 && !state.dirty
    })
    for (const entities of [8, 9]) {
      await kit.getByTestId('cad-redo').click()
      await until('Redo restores one complete command', async () => (await inspect()).details.entities === entities)
    }
    await kit.getByTestId('cad-save').click()
    await until('host save completes', async () => !(await inspect()).dirty)
    const bytes = await page.evaluate(() => Array.from(new Uint8Array(window.devHost.files.get('Sample.dxf').bytes)))
    const geometry = savedLines(bytes)
    for (const endY of [170, 90]) {
      assert.equal(geometry.filter((line) => JSON.stringify(line) === JSON.stringify([235, 130, 280, endY])).length, 1)
    }
    await page.evaluate(async () => {
      const file = window.devHost.files.get('Sample.dxf')
      await window.devHost.open('Sample.dxf', file.bytes)
    })
    await until('reopen keeps both lines', async () => {
      const state = await inspect()
      return state.details.entities === 9 && !state.dirty
    })
    assert.deepEqual(errors, [])
    results.push({ trial, entities: 9, savedLines: geometry, undoRedo: true, reopened: true })
    console.log(`PASS ${which} round ${trial}: consecutive commands, separate undo/redo, exact saved geometry and reopen`)
  }
  await until('reopened drawing has painted before capturing evidence', async () => {
    const surface = kit.getByTestId('kit-canvas-surface')
    if (!await surface.count()) return false
    return surface.evaluate((canvas) => {
      if (!canvas.width || !canvas.height) return false
      const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
      for (let offset = 0; offset < pixels.length; offset += 4) {
        if (pixels[offset] < 80 && pixels[offset + 1] < 120 && pixels[offset + 2] > 160 && pixels[offset + 3] > 0) return true
      }
      return false
    })
  })
  await page.screenshot({ path: new URL(`${which}-complete.png`, output).pathname })
} catch (error) {
  results.push({ failure: String(error), state: await inspect().catch(() => null) })
  await page.screenshot({ path: new URL(`${which}-failure.png`, output).pathname }).catch(() => {})
  throw error
} finally {
  await writeFile(new URL(`${which}.json`, output), JSON.stringify(results, null, 2))
  await browser.close()
  await server.close()
}
