// A drawing changed outside Kition keeps its replica (Kit UI protocol UI-R1):
// node tests/replica.mjs [chromium|webkit]. Opens dev.html with the replica and checkpoints kept,
// draws and saves, then gives the file new bytes from elsewhere (`devHost.reopenChanged`): another
// program turned the Furniture layer off. The editor opens with the stored snapshot and the engine
// records what the file changed as one `import` change: the outside change shows, the replica is
// the same one (its id, its version, its checkpoints), and the layers keep their ids. Undo steps
// live in the session, so they are not asserted here.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which}: ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 100) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const PATH = 'Sample.dxf'
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details
/** Every layer, id → { name, on }. */
const layers = async () => Object.fromEntries((await details()).layers.map((layer) => [layer.id, { name: layer.name, on: layer.on }]))
const furniture = async () => (await details()).layers.find((layer) => layer.name === 'Furniture')
const opened = () => page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })

try {
  await page.goto(`${origin}/dev.html?replica=1&checkpoints=1`)
  await opened()
  await until(async () => (await page.evaluate(() => window.devHost.connection() != null)) && (await details()).entities > 0)
  const start = (await details()).entities
  step('the sample has a Furniture layer that is on', (await furniture())?.on === true)

  // An edit, saved: the host keeps the file and the replica beside it.
  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'circle', params: { center: [60, 250], radius: 10 } }], false))
  step('edits the drawing', run.completed[0].changed)
  await page.frameLocator('#kit').getByTestId('cad-save').click()
  const saved = () => page.evaluate((path) => window.devHost.files.get(path).version > 1 && window.devHost.history.replica(path) != null, PATH)
  await until(async () => (await saved()) === true)
  step('saves the file and its replica', await saved())
  const before = await layers()
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save' }))
  step('the host records a checkpoint', checkpoint != null)
  const docId = await page.evaluate(async () => {
    const connection = window.devHost.connection()
    window.versionBefore = await connection.replica.version()
    return connection.replica.info()
  })
  step('the replica has an id', typeof docId === 'string' && docId.length > 0, String(docId))

  // Another program turns the Furniture layer off in the saved file (a negative colour, group 62).
  const changed = await page.evaluate((path) => {
    const text = new TextDecoder().decode(window.devHost.files.get(path).bytes)
    const eol = text.includes('\r\n') ? '\r\n' : '\n'
    const lines = text.split(/\r?\n/)
    let record = ''
    for (let i = 0; i + 1 < lines.length; i += 2) {
      const code = lines[i].trim()
      if (code === '0') record = lines[i + 1].trim()
      if (record === 'LAYER' && code === '2' && lines[i + 1].trim() === 'Furniture') {
        for (let j = i + 2; j + 1 < lines.length && lines[j].trim() !== '0'; j += 2) {
          if (lines[j].trim() === '62') {
            lines[j + 1] = String(-Math.abs(Number(lines[j + 1].trim())))
            return lines.join(eol)
          }
        }
      }
    }
    return null
  }, PATH)
  step('the outside program changes the file', changed != null)
  await page.evaluate((text) => window.devHost.reopenChanged(new TextEncoder().encode(text).buffer), changed)
  await opened()
  await until(async () => (await furniture())?.on === false)
  step('the outside change shows', (await furniture())?.on === false)
  step('the drawing keeps the edit from before', (await details()).entities === start + 1, String((await details()).entities))
  step('the drawing opens clean', (await inspect()).dirty === false)
  const after = await layers()
  step(
    'every layer keeps its id',
    JSON.stringify(Object.keys(after).sort()) === JSON.stringify(Object.keys(before).sort()),
    `${Object.keys(before)} -> ${Object.keys(after)}`,
  )
  step(
    'the other layers are unchanged',
    Object.keys(before).every((id) => before[id].name === 'Furniture' || JSON.stringify(before[id]) === JSON.stringify(after[id])),
  )
  const kept = await page.evaluate(async () => {
    const connection = window.devHost.connection()
    const { changes } = await connection.replica.exportChanges(window.versionBefore)
    return { docId: await connection.replica.info(), since: changes.byteLength }
  })
  step('the stored replica opened: the same replica id', kept.docId === docId, `${docId} -> ${kept.docId}`)
  step('it goes on from the version from before, with the import after it', kept.since > 0, String(kept.since))
  step('the stored replica is still kept', await page.evaluate((path) => window.devHost.history.replica(path) != null, PATH))
  const checkpoints = await page.evaluate((path) => window.devHost.history.checkpoints(path), PATH)
  step('the checkpoint from before stays', checkpoints.some((entry) => entry.id === checkpoint?.id))
} catch (error) {
  failures += 1
  console.log(`FAIL ${which}: ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
