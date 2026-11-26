// The command line, by hand: the history of what commands write (DIST's result, "Command: X",
// "*Cancel*"), errors on the history instead of a card, the prompt's options and default as buttons,
// the prompt above the input and a placeholder that says what the prompt waits for, completion of
// typed names (Enter, Tab, a click), F2's history window, the history's rows, every menu and Home
// button of a command with prompts starting them (from the engine's command list, not by hand),
// Classic as the default display mode, and Pan and Zoom from the command menu inside a command.
// node tests/command-line.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
// A missing control fails its step in seconds, not after Playwright's 30 s.
page.setDefaultTimeout(8000)
const kit = page.frameLocator('#kit')
const input = kit.getByTestId('cad-command-line')
const output = new URL('../test-results/command-line/', import.meta.url)
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
const command = async (...entries) => {
  for (const text of entries) { await input.fill(text); await input.press('Enter'); await page.waitForTimeout(150) }
}
const idle = () => until(async () => (await details()).running == null)
const engineHistory = async () => (await details()).drafting.commandLine.history.lines
const history = async () => (await kit.getByTestId('cad-history').innerText({ timeout: 1000 }).catch(() => '')).split('\n').filter(Boolean)
const toasts = () => kit.locator('.kui-toast').count()
const open = async (query = '?modes=classic&locale=en-US') => {
  await page.goto(`${origin}/dev.html${query}`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  await page.waitForTimeout(300)
}
const screenshot = async (name) => page.screenshot({ path: new URL(`${which}-${name}.png`, output).pathname }).catch(() => {})
const section = async (name, body, query) => {
  try {
    await open(query)
    await body()
  } catch (error) {
    step(`${name} (threw)`, false, error instanceof Error ? error.message.split('\n')[0] : String(error))
    await screenshot(`failure-${name.replace(/\W+/g, '-')}`)
  }
}
const escape = async () => { await page.waitForTimeout(300); await input.press('Escape'); await input.press('Escape') }

await mkdir(output, { recursive: true })
try {
  // 1. The history (B7, N7): what DIST answers, the command it started, "*Cancel*".
  await section('history', async () => {
    await command('dist', '0,0', '30,40')
    await idle()
    const lines = await history()
    step('DIST\'s result is on the history', lines.some((line) => line.startsWith('Distance = 50')), lines.join(' | '))
    step('the history has the command it started', (await engineHistory()).includes('Command: DIST'), (await engineHistory()).join(' | '))
    step('three history lines show by default', lines.length === 3, String(lines.length))
    await screenshot('history')
    await command('line')
    await page.waitForTimeout(200)
    await input.press('Escape')
    await idle()
    step('Escape writes *Cancel*', await until(async () => (await history()).at(-1) === '*Cancel*'), (await history()).join(' | '))
    // The history's rows follow a drag of its top edge (0 to 12, 3 by default).
    const grip = await kit.getByTestId('cad-history-grip').boundingBox()
    await page.mouse.move(grip.x + grip.width / 2, grip.y + 2)
    await page.mouse.down()
    await page.mouse.move(grip.x + grip.width / 2, grip.y + 2 - 17 * 3, { steps: 4 })
    await page.mouse.up()
    step('dragging the history up shows more lines', await until(async () => (await history()).length >= 5), String((await history()).length))
    // F2 opens the whole history, scrollable, and closes it; from the canvas and from the input.
    await kit.locator('.kui-app').focus()
    await page.keyboard.press('F2')
    step('F2 opens the history window', await until(async () => (await kit.locator('[data-testid="cad-history"][data-open="true"]').count()) === 1))
    await screenshot('history-window')
    await input.focus()
    await page.keyboard.press('F2')
    step('F2 closes it again', await until(async () => (await kit.locator('[data-testid="cad-history"][data-open="true"]').count()) === 0))
    await kit.getByTestId('cad-history-toggle').click()
    step('the history button opens it too', await until(async () => (await kit.locator('[data-testid="cad-history"][data-open="true"]').count()) === 1))
  })

  // 2. Errors go on the history (awkward 1): no card, a line, the input shakes.
  await section('errors', async () => {
    const cards = await toasts()
    await input.fill('qqqz')
    await input.press('Enter')
    const shook = await until(async () => (await kit.locator('form.cad-command-line[data-shake="true"]').count()) === 1, 20)
    await page.waitForTimeout(400)
    step('an unknown command shows no card', (await toasts()) === cards, `${await toasts()} cards`)
    const error = kit.locator('.cad-history-line[data-error="true"]')
    step('an unknown command is a line on the history', (await error.count()) === 1 && /qqqz/i.test(await error.innerText()), await error.allInnerTexts().then((t) => t.join(' | ')))
    step('the input shakes', shook)
    await command('line', '0,0')
    await input.fill('nowhere')
    await input.press('Enter')
    step('an entry the command refuses shakes the input', await until(async () => (await kit.locator('form.cad-command-line[data-shake="true"]').count()) === 1, 20))
    step('and its reason is on the history', await until(async () => (await history()).some((line) => /required|invalid/i.test(line))), (await history()).join(' | '))
    step('the command still asks', (await details()).running === 'line.add')
    await escape()
  })

  // 3. The prompt (N5, awkward 3, 11): above the input, its options and default as buttons, a
  // placeholder saying what it waits for, a long prompt leaving the input its width.
  await section('prompt', async () => {
    const idlePlaceholder = await input.getAttribute('placeholder')
    const idleWidth = (await input.boundingBox()).width
    await command('zoom')
    const prompt = (await kit.getByTestId('cad-prompt').boundingBox())
    const field = await input.boundingBox()
    step('the prompt is on the line above the input', prompt.y + prompt.height <= field.y + 1, `${prompt.y + prompt.height} / ${field.y}`)
    step('a long prompt leaves the input its width', Math.abs((await input.boundingBox()).width - idleWidth) < 2, `${idleWidth} -> ${(await input.boundingBox()).width}`)
    await escape()
    await idle()
    const start = (await details()).entities
    await command('line', '10,10', '100,10', '100,100')
    const waiting = await input.getAttribute('placeholder')
    step('the placeholder says what the prompt waits for', waiting !== idlePlaceholder && /point/i.test(waiting), `${idlePlaceholder} / ${waiting}`)
    await screenshot('prompt-options')
    await kit.getByTestId('cad-prompt-option-Close').click()
    await idle()
    step('clicking [Close] closes the line', (await details()).entities === start + 3 && (await details()).running == null, `${(await details()).entities - start} lines`)
    await command('circle', '300,100', '12')
    await idle()
    await command('circle', '340,100')
    const fallback = kit.getByTestId('cad-prompt-default')
    step('the radius prompt offers its default', (await fallback.count()) === 1 && /12/.test(await fallback.innerText()), await kit.getByTestId('cad-prompt').innerText())
    await fallback.click()
    await idle()
    const circles = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'object.query', params: { type: 'circle' } }], false))
    const drawn = (circles.completed?.[0]?.result ?? circles.completed?.[0]?.value ?? circles.completed?.[0])?.entities ?? []
    step('clicking the default enters it', drawn.some((c) => Math.abs((c.geometry?.radius ?? 0) - 12) < 1e-9 && Math.abs((c.geometry?.center?.x ?? 0) - 340) < 1e-9), JSON.stringify(drawn.map((c) => c.geometry)))
  })

  // 4. Completion (N6): CI and Enter starts CIRCLE, Tab completes, a click starts, at most 8.
  await section('completion', async () => {
    await input.fill('ci')
    const list = kit.getByTestId('cad-completions')
    step('typing a part of a name lists the commands it may mean', await until(async () => (await list.count()) === 1))
    step('at most eight', (await list.locator('li').count()) <= 8, String(await list.locator('li').count()))
    step('the first is CIRCLE', /^CIRCLE\b/.test(await kit.getByTestId('cad-completion-0').innerText()), await kit.getByTestId('cad-completion-0').innerText())
    await screenshot('completion')
    await input.press('Enter')
    step('CI and Enter starts CIRCLE', await until(async () => (await details()).running === 'circle.add'))
    await escape()
    await idle()
    await input.fill('rec')
    await until(async () => (await list.count()) === 1)
    await input.press('Tab')
    step('Tab completes the name', (await input.inputValue()) === 'rectang', await input.inputValue())
    await input.press('Enter')
    step('the completed name runs', await until(async () => (await details()).running === 'rectangle.add'))
    await escape()
    await idle()
    await input.fill('donu')
    await until(async () => (await list.count()) === 1)
    await kit.getByTestId('cad-completion-0').click()
    step('clicking a suggestion starts it', await until(async () => (await details()).running === 'donut.add'))
    await escape()
    await idle()
    await input.fill('l')
    await until(async () => (await list.count()) === 1)
    step('an alias is listed first as the command it names', /^L \(LINE\)/.test(await kit.getByTestId('cad-completion-0').innerText()), await kit.getByTestId('cad-completion-0').innerText())
    await input.press('ArrowUp')
    step('Up still recalls the history, not the list', (await input.inputValue()) !== 'l', await input.inputValue())
    await input.press('Escape')
  })

  // 5. Buttons start the prompts (B6): Draw › Donut, then every enabled button of a command with
  // prompts on the menu tabs and Home, from the engine's command list.
  await section('buttons', async () => {
    const start = (await details()).entities
    await kit.getByTestId('ribbon-tab-Draw').click()
    await kit.getByTestId('cad-cmd-donut.add').click()
    step('Draw › Donut starts DONUT\'s first prompt', await until(async () => /^DONUT /.test(await kit.getByTestId('cad-prompt').innerText())), await kit.getByTestId('cad-prompt').innerText())
    step('and draws nothing', (await details()).entities === start && (await details()).running === 'donut.add')
    await escape()
    await idle()
    const listed = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'command.list', params: {} }], false))
    const catalog = (listed.completed?.[0]?.result ?? listed.completed?.[0]?.value ?? listed.completed?.[0])?.commands ?? []
    step('the engine lists its commands', catalog.length > 250, String(catalog.length))
    const prompts = catalog.filter((c) => c.interactive && c.commandLine && !/(\.\.\.|…)\s*$/.test(c.title))
    const tabs = ['home', 'File', 'Edit', 'Insert', 'Format', 'Tools', 'Draw', 'Dimension', 'Modify', 'View']
    const wrong = []
    let tried = 0
    for (const tab of tabs) {
      await kit.getByTestId(`ribbon-tab-${tab}`).click()
      await page.waitForTimeout(150)
      for (const entry of prompts) {
        const button = kit.getByTestId(`cad-cmd-${entry.id}`)
        if ((await button.count()) !== 1 || !(await button.isVisible()) || (await button.isDisabled())) continue
        tried += 1
        const before = await details()
        await button.click()
        // It starts its prompts; one with nothing to act on here (DIMCONTINUE with no dimension)
        // starts and ends at once, saying why on the history.
        const echo = `Command: ${entry.name.toUpperCase()}`
        const started = await until(async () => {
          const now = await details()
          const added = now.drafting.commandLine.history.serial - before.drafting.commandLine.history.serial
          return now.running === entry.id || (added > 0 && now.drafting.commandLine.history.lines.slice(-added).includes(echo))
        }, 30)
        const after = await details()
        if (!started || after.entities !== before.entities) wrong.push(`${tab}/${entry.id}: running ${after.running}, ${after.entities - before.entities} objects`)
        await escape()
        await idle()
        if ((await details()).entities !== before.entities) {
          await command('u')
          await idle()
        }
      }
    }
    step('every enabled button of a command with prompts starts them and draws nothing', wrong.length === 0 && tried > 40, `${tried} buttons; ${wrong.join('; ')}`)
    // A button chosen while another command runs ends it first, as a CAD menu does.
    await command('line', '0,0')
    await kit.getByTestId('ribbon-tab-Draw').click()
    await kit.getByTestId('cad-cmd-donut.add').click()
    step('a button during another command starts its own', await until(async () => (await details()).running === 'donut.add'))
    await escape()
  })

  // 6. Pan and Zoom from the command menu, and typed transparently, keep the command running.
  await section('transparent', async () => {
    await command('line', '20,20')
    const view = JSON.stringify((await details()).view)
    await command("'zoom.in")
    step("'zoom.in keeps LINE running", (await details()).running === 'line.add')
    const surface = await kit.getByTestId('kit-canvas-surface').boundingBox()
    await page.mouse.move(surface.x + surface.width * 0.7, surface.y + surface.height * 0.4)
    await page.mouse.down({ button: 'right' })
    await page.waitForTimeout(400)
    await page.mouse.up({ button: 'right' })
    await kit.getByTestId('cad-command-menu').waitFor({ timeout: 3000 })
    await kit.getByTestId('cad-command-menu-zoom').click()
    await kit.getByTestId('cad-zoom-menu-zoom.out').click()
    await page.waitForTimeout(300)
    step('Zoom ▸ Out from the command menu keeps LINE running', (await details()).running === 'line.add')
    await page.mouse.down({ button: 'right' })
    await page.waitForTimeout(400)
    await page.mouse.up({ button: 'right' })
    await kit.getByTestId('cad-command-menu').waitFor({ timeout: 3000 })
    await kit.getByTestId('cad-command-menu-pan').click()
    await kit.getByTestId('cad-pan-menu-pan.left').click()
    await page.waitForTimeout(300)
    step('Pan ▸ Left from the command menu keeps LINE running', (await details()).running === 'line.add')
    step('the view moved meanwhile', JSON.stringify((await details()).view) !== view, `${view} -> ${JSON.stringify((await details()).view)}`)
    await escape()
  })

  // 7. Classic is the default display mode (B9): the ribbon stays open, Line is one click.
  await section('default mode', async () => {
    const mode = await kit.locator('.kui-app').getAttribute('data-mode')
    step('with no mode from the host the editor opens in Classic', mode === 'classic', String(mode))
    await kit.getByTestId('cad-tool-line.add').click()
    step('Line is one click away', await until(async () => (await details()).running === 'line.add'))
    for (const id of ['cad-cmd-object.chamfer', 'cad-cmd-object.break', 'cad-cmd-object.join', 'cad-cmd-object.explode', 'cad-cmd-point.add']) {
      step(`Home shows ${id.replace('cad-cmd-', '')}`, (await kit.getByTestId(id).count()) === 1)
    }
    await screenshot('home-classic')
    await escape()
  }, '?locale=en-US')
} finally {
  if (errors.length) step('no page errors', false, errors.slice(0, 5).join(' | '))
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
