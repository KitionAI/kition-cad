// Every label the editor shows has its Chinese (walkthrough 别扭 2): the labels written in the
// editor's code (phrase keys, the context menus' tables, the status bar's modes), the Properties
// rows and object types the engine's describe command and documents name, the engine's command
// titles and menu groups (ribbon tabs, menus and the command line's completions), and what the page
// actually draws in zh-CN: every ribbon tab's groups and buttons, the status bar and its menus, the
// Model tab (a layout's name is the drawing's own), the drawing menu with and without a selection, and the Properties panel. Command
// prompts stay English (owner's decision), so the command line's prompt is not checked.
// node tests/phrases.mjs [chromium|webkit]; prints PASS or FAIL per step (naming what is untranslated), exits 1 on any FAIL.
import { readdir, readFile } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}
const source = (path) => readFile(new URL(`../${path}`, import.meta.url), 'utf8')
const all = (text, pattern) => [...text.matchAll(pattern)].map((match) => match.slice(1).filter(Boolean)).flat()

const { PHRASES, CATALOGS } = await server.ssrLoadModule('/src/strings.ts')
const zh = PHRASES['zh-CN']
/** Text that needs no translation: no word of two Latin letters, or only units, keys and file formats. */
const ALLOWED = /\b(mm|PDF|DXF|DWG|F\d{1,2}|Ctrl|Cmd|Shift)\b/g
const latin = (text) => /[A-Za-z]{2,}/.test(text.replace(ALLOWED, ''))
const chinese = (text) => /[㐀-鿿]/.test(text)
/** The keys of `keys` that zh-CN does not translate. */
const missing = (keys) => [...new Set(keys)].filter((key) => latin(key) && !(key in zh)).sort()
const report = (name, list) => step(name, list.length === 0, list.length ? `${list.length} untranslated: ${list.slice(0, 40).join(' | ')}` : '')

// 1. The editor's own catalog: every en-US string has its zh-CN one.
const en = Object.keys(CATALOGS['en-US'])
report('every editor string has zh-CN', en.filter((key) => !(key in CATALOGS['zh-CN'])))

// 2. Labels written in the editor's code, translated with phrase().
const files = (await readdir(new URL('../src/', import.meta.url), { recursive: true })).filter((file) => /\.tsx?$/.test(file) && file !== 'strings.ts')
const code = (await Promise.all(files.map((file) => source(`src/${file}`)))).join('\n')
// The context menus' and the status bar's label tables: `Array<[typed, label]>`, `Array<[kinds, id, label]>`.
const tables = all(code, /: Array<\[string(?:\[\])?, string(?:, string)?\]> = \[([^\n]*\]\n|[\s\S]*?\n\])/g)
const written = [
  ...all(code, /\b(?:phrase|p)\('((?:[^'\\]|\\.)+)'\)/g),
  ...tables.flatMap((table) => all(table, /'([^']*)'\s*\]/g)),
  ...all(code, /\b(?:command|edit)\('[^']+',\s*'([^']+)'\)/g),
  ...all(code, /\blabel: '([^']+)'/g),
]
step('the editor code\'s labels are read', written.length > 60 && tables.length >= 5, `${written.length} labels in ${tables.length} tables`)
report('labels in the editor code', missing(written))

// 3. The Properties panel's groups and rows (the engine's describe command) and object types.
const properties = await source('../bindings/wasm/src/properties.rs')
const rows = [
  ...all(properties, /rows\.(?:push|number|text|choice)\("([^"]+)",\s*"[^"]*",\s*"([^"]+)"/g),
  ...all(properties, /rows\.point\("([^"]+)",\s*"[^"]*",\s*\["([^"]+)",\s*"([^"]+)"\]/g),
  ...all(properties, /\brow\(\s*"[^"]*",\s*"([^"]+)"/g),
  ...all(properties, /\bgroup: "([^"]+)"/g),
]
step('the describe command names its rows', rows.length > 40, `${rows.length} labels`)
report('Properties rows and groups', missing(rows))
const entity = await source('../crates/engine/doc/src/entity.rs')
const typeNames = all(entity.slice(entity.indexOf('fn type_name'), entity.indexOf('fn dxf_name')), /=> "([^"]+)"/g)
step('the object types are read', typeNames.length > 20, `${typeNames.length} types`)
report('object types', missing(typeNames))
const grips = await source('../crates/engine/engine/src/grips.rs')
const gripLabels = all(grips.slice(grips.indexOf('pub fn label(self)'), grips.indexOf('pub fn title(self)')), /=> "([^"]+)"/g)
step('the grip menu\'s options are read', gripLabels.length >= 3, `${gripLabels.length} options`)
report('grip menu options', missing(gripLabels))

// 4. What the page draws in Chinese.
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
page.setDefaultTimeout(10_000)
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const kit = page.frameLocator('#kit')
const until = async (check, tries = 50) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
/** The texts of the elements `selector` finds in the editor that are English and not Chinese. */
const english = async (selector, attribute = null) => {
  const texts = await kit.locator(selector).evaluateAll((elements, attribute) => elements.map((element) => (attribute ? element.getAttribute(attribute) : element.textContent) ?? ''), attribute)
  return [...new Set(texts.map((text) => text.replace(/[▸▾…]/g, '').trim()).filter((text) => text && latin(text) && !chinese(text)))].sort()
}
try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  await until(async () => (await details()).entities === 7, 100)
  await page.waitForTimeout(500)

  const listed = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'command.list', params: {} }], false))
  const commands = listed.completed?.[0]?.result?.commands ?? []
  step('the engine lists its commands', commands.length > 200, `${commands.length} commands`)
  report('command titles and menu groups', missing(commands.flatMap((command) => [command.title, ...command.menu.slice(0, -1)])))

  const tabs = await kit.locator('.kui-ribbon-tabs [role=tab]').evaluateAll((elements) => elements.map((element) => element.dataset.tab))
  step('the ribbon has its tabs', tabs.length >= 10, tabs.join(', '))
  report('tab names', await english('.kui-ribbon-tabs [role=tab]'))
  const ribbon = []
  for (const tab of tabs) {
    await kit.getByTestId(`ribbon-tab-${tab}`).click()
    await page.waitForTimeout(400)
    for (const text of [...await english('.kui-ribbon-panel .kui-ribbon-group-label'), ...await english('.kui-ribbon-panel button', 'aria-label'), ...await english('.kui-ribbon-panel button', 'title')]) ribbon.push(`${tab}: ${text}`)
  }
  report('every ribbon tab\'s groups and buttons', [...new Set(ribbon)])
  await kit.getByTestId('ribbon-tab-home').click()

  const chrome = '.kui-status-bar button:not(.cad-layout-tab)'
  const status = [...await english(chrome), ...await english(chrome, 'title'), ...await english(chrome, 'aria-label'), ...await english('[data-testid=cad-layout-Model]')]
  report('the status bar', status)
  for (const menu of ['osnap', 'polar']) {
    const arrow = kit.getByTestId(`cad-mode-${menu}-menu`)
    if (!(await arrow.count())) continue
    await arrow.click()
    await page.waitForTimeout(200)
    report(`the status bar's ${menu} menu`, await english('[role=menu] [role^=menuitem]'))
    await page.keyboard.press('Escape')
    await page.waitForTimeout(200)
  }

  const surface = kit.getByTestId('kit-canvas-surface')
  const bounds = await surface.boundingBox()
  await page.mouse.click(bounds.x + 40, bounds.y + 40, { button: 'right' })
  await until(async () => (await kit.getByTestId('cad-context-menu').count()) > 0)
  report('the drawing menu with nothing selected', await english('[data-testid=cad-context-menu] [role^=menuitem]'))
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)

  report('the Properties panel with nothing selected', [...await english('.cad-property-group-title'), ...await english('.cad-property > span:first-child')])
  await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'edit.selectAll', params: {} }], false))
  await until(async () => (await details()).selection > 0)
  await page.waitForTimeout(400)
  await page.mouse.click(bounds.x + 40, bounds.y + 40, { button: 'right' })
  await until(async () => (await kit.getByTestId('cad-context-menu').count()) > 0)
  report('the drawing menu with a selection', await english('[data-testid=cad-context-menu] [role^=menuitem]'))
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  report('the Properties panel with a selection', [...await english('.cad-property-group-title'), ...await english('.cad-property > span:first-child'), ...await english('[data-testid=cad-selection-type]')])
  step('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '))
} catch (error) {
  step('threw', false, error instanceof Error ? error.message.split('\n')[0] : String(error))
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
