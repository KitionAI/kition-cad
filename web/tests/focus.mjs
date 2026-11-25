// Keys in Focus, where the command line is hidden: a letter picks its tool (the command line's
// type-to-command capture is off), Escape cancels the running command and stays in Focus, and
// only an Escape with nothing to cancel leaves Focus. Outside Focus a letter still goes to the
// command line. node tests/focus.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const app = kit.locator('.kui-app')
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
const mode = () => app.getAttribute('data-mode')
const until = async (name, check) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await check()) {
      console.log(`PASS ${which} ${name}`)
      return
    }
    await page.waitForTimeout(100)
  }
  assert.fail(`${which} ${name}`)
}

try {
  await page.goto(`${origin}/dev.html?modes=classic,line,menus,focus&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until('sample ready', async () => (await details()).entities === 7)

  // Outside Focus, a letter typed on the editor starts a command-line entry.
  await app.focus()
  await page.keyboard.press('l')
  await until('outside Focus a letter goes to the command line', async () => (await kit.getByTestId('cad-command-line').inputValue()) === 'l')
  await kit.getByTestId('cad-command-line').press('Escape')
  await app.focus()

  await page.keyboard.press('ControlOrMeta+Backslash')
  await until('Cmd+\\ enters Focus', async () => (await mode()) === 'focus')
  assert.equal(await kit.getByTestId('cad-command-line').count(), 0, 'the command line is hidden in Focus')

  // In Focus, the letter picks its tool: the line command runs and the Focus bar names it.
  await page.keyboard.press('l')
  await until('in Focus a letter picks its tool', async () => (await details()).running === 'line.add')
  await until('the Focus bar names the tool', async () => (await kit.locator('.kui-focus-tool').textContent())?.trim() === 'Line')

  // Escape cancels the running command and stays in Focus.
  await page.keyboard.press('Escape')
  await until('Escape cancels the running command', async () => (await details()).running == null)
  assert.equal(await mode(), 'focus', 'an Escape that cancelled a command does not leave Focus')
  console.log(`PASS ${which} an Escape that cancelled a command stays in Focus`)

  // Nothing left to cancel: Escape leaves Focus.
  await page.keyboard.press('Escape')
  await until('Escape with nothing to cancel leaves Focus', async () => (await mode()) === 'classic')

  // A selection is something to cancel too: the first Escape clears it, the second leaves Focus.
  await kit.getByTestId('cad-command-line').fill('selectall')
  await kit.getByTestId('cad-command-line').press('Enter')
  await until('everything selected', async () => (await details()).selection === 7)
  await app.focus()
  await page.keyboard.press('ControlOrMeta+Backslash')
  await until('Cmd+\\ enters Focus with a selection', async () => (await mode()) === 'focus')
  await page.keyboard.press('Escape')
  await until('Escape clears the selection', async () => (await details()).selection === 0)
  assert.equal(await mode(), 'focus', 'an Escape that cleared the selection does not leave Focus')
  console.log(`PASS ${which} an Escape that cleared the selection stays in Focus`)
  await page.keyboard.press('Escape')
  await until('then Escape leaves Focus', async () => (await mode()) === 'classic')
  assert.deepEqual(errors, [])
  console.log(`PASS ${which} no page errors`)
} finally {
  await browser.close()
  await server.close()
}
